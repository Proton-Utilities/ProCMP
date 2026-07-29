//! The darklua configuration a task compiles to.
//!
//! Assembled here and nowhere else, so a plan is complete and `build` only executes it.
//!
//! Loaders are the awkward part. darklua takes the first matching pattern, but its loader
//! deserialiser implements `visit_map` and not `visit_seq`, and `Configuration::add_loader`
//! which would let the order live in Rust, cannot be called from outside, because its
//! `Loader` parameter is `pub` inside a private module and is not re-exported. So loaders
//! must travel as a JSON map, and their order survives only because `serde_json` is built
//! with `preserve_order`. That is why they are a `Vec` here, hashed as a sequence, and
//! become a map at the last possible moment.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use darklua_core::Configuration;
use serde_json::{Map, Value};

use crate::manifest::{Ident, Loader, Scalar};
use crate::report::{Code, Diagnostic, Location};

/// darklua substitutes a define by name, so every injection has to precede every rule
/// that reads the value it substitutes.
pub const INJECT: &str = "inject_global_value";

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Config {
    /// [`None`] lets darklua apply its own defaults, which is reachable only when a task
    /// declares no rules and no defines.
    pub rules: Option<Vec<Value>>,
    /// First match wins, so this is a sequence and stays one.
    pub loaders: Vec<Loader>,
    /// Everything else darklua understands, verbatim.
    pub rest: Map<String, Value>,
}

impl Config {
    /// `declared` is the profile's `darklua` block. `rules` is lifted out of it because
    /// injections go in front of whatever was written.
    pub fn assemble(
        declared: Option<&Map<String, Value>>,
        loaders: Option<&[Loader]>,
        defines: &BTreeMap<Ident, Scalar>,
        at: &Location,
    ) -> Result<Self, Diagnostic> {
        let mut rest = declared.cloned().unwrap_or_default();

        let written = match rest.remove("rules") {
            None => None,
            Some(Value::Array(rules)) => Some(rules),
            Some(_) => {
                return Err(
                    Diagnostic::new(Code::BadRules, "`darklua.rules` is not a list")
                        .at(at.clone().field("darklua").field("rules")),
                );
            }
        };

        let injections = injections(defines);
        let rules = match (written, injections.is_empty()) {
            (None, true) => None,
            (None, false) => Some([injections, defaults()].concat()),
            (Some(written), _) => Some([injections, written].concat()),
        };

        let config = Self {
            rules,
            loaders: loaders.unwrap_or_default().to_vec(),
            rest,
        };

        // Checked here rather than at build time, so a loader reports against the field
        // that holds it instead of failing the whole configuration later with one generic
        // message. Each is probed on its own, because darklua cannot say which of them it
        // objected to.
        for (index, loader) in config.loaders.iter().enumerate() {
            check(loader, &at.clone().field("loaders").index(index))?;
        }

        config.build("")?;
        Ok(config)
    }

    /// A valid `.darklua.json`, which is what `plan <TASK>` prints.
    pub fn json(&self) -> Value {
        let mut config = self.rest.clone();

        if let Some(rules) = &self.rules {
            config.insert("rules".to_owned(), Value::Array(rules.clone()));
        }

        if !self.loaders.is_empty() {
            // Insertion order is the semantics: darklua takes the first pattern that
            // matches, and `preserve_order` is what keeps this map in the order written.
            let loaders: Map<String, Value> = self
                .loaders
                .iter()
                .map(|entry| (entry.pattern.clone(), Value::String(entry.loader.clone())))
                .collect();
            config.insert("loaders".to_owned(), Value::Object(loaders));
        }

        Value::Object(config)
    }

    /// The one conversion into darklua's own type.
    pub fn build(&self, task: &str) -> Result<Configuration, Diagnostic> {
        serde_json::from_value(self.json()).map_err(|error| {
            let message = if task.is_empty() {
                "darklua rejected this configuration".to_owned()
            } else {
                format!("darklua rejected the configuration `{task}` compiles to")
            };

            Diagnostic::new(Code::DarkluaConfig, message)
                .help(error.to_string())
                .help(serde_json::to_string_pretty(&self.json()).unwrap_or_default())
        })
    }

    /// Rule names in order, for the ordering lints.
    pub fn rule_names(&self) -> Vec<&str> {
        self.rules
            .iter()
            .flatten()
            .filter_map(|rule| match rule {
                Value::String(name) => Some(name.as_str()),
                Value::Object(map) => map.get("rule").and_then(Value::as_str),
                _ => None,
            })
            .collect()
    }
}

/// Checks one loader, naming whichever half darklua objected to.
///
/// darklua reports a rejected configuration as a whole, so the name is tried against a
/// pattern known to be good and the pattern against a loader known to be good. Two probes
/// are cheaper than a message that says only "this configuration is wrong".
fn check(loader: &Loader, at: &Location) -> Result<(), Diagnostic> {
    if probe("**/*", &loader.loader).is_err() {
        return Err(Diagnostic::new(
            Code::BadLoader,
            format!("`{}` is not a loader", loader.loader),
        )
        .at(at.clone().field("use"))
        .help(
            "copy, skip, luau, json, json_lines, toml, yaml, string, buffer, bytes, \
             and string/base64, string/zstd, string/gzip, string/zlib, likewise for \
             buffer and bytes",
        ));
    }

    probe(&loader.pattern, "copy").map_err(|error| {
        Diagnostic::new(
            Code::BadLoaderPattern,
            format!("`{}` is not a valid loader pattern", loader.pattern),
        )
        .at(at.clone().field("pattern"))
        .help(error)
    })
}

/// One loader, on its own, through darklua's own deserialiser, which is the only thing
/// that knows what it accepts.
fn probe(pattern: &str, loader: &str) -> Result<(), String> {
    let mut loaders = Map::new();
    loaders.insert(pattern.to_owned(), Value::String(loader.to_owned()));

    let mut config = Map::new();
    config.insert("loaders".to_owned(), Value::Object(loaders));

    serde_json::from_value::<Configuration>(Value::Object(config))
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// One `inject_global_value` per define, in key order because `defines` is a `BTreeMap`.
fn injections(defines: &BTreeMap<Ident, Scalar>) -> Vec<Value> {
    defines
        .iter()
        .map(|(identifier, value)| {
            let mut rule = Map::new();
            rule.insert("rule".to_owned(), Value::String(INJECT.to_owned()));
            rule.insert(
                "identifier".to_owned(),
                Value::String(identifier.to_string()),
            );
            rule.insert("value".to_owned(), value.json());
            Value::Object(rule)
        })
        .collect()
}

/// Read from the linked darklua once for the process.
fn defaults() -> Vec<Value> {
    static DEFAULTS: OnceLock<Vec<Value>> = OnceLock::new();

    DEFAULTS
        .get_or_init(|| {
            darklua_core::rules::get_default_rules()
                .iter()
                .filter_map(|rule| serde_json::to_value(rule.as_ref()).ok())
                .collect()
        })
        .clone()
}

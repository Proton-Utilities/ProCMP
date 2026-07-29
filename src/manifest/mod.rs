//! The config surface, as written by a user.
//!
//! Every field a user can get wrong is a `String` here, because a diagnostic has to be
//! able to quote what they wrote. [`crate::plan`] converts them into the validated types
//! a [`crate::plan::Task`] holds, which is why nothing downstream can carry an invalid
//! identifier.
//!
//! Every unordered collection here is a `BTreeMap`, so key order is a property of the
//! types rather than something to remember to impose. `serde_json::Map` is the exception
//! because it preserves insertion order, on which loader order depends, which is why
//! [`crate::plan::canonical`] sorts explicitly rather than trusting what it is given.

pub mod format;
pub mod ledger;
pub mod luau;
pub mod scaffold;
pub mod schema;

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::report::{Code, Diagnostic};

/// Reserved words, which cannot be a Luau global and so cannot be an [`Ident`].
///
/// Also what [`schema`] quotes a field name against: `end` is a legal JSON key and not a
/// bare Luau one.
pub const KEYWORDS: &[&str] = &[
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "if", "in", "local",
    "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];

/// A `templates` or `profiles` key.
///
/// The excluded characters delimit a task identifier such as `dist[target=roblox]`, so a
/// name holding one could not be selected unambiguously on the command line.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct Name(String);

/// A Luau identifier.
///
/// Var names, axis names and define keys are all identifiers, because each becomes both
/// a `{token}` and a `PCMP_<NAME>` global.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct Ident(String);

/// The value of a var or a define.
///
/// `Int` and `Float` are separate so that `3` stays an integer through the digest and
/// into the emitted Luau, rather than becoming `3.0` on the way.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum Scalar {
    Bool(bool),
    Int(i64),
    /// Infinity and NaN are rejected during resolution.
    Float(f64),
    Text(String),
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Pointer for editor validation. Accepted and ignored.
    #[serde(default, rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,

    /// Named values every profile starts from.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub vars: BTreeMap<String, Scalar>,

    /// Never built. Extended, or used as the base of one that is.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub templates: BTreeMap<String, Profile>,

    #[serde(default)]
    pub profiles: BTreeMap<String, Profile>,
}

/// One build, or one per axis combination when it declares `axes`.
///
/// Every field that replaces wholesale is an [`Option`], collections included:
/// inheritance has to tell "not declared" from "declared empty". `vars` and `define`
/// accumulate instead, so they need no sentinel.
#[derive(Debug, Clone, Default, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extends: Option<String>,

    /// One file, or a directory processed into a directory. Token-expanded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,

    /// Token-expanded. No default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,

    /// Extra files and directories whose contents count as build inputs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sources: Option<Vec<String>>,

    /// Globs excluded from that input set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ignore: Option<Vec<String>>,

    /// Each becomes a `{token}` and a `PCMP_<NAME>` constant.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub vars: BTreeMap<String, Scalar>,

    /// Compile-time constants: injected as globals, then folded away.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub define: BTreeMap<String, Scalar>,

    /// Written above each artifact after darklua runs, which discards comments.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<Vec<String>>,

    /// darklua takes the first matching pattern, so this is a list rather than a map:
    /// order decides which loader wins, and only a list has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loaders: Option<Vec<Loader>>,

    /// darklua's configuration, verbatim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub darklua: Option<Map<String, Value>>,

    /// One task per combination. A profile with no axes is one task.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub axes: BTreeMap<String, Axis>,
}

/// A list of values, or a value-to-overlay map when a combination needs settings of its
/// own.
#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum Axis {
    Values(Vec<String>),
    Overlays(BTreeMap<String, Profile>),
}

impl Axis {
    pub fn values(&self) -> Vec<&String> {
        match self {
            Self::Values(values) => values.iter().collect(),
            Self::Overlays(overlays) => overlays.keys().collect(),
        }
    }

    pub fn overlay(&self, value: &str) -> Option<&Profile> {
        match self {
            Self::Values(_) => None,
            Self::Overlays(overlays) => overlays.get(value),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Loader {
    pub pattern: String,
    /// `copy`, `json`, `toml`, `yaml`, `string`, `buffer`, `bytes`, `skip`, or an encoded
    /// form such as `string/base64`.
    #[serde(rename = "use")]
    pub loader: String,
}

impl Name {
    pub fn new(name: &str) -> Result<Self, Diagnostic> {
        if name.is_empty() {
            return Err(Diagnostic::new(Code::BadName, "a name is empty"));
        }

        if let Some(character) = name.chars().find(|c| matches!(c, '[' | ']' | ',' | '=')) {
            return Err(Diagnostic::new(
                Code::BadName,
                format!("name `{name}` contains `{character}`"),
            )
            .help(
                "`[`, `]`, `,` and `=` delimit a task identifier such as `dist[target=roblox]`",
            ));
        }

        Ok(Self(name.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Ident {
    pub fn new(name: &str) -> Result<Self, Diagnostic> {
        let mut characters = name.chars();

        let shaped = characters
            .next()
            .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
            && characters.all(|rest| rest.is_ascii_alphanumeric() || rest == '_');

        if !shaped || KEYWORDS.contains(&name) {
            return Err(Diagnostic::new(
                Code::BadVar,
                format!("`{name}` is not a Luau identifier"),
            )
            .help(
                "letters, digits and underscores, not starting with a digit, and not a keyword",
            ));
        }

        Ok(Self(name.to_owned()))
    }

    /// The `PCMP_<NAME>` global this identifier contributes.
    #[must_use]
    pub fn constant(&self) -> Self {
        Self(format!("PCMP_{}", self.0.to_uppercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Scalar {
    /// How a scalar reads inside a `{token}`. The injected define keeps its type, and only
    /// the template form is text.
    pub fn text(&self) -> String {
        match self {
            Self::Bool(value) => value.to_string(),
            Self::Int(value) => value.to_string(),
            Self::Float(value) => value.to_string(),
            Self::Text(value) => value.clone(),
        }
    }

    /// The JSON darklua injects, which is where the type distinction matters.
    pub fn json(&self) -> Value {
        match self {
            Self::Bool(value) => Value::Bool(*value),
            Self::Int(value) => Value::Number((*value).into()),
            Self::Float(value) => {
                serde_json::Number::from_f64(*value).map_or(Value::Null, Value::Number)
            }
            Self::Text(value) => Value::String(value.clone()),
        }
    }

    /// Type-tagged, so `true` and `"true"` never share a cache key.
    pub fn tagged(&self) -> String {
        match self {
            Self::Bool(value) => format!("bool:{value}"),
            Self::Int(value) => format!("int:{value}"),
            Self::Float(value) => format!("float:{value:?}"),
            Self::Text(value) => format!("string:{value}"),
        }
    }

    /// Whether this can reach Luau intact.
    ///
    /// A Luau number is an IEEE double, which represents every integer up to 2^53 and not
    /// every one past it, so a larger `Int` would arrive as a different number.
    pub fn representable(&self) -> bool {
        match self {
            Self::Float(value) => value.is_finite(),
            Self::Int(value) => value.unsigned_abs() <= (1u64 << 53),
            _ => true,
        }
    }

    /// Parses a `--define` or `--var` value. Only a number that survives a round trip is
    /// read as one, so `007` stays the string it was written as.
    pub fn parse(text: &str) -> Self {
        match text {
            "true" => Self::Bool(true),
            "false" => Self::Bool(false),
            other => {
                if let Ok(int) = other.parse::<i64>()
                    && int.to_string() == other
                {
                    return Self::Int(int);
                }
                match other.parse::<f64>() {
                    Ok(float) if float.is_finite() && float.to_string() == other => {
                        Self::Float(float)
                    }
                    _ => Self::Text(other.to_owned()),
                }
            }
        }
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Display for Ident {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Display for Scalar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text())
    }
}

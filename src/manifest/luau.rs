//! The Luau front end.
//!
//! Luau's own sandbox removes `os`, `io`, `load`, `dofile`, `debug` and `coroutine`, and
//! [`REVOKED`] takes the rest. What is left of the outside reaches a manifest only
//! through `pcmp`, and everything `pcmp` hands over is written into the ledger, which is
//! what makes reading the clock safe rather than forbidden.

use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use mlua::{Lua, LuaOptions, LuaSerdeExt, StdLib, Value, VmState};
// Aliased, because `Value` in this module is Lua's.
use serde_json::{Map, Value as Json};

use super::ledger::Reader;
use super::{Axis, Manifest, Profile};
use crate::report::{Code, Diagnostic};
use crate::vfs::{self, AbsPath, RelPath, digest};

/// Globals that survive Luau's sandbox but would let a manifest observe the outside, load
/// code at run time, or answer differently on a second evaluation.
pub const REVOKED: &[&str] = &[
    "collectgarbage",
    "getfenv",
    "loadstring",
    "newproxy",
    "require",
    "setfenv",
];

/// `math.random` is the one impurity the ledger cannot capture: there is nothing to write
/// down that would reproduce it.
const REVOKED_MEMBERS: &[(&str, &str)] = &[("math", "random"), ("math", "randomseed")];

/// Bounds a manifest that never terminates.
const BUDGET: u64 = 50_000_000;
const MEMORY: usize = 32 * 1024 * 1024;

/// A manifest that exhausted [`BUDGET`] raises this, which separates it from a manifest
/// that raised an error of its own.
const EXHAUSTED: &str = "evaluation budget exhausted";

/// Likewise for an unset variable, which is a manifest asking for something that is not
/// there rather than a manifest that is wrong.
///
/// This must stay a prefix of what [`install`] raises from `pcmp.env`. A Lua error is a
/// string by the time it arrives, so there is nothing else to match on.
const UNSET: &str = "is not set. Pass `--env";

pub fn eval(
    source: &str,
    origin: &str,
    root: &AbsPath,
    reader: &Rc<Reader>,
) -> Result<Manifest, Diagnostic> {
    let lua = Lua::new_with(
        StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::BIT | StdLib::UTF8,
        LuaOptions::new(),
    )
    .map_err(|error| vm(origin, "create the interpreter", &error))?;

    // Both must precede `sandbox`, which freezes the global and library tables.
    harden(&lua, origin)?;
    install(&lua, origin, root, reader)?;

    lua.sandbox(true)
        .map_err(|error| vm(origin, "enable the sandbox", &error))?;
    lua.set_memory_limit(MEMORY)
        .map_err(|error| vm(origin, "set the memory limit", &error))?;

    let steps = AtomicU64::new(0);
    lua.set_interrupt(move |_| {
        if steps.fetch_add(1, Ordering::Relaxed) >= BUDGET {
            return Err(mlua::Error::runtime(EXHAUSTED));
        }
        Ok(VmState::Continue)
    });

    let value: Value = lua.load(source).set_name(origin).eval().map_err(|error| {
        let message = error.to_string();

        // A Lua error is a string by the time it arrives, so the two cases a manifest
        // author can act on are told apart by what raised them.
        if message.contains(EXHAUSTED) {
            Diagnostic::new(
                Code::Budget,
                format!("`{origin}` exceeded its evaluation budget"),
            )
            .help("a manifest describes a build, so it has to terminate")
        } else if message.contains(UNSET) {
            Diagnostic::new(Code::UnsetEnv, "an environment variable is not set").help(message)
        } else {
            Diagnostic::new(Code::Eval, format!("`{origin}` failed while evaluating")).help(message)
        }
    })?;

    if !matches!(value, Value::Table(_)) {
        return Err(Diagnostic::new(
            Code::NotATable,
            format!("`{origin}` returned {}, not a table", value.type_name()),
        )
        .help("a manifest is a table of `vars`, `templates` and `profiles`"));
    }

    let mut manifest: Manifest = lua.from_value(value).map_err(|error| {
        Diagnostic::new(Code::Syntax, format!("`{origin}` is not a valid manifest"))
            .help(error.to_string())
    })?;

    disambiguate(&mut manifest);
    Ok(manifest)
}

/// An empty Luau table is both an empty list and an empty map, and the language offers no
/// way to say which, so `{}` arrives as a map wherever the target is untyped.
///
/// That matters because an empty list is meaningful: `rules = {}` means "bundle and
/// generate, transform nothing", which is different from omitting `rules` and getting
/// darklua's defaults.
///
/// Only the keys darklua reads as lists are converted. Turning *every* empty table into a
/// list would break `bundle.require_mode.aliases = {}`, which is genuinely a map, and
/// breaking something that works is worse than leaving an ambiguity documented.
fn disambiguate(manifest: &mut Manifest) {
    for profile in manifest
        .templates
        .values_mut()
        .chain(manifest.profiles.values_mut())
    {
        in_profile(profile);
    }
}

/// darklua's list-valued configuration keys, at every level a manifest can reach.
const LISTS: &[&str] = &[
    "rules",
    "apply_to_files",
    "skip_files",
    "excludes",
    "globals",
];

fn in_profile(profile: &mut Profile) {
    if let Some(darklua) = profile.darklua.as_mut() {
        empty_lists(darklua);
    }

    for axis in profile.axes.values_mut() {
        if let Axis::Overlays(overlays) = axis {
            for overlay in overlays.values_mut() {
                in_profile(overlay);
            }
        }
    }
}

fn empty_lists(map: &mut Map<String, Json>) {
    for (key, value) in map.iter_mut() {
        match value {
            Json::Object(nested) if nested.is_empty() && LISTS.contains(&key.as_str()) => {
                *value = Json::Array(Vec::new());
            }
            Json::Object(nested) => empty_lists(nested),
            Json::Array(items) => {
                for item in items {
                    if let Json::Object(nested) = item {
                        empty_lists(nested);
                    }
                }
            }
            _ => {}
        }
    }
}

fn harden(lua: &Lua, origin: &str) -> Result<(), Diagnostic> {
    let globals = lua.globals();

    // Luau's own `print` writes to stdout, where it would land in the middle of `--json`.
    // This is the one place outside `cli::render` that writes to a stream, because a
    // foreign language's `print` has to go somewhere and stdout is spoken for.
    #[allow(clippy::print_stderr, reason = "redirecting a manifest's own print")]
    let printed = lua
        .create_function(|_, values: mlua::Variadic<Value>| {
            let line = values
                .iter()
                .map(Value::to_string)
                .collect::<mlua::Result<Vec<_>>>()?;
            eprintln!("{}", line.join("\t"));
            Ok(())
        })
        .map_err(|error| vm(origin, "redirect `print`", &error))?;

    globals
        .set("print", printed)
        .map_err(|error| vm(origin, "redirect `print`", &error))?;

    for name in REVOKED {
        globals
            .set(*name, Value::Nil)
            .map_err(|error| vm(origin, "revoke a global", &error))?;
    }

    for (library, member) in REVOKED_MEMBERS {
        let table: Option<mlua::Table> = globals
            .get(*library)
            .map_err(|error| vm(origin, "read a library", &error))?;

        if let Some(table) = table {
            table
                .set(*member, Value::Nil)
                .map_err(|error| vm(origin, "revoke a library member", &error))?;
        }
    }

    Ok(())
}

/// Installs `pcmp`.
///
/// The reader is shared rather than borrowed because mlua requires a closure to be
/// `'static`. It costs five clones, one per entry point, once per evaluation.
fn install(lua: &Lua, origin: &str, root: &AbsPath, reader: &Rc<Reader>) -> Result<(), Diagnostic> {
    let api = lua
        .create_table()
        .map_err(|error| vm(origin, "create the api", &error))?;

    let held = Rc::clone(reader);
    let env = lua
        .create_function(move |_, name: String| {
            held.env(&name).ok_or_else(|| {
                mlua::Error::runtime(format!(
                    "environment variable `{name}` is not set. \
                     Pass `--env {name}=VALUE`, or use pcmp.envOr(name, fallback)"
                ))
            })
        })
        .map_err(|error| vm(origin, "create pcmp.env", &error))?;

    let held = Rc::clone(reader);
    let env_or = lua
        .create_function(move |_, (name, fallback): (String, String)| {
            Ok(held.env(&name).unwrap_or(fallback))
        })
        .map_err(|error| vm(origin, "create pcmp.envOr", &error))?;

    let held = Rc::clone(reader);
    let now = lua
        .create_function(move |_, ()| Ok(held.now()))
        .map_err(|error| vm(origin, "create pcmp.now", &error))?;

    let held = Rc::clone(reader);
    let epoch = lua
        .create_function(move |_, ()| Ok(held.epoch()))
        .map_err(|error| vm(origin, "create pcmp.epoch", &error))?;

    let held = Rc::clone(reader);
    let base = root.clone();
    let read = lua
        .create_function(move |_, path: String| {
            contents(&base, &held, &path).map_err(mlua::Error::runtime)
        })
        .map_err(|error| vm(origin, "create pcmp.read", &error))?;

    api.set("env", env)
        .and_then(|()| api.set("envOr", env_or))
        .and_then(|()| api.set("now", now))
        .and_then(|()| api.set("epoch", epoch))
        .and_then(|()| api.set("read", read))
        .and_then(|()| api.set("root", root.as_str()))
        .and_then(|()| api.set("darklua", crate::DARKLUA))
        .map_err(|error| vm(origin, "populate the api", &error))?;

    lua.globals()
        .set("pcmp", api)
        .map_err(|error| vm(origin, "install the pcmp global", &error))
}

/// `pcmp.read`. The file's digest goes into the ledger, so `--frozen` can check that the
/// file still says what it said when the lock was written.
fn contents(root: &AbsPath, reader: &Reader, path: &str) -> Result<String, String> {
    let relative = RelPath::new(path).map_err(|error| error.message)?;
    let absolute = root
        .join(relative.as_str())
        .map_err(|error| error.message)?;

    let bytes = vfs::read(&absolute).map_err(|error| error.message)?;
    reader
        .file(&relative, digest::of(&bytes))
        .map_err(|error| error.message)?;

    String::from_utf8(bytes).map_err(|_| format!("`{path}` is not UTF-8"))
}

fn vm(origin: &str, action: &str, error: &mlua::Error) -> Diagnostic {
    Diagnostic::new(Code::Eval, format!("`{origin}` could not {action}")).help(error.to_string())
}

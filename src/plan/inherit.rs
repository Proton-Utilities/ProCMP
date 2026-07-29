//! Collapsing an `extends` chain into one profile.
//!
//! `templates` and `profiles` share one namespace, so `extends` needs no precedence rule
//! and a name in both maps is a collision rather than a silent winner.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::manifest::{Manifest, Profile};
use crate::report::{Code, Diagnostic, Location};

/// Nearer profiles win field by field. `vars` and `define` accumulate, and `darklua`
/// merges key by key so a profile can set `generator` without restating `bundle`.
pub fn flatten(
    manifest: &Manifest,
    name: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Profile> {
    let mut chain: Vec<&Profile> = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    let mut cursor = name;

    loop {
        // Also bounds the walk: a chain longer than the profile count must repeat.
        if let Some(entered) = seen.iter().position(|step| *step == cursor) {
            // From where the chain re-enters itself, so the prefix that led there is
            // left out of the list.
            let mut cycle: Vec<&str> = seen.get(entered..).unwrap_or_default().to_vec();
            cycle.push(cursor);

            diagnostics.push(
                Diagnostic::new(
                    Code::CyclicExtends,
                    format!("`{name}` has a cyclic `extends` chain"),
                )
                .at(located(manifest, name))
                .help(format!("the cycle is: {}", cycle.join(" -> "))),
            );
            return None;
        }

        let Some(profile) = lookup(manifest, cursor) else {
            diagnostics.push(
                Diagnostic::new(Code::UnknownBase, format!("`{cursor}` does not exist"))
                    .at(located(manifest, name).field("extends"))
                    .help(format!(
                        "referenced by `{name}`; known: {}",
                        known(manifest)
                    )),
            );
            return None;
        };

        seen.push(cursor);
        chain.push(profile);

        match profile.extends.as_deref() {
            Some(parent) => cursor = parent,
            None => break,
        }
    }

    let mut merged = Profile {
        vars: manifest.vars.clone(),
        ..Profile::default()
    };

    // `chain` runs child to ancestor, so fold from the ancestor end.
    for profile in chain.iter().rev() {
        overlay(&mut merged, profile);
    }

    merged.extends = None;
    Some(merged)
}

/// Applies `over` on top of `base`, one field at a time.
pub fn overlay(base: &mut Profile, over: &Profile) {
    replace(&mut base.entry, over.entry.as_ref());
    replace(&mut base.output, over.output.as_ref());
    replace(&mut base.sources, over.sources.as_ref());
    replace(&mut base.ignore, over.ignore.as_ref());
    replace(&mut base.header, over.header.as_ref());
    replace(&mut base.loaders, over.loaders.as_ref());

    if let Some(over) = &over.darklua {
        merge(base.darklua.get_or_insert_with(Map::new), over);
    }

    extend(&mut base.vars, &over.vars);
    extend(&mut base.define, &over.define);

    for (axis, values) in &over.axes {
        base.axes.insert(axis.clone(), values.clone());
    }
}

/// A declared field replaces, and an absent one leaves what was inherited. This is why every
/// wholesale field is an `Option`, collections included: a child clears an inherited list
/// by declaring it empty, which is different from not mentioning it.
fn replace<T: Clone>(base: &mut Option<T>, over: Option<&T>) {
    if let Some(value) = over {
        *base = Some(value.clone());
    }
}

fn extend<V: Clone>(base: &mut BTreeMap<String, V>, over: &BTreeMap<String, V>) {
    for (key, value) in over {
        base.insert(key.clone(), value.clone());
    }
}

/// Objects merge, arrays and scalars replace, and `null` unsets, without which a child
/// could never clear an inherited `bundle`.
fn merge(base: &mut Map<String, Value>, over: &Map<String, Value>) {
    for (key, value) in over {
        match value {
            Value::Null => {
                base.remove(key);
            }
            Value::Object(nested) => match base.get_mut(key) {
                Some(Value::Object(existing)) => merge(existing, nested),
                _ => {
                    base.insert(key.clone(), value.clone());
                }
            },
            _ => {
                base.insert(key.clone(), value.clone());
            }
        }
    }
}

pub fn lookup<'m>(manifest: &'m Manifest, name: &str) -> Option<&'m Profile> {
    manifest
        .templates
        .get(name)
        .or_else(|| manifest.profiles.get(name))
}

fn located(manifest: &Manifest, name: &str) -> Location {
    let map = if manifest.templates.contains_key(name) {
        "templates"
    } else {
        "profiles"
    };
    Location::new(map, name)
}

pub fn known(manifest: &Manifest) -> String {
    super::listed(
        manifest
            .templates
            .keys()
            .chain(manifest.profiles.keys())
            .map(String::as_str),
    )
}

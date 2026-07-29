//! Editor-facing schema emission.
//!
//! Both outputs come from the [`Manifest`] type the parser uses, so neither can describe a
//! manifest format this binary does not accept.

use std::fmt::Write as _;

use serde_json::Value;

use super::{KEYWORDS, Manifest};

pub fn json() -> String {
    serde_json::to_string_pretty(&schema()).unwrap_or_default()
}

/// Luau type definitions, giving `luau-lsp` completion inside a manifest.
pub fn luau() -> String {
    let schema = schema();
    let mut out = String::from(
        "--!strict\n\
         --- Type definitions for ProCMP manifests. Emitted by `pcmp schema --format luau`.\n\n",
    );

    if let Some(definitions) = schema.get("$defs").and_then(Value::as_object) {
        // Sorted, so regeneration is stable.
        let mut names: Vec<&String> = definitions.keys().collect();
        names.sort();

        for name in names {
            let Some(body) = definitions.get(name) else {
                continue;
            };
            let _ = write!(out, "export type {name} = {}\n\n", type_of(body));
        }
    }

    let _ = write!(out, "export type Manifest = {}\n\n", type_of(&schema));

    // Hand-written: this describes the globals the Luau front end installs, which are not
    // part of the manifest's shape.
    out.push_str(
        "--- The globals available inside a Luau manifest. Every reading is recorded in\n\
         --- pcmp.lock, which is what lets `pcmp.now` exist at all.\n\
         export type Api = {\n\
         \t--- Reads an environment variable. Errors when it is not set.\n\
         \tenv: (name: string) -> string,\n\
         \t--- Reads an environment variable, with an explicit fallback.\n\
         \tenvOr: (name: string, fallback: string) -> string,\n\
         \t--- An RFC 3339 instant in UTC. Pinned by --now or SOURCE_DATE_EPOCH.\n\
         \tnow: () -> string,\n\
         \t--- Seconds since the Unix epoch, consistent with now() within a run.\n\
         \tepoch: () -> number,\n\
         \t--- Reads a file relative to the manifest. Recorded by digest.\n\
         \tread: (path: string) -> string,\n\
         \t--- The manifest's own directory.\n\
         \troot: string,\n\
         \t--- The darklua this binary links against.\n\
         \tdarklua: string,\n\
         }\n\n\
         declare pcmp: Api\n\n\
         return nil\n",
    );

    out
}

fn schema() -> Value {
    serde_json::to_value(schemars::schema_for!(Manifest)).unwrap_or_default()
}

/// Handles exactly the shapes `schemars` produces for [`Manifest`]. Anything else becomes
/// `any`, which is honest: a wrong type is worse than a wide one.
fn type_of(node: &Value) -> String {
    if let Some(reference) = node.get("$ref").and_then(Value::as_str) {
        return match reference.rsplit('/').next() {
            Some(name) if !name.is_empty() => name.to_owned(),
            _ => "any".to_owned(),
        };
    }

    // `oneOf` carries string enums: each branch is a single `const`.
    if let Some(branches) = node.get("oneOf").and_then(Value::as_array) {
        let literals: Vec<String> = branches
            .iter()
            .filter_map(|branch| branch.get("const").and_then(Value::as_str))
            .map(|constant| format!("\"{constant}\""))
            .collect();
        if !literals.is_empty() {
            return literals.join(" | ");
        }
    }

    // `anyOf` is an untagged union, or an optional wrapping one branch.
    if let Some(branches) = node.get("anyOf").and_then(Value::as_array) {
        let nullable = branches.iter().any(is_null);
        let rendered: Vec<String> = branches
            .iter()
            .filter(|branch| !is_null(branch))
            .map(type_of)
            .collect();

        let joined = match rendered.len() {
            0 => "any".to_owned(),
            1 => rendered.into_iter().next().unwrap_or_default(),
            _ => rendered.join(" | "),
        };
        return if nullable { optional(&joined) } else { joined };
    }

    match node.get("type") {
        // `["array", "null"]` is an optional anything, which is what `Option<Vec<T>>` and
        // `Option<Struct>` produce.
        Some(Value::Array(kinds)) => {
            let nullable = kinds.iter().any(|kind| kind == "null");
            let base = kinds
                .iter()
                .filter(|kind| *kind != "null")
                .find_map(Value::as_str)
                .map_or_else(|| "any".to_owned(), |kind| named(kind, node));
            if nullable { optional(&base) } else { base }
        }
        Some(Value::String(kind)) => named(kind, node),
        _ => "any".to_owned(),
    }
}

/// `array` and `object` need the node too, because their shape sits beside the name.
fn named(kind: &str, node: &Value) -> String {
    match kind {
        "array" => format!(
            "{{ {} }}",
            node.get("items").map_or_else(|| "any".to_owned(), type_of)
        ),
        "object" => object_of(node),
        other => primitive(other).to_owned(),
    }
}

fn object_of(node: &Value) -> String {
    if let Some(values) = node.get("additionalProperties")
        && values.is_object()
    {
        return format!("{{ [string]: {} }}", type_of(values));
    }

    let Some(properties) = node.get("properties").and_then(Value::as_object) else {
        return "{ [string]: any }".to_owned();
    };

    let required: Vec<&str> = node
        .get("required")
        .and_then(Value::as_array)
        .map(|names| names.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();

    let mut fields = String::new();
    for (name, body) in properties {
        let mut rendered = type_of(body);
        if !required.contains(&name.as_str()) {
            rendered = optional(&rendered);
        }
        let _ = writeln!(fields, "\t{}: {rendered},", key(name));
    }

    format!("{{\n{fields}}}")
}

/// A keyword is quoted too: a field named `end` is legal JSON but not a bare Luau name.
fn key(name: &str) -> String {
    let bare = !name.is_empty()
        && !name.starts_with(|character: char| character.is_ascii_digit())
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
        && !KEYWORDS.contains(&name);

    if bare {
        name.to_owned()
    } else {
        format!("[\"{name}\"]")
    }
}

/// Without producing `T??` or an ambiguous `A | B?`.
fn optional(rendered: &str) -> String {
    if rendered.ends_with('?') {
        rendered.to_owned()
    } else if rendered.contains(" | ") {
        format!("({rendered})?")
    } else {
        format!("{rendered}?")
    }
}

fn is_null(node: &Value) -> bool {
    node.get("type").and_then(Value::as_str) == Some("null")
}

const fn primitive(kind: &str) -> &str {
    match kind.as_bytes() {
        b"string" => "string",
        b"boolean" => "boolean",
        b"integer" | b"number" => "number",
        _ => "any",
    }
}

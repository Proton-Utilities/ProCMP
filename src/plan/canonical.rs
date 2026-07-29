//! Hashing a value by what it means rather than by how it was written.
//!
//! The whole struct goes in, so a field added to a [`super::Task`] cannot be left out of
//! its cache key by forgetting to list it, which is exactly how `sources` and `ignore`
//! came to be missing from the digest this replaces.
//!
//! Key order is sorted away first. `serde_json::Map` preserves insertion order here,
//! because loader order depends on it, so a manifest that declares its darklua settings
//! inline and one that inherits them would otherwise hash differently despite meaning the
//! same thing.
//!
//! Sequences are left alone: a `Vec` in a task is a `Vec` because its order is part of
//! what it means. That is why `rules` and `loaders` are lists rather than maps.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;

use crate::vfs::{Digest, Hasher};

pub fn canonical<T: Serialize>(value: &T) -> Digest {
    let mut hasher = Hasher::new();

    match serde_json::to_value(value).map(sorted) {
        Ok(canonical) => hasher.field("canonical", canonical.to_string()),
        // Unreachable for plan data: every map key is a `String`, and resolution rejects
        // a scalar that is not finite. Hashing the reason rather than a constant keeps
        // two different failures from silently sharing a key.
        Err(error) => hasher.field("uncanonical", error.to_string()),
    };

    hasher.finish()
}

fn sorted(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let ordered: BTreeMap<String, Value> = map
                .into_iter()
                .map(|(key, nested)| (key, sorted(nested)))
                .collect();
            Value::Object(ordered.into_iter().collect())
        }
        Value::Array(items) => Value::Array(items.into_iter().map(sorted).collect()),
        scalar => scalar,
    }
}

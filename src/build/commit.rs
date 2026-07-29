//! Getting artifacts onto disk, and taking away what is no longer one.
//!
//! Three rules, each closing one way an artifact can end up wrong.
//!
//! Write only what differs, so an unchanged artifact keeps its mtime and nothing
//! downstream rebuilds for no reason. Write atomically, so an interrupted build leaves the
//! previous artifact whole rather than half a file. And remove only what a *previous
//! record* says this task wrote, never "everything under the output", so a directory
//! output stops accumulating files whose sources were deleted, without `pcmp` ever
//! deleting something it did not create.

use std::collections::BTreeMap;

use crate::report::Diagnostic;
use crate::vfs::{self, AbsPath, Digest, RelPath, digest};

/// `luau` and `lua`, or the single extension `darklua.lua_extension` names.
///
/// A header is a Lua comment, so it goes on Lua source and nowhere else. A `copy` loader
/// can put a `.png` in the output tree, and a `.png` with a Lua comment on the front is a
/// broken `.png`.
pub fn headable(lua_extension: Option<&str>) -> Vec<String> {
    match lua_extension {
        Some(extension) => vec![extension.to_owned()],
        None => vec!["luau".to_owned(), "lua".to_owned()],
    }
}

/// Prepends the header to every artifact it applies to, in memory.
///
/// In memory is the point. Composing after the write would mean re-reading the output
/// directory and prepending to whatever is in it, which gives a file that survives from an
/// earlier build one banner per build.
pub fn compose(
    artifacts: &mut BTreeMap<RelPath, Vec<u8>>,
    header: &[String],
    extensions: &[String],
) {
    if header.is_empty() {
        return;
    }

    let mut banner = header.join("\n");
    banner.push('\n');

    for (path, bytes) in artifacts.iter_mut() {
        let applies = path
            .extension()
            .is_some_and(|extension| extensions.iter().any(|allowed| allowed == extension));

        if applies {
            let mut composed = banner.clone().into_bytes();
            composed.append(bytes);
            *bytes = composed;
        }
    }
}

pub fn fingerprint(artifacts: &BTreeMap<RelPath, Vec<u8>>) -> Digest {
    digest::of_files("artifacts", artifacts)
}

/// What is on disk now, for the digest that notices a hand-edited artifact.
pub fn current(root: &AbsPath, outputs: &[RelPath]) -> BTreeMap<RelPath, Vec<u8>> {
    outputs
        .iter()
        .filter_map(|path| {
            let absolute = root.join(path.as_str()).ok()?;
            let bytes = vfs::read(&absolute).ok()?;
            Some((path.clone(), bytes))
        })
        .collect()
}

/// Writes what changed and removes what this task no longer produces.
pub fn write(
    root: &AbsPath,
    artifacts: &BTreeMap<RelPath, Vec<u8>>,
    previous: &[RelPath],
) -> Result<(), Diagnostic> {
    for (path, bytes) in artifacts {
        let absolute = root.join(path.as_str())?;

        // Reading to compare costs one read of a file already in the page cache. Writing
        // when nothing changed costs every watcher downstream a rebuild.
        if vfs::read(&absolute).is_ok_and(|existing| existing == *bytes) {
            continue;
        }

        vfs::write(&absolute, bytes)?;
    }

    for stale in previous {
        if artifacts.contains_key(stale) {
            continue;
        }
        vfs::remove_file(&root.join(stale.as_str())?)?;
    }

    Ok(())
}

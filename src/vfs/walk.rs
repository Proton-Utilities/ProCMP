//! Directory traversal.
//!
//! Yields what is there. Deciding what any of it means belongs to the caller. This is
//! the traversal behind the shape digest, which is why an entry carries its kind and a
//! symlink carries its target: a retarget changes the build without changing any file.

use std::collections::BTreeMap;

use super::path::{AbsPath, RelPath};
use crate::report::Diagnostic;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    File,
    Dir,
    Symlink(String),
}

impl Kind {
    /// What the shape digest folds in. A symlink's target is part of its identity.
    pub fn tag(&self) -> String {
        match self {
            Self::File => "file".to_owned(),
            Self::Dir => "dir".to_owned(),
            Self::Symlink(target) => format!("symlink:{target}"),
        }
    }
}

/// Walks `root`, keeping every entry `keep` accepts and descending only into accepted
/// directories.
///
/// A root that does not exist contributes nothing rather than failing. A `sources`
/// directory can be created after the manifest that names it, and the shape digest notices
/// the moment it appears.
pub fn walk(
    root: &AbsPath,
    keep: &dyn Fn(&RelPath, &Kind) -> bool,
) -> Result<BTreeMap<RelPath, Kind>, Diagnostic> {
    let mut found = BTreeMap::new();
    let mut pending = vec![(root.clone(), None::<RelPath>)];

    while let Some((directory, prefix)) = pending.pop() {
        for entry in super::entries(&directory)? {
            let relative = match &prefix {
                Some(prefix) => prefix.join(&entry.name),
                None => RelPath::new(&entry.name)?,
            };

            if !keep(&relative, &entry.kind) {
                continue;
            }

            if entry.kind == Kind::Dir {
                pending.push((directory.join(&entry.name)?, Some(relative.clone())));
            }

            found.insert(relative, entry.kind);
        }
    }

    Ok(found)
}

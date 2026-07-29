//! Every filesystem effect in the crate.
//!
//! Reading, writing, listing, renaming and removing happen here and nowhere else, so
//! there is one place that maps an `io::Error` onto a diagnostic and one place that
//! knows a write is a rename. [`path`] is the namespace, [`digest`] is content identity,
//! [`walk`] is traversal. Together they are what a file *is* to this crate.

pub mod digest;
pub mod path;
pub mod walk;

pub use digest::{Digest, Hasher};
pub use path::{AbsPath, RelPath};
pub use walk::Kind;

use std::sync::atomic::{AtomicU64, Ordering};

use crate::report::{Code, Diagnostic};

/// Prefix of the temporary file an atomic write goes through.
///
/// It has to sit beside its target, because a rename is only atomic within one
/// filesystem, so it is briefly visible inside the project and must be recognisable
/// everywhere that looks at a directory: the shape walk would count it as a file
/// appearing, and `watch` would rebuild because of it.
pub const TEMPORARY: &str = ".pcmp-write-";

/// One directory entry, named relative to the directory that held it.
#[derive(Debug)]
pub struct Entry {
    pub name: String,
    pub kind: Kind,
}

pub fn read(path: &AbsPath) -> Result<Vec<u8>, Diagnostic> {
    std::fs::read(path.as_std()).map_err(|error| unreadable(path, error))
}

pub fn read_to_string(path: &AbsPath) -> Result<String, Diagnostic> {
    std::fs::read_to_string(path.as_std()).map_err(|error| unreadable(path, error))
}

pub fn exists(path: &AbsPath) -> bool {
    path.as_std().exists()
}

pub fn is_file(path: &AbsPath) -> bool {
    path.as_std().is_file()
}

/// Writes through a temporary file in the same directory, then renames.
///
/// Same directory because a rename is only atomic within one filesystem. A failure
/// therefore leaves whatever was there before intact rather than truncated.
pub fn write(path: &AbsPath, bytes: &[u8]) -> Result<(), Diagnostic> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let Some(directory) = path.parent() else {
        return Err(Diagnostic::new(
            Code::WriteFailed,
            format!("`{path}` has no parent directory"),
        ));
    };

    create_dir_all(&directory)?;

    let temporary = directory.join(format!(
        "{TEMPORARY}{}-{}.tmp",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ))?;

    std::fs::write(temporary.as_std(), bytes).map_err(|error| unwritable(&temporary, error))?;

    std::fs::rename(temporary.as_std(), path.as_std()).map_err(|error| {
        // The temporary is dead either way, and a failure to clear it must not mask the
        // rename's own error.
        drop(std::fs::remove_file(temporary.as_std()));
        unwritable(path, error)
    })
}

pub fn remove_file(path: &AbsPath) -> Result<(), Diagnostic> {
    match std::fs::remove_file(path.as_std()) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(unwritable(path, error)),
    }
}

fn create_dir_all(path: &AbsPath) -> Result<(), Diagnostic> {
    std::fs::create_dir_all(path.as_std()).map_err(|error| unwritable(path, error))
}

/// A directory that does not exist lists as empty. A name that is not UTF-8 is skipped:
/// it cannot be named by a manifest, so it cannot be an input.
pub fn entries(directory: &AbsPath) -> Result<Vec<Entry>, Diagnostic> {
    let read = match std::fs::read_dir(directory.as_std()) {
        Ok(read) => read,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(unreadable(directory, error)),
    };

    let mut found = Vec::new();

    for entry in read {
        let entry = entry.map_err(|error| unreadable(directory, error))?;
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if name.starts_with(TEMPORARY) {
            continue;
        }

        let kind = entry
            .file_type()
            .map_err(|error| unreadable(directory, error))?;

        found.push(Entry {
            kind: if kind.is_dir() {
                Kind::Dir
            } else if kind.is_symlink() {
                Kind::Symlink(link_target(directory, &name))
            } else if kind.is_file() {
                Kind::File
            } else {
                continue;
            },
            name,
        });
    }

    found.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(found)
}

/// An unreadable link is still a link, and its target reads as empty, which differs from
/// every real target and so still moves the shape digest when it changes.
fn link_target(directory: &AbsPath, name: &str) -> String {
    directory
        .join(name)
        .ok()
        .and_then(|path| std::fs::read_link(path.as_std()).ok())
        .map(|target| target.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default()
}

fn unreadable(path: &AbsPath, error: std::io::Error) -> Diagnostic {
    Diagnostic::new(Code::Unreadable, format!("could not read `{path}`")).caused_by(error)
}

fn unwritable(path: &AbsPath, error: std::io::Error) -> Diagnostic {
    Diagnostic::new(Code::WriteFailed, format!("could not write `{path}`")).caused_by(error)
}

//! Absolute and root-relative paths, normalised lexically.
//!
//! Lexical means no symlink resolution and no existence check, so nothing here touches
//! the filesystem. [`RelPath`] is always `/`-separated whatever the platform, because a
//! digest computed on Windows has to equal one computed on Linux.

use std::fmt;

use camino::{Utf8Component, Utf8Path, Utf8PathBuf};

use crate::report::{Code, Diagnostic};

/// An absolute, normalised, UTF-8 path. Absoluteness is checked once, at construction.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct AbsPath(Utf8PathBuf);

/// A path relative to the manifest's directory: normalised, `/`-separated, never empty.
///
/// Leading `..` survives, because `sources: ["../shared"]` names a real place and is
/// still the same string on every machine. What cannot survive is an absolute path:
/// those exist only inside [`crate::vfs`], which is what stops a checkout's location
/// from reaching a cache key.
#[derive(
    Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct RelPath(String);

impl AbsPath {
    pub fn new(path: impl AsRef<Utf8Path>) -> Result<Self, Diagnostic> {
        let path = path.as_ref();

        if path.as_str().is_empty() {
            return Err(empty());
        }
        if !path.is_absolute() {
            return Err(Diagnostic::new(
                Code::BadPath,
                format!("path `{path}` is not absolute"),
            ));
        }

        Ok(Self(normalise(path)?))
    }

    /// The only place the crate reads the process working directory.
    pub fn cwd() -> Result<Self, Diagnostic> {
        let cwd = std::env::current_dir().map_err(|error| {
            Diagnostic::new(Code::BadPath, "the working directory is not usable").caused_by(error)
        })?;

        Utf8PathBuf::from_path_buf(cwd)
            .map_err(|path| {
                Diagnostic::new(
                    Code::BadPath,
                    format!("the working directory `{}` is not UTF-8", path.display()),
                )
            })
            .and_then(Self::new)
    }

    /// Resolves against this directory. An absolute argument replaces it outright.
    pub fn join(&self, path: impl AsRef<Utf8Path>) -> Result<Self, Diagnostic> {
        let path = path.as_ref();

        if path.as_str().is_empty() {
            return Err(empty());
        }

        let joined = if path.is_absolute() {
            path.to_owned()
        } else {
            self.0.join(path)
        };

        Ok(Self(normalise(&joined)?))
    }

    pub fn parent(&self) -> Option<Self> {
        self.0.parent().map(|parent| Self(parent.to_owned()))
    }

    pub fn file_name(&self) -> Option<&str> {
        self.0.file_name()
    }

    pub fn extension(&self) -> Option<&str> {
        self.0.extension()
    }

    /// Names `self` from `base`, climbing with `..` when it has to.
    ///
    /// A `sources` root may sit *beside* the manifest rather than below it, and anything
    /// under it still needs a name relative to the manifest, because otherwise it has no name at
    /// all, and a file with no name is a file no digest covers.
    ///
    /// [`None`] when the two share no root, or when they are the same path.
    pub fn relative_to(&self, base: &Self) -> Option<RelPath> {
        let mut ours = self.0.components().peekable();
        let mut theirs = base.0.components().peekable();
        let mut shared = 0usize;

        while let (Some(here), Some(there)) = (ours.peek(), theirs.peek()) {
            if here != there {
                break;
            }
            ours.next();
            theirs.next();
            shared += 1;
        }

        // Nothing in common means different drives, or one of them is not absolute.
        if shared == 0 {
            return None;
        }

        let mut segments: Vec<&str> = vec![".."; theirs.count()];
        segments.extend(ours.map(|component| component.as_str()));

        RelPath::new(segments.join("/")).ok()
    }

    /// Component-wise, so `/a/bc` is not under `/a/b`.
    pub fn is_under(&self, base: &Self) -> bool {
        self.0.starts_with(&base.0)
    }

    pub fn as_std(&self) -> &std::path::Path {
        self.0.as_std_path()
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl RelPath {
    pub fn new(path: impl AsRef<str>) -> Result<Self, Diagnostic> {
        let path = path.as_ref();

        if path.is_empty() {
            return Err(empty());
        }
        if Utf8Path::new(path).is_absolute() {
            return Err(Diagnostic::new(
                Code::BadPath,
                format!("path `{path}` is absolute, but a relative one is required"),
            ));
        }

        // `a/../b` collapses to `b`, and a leading `..` is kept, because it names a
        // directory beside the root rather than an error.
        let mut segments: Vec<&str> = Vec::new();
        for segment in path.split(['/', '\\']) {
            match segment {
                "" | "." => {}
                ".." if matches!(segments.last(), Some(&last) if last != "..") => {
                    segments.pop();
                }
                segment => segments.push(segment),
            }
        }

        if segments.is_empty() {
            return Err(Diagnostic::new(
                Code::BadPath,
                format!("path `{path}` names no file"),
            ));
        }

        Ok(Self(segments.join("/")))
    }

    /// Extends this path, keeping the `/` separator.
    #[must_use]
    pub fn join(&self, segment: &str) -> Self {
        Self(format!("{}/{segment}", self.0))
    }

    pub fn extension(&self) -> Option<&str> {
        Utf8Path::new(&self.0).extension()
    }

    /// Whether any segment equals `name`, which is how `.git` is excluded.
    pub fn has_segment(&self, name: &str) -> bool {
        self.0.split('/').any(|segment| segment == name)
    }

    pub fn starts_with(&self, prefix: &Self) -> bool {
        self.0
            .strip_prefix(&prefix.0)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn empty() -> Diagnostic {
    Diagnostic::new(Code::BadPath, "a path is empty").help("remove the field, or give it a value")
}

/// Clamping `..` silently would let `../../..` escape the project, so it is an error.
fn normalise(path: &Utf8Path) -> Result<Utf8PathBuf, Diagnostic> {
    let mut out = Utf8PathBuf::new();

    for component in path.components() {
        match component {
            Utf8Component::Prefix(prefix) => out.push(prefix.as_str()),
            Utf8Component::RootDir => out.push("/"),
            Utf8Component::CurDir => {}
            Utf8Component::ParentDir => {
                if !out.pop() {
                    return Err(Diagnostic::new(
                        Code::BadPath,
                        format!("path `{path}` escapes the filesystem root"),
                    )
                    .help("too many `..` segments"));
                }
            }
            Utf8Component::Normal(segment) => out.push(segment),
        }
    }

    Ok(out)
}

impl fmt::Display for AbsPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl fmt::Display for RelPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

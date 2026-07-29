//! What counts as a build input, and the two digests that decide whether it changed.
//!
//! Two tiers, because one is unsound. A build can depend on a file's *absence*: if
//! `require("./mod")` resolves to `mod.luau` and someone adds `mod/init.luau`, resolution
//! changes although no recorded file did. darklua does not report the paths it tried, so
//! **shape** covers every path under the roots without reading any of them, and **reads**
//! covers the contents of the files darklua actually opened.
//!
//! Shape reads no file contents, only names. That is what keeps a no-op rebuild flat in
//! the size of the repository rather than linear in it: a directory holding 460 MB costs
//! one `readdir` per directory it contains, not 460 MB of hashing.

use std::collections::{BTreeMap, BTreeSet};

use wax::{Glob, Program};

use crate::plan::{Plan, Task};
use crate::report::{Code, Diagnostic};
use crate::vfs::{self, AbsPath, Digest, Hasher, Kind, RelPath, digest, walk};

/// Version-control metadata, never an input.
const VCS: &str = ".git";

/// The roots one task draws from, with what it excludes.
///
/// Per task, because a per-profile `ignore` applied to every profile is how the old
/// design shipped stale artifacts. Shape is memoised on this, so a matrix whose tasks
/// share roots and exclusions walks once rather than once per combination.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Scope {
    roots: Vec<RelPath>,
    ignore: Vec<String>,
    excluded: Vec<RelPath>,
}

impl Scope {
    /// `plan` is the whole plan rather than the selection: every task's output is
    /// excluded from every scope, so building one task cannot move another's shape.
    pub fn of(task: &Task, plan: &Plan, cache: Option<&RelPath>) -> Self {
        let mut excluded: BTreeSet<RelPath> = plan
            .tasks
            .iter()
            .map(|other| other.output.clone())
            .collect();
        excluded.extend(cache.cloned());

        let mut roots = task.sources.clone();
        roots.sort();
        roots.dedup();

        Self {
            roots,
            ignore: task.ignore.clone(),
            excluded: excluded.into_iter().collect(),
        }
    }

    /// The manifest's directory is always a root, and `sources` adds to it.
    ///
    /// Each root carries the prefix that names it relative to the manifest, so a file
    /// under `../shared` gets a name that is still relative to the manifest and still the
    /// same on every machine. Going through an absolute path and back would lose that,
    /// because a root beside the manifest is not below it.
    fn directories(&self, root: &AbsPath) -> Vec<(Option<&RelPath>, AbsPath)> {
        let mut directories = vec![(None, root.clone())];

        for source in &self.roots {
            if let Ok(absolute) = root.join(source.as_str()) {
                directories.push((Some(source), absolute));
            }
        }

        directories
    }

    fn globs(&self) -> Result<Vec<Glob<'static>>, Diagnostic> {
        self.ignore
            .iter()
            .map(|pattern| {
                Glob::new(pattern).map(Glob::into_owned).map_err(|error| {
                    Diagnostic::new(Code::BadGlob, format!("`{pattern}` is not a valid glob"))
                        .help(error.to_string())
                })
            })
            .collect()
    }
}

/// Answers "did any file appear, vanish or move?" without opening one.
///
/// Files and symlinks only. A directory that holds nothing cannot change a build, and
/// counting them would make every output directory a change the moment it is first
/// created, so the first build after a clean checkout would never be cacheable. The
/// negative-dependency case that shape exists for is a *file* appearing: adding
/// `mod/init.luau` beside `mod.luau` moves the digest because the file does.
///
/// A symlink's target is part of its entry, so retargeting one, which changes a build
/// while changing no file, still moves the digest.
pub fn shape(scope: &Scope, root: &AbsPath) -> Result<Digest, Diagnostic> {
    let globs = scope.globs()?;
    let mut entries: BTreeMap<String, String> = BTreeMap::new();

    for (prefix, directory) in scope.directories(root) {
        let found = walk::walk(&directory, &|path, _| keep(scope, &globs, path))?;
        for (path, kind) in found {
            if kind != Kind::Dir {
                entries.insert(named(prefix, &path).to_string(), kind.tag());
            }
        }
    }

    let mut hasher = Hasher::new();
    hasher.seq(
        "shape",
        entries.iter().map(|(path, kind)| format!("{path}\0{kind}")),
    );
    Ok(hasher.finish())
}

/// Answers "did a file we read change?".
///
/// A cold build has no recorded read set and falls back to every file in scope. That is
/// the one build that reads everything, and the record it writes means the next one does
/// not.
pub fn reads(
    scope: &Scope,
    root: &AbsPath,
    recorded: Option<&[RelPath]>,
) -> Result<(Digest, BTreeMap<RelPath, Vec<u8>>), Diagnostic> {
    let paths = match recorded {
        Some(recorded) => recorded.to_vec(),
        None => everything(scope, root)?,
    };

    let mut contents = BTreeMap::new();
    for path in paths {
        let Ok(absolute) = root.join(path.as_str()) else {
            continue;
        };
        // A recorded input that has since been deleted is a change, not a failure: the
        // shape digest will have moved too, and the rebuild will find out why.
        if let Ok(bytes) = vfs::read(&absolute) {
            contents.insert(path, bytes);
        }
    }

    Ok((fingerprint(&contents), contents))
}

/// The reads digest, over whatever set of files is handed to it.
///
/// A cold build has to over-approximate, so it hashes everything in scope, so the record it
/// then writes must hold the digest of the set darklua *actually* read, or the next build
/// would compare a digest over one set against a digest over another and rebuild forever.
pub fn fingerprint(contents: &BTreeMap<RelPath, Vec<u8>>) -> Digest {
    digest::of_files("reads", contents)
}

/// Every file in scope, which is what a cold build has to stage.
pub fn everything(scope: &Scope, root: &AbsPath) -> Result<Vec<RelPath>, Diagnostic> {
    let globs = scope.globs()?;
    let mut found = BTreeSet::new();

    for (prefix, directory) in scope.directories(root) {
        for (path, kind) in walk::walk(&directory, &|path, _| keep(scope, &globs, path))? {
            if kind == Kind::File {
                found.insert(named(prefix, &path));
            }
        }
    }

    Ok(found.into_iter().collect())
}

/// One name per file, relative to the manifest, however many roots overlap.
fn named(prefix: Option<&RelPath>, path: &RelPath) -> RelPath {
    match prefix {
        Some(prefix) => prefix.join(path.as_str()),
        None => path.clone(),
    }
}

fn keep(scope: &Scope, globs: &[Glob<'static>], path: &RelPath) -> bool {
    if path.has_segment(VCS) {
        return false;
    }
    if scope
        .excluded
        .iter()
        .any(|excluded| path.starts_with(excluded))
    {
        return false;
    }

    !globs.iter().any(|glob| glob.is_match(path.as_str()))
}

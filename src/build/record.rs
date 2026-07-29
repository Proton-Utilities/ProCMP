//! What a build wrote down, in two places for two reasons.
//!
//! `.pcmp/` is a local cache: disposable, gitignored, one record per task. `pcmp.lock` is
//! provenance: committed, and what `--frozen` reproduces. Keeping them apart is what lets
//! the lock stay still while the cache churns. A manifest that calls `pcmp.now()` would
//! otherwise rewrite the lock on every invocation.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::manifest::ledger::Ledger;
use crate::report::Diagnostic;
use crate::vfs::{self, AbsPath, Digest, RelPath, digest};

/// Bumped when the shape of a record changes.
///
/// A record that does not match is ignored rather than migrated. Ignoring one costs a
/// rebuild. Migrating one wrongly costs a wrong artifact that nothing will notice.
const VERSION: u32 = 1;

/// One task's last successful build.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    pub version: u32,
    pub plan: Digest,
    pub shape: Digest,
    pub reads: Digest,
    pub artifacts: Digest,
    /// The linked darklua. A patch release can change emitted bytes.
    pub darklua: String,
    /// What darklua actually opened, so the next build hashes only these.
    pub read_set: Vec<RelPath>,
    /// What this task wrote, so a later build can remove what it no longer writes,
    /// and only that. `pcmp` never deletes a file it did not create.
    pub outputs: Vec<RelPath>,
}

/// Which digest moved, for `plan --why`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Reason {
    NoRecord,
    Version,
    Darklua,
    Plan,
    Shape,
    Reads,
    Artifacts,
}

impl Reason {
    pub const fn describe(self) -> &'static str {
        match self {
            Self::NoRecord => "no previous build",
            Self::Version => "the record predates this version of pcmp",
            Self::Darklua => "a different darklua",
            Self::Plan => "the task's configuration changed",
            Self::Shape => "a file appeared, vanished or moved",
            Self::Reads => "a source file changed",
            Self::Artifacts => "the artifacts on disk are not the ones pcmp wrote",
        }
    }
}

impl Record {
    /// [`None`] when nothing needs doing. Everything must match: the artifacts digest is
    /// what notices a hand-edited output, which an inputs-only stamp never could.
    pub fn stale(
        current: Option<&Self>,
        plan: Digest,
        shape: Digest,
        reads: Digest,
        artifacts: Digest,
    ) -> Option<Reason> {
        let Some(record) = current else {
            return Some(Reason::NoRecord);
        };

        if record.version != VERSION {
            return Some(Reason::Version);
        }
        if record.darklua != crate::DARKLUA {
            return Some(Reason::Darklua);
        }
        if record.plan != plan {
            return Some(Reason::Plan);
        }
        if record.shape != shape {
            return Some(Reason::Shape);
        }
        if record.reads != reads {
            return Some(Reason::Reads);
        }
        if record.artifacts != artifacts {
            return Some(Reason::Artifacts);
        }

        None
    }

    pub fn new(
        plan: Digest,
        shape: Digest,
        reads: Digest,
        artifacts: Digest,
        read_set: Vec<RelPath>,
        outputs: Vec<RelPath>,
    ) -> Self {
        Self {
            version: VERSION,
            plan,
            shape,
            reads,
            artifacts,
            darklua: crate::DARKLUA.to_owned(),
            read_set,
            outputs,
        }
    }

    /// Named by digest: a task identifier holds `[`, `]`, `=` and `,`.
    fn path(cache: &AbsPath, task: &str) -> Result<AbsPath, Diagnostic> {
        cache.join(format!("{}.json", digest::of(task)))
    }

    /// A missing or unreadable record reads as "never built", which is always safe.
    pub fn load(cache: &AbsPath, task: &str) -> Option<Self> {
        let path = Self::path(cache, task).ok()?;
        let bytes = vfs::read(&path).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    pub fn save(&self, cache: &AbsPath, task: &str) -> Result<(), Diagnostic> {
        let path = Self::path(cache, task)?;
        let body = serde_json::to_vec_pretty(self).map_err(|error| {
            Diagnostic::new(
                crate::report::Code::WriteFailed,
                format!("could not encode the record for `{task}`"),
            )
            .caused_by(error)
        })?;

        vfs::write(&path, &body)
    }
}

/// The committed side: what a release was built from, and what it produced.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lock {
    pub version: u32,
    /// Every value the manifest took from outside itself, including the clock.
    pub ledger: Ledger,
    pub tasks: BTreeMap<String, Locked>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Locked {
    pub plan: Digest,
    pub artifacts: Digest,
}

impl Lock {
    pub const NAME: &'static str = "pcmp.lock";

    pub fn new(ledger: Ledger, tasks: BTreeMap<String, Locked>) -> Self {
        Self {
            version: VERSION,
            ledger,
            tasks,
        }
    }

    pub fn path(root: &AbsPath) -> Result<AbsPath, Diagnostic> {
        root.join(Self::NAME)
    }

    pub fn load(root: &AbsPath) -> Option<Self> {
        let path = Self::path(root).ok()?;
        let bytes = vfs::read(&path).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// Written only on `--lock`, and formatted so a diff shows what changed about a build.
    pub fn save(&self, root: &AbsPath) -> Result<(), Diagnostic> {
        let path = Self::path(root)?;
        let mut body = serde_json::to_vec_pretty(self).map_err(|error| {
            Diagnostic::new(
                crate::report::Code::WriteFailed,
                "could not encode pcmp.lock",
            )
            .caused_by(error)
        })?;
        body.push(b'\n');

        vfs::write(&path, &body)
    }
}

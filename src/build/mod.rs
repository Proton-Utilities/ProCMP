//! Execution: a [`Plan`] in, artifacts out.
//!
//! A task is skipped only when all four digests match its record: configuration, shape,
//! reads and artifacts. The fourth is the one that notices an artifact edited by hand,
//! which an inputs-only stamp never could.

pub mod commit;
pub mod inputs;
pub mod record;
pub mod stage;
pub mod watch;

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Instant;

use darklua_core::Options;

use crate::plan::{Plan, Task, TaskId};
use crate::report::{Code, Diagnostic};
use crate::vfs::{self, AbsPath, Digest, RelPath};

use inputs::Scope;
use record::{Reason, Record};

/// Where build state lives when `--cache-dir` is not given.
pub const CACHE_DIR: &str = ".pcmp";

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Built,
    Cached,
    Failed,
}

#[derive(Debug, serde::Serialize)]
pub struct TaskReport {
    pub task: TaskId,
    pub output: RelPath,
    pub status: Status,
    /// Absent when the task failed, which is the only time there is nothing to name.
    pub artifacts: Option<Digest>,
    /// Why it rebuilt, which `--why` prints and a cached task does not have.
    pub why: Option<Reason>,
    /// Printed only under `--timings`, in either output mode: a report has to be
    /// byte-identical for the same build or it cannot be diffed.
    #[serde(skip)]
    pub millis: u128,
    /// Why it failed, in the same shape every other diagnostic uses.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, serde::Serialize)]
pub struct Report {
    pub tasks: Vec<TaskReport>,
}

impl Report {
    pub fn succeeded(&self) -> bool {
        !self.tasks.iter().any(|task| task.status == Status::Failed)
    }

    /// Built, cached, failed.
    pub fn counts(&self) -> (usize, usize, usize) {
        self.tasks
            .iter()
            .fold((0, 0, 0), |(built, cached, failed), task| {
                match task.status {
                    Status::Built => (built + 1, cached, failed),
                    Status::Cached => (built, cached + 1, failed),
                    Status::Failed => (built, cached, failed + 1),
                }
            })
    }
}

#[derive(Debug)]
pub struct Engine {
    root: AbsPath,
    cache: AbsPath,
    /// `--frozen` and `--no-cache` both turn this off.
    cached: bool,
}

/// Whether a run acts on what it decides, or only reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Work {
    Do,
    Skip,
}

impl Engine {
    pub fn new(root: AbsPath, cache: AbsPath, cached: bool) -> Self {
        Self {
            root,
            cache,
            cached,
        }
    }

    /// Reports what each task would do without doing any of it, for `plan --why`.
    ///
    /// Every digest is still computed, and only the work is skipped.
    pub fn inspect(&self, plan: &Plan, selection: &Plan) -> Report {
        self.execute(plan, selection, Work::Skip)
    }

    /// Failures land in the report rather than returning early, so one run names every
    /// task that went wrong.
    pub fn run(&self, plan: &Plan, selection: &Plan) -> Report {
        self.execute(plan, selection, Work::Do)
    }

    fn execute(&self, plan: &Plan, selection: &Plan, work: Work) -> Report {
        // `None` when the cache lives outside the manifest, in which case it is already
        // outside every root and needs no excluding.
        let cache = self.cache.relative_to(&self.root);

        // Shape is a pure function of a scope. Combinations of one profile share a scope
        // unless an axis overlay changes `sources` or `ignore`, so this is computed once
        // per distinct scope rather than once per task.
        let shapes: Mutex<BTreeMap<Scope, Digest>> = Mutex::new(BTreeMap::new());

        let mut tasks: Vec<TaskReport> = std::thread::scope(|threads| {
            let handles: Vec<_> = selection
                .tasks
                .iter()
                .map(|task| {
                    let shapes = &shapes;
                    let cache = cache.as_ref();
                    threads.spawn(move || self.one(task, plan, cache, shapes, work))
                })
                .collect();

            handles
                .into_iter()
                .filter_map(|handle| handle.join().ok())
                .collect()
        });

        tasks.sort_by(|a, b| a.task.cmp(&b.task));
        Report { tasks }
    }

    fn one(
        &self,
        task: &Task,
        plan: &Plan,
        cache: Option<&RelPath>,
        shapes: &Mutex<BTreeMap<Scope, Digest>>,
        work: Work,
    ) -> TaskReport {
        let started = Instant::now();
        let scope = Scope::of(task, plan, cache);

        let outcome = self.attempt(task, &scope, shapes, work);
        let (status, artifacts, why, diagnostics) = match outcome {
            Ok(Skipped { artifacts }) => (Status::Cached, Some(artifacts), None, Vec::new()),
            Ok(Rebuilt { artifacts, why }) => {
                (Status::Built, Some(artifacts), Some(why), Vec::new())
            }
            Err(diagnostic) => (Status::Failed, None, None, vec![diagnostic]),
        };

        TaskReport {
            task: task.id.clone(),
            output: task.output.clone(),
            status,
            artifacts,
            why,
            millis: started.elapsed().as_millis(),
            diagnostics,
        }
    }

    fn attempt(
        &self,
        task: &Task,
        scope: &Scope,
        shapes: &Mutex<BTreeMap<Scope, Digest>>,
        work: Work,
    ) -> Result<Outcome, Diagnostic> {
        let record = Record::load(&self.cache, task.id.as_str());
        let shape = self.shape(scope, shapes)?;
        let plan_digest = task.digest();

        let recorded_reads = record.as_ref().map(|record| record.read_set.as_slice());
        let (reads, _) = inputs::reads(scope, &self.root, recorded_reads)?;

        let previous: &[RelPath] = record.as_ref().map_or(&[], |record| &record.outputs);
        let on_disk = commit::fingerprint(&commit::current(&self.root, previous));

        let stale = Record::stale(record.as_ref(), plan_digest, shape, reads, on_disk);

        if self.cached && stale.is_none() {
            return Ok(Skipped { artifacts: on_disk });
        }

        let why = stale.unwrap_or(Reason::NoRecord);
        if work == Work::Skip {
            return Ok(Rebuilt {
                artifacts: on_disk,
                why,
            });
        }

        let artifacts = self.build(task, scope, plan_digest, shape, previous)?;
        Ok(Rebuilt { artifacts, why })
    }

    fn shape(
        &self,
        scope: &Scope,
        shapes: &Mutex<BTreeMap<Scope, Digest>>,
    ) -> Result<Digest, Diagnostic> {
        if let Ok(cached) = shapes.lock()
            && let Some(digest) = cached.get(scope)
        {
            return Ok(*digest);
        }

        let digest = inputs::shape(scope, &self.root)?;

        if let Ok(mut cached) = shapes.lock() {
            cached.insert(scope.clone(), digest);
        }

        Ok(digest)
    }

    fn build(
        &self,
        task: &Task,
        scope: &Scope,
        plan: Digest,
        shape: Digest,
        previous: &[RelPath],
    ) -> Result<Digest, Diagnostic> {
        let entry = self.root.join(task.entry.as_str())?;
        let output = self.root.join(task.output.as_str())?;

        if !vfs::exists(&entry) {
            return Err(Diagnostic::new(
                Code::MissingEntryFile,
                format!("`{}` does not exist", task.entry),
            )
            .help("the path resolves against the manifest's directory"));
        }

        // A rebuild stages everything in scope rather than the recorded read set: the set
        // may have grown, and a file that is not staged cannot be opened.
        let (_, contents) = inputs::reads(scope, &self.root, None)?;
        let staged: Vec<RelPath> = contents.keys().cloned().collect();
        let resources = stage::inputs(&self.root, &contents)?;

        let worked = darklua_core::process(
            &resources,
            Options::new(entry.as_std())
                .with_output(output.as_std())
                .with_configuration(task.config.build(task.id.as_str())?),
        )
        .map_err(|error| failed(task, &error.to_string()))?;

        // `process` reports per-file failures inside the returned tree rather than as an
        // `Err`, so a build that produced nothing would otherwise look successful.
        let errors = worked.collect_errors();
        if !errors.is_empty() {
            let detail = errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n");
            return Err(failed(task, &detail));
        }

        let mut artifacts = stage::outputs(&resources, &self.root, &output);
        if artifacts.is_empty() {
            return Err(Diagnostic::new(
                Code::NoOutput,
                format!("`{}` reported no failure but wrote nothing", task.id),
            )
            .help(
                "a file filter that matches nothing is the usual cause: `apply_to_files` and \
                 `skip_files` match each file's path relative to the entry",
            ));
        }

        commit::compose(
            &mut artifacts,
            &task.header,
            &commit::headable(lua_extension(task)),
        );

        let digest = commit::fingerprint(&artifacts);
        commit::write(&self.root, &artifacts, previous)?;

        let read_set = read_set(&worked, &self.root, &task.entry, &staged);
        let outputs: Vec<RelPath> = artifacts.keys().cloned().collect();

        // Over the set darklua read, not the set that was staged. Those differ on a cold
        // build, and recording the wrong one is a cache that never hits.
        let read: BTreeMap<RelPath, Vec<u8>> = contents
            .into_iter()
            .filter(|(path, _)| read_set.contains(path))
            .collect();

        Record::new(
            plan,
            shape,
            inputs::fingerprint(&read),
            digest,
            read_set,
            outputs,
        )
        .save(&self.cache, task.id.as_str())?;

        Ok(digest)
    }
}

enum Outcome {
    Skipped { artifacts: Digest },
    Rebuilt { artifacts: Digest, why: Reason },
}

use Outcome::{Rebuilt, Skipped};

/// What darklua opened: the modules it followed, plus the entry tree it walked.
fn read_set(
    worked: &darklua_core::WorkerTree,
    root: &AbsPath,
    entry: &RelPath,
    staged: &[RelPath],
) -> Vec<RelPath> {
    let mut found: Vec<RelPath> = worked
        .iter_external_dependencies()
        .filter_map(|path| path.to_str())
        .filter_map(|path| AbsPath::new(path).ok())
        .filter_map(|path| path.relative_to(root))
        .collect();

    // A directory entry is processed file by file rather than followed, so those are not
    // external dependencies, but they are certainly reads.
    found.extend(
        staged
            .iter()
            .filter(|path| path.starts_with(entry))
            .cloned(),
    );

    found.sort();
    found.dedup();
    found
}

/// darklua's own `lua_extension`, so the header applies to the extension darklua writes
/// rather than to a list this crate keeps of its own.
fn lua_extension(task: &Task) -> Option<&str> {
    task.config
        .rest
        .get("lua_extension")
        .and_then(serde_json::Value::as_str)
}

/// Split on darklua's own wording, which is the only signal it gives. A reword upstream
/// would silently reclassify every missing dependency as a generic failure, so this
/// string is checked against darklua when the `=` pin in `Cargo.toml` moves.
fn failed(task: &Task, detail: &str) -> Diagnostic {
    // An unstaged file is not a transformation failure. It is a dependency nobody
    // declared, and saying so is the whole point of staging.
    let code = if detail.contains("unable to find") {
        Code::UndeclaredInput
    } else {
        Code::ProcessFailed
    };

    let diagnostic = Diagnostic::new(code, format!("`{}` failed", task.id)).help(detail.to_owned());

    match code {
        Code::UndeclaredInput => diagnostic.help("add the directory that holds it to `sources`"),
        _ => diagnostic,
    }
}

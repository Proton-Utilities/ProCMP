//! Rebuilding on change.

use std::sync::mpsc;
use std::time::Duration;

use notify::{EventKind, RecursiveMode};
use notify_debouncer_full::new_debouncer;

use super::Engine;
use crate::cli::render;
use crate::plan::Plan;
use crate::report::{Code, Diagnostic, Exit};
use crate::vfs::AbsPath;

/// A save is a create, a write and a rename, or a truncate and a write. A formatter on
/// save doubles that. One debounce window turns the lot into one rebuild.
const SETTLE: Duration = Duration::from_millis(200);

/// Watches the manifest's directory and every extra root the selection declares.
///
/// The plan is resolved once, at startup. Re-reading it each cycle would mean a half-saved
/// manifest producing a run of parse errors, so a manifest edit is reported and otherwise
/// ignored until the process is restarted.
pub fn run(
    root: &AbsPath,
    cache: &AbsPath,
    manifest: &AbsPath,
    plan: &Plan,
    selection: &Plan,
    emit: bool,
    timings: bool,
) -> Result<Exit, Diagnostic> {
    let (sender, receiver) = mpsc::channel();
    let mut debouncer = new_debouncer(SETTLE, None, sender).map_err(|error| {
        Diagnostic::new(Code::Unreadable, "could not watch the filesystem").help(error.to_string())
    })?;

    let mut roots = vec![root.clone()];
    roots.extend(
        selection
            .tasks
            .iter()
            .flat_map(|task| &task.sources)
            .filter_map(|source| root.join(source.as_str()).ok()),
    );
    roots.sort();
    roots.dedup();

    for directory in &roots {
        debouncer
            .watch(directory.as_std(), RecursiveMode::Recursive)
            .map_err(|error| {
                Diagnostic::new(Code::Unreadable, format!("could not watch `{directory}`"))
                    .help(error.to_string())
            })?;
    }

    // What a build writes itself. Without this the first cycle's own records and artifacts
    // arrive as events and set off a cascade of cycles that find nothing to do.
    let mut ours = vec![cache.clone()];
    ours.extend(
        plan.tasks
            .iter()
            .filter_map(|task| root.join(task.output.as_str()).ok()),
    );

    let engine = Engine::new(root.clone(), cache.clone(), true);
    let cycle = || render::build(&engine.run(plan, selection), emit, timings, false);

    // Nothing else says the process is alive or which directories reach it, and `--json`
    // is a stream of build reports that a banner would make unparseable.
    if !emit {
        let listed: Vec<String> = roots.iter().map(AbsPath::to_string).collect();
        render::watching(&listed, plan);
    }

    // Watching is established first, so an edit made during the first build is queued
    // rather than lost.
    cycle();

    for batch in receiver {
        // One blank line, so consecutive rounds do not read as a single report.
        let separated = || {
            if !emit {
                render::line("");
            }
            cycle();
        };

        // A failed batch means lost events, and a rebuild is cheaper than a wrong answer.
        let Ok(events) = batch else {
            separated();
            continue;
        };

        let touched: Vec<&std::path::Path> = events
            .iter()
            .filter(|event| changed(event.kind))
            .flat_map(|event| &event.paths)
            .map(std::path::PathBuf::as_path)
            .collect();

        // Rebuilding with the plan from startup would silently ignore the edit, so the
        // one thing to do about a changed manifest is say so.
        if touched.iter().any(|path| *path == manifest.as_std()) {
            render::problem(format!(
                "note: `{manifest}` changed. This run is using the plan it had at startup, \
                 so restart `pcmp watch` to pick the edit up"
            ));
        }

        if touched.iter().any(|path| ours_not(path, &ours)) {
            separated();
        }
    }

    Ok(Exit::Success)
}

/// Whether a changed path is something the project owns rather than something a build
/// just produced.
fn ours_not(path: &std::path::Path, ours: &[AbsPath]) -> bool {
    let Some(path) = path.to_str().and_then(|text| AbsPath::new(text).ok()) else {
        return false;
    };
    if path.as_str().contains("/.git/")
        || path
            .file_name()
            .is_some_and(|name| name.starts_with(crate::vfs::TEMPORARY))
    {
        return false;
    }

    !ours.iter().any(|written| path.is_under(written))
}

/// Reads are excluded: fingerprinting opens every input on each build.
fn changed(kind: EventKind) -> bool {
    match kind {
        EventKind::Create(_) | EventKind::Remove(_) => true,
        EventKind::Modify(modify) => !matches!(modify, notify::event::ModifyKind::Metadata(_)),
        _ => false,
    }
}

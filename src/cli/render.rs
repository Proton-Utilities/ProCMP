//! Everything written to a stream.
//!
//! The only module that prints, which the crate's `print_stdout` and `print_stderr` lints
//! make a compile error to forget. `println!` is avoided too: it panics on a closed pipe,
//! which `pcmp plan | head` produces routinely.
//!
//! Three shapes, and every screen is built from them. A [`labels`] group for anything
//! keyed by a word, a [`table`] for anything with one row per task, and a summary line
//! counting what the screen just showed. A command decides what goes in them and never
//! how they are spaced.

#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "the one module that writes to a stream"
)]

use std::fmt::Write as _;
use std::io::Write as _;

use crate::build::{Report, Status};
use crate::plan::{Plan, Task};
use crate::report::{self, Diagnostic, Severity};
use crate::vfs::RelPath;

pub fn line(text: impl std::fmt::Display) {
    let _ = writeln!(std::io::stdout(), "{text}");
}

pub fn problem(text: impl std::fmt::Display) {
    let _ = writeln!(std::io::stderr(), "{text}");
}

fn json<T: serde::Serialize>(value: &T) {
    match serde_json::to_string_pretty(value) {
        Ok(rendered) => line(rendered),
        // On stderr, so stdout stays parseable.
        Err(error) => problem(format!("error: could not encode output: {error}")),
    }
}

/// `1 task`, `3 tasks`.
///
/// Every count in the CLI is a phrase a person could say aloud, so none of them is
/// written `task(s)`. A summary counting adjectives writes them inline instead, because
/// `3 builts` is not a phrase.
pub fn count(number: usize, noun: &str) -> String {
    if number == 1 {
        format!("{number} {noun}")
    } else {
        format!("{number} {noun}s")
    }
}

/// Measured in characters, not bytes.
fn pad(text: &str, to: usize) -> String {
    let width = text.chars().count();
    if width >= to {
        text.to_owned()
    } else {
        format!("{text}{}", " ".repeat(to - width))
    }
}

fn widest<'a>(items: impl Iterator<Item = &'a str>) -> usize {
    items.map(|text| text.chars().count()).max().unwrap_or(0)
}

/// A `label  value` block.
///
/// Labels are padded to the widest in this group and to nothing outside it, so a group
/// reads as a unit and two groups on one screen never have to agree on a width.
fn labels<'a>(rows: impl IntoIterator<Item = (&'a str, String)>) {
    let rows: Vec<_> = rows.into_iter().collect();
    let width = widest(rows.iter().map(|(label, _)| *label));

    for (label, value) in &rows {
        line(format!("{}  {value}", pad(label, width)));
    }
}

fn widths(rows: &[Vec<String>]) -> Vec<usize> {
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    (0..columns)
        .map(|column| {
            widest(
                rows.iter()
                    .filter_map(|row| row.get(column))
                    .map(String::as_str),
            )
        })
        .collect()
}

/// One table row: indented by two, columns separated by two.
///
/// The trailing trim matters. A task with nothing in its last column would otherwise end
/// in the padding of a column it did not fill, which every diff tool flags and nothing
/// needs.
fn row(cells: &[String], widths: &[usize]) -> String {
    let padded: Vec<String> = cells
        .iter()
        .enumerate()
        .map(|(column, cell)| pad(cell, widths.get(column).copied().unwrap_or(0)))
        .collect();

    format!("  {}", padded.join("  ")).trim_end().to_owned()
}

fn table(rows: &[Vec<String>]) {
    let widths = widths(rows);
    for cells in rows {
        line(row(cells, &widths));
    }
}

/// The identity of the resolved plan, which is what `--frozen` compares and what names a
/// set of artifacts. Printed once per invocation, ahead of any work.
pub fn heading(plan: &Plan, emit: bool) {
    if !emit {
        labels([("plan", plan.digest().short().to_string())]);
        line("");
    }
}

pub fn plan(plan: &Plan, emit: bool) {
    if emit {
        return json(plan);
    }

    heading(plan, false);
    table(
        &plan
            .tasks
            .iter()
            .map(|task| {
                vec![
                    task.id.to_string(),
                    task.output.to_string(),
                    task.config.rules.as_ref().map_or_else(
                        || "darklua defaults".to_owned(),
                        |rules| count(rules.len(), "rule"),
                    ),
                ]
            })
            .collect::<Vec<_>>(),
    );

    line(format!("\n{}", count(plan.len(), "task")));
}

/// One task in full, which is what `pcmp plan <TASK>` prints.
pub fn task(task: &Task, emit: bool) {
    if emit {
        return json(&serde_json::json!({
            "task": task,
            "digest": task.digest().to_string(),
            "darklua": task.config.json(),
        }));
    }

    labels([
        ("task", task.id.to_string()),
        ("entry", task.entry.to_string()),
        ("output", task.output.to_string()),
        ("digest", task.digest().short().to_string()),
    ]);

    line("");
    section(
        "vars",
        task.vars.iter().map(|(k, v)| (k.to_string(), v.text())),
    );
    line("");
    section(
        "defines",
        task.defines
            .iter()
            .map(|(k, v)| (k.to_string(), v.tagged())),
    );

    line("");
    line("darklua");
    for text in serde_json::to_string_pretty(&task.config.json())
        .unwrap_or_default()
        .lines()
    {
        line(format!("  {text}"));
    }
}

/// A heading and its rows, indented like every other table.
fn section(label: &str, pairs: impl Iterator<Item = (String, String)>) {
    line(label);
    table(
        &pairs
            .map(|(key, value)| vec![key, value])
            .collect::<Vec<_>>(),
    );
}

/// `plan --why` reports what a build would do and does none of it, so it must not say a
/// task was built.
///
/// Padded here rather than by the table, because this is the one column whose width must
/// not depend on the run. Measuring it would make a report of three cached tasks a
/// character wider than a report of three built ones, and two runs of the same project
/// would not line up.
const fn status(status: Status, why: bool) -> &'static str {
    match (status, why) {
        (Status::Built, false) => "built ",
        (Status::Built, true) => "stale ",
        (Status::Cached, false) => "cached",
        (Status::Cached, true) => "fresh ",
        (Status::Failed, _) => "FAILED",
    }
}

pub fn build(report: &Report, emit: bool, timings: bool, why: bool) {
    if emit {
        return json(report);
    }

    let rows: Vec<Vec<String>> = report
        .tasks
        .iter()
        .map(|task| {
            let note = match (why, timings) {
                (true, _) => task.why.map(|reason| reason.describe().to_owned()),
                (false, true) => Some(format!("{} ms", task.millis)),
                (false, false) => None,
            };

            vec![
                status(task.status, why).to_owned(),
                task.task.to_string(),
                task.output.to_string(),
                note.unwrap_or_default(),
            ]
        })
        .collect();

    // A diagnostic belongs under its own row, so the widths are measured once here and
    // the rows are written one at a time rather than by `table`.
    let widths = widths(&rows);
    let indent = " ".repeat(widths.first().copied().unwrap_or(0) + 4);

    for (cells, task) in rows.iter().zip(&report.tasks) {
        line(row(cells, &widths));

        for diagnostic in &task.diagnostics {
            for text in rendered(diagnostic).lines() {
                line(format!("{indent}{text}"));
            }
        }
    }

    let (built, cached, failed) = report.counts();
    line(if why {
        format!("\n{built} stale, {cached} fresh, {failed} failed")
    } else {
        format!("\n{built} built, {cached} cached, {failed} failed")
    });
}

/// Findings that are the command's answer, as `check`'s are.
pub fn diagnostics(diagnostics: &[Diagnostic], emit: bool) {
    report_to(diagnostics, emit, line);
}

/// Findings that mean the command failed.
///
/// On stderr, so that `pcmp build > artifacts.json` still says why it did not work. With
/// `--json` both go to stdout, because that is where machine output lives and the exit
/// code already says which happened.
pub fn failures(diagnostics: &[Diagnostic], emit: bool) {
    if emit {
        json(&diagnostics);
    } else {
        report_to(diagnostics, false, problem);
    }
}

fn report_to(diagnostics: &[Diagnostic], emit: bool, out: fn(String)) {
    if emit {
        return json(&diagnostics);
    }

    for diagnostic in diagnostics {
        out(rendered(diagnostic));
        out(String::new());
    }

    // Always, so a clean run and a dirty one end the same way and a script reading the
    // last line has one case rather than two.
    let (errors, warnings) = report::tally(diagnostics);
    out(format!(
        "{}, {}",
        count(errors, "error"),
        count(warnings, "warning")
    ));
}

/// One diagnostic, in the shape `error[missing-output]`, code first, so the reader knows
/// what to pass to `pcmp explain`. The continuation lines are a `labels` group in all but
/// name, which is why each carries a keyword and a colon.
pub fn rendered(diagnostic: &Diagnostic) -> String {
    let marker = match diagnostic.severity() {
        Severity::Error => "error",
        Severity::Warning => "warning",
    };

    let mut out = format!(
        "{marker}[{}]: {}",
        diagnostic.code.slug(),
        diagnostic.message
    );

    if let Some(at) = &diagnostic.at {
        let _ = write!(out, "\n  at:   {at}");
    }
    if let Some(source) = &diagnostic.source {
        let _ = write!(out, "\n  from: {source}");
    }
    for help in diagnostic.help.iter().flat_map(|help| help.lines()) {
        let _ = write!(out, "\n  help: {help}");
    }

    out
}

pub fn created(paths: &[RelPath]) {
    labels(
        paths
            .iter()
            .map(|path| ("created", path.to_string()))
            .chain(std::iter::once(("next", "pcmp plan".to_owned()))),
    );
}

/// `pcmp watch` opens with the roots it will react to and the plan it resolved, then
/// prints one build report per cycle.
pub fn watching(roots: &[String], plan: &Plan) {
    labels(
        roots
            .iter()
            .map(|root| ("watching", root.clone()))
            .chain(std::iter::once(("plan", plan.digest().short().to_string()))),
    );
    line("");
}

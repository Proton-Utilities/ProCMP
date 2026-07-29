//! Checks a type checker cannot express. All report, none repair.
//!
//! Two of these restate darklua's documented rule orderings. The rest catch a manifest
//! that is valid and does not do what it says: a define nothing reads substitutes nothing,
//! and a build that reads the clock without recording it cannot be reproduced.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

use super::darklua::INJECT;
use super::{Plan, Task};
use crate::manifest::ledger::Ledger;
use crate::manifest::{Loader, Manifest, schema};
use crate::report::{Code, Diagnostic, Location};
use crate::vfs::{self, AbsPath, Digest, RelPath, digest};

const FOLD: &str = "compute_expression";
const BRANCH: &str = "remove_unused_if_branch";

/// An ordering darklua's own documentation calls out.
struct Ordering {
    earlier: &'static str,
    later: &'static str,
    code: Code,
    why: &'static str,
}

const PIPELINE: &[Ordering] = &[
    Ordering {
        earlier: INJECT,
        later: FOLD,
        code: Code::FoldBeforeInject,
        why: "folding cannot see a value substituted after it, so the define does nothing",
    },
    Ordering {
        earlier: FOLD,
        later: BRANCH,
        code: Code::BranchBeforeFold,
        why: "a branch is only removable once its condition has folded to a constant",
    },
];

/// `sources` is every source byte in the project, when the caller has staged them.
/// Without it the define check is skipped rather than guessed at.
pub fn run(
    manifest: &Manifest,
    plan: &Plan,
    ledger: &Ledger,
    root: &AbsPath,
    sources: Option<&str>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for task in &plan.tasks {
        ordering(task, &mut diagnostics);
        escaping(task, &mut diagnostics);

        if let Some(sources) = sources {
            unread(task, sources, &mut diagnostics);
        }
    }

    templates(manifest, &mut diagnostics);
    shadowed(manifest, &mut diagnostics);
    duplicates(plan, &mut diagnostics);
    unrecorded(ledger, root, &mut diagnostics);
    stale_schema(root, &mut diagnostics);

    diagnostics
}

/// A rule listed twice is fine. The ordering holds only if every `earlier` precedes every
/// `later`, so the first `later` is compared against the last `earlier`.
fn ordering(task: &Task, diagnostics: &mut Vec<Diagnostic>) {
    let names = task.config.rule_names();

    for pair in PIPELINE {
        let (Some(later), Some(earlier)) = (
            names.iter().position(|name| *name == pair.later),
            names.iter().rposition(|name| *name == pair.earlier),
        ) else {
            continue;
        };

        if later < earlier {
            diagnostics.push(
                Diagnostic::new(
                    pair.code,
                    format!(
                        "`{}`: `{}` runs before `{}`",
                        task.id, pair.later, pair.earlier
                    ),
                )
                .at(Location::new("profiles", task.profile.as_str())
                    .field("darklua")
                    .field("rules"))
                .help(format!(
                    "list every `{}` ahead of `{}`",
                    pair.earlier, pair.later
                ))
                .help(pair.why),
            );
        }
    }
}

/// Legal, and sometimes intended. Reported because it puts `pcmp` outside the directory
/// the manifest describes, where a reader of that manifest would not look for it.
fn escaping(task: &Task, diagnostics: &mut Vec<Diagnostic>) {
    if task.output.as_str().starts_with("..") {
        diagnostics.push(
            Diagnostic::new(
                Code::OutputOutsideRoot,
                format!(
                    "`{}` writes `{}`, outside the project",
                    task.id, task.output
                ),
            )
            .at(Location::new("profiles", task.profile.as_str()).field("output")),
        );
    }
}

/// A define whose identifier appears in none of the sources, so it substitutes nothing.
///
/// A substring scan over bytes already in memory, which is why it can afford to be exact
/// rather than clever. `PCMP_` constants are exempt, because every var produces one whether
/// or not the source reads it.
fn unread(task: &Task, sources: &str, diagnostics: &mut Vec<Diagnostic>) {
    for identifier in task.defines.keys() {
        let name = identifier.as_str();

        if name.starts_with("PCMP_") || sources.contains(name) {
            continue;
        }

        diagnostics.push(
            Diagnostic::new(
                Code::UnreachableDefine,
                format!("`{name}` appears in no source `{}` reads", task.id),
            )
            .at(Location::new("profiles", task.profile.as_str())
                .field("define")
                .field(name))
            .help("nothing will be substituted, so check the spelling on both sides"),
        );
    }
}

/// Templates are not built, so one nothing extends does nothing at all.
fn templates(manifest: &Manifest, diagnostics: &mut Vec<Diagnostic>) {
    for name in manifest.templates.keys() {
        let extended = manifest
            .templates
            .values()
            .chain(manifest.profiles.values())
            .any(|profile| profile.extends.as_deref() == Some(name.as_str()));

        if !extended {
            diagnostics.push(
                Diagnostic::new(
                    Code::UnusedTemplate,
                    format!("template `{name}` is never extended"),
                )
                .at(Location::new("templates", name))
                .help("remove it, or move it to `profiles` so it builds"),
            );
        }
    }
}

/// `profile` is set by `pcmp` after everything else, so a declared one never wins.
fn shadowed(manifest: &Manifest, diagnostics: &mut Vec<Diagnostic>) {
    let declared = manifest
        .vars
        .keys()
        .map(|name| (Location::map("vars"), name))
        .chain(manifest.profiles.iter().flat_map(|(profile, body)| {
            body.vars
                .keys()
                .map(move |name| (Location::new("profiles", profile).field("vars"), name))
        }));

    for (at, name) in declared {
        if name == "profile" {
            diagnostics.push(
                Diagnostic::new(Code::ShadowedVar, "a var named `profile` is overwritten")
                    .at(at.field(name))
                    .help("`pcmp` sets `{profile}` itself, so rename this one"),
            );
        }
    }
}

/// Two profiles that resolve to the same work, however differently they were written.
///
/// What is compared is what the manifest said, normalised, not the assembled task. A task
/// carries `PCMP_PROFILE`, which differs by construction, so comparing tasks whole would
/// never find anything. Output is excluded too: differing only there is the normal case.
fn duplicates(plan: &Plan, diagnostics: &mut Vec<Diagnostic>) {
    let mut by_shape: BTreeMap<Digest, Vec<&str>> = BTreeMap::new();
    let mut seen: BTreeSet<&str> = BTreeSet::new();

    for task in &plan.tasks {
        // One task per profile. Two combinations of one axis differ only in ways this
        // shape deliberately ignores, and telling someone their matrix is repetitive is
        // telling them their matrix works.
        if !seen.insert(task.profile.as_str()) {
            continue;
        }

        by_shape
            .entry(super::canonical::canonical(&Shape::of(task)))
            .or_default()
            .push(task.profile.as_str());
    }

    for names in by_shape.into_values().filter(|names| names.len() > 1) {
        diagnostics.push(
            Diagnostic::new(
                Code::IdenticalProfiles,
                format!("{} resolve identically", names.join(", ")),
            )
            .help("extract what they share into a template, or give one an axis"),
        );
    }
}

/// What two profiles have to share to be the same profile.
#[derive(serde::Serialize)]
struct Shape<'t> {
    entry: &'t RelPath,
    sources: &'t [RelPath],
    ignore: &'t [String],
    header: &'t [String],
    loaders: &'t [Loader],
    rest: &'t Map<String, Value>,
    /// Without the constants `pcmp` generates from the profile's own name and vars, which
    /// are different on purpose. A define the manifest declared is kept, because two
    /// profiles that inject different values are not the same profile.
    rules: Vec<&'t Value>,
}

impl<'t> Shape<'t> {
    fn of(task: &'t Task) -> Self {
        Self {
            entry: &task.entry,
            sources: &task.sources,
            ignore: &task.ignore,
            header: &task.header,
            loaders: &task.config.loaders,
            rest: &task.config.rest,
            rules: task
                .config
                .rules
                .iter()
                .flatten()
                .filter(|rule| !generated(rule))
                .collect(),
        }
    }
}

/// An `inject_global_value` for a `PCMP_` constant, which every profile has and no two
/// profiles share.
fn generated(rule: &Value) -> bool {
    rule.get("rule").and_then(Value::as_str) == Some(INJECT)
        && rule
            .get("identifier")
            .and_then(Value::as_str)
            .is_some_and(|name| name.starts_with("PCMP_"))
}

/// The lint that teaches the model at the moment it matters.
fn unrecorded(ledger: &Ledger, root: &AbsPath, diagnostics: &mut Vec<Diagnostic>) {
    if !ledger.has_ambient() {
        return;
    }
    if crate::build::record::Lock::path(root).is_ok_and(|path| vfs::is_file(&path)) {
        return;
    }

    diagnostics.push(
        Diagnostic::new(
            Code::UnrecordedReading,
            "this manifest reads the clock or the environment, and nothing records it",
        )
        .help("run `pcmp build --lock`, and `pcmp build --frozen` then reproduces it exactly"),
    );
}

/// A committed schema describing a different version of the format is worse than none.
fn stale_schema(root: &AbsPath, diagnostics: &mut Vec<Diagnostic>) {
    let Ok(path) = root.join("pcmp.schema.json") else {
        return;
    };
    let Ok(committed) = vfs::read(&path) else {
        return;
    };

    if digest::of(&committed) != digest::of(schema::json()) {
        diagnostics.push(
            Diagnostic::new(
                Code::StaleSchema,
                "pcmp.schema.json does not match this version of pcmp",
            )
            .help("regenerate it with `pcmp schema`, or delete it, because it is not required"),
        );
    }
}

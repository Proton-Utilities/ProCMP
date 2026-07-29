//! Resolution: a [`Manifest`] in, a [`Plan`] out.
//!
//! Pure, with no filesystem, no clock and no network. Findings accumulate rather than
//! short-circuit, so one edit can fix every profile a run complains about.
//!
//! This is where a manifest's `String`s become validated types. Nothing downstream can
//! hold an identifier that is not one, because [`Task`] has nowhere to put it.

pub mod canonical;
pub mod darklua;
pub mod inherit;
pub mod lint;
pub mod template;

use std::collections::BTreeMap;
use std::fmt;

use crate::manifest::{Ident, Manifest, Name, Profile, Scalar};
use crate::report::{Code, Diagnostic, Location, Outcome};
use crate::vfs::{Digest, Hasher, RelPath};

use canonical::canonical;
use darklua::Config;

/// A profile name, or `dist[flavour=min,target=roblox]` for one axis combination.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
#[serde(transparent)]
pub struct TaskId(String);

/// One unit of work: no inherited fields, no unexpanded templates, no invalid values.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Task {
    pub id: TaskId,
    /// The `profiles` key this came from, which selection also accepts.
    pub profile: Name,
    pub axes: BTreeMap<Ident, String>,

    pub entry: RelPath,
    pub output: RelPath,
    /// Roots *beyond* the manifest's own directory, which is always one.
    pub sources: Vec<RelPath>,
    /// Validated as globs during resolution and compiled again in `build`, which is
    /// cheaper than making a `wax::Glob` travel through a serialisable plan.
    pub ignore: Vec<String>,

    pub vars: BTreeMap<Ident, Scalar>,
    pub defines: BTreeMap<Ident, Scalar>,
    pub header: Vec<String>,

    pub config: Config,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Plan {
    pub tasks: Vec<Task>,
}

/// `--var` and `--define`, applied after inheritance so they beat the manifest without it
/// having to anticipate them.
#[derive(Debug, Clone, Default)]
pub struct Overrides {
    pub vars: BTreeMap<String, Scalar>,
    pub defines: BTreeMap<String, Scalar>,
}

impl TaskId {
    fn new(profile: &Name, axes: &BTreeMap<Ident, String>) -> Self {
        if axes.is_empty() {
            return Self(profile.to_string());
        }

        let coordinates = axes
            .iter()
            .map(|(axis, value)| format!("{axis}={value}"))
            .collect::<Vec<_>>()
            .join(",");

        Self(format!("{profile}[{coordinates}]"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Task {
    /// Everything that decides the output except the source bytes, which `build` folds in
    /// separately.
    pub fn digest(&self) -> Digest {
        canonical(self)
    }
}

impl Plan {
    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    pub fn get(&self, id: &str) -> Option<&Task> {
        self.tasks.iter().find(|task| task.id.as_str() == id)
    }

    pub fn digest(&self) -> Digest {
        let mut hasher = Hasher::new();
        hasher.seq(
            "tasks",
            self.tasks.iter().map(|task| task.digest().to_string()),
        );
        hasher.finish()
    }

    /// Every identifier, for a "did you mean" line.
    pub fn known(&self) -> String {
        listed(self.tasks.iter().map(|task| task.id.as_str()))
    }
}

/// Names for a "did you mean" line, or `<none>`, which is more use than an empty string
/// in the middle of a sentence.
pub fn listed<'a>(names: impl Iterator<Item = &'a str>) -> String {
    let names: Vec<&str> = names.collect();
    if names.is_empty() {
        "<none>".to_owned()
    } else {
        names.join(", ")
    }
}

/// Warnings accompany a usable plan, and any error suppresses it.
pub fn resolve(manifest: &Manifest, overrides: &Overrides) -> Outcome<Plan> {
    let mut diagnostics = Vec::new();
    let mut tasks = Vec::new();

    for name in manifest.templates.keys() {
        if manifest.profiles.contains_key(name) {
            diagnostics.push(
                Diagnostic::new(
                    Code::NameCollision,
                    format!("`{name}` is both a template and a profile"),
                )
                .at(Location::new("templates", name))
                .help("`extends` looks in one namespace, so a name belongs to one map"),
            );
        }
    }

    if manifest.profiles.is_empty() {
        diagnostics.push(
            Diagnostic::new(Code::NoTasks, "this manifest defines no build tasks")
                .at(Location::map("profiles"))
                .help("a `templates` entry is never built on its own"),
        );
    }

    for name in manifest.profiles.keys() {
        expand(manifest, name, overrides, &mut tasks, &mut diagnostics);
    }

    tasks.sort_by(|a, b| a.id.cmp(&b.id));
    collisions(&tasks, &mut diagnostics);

    let failed = diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity() == crate::report::Severity::Error);

    Outcome::new((!failed).then_some(Plan { tasks }), diagnostics)
}

/// One profile becomes one task, or one per axis combination.
fn expand(
    manifest: &Manifest,
    name: &str,
    overrides: &Overrides,
    tasks: &mut Vec<Task>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let at = Location::new("profiles", name);

    let Ok(profile_name) = Name::new(name).inspect_err(|_| {}) else {
        diagnostics.push(
            Diagnostic::new(
                Code::BadName,
                format!("`{name}` is not a usable profile name"),
            )
            .at(at.clone())
            .help("`[`, `]`, `,` and `=` delimit a task identifier"),
        );
        return;
    };

    let Some(base) = inherit::flatten(manifest, name, diagnostics) else {
        return;
    };

    let mut axes: BTreeMap<Ident, Vec<String>> = BTreeMap::new();
    for (axis, values) in &base.axes {
        let Ok(identifier) = Ident::new(axis) else {
            diagnostics.push(
                Diagnostic::new(Code::BadVar, format!("axis `{axis}` is not an identifier"))
                    .at(at.clone().field("axes").field(axis))
                    .help("an axis becomes a `{token}` and a `PCMP_<NAME>` constant"),
            );
            return;
        };

        let listed: Vec<String> = values.values().into_iter().cloned().collect();
        if listed.is_empty() {
            diagnostics.push(
                Diagnostic::new(Code::EmptyAxis, format!("axis `{axis}` has no values"))
                    .at(at.clone().field("axes").field(axis))
                    .help("an empty axis expands to zero tasks"),
            );
            return;
        }

        axes.insert(identifier, listed);
    }

    for combination in combinations(&axes) {
        let mut profile = base.clone();

        // Axis name order, so two axes touching one key resolve predictably.
        for (axis, value) in &combination {
            if let Some(overlay) = base
                .axes
                .get(axis.as_str())
                .and_then(|values| values.overlay(value))
            {
                inherit::overlay(&mut profile, overlay);
            }
        }

        if let Some(task) = task(
            &profile_name,
            &profile,
            &combination,
            overrides,
            &at,
            diagnostics,
        ) {
            tasks.push(task);
        }
    }
}

/// The cartesian product, with axes in name order so identifiers read the same way twice.
fn combinations(axes: &BTreeMap<Ident, Vec<String>>) -> Vec<BTreeMap<Ident, String>> {
    let mut result: Vec<BTreeMap<Ident, String>> = vec![BTreeMap::new()];

    for (axis, values) in axes {
        let mut next = Vec::with_capacity(result.len() * values.len());
        for partial in &result {
            for value in values {
                let mut extended = partial.clone();
                extended.insert(axis.clone(), value.clone());
                next.push(extended);
            }
        }
        result = next;
    }

    result
}

fn task(
    profile_name: &Name,
    profile: &Profile,
    axes: &BTreeMap<Ident, String>,
    overrides: &Overrides,
    at: &Location,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Task> {
    let id = TaskId::new(profile_name, axes);

    let vars = vars(profile, profile_name, axes, overrides, at, diagnostics)?;
    let defines = defines(profile, &vars, overrides, at, diagnostics)?;

    let entry = path(profile.entry.as_deref(), &vars, "entry", at, diagnostics)?;
    let output = path(profile.output.as_deref(), &vars, "output", at, diagnostics)?;

    let mut sources = Vec::new();
    for (index, source) in profile.sources.iter().flatten().enumerate() {
        let expanded = report(
            template::expand_path(source, &vars),
            at.clone().field("sources").index(index),
            diagnostics,
        )?;
        sources.push(expanded);
    }
    sources.sort();
    sources.dedup();

    let mut header = Vec::new();
    for (index, line) in profile.header.iter().flatten().enumerate() {
        header.push(report(
            template::expand(line, &vars),
            at.clone().field("header").index(index),
            diagnostics,
        )?);
    }

    let ignore = profile.ignore.clone().unwrap_or_default();
    for (index, pattern) in ignore.iter().enumerate() {
        if let Err(error) = wax::Glob::new(pattern) {
            diagnostics.push(
                Diagnostic::new(Code::BadGlob, format!("`{pattern}` is not a valid glob"))
                    .at(at.clone().field("ignore").index(index))
                    .help(error.to_string()),
            );
            return None;
        }
    }

    let config = report(
        Config::assemble(
            profile.darklua.as_ref(),
            profile.loaders.as_deref(),
            &defines,
            at,
        ),
        at.clone(),
        diagnostics,
    )?;

    Some(Task {
        id,
        profile: profile_name.clone(),
        axes: axes.clone(),
        entry,
        output,
        sources,
        ignore,
        vars,
        defines,
        header,
        config,
    })
}

/// `--var` last, so a command line beats both the manifest and an axis.
fn vars(
    profile: &Profile,
    profile_name: &Name,
    axes: &BTreeMap<Ident, String>,
    overrides: &Overrides,
    at: &Location,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<BTreeMap<Ident, Scalar>> {
    let declared = profile
        .vars
        .iter()
        .map(|(name, value)| (name.clone(), value.clone()))
        .chain(
            overrides
                .vars
                .iter()
                .map(|(name, value)| (name.clone(), value.clone())),
        );

    let mut vars: BTreeMap<Ident, Scalar> = BTreeMap::new();
    let mut constants: BTreeMap<String, Ident> = BTreeMap::new();

    for (name, value) in declared {
        let identifier = report(
            Ident::new(&name),
            at.clone().field("vars").field(&name),
            diagnostics,
        )?;

        // `PCMP_<NAME>` is uppercased, so `channel` and `Channel` would collide.
        let constant = identifier.constant().to_string();
        if let Some(first) = constants.insert(constant.clone(), identifier.clone())
            && first != identifier
        {
            diagnostics.push(
                Diagnostic::new(
                    Code::BadVar,
                    format!("vars `{first}` and `{identifier}` share one constant"),
                )
                .at(at.clone().field("vars"))
                .help(format!("both become `{constant}`, so rename one")),
            );
            return None;
        }

        vars.insert(identifier, value);
    }

    for (axis, value) in axes {
        vars.insert(axis.clone(), Scalar::Text(value.clone()));
    }

    // Built in, and last, so it always names the profile that produced the task.
    if let Ok(profile_token) = Ident::new("profile") {
        vars.insert(profile_token, Scalar::Text(profile_name.to_string()));
    }

    Some(vars)
}

/// One constant per var. Ordinary defines, so a `define` of the same name overrides them.
fn defines(
    profile: &Profile,
    vars: &BTreeMap<Ident, Scalar>,
    overrides: &Overrides,
    at: &Location,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<BTreeMap<Ident, Scalar>> {
    let mut defines: BTreeMap<Ident, Scalar> = vars
        .iter()
        .map(|(name, value)| (name.constant(), value.clone()))
        .collect();

    let declared = profile
        .define
        .iter()
        .map(|(name, value)| (name.clone(), value.clone()))
        .chain(
            overrides
                .defines
                .iter()
                .map(|(name, value)| (name.clone(), value.clone())),
        );

    for (name, value) in declared {
        // `inject_global_value` substitutes by name, so a key such as `my-flag` or `end`
        // would match nothing.
        let identifier = report(
            Ident::new(&name).map_err(|error| Diagnostic {
                code: Code::BadDefine,
                ..error
            }),
            at.clone().field("define").field(&name),
            diagnostics,
        )?;

        if !value.representable() {
            diagnostics.push(
                Diagnostic::new(
                    Code::BadDefine,
                    format!("define `{identifier}` cannot reach Luau intact"),
                )
                .at(at.clone().field("define").field(&name))
                .help("a Luau number is a double: infinity, NaN and integers past 2^53 are out"),
            );
            return None;
        }

        defines.insert(identifier, value);
    }

    Some(defines)
}

fn path(
    declared: Option<&str>,
    vars: &BTreeMap<Ident, Scalar>,
    field: &str,
    at: &Location,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<RelPath> {
    let Some(declared) = declared else {
        let code = if field == "entry" {
            Code::MissingEntry
        } else {
            Code::MissingOutput
        };
        diagnostics.push(
            Diagnostic::new(code, format!("no `{field}` after inheritance"))
                .at(at.clone().field(field))
                .help(match code {
                    Code::MissingEntry => "a file to bundle, or a directory to process as a tree",
                    _ => "a template, e.g. \"dist/{profile}/app.luau\"",
                }),
        );
        return None;
    };

    report(
        template::expand_path(declared, vars),
        at.clone().field(field),
        diagnostics,
    )
}

/// Attaches a location to a leaf failure and records it.
fn report<T>(
    result: Result<T, Diagnostic>,
    at: Location,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(diagnostic) => {
            diagnostics.push(diagnostic.within(at));
            None
        }
    }
}

/// Two tasks writing one path would race, and a task writing into another's sources would
/// feed a build its own output.
fn collisions(tasks: &[Task], diagnostics: &mut Vec<Diagnostic>) {
    let mut claimed: BTreeMap<&RelPath, &TaskId> = BTreeMap::new();

    for task in tasks {
        if let Some(first) = claimed.insert(&task.output, &task.id) {
            diagnostics.push(
                Diagnostic::new(
                    Code::OutputCollision,
                    format!("`{first}` and `{}` both write `{}`", task.id, task.output),
                )
                .help("give them distinct `output` templates"),
            );
        }
    }

    // One finding per writing task, not one per task that would read it back. An output
    // landing inside three tasks' roots is one mistake with one fix.
    for writer in tasks {
        // Its own roots first: a task feeding itself is the usual way this happens and the
        // clearer thing to say when both are true.
        let found = swallowed(&writer.output, writer)
            .map(|(field, root)| (&writer.id, field, root))
            .or_else(|| {
                tasks.iter().find_map(|reader| {
                    swallowed(&writer.output, reader).map(|(field, root)| (&reader.id, field, root))
                })
            });

        let Some((owner, field, root)) = found else {
            continue;
        };

        let message = if *owner == writer.id {
            format!(
                "`{}` writes `{}` inside its own `{field}`",
                writer.id, writer.output
            )
        } else {
            format!(
                "`{}` writes `{}` inside `{owner}`'s `{field}`",
                writer.id, writer.output
            )
        };

        diagnostics.push(
            Diagnostic::new(Code::OutputInInputs, message)
                .help(format!(
                    "`{root}` is a build input, so the next build would read this artifact"
                ))
                .help("move the output outside every root, or exclude it with `ignore`"),
        );
    }
}

/// Which of `reader`'s roots contains `output`, and the field that named that root.
///
/// `entry` first, because a task that swallows its own output usually does it through the
/// entry directory rather than through an extra source root.
fn swallowed<'t>(output: &RelPath, reader: &'t Task) -> Option<(&'static str, &'t RelPath)> {
    if output.starts_with(&reader.entry) {
        return Some(("entry", &reader.entry));
    }

    reader
        .sources
        .iter()
        .find(|root| output.starts_with(root))
        .map(|root| ("sources", root))
}

impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

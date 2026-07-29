//! Argument parsing and dispatch.
//!
//! `clap` already prints the flag reference and `pcmp explain` already prints the
//! diagnostic reference, so neither is restated in the documentation and neither can go
//! stale.

pub mod render;
pub mod select;

use std::collections::BTreeMap;
use std::rc::Rc;

use clap::{Parser, Subcommand, ValueEnum};

use crate::build::{self, Engine};
use crate::manifest::ledger::Reader;
use crate::manifest::{Manifest, Scalar, format, scaffold, schema};
use crate::plan::{self, Overrides, Plan};
use crate::report::{self, Code, Diagnostic, Exit, Severity};
use crate::vfs::AbsPath;

#[derive(Debug, Parser)]
#[command(
    name = "pcmp",
    version,
    long_version = concat!(env!("CARGO_PKG_VERSION"), "\ndarklua ", "0.19.0"),
    about = "Multi-target build composition for Luau projects",
    // There is no default subcommand. One would only fire with no arguments, so `pcmp`
    // would build and `pcmp release` would report an unrecognised subcommand, which is
    // a worse answer than asking for the word `build`.
    arg_required_else_help = true
)]
pub struct Cli {
    /// Manifest to use. Discovered from the working directory upwards otherwise.
    #[arg(long, short, global = true, value_name = "PATH")]
    manifest: Option<String>,

    /// Where build state is kept. Defaults to `.pcmp/` beside the manifest.
    #[arg(long, global = true, value_name = "PATH")]
    cache_dir: Option<String>,

    /// A value for `pcmp.env`, ahead of the process environment. Repeatable.
    #[arg(long = "env", short = 'e', global = true, value_name = "KEY=VALUE")]
    env: Vec<String>,

    /// Set a token. Repeatable. Beats the manifest.
    #[arg(long = "var", global = true, value_name = "KEY=VALUE")]
    var: Vec<String>,

    /// Set a constant. Repeatable. Beats the manifest.
    #[arg(long = "define", short = 'D', global = true, value_name = "KEY=VALUE")]
    define: Vec<String>,

    /// Pin `pcmp.now()`. `SOURCE_DATE_EPOCH` is honoured when this is absent.
    #[arg(long, global = true, value_name = "RFC3339")]
    now: Option<String>,

    /// Machine-readable output, byte-stable for a given build.
    #[arg(long, global = true)]
    json: bool,

    /// Add a duration to each row. Left out by default so a report can be diffed.
    #[arg(long, global = true)]
    timings: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Build every task, or a selection.
    Build {
        /// Profile names or exact task identifiers. All when omitted.
        tasks: Vec<String>,
        /// Filter an expansion by coordinate. Repeatable.
        #[arg(long = "axis", value_name = "KEY=VALUE")]
        axis: Vec<String>,
        /// Ignore cached state.
        #[arg(long)]
        no_cache: bool,
        /// Write pcmp.lock, recording what this build read and produced.
        #[arg(long, conflicts_with = "frozen")]
        lock: bool,
        /// Reproduce what pcmp.lock records, and fail if anything differs.
        #[arg(long)]
        frozen: bool,
    },

    /// Resolve and print without building. Naming a task prints its full configuration.
    Plan {
        /// An exact task identifier. Every task when omitted.
        task: Option<String>,
        /// Say whether each task is stale or fresh, and why.
        #[arg(long)]
        why: bool,
    },

    /// Lint the manifest and the resolved plan.
    Check {
        /// Fail on warnings too.
        #[arg(long)]
        strict: bool,
    },

    /// Rebuild whenever an input or the manifest changes.
    Watch {
        /// Profile names or exact task identifiers. All when omitted.
        tasks: Vec<String>,
        /// Filter an expansion by coordinate. Repeatable.
        #[arg(long = "axis", value_name = "KEY=VALUE")]
        axis: Vec<String>,
    },

    /// Write a starter manifest.
    Init {
        /// Defaults to the directory name.
        #[arg(long)]
        name: Option<String>,
        /// Detected from common locations when omitted.
        #[arg(long)]
        entry: Option<String>,
        /// `luau` computes values and adds `pcmp.env`, `pcmp.now` and `pcmp.read`.
        #[arg(long, value_enum, default_value_t = scaffold::Format::Json5)]
        format: scaffold::Format,
    },

    /// Emit the manifest schema.
    Schema {
        #[arg(long, value_enum, default_value_t = Shape::Json)]
        format: Shape,
    },

    /// Explain a diagnostic code, as printed in `error[missing-output]`.
    Explain {
        /// Omit to list every code.
        code: Option<String>,
        /// `markdown` emits the whole catalogue as a documentation page.
        #[arg(long, value_enum, default_value_t = Prose::Text)]
        format: Prose,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Shape {
    Json,
    Luau,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Prose {
    Text,
    Markdown,
}

/// Whatever ended the run.
///
/// A `Vec`, because resolution reports every bad profile at once. The `From<Diagnostic>`
/// impl is what lets `?` carry a single finding and a whole resolution failure along the
/// same path, so neither has to be printed before the run is over.
#[derive(Debug)]
pub struct Failure(Vec<Diagnostic>);

impl From<Diagnostic> for Failure {
    fn from(diagnostic: Diagnostic) -> Self {
        Self(vec![diagnostic])
    }
}

/// Prints what ended the run and says what to exit with. The one place either happens.
pub fn fail(cli: &Cli, failure: &Failure) -> Exit {
    render::failures(&failure.0, cli.json);

    // Sorted worst first by `report::sort`, so the first is the one that decides.
    failure
        .0
        .first()
        .map_or(Exit::Config, |first| first.code.exit())
}

/// A manifest found, loaded and resolved. Every command except `schema`, `init` and
/// `explain` opens one, and each says so by asking for it.
struct Project {
    root: AbsPath,
    /// The manifest itself, which is inside the roots but is not a source.
    manifest_path: AbsPath,
    cache: AbsPath,
    manifest: Manifest,
    plan: Plan,
    diagnostics: Vec<Diagnostic>,
    reader: Rc<Reader>,
}

impl Project {
    fn open(cli: &Cli, cwd: &AbsPath, frozen: bool) -> Result<Self, Failure> {
        let overrides = overrides(cli)?;

        let mut reader = Reader::new(select::pairs(&cli.env)?, cli.now.clone())?;
        let path = match cli.manifest.as_deref() {
            Some(given) => cwd.join(given)?,
            None => format::discover(cwd)?,
        };
        let root = path
            .parent()
            .ok_or_else(|| Diagnostic::new(Code::NoManifest, format!("`{path}` has no parent")))?;

        if frozen {
            let lock = build::record::Lock::load(&root).ok_or_else(|| {
                Diagnostic::new(Code::Frozen, "no pcmp.lock to reproduce")
                    .help("run `pcmp build --lock` first")
            })?;
            reader = reader.frozen(lock.ledger);
        }

        let reader = Rc::new(reader);
        for (name, value) in overrides.vars.iter().chain(&overrides.defines) {
            reader.note_override(name, &value.text());
        }

        let loaded = format::load(&path, &reader)?;
        let cache = match cli.cache_dir.as_deref() {
            Some(given) => cwd.join(given)?,
            None => loaded.root.join(build::CACHE_DIR)?,
        };

        // Every finding travels out rather than being printed here, so one place decides
        // how a failure is rendered and `--json` means the same thing on every path.
        let outcome = plan::resolve(&loaded.manifest, &overrides);
        let Some(plan) = outcome.value else {
            return Err(Failure(outcome.diagnostics));
        };

        Ok(Self {
            root: loaded.root,
            manifest_path: loaded.path,
            cache,
            manifest: loaded.manifest,
            plan,
            diagnostics: outcome.diagnostics,
            reader,
        })
    }
}

fn overrides(cli: &Cli) -> Result<Overrides, Diagnostic> {
    let scalars = |arguments: &[String]| -> Result<BTreeMap<String, Scalar>, Diagnostic> {
        Ok(select::pairs(arguments)?
            .into_iter()
            .map(|(key, value)| (key, Scalar::parse(&value)))
            .collect())
    };

    Ok(Overrides {
        vars: scalars(&cli.var)?,
        defines: scalars(&cli.define)?,
    })
}

pub fn run(cli: &Cli) -> Result<Exit, Failure> {
    let cwd = AbsPath::cwd()?;

    match &cli.command {
        Command::Schema { format } => {
            render::line(match format {
                Shape::Json => schema::json(),
                Shape::Luau => schema::luau(),
            });
            Ok(Exit::Success)
        }

        Command::Explain { code, format } => explain(code.as_deref(), *format),

        Command::Init {
            name,
            entry,
            format,
        } => {
            let name = name
                .clone()
                .unwrap_or_else(|| cwd.file_name().unwrap_or("project").to_owned());
            let written = scaffold::write(&cwd, &name, entry.as_deref(), *format)?;
            render::created(&[written]);
            Ok(Exit::Success)
        }

        Command::Plan { task, why } => plan(cli, &cwd, task.as_deref(), *why),

        Command::Check { strict } => {
            let project = Project::open(cli, &cwd, false)?;

            let staged = sources(&project);
            let mut diagnostics = plan::lint::run(
                &project.manifest,
                &project.plan,
                &project.reader.ledger(),
                &project.root,
                staged.as_deref(),
            );
            diagnostics.extend(project.diagnostics);
            report::sort(&mut diagnostics);
            render::diagnostics(&diagnostics, cli.json);

            Ok(match report::worst(&diagnostics) {
                Some(Severity::Error) => Exit::Lint,
                Some(Severity::Warning) if *strict => Exit::Lint,
                _ => Exit::Success,
            })
        }

        Command::Build {
            tasks,
            axis,
            no_cache,
            lock,
            frozen,
        } => build(cli, &cwd, tasks, axis, *no_cache, *lock, *frozen),

        Command::Watch { tasks, axis } => {
            let project = Project::open(cli, &cwd, false)?;
            let selection = select::select(&project.plan, tasks, &select::pairs(axis)?)?;
            Ok(build::watch::run(
                &project.root,
                &project.cache,
                &project.manifest_path,
                &project.plan,
                &selection,
                cli.json,
                cli.timings,
            )?)
        }
    }
}

fn plan(cli: &Cli, cwd: &AbsPath, task: Option<&str>, why: bool) -> Result<Exit, Failure> {
    let project = Project::open(cli, cwd, false)?;

    match task {
        Some(name) => {
            let chosen = project.plan.get(name).ok_or_else(|| {
                Diagnostic::new(Code::NoSuchTask, format!("no task `{name}`"))
                    .help(format!("known tasks: {}", project.plan.known()))
            })?;
            render::task(chosen, cli.json);
        }
        // Every digest is computed, and nothing is built, so this says what a build would
        // do without doing it.
        None if why => {
            let engine = Engine::new(project.root.clone(), project.cache, true);
            render::heading(&project.plan, cli.json);
            render::build(
                &engine.inspect(&project.plan, &project.plan),
                cli.json,
                cli.timings,
                true,
            );
        }
        None => render::plan(&project.plan, cli.json),
    }

    Ok(Exit::Success)
}

fn build(
    cli: &Cli,
    cwd: &AbsPath,
    tasks: &[String],
    axis: &[String],
    no_cache: bool,
    lock: bool,
    frozen: bool,
) -> Result<Exit, Failure> {
    let project = Project::open(cli, cwd, frozen)?;
    let selection = select::select(&project.plan, tasks, &select::pairs(axis)?)?;

    let engine = Engine::new(
        project.root.clone(),
        project.cache.clone(),
        !no_cache && !frozen,
    );

    render::heading(&project.plan, cli.json);
    let report = engine.run(&project.plan, &selection);
    render::build(&report, cli.json, cli.timings, false);

    if !report.succeeded() {
        return Ok(Exit::Build);
    }
    if lock {
        write_lock(&project, &report)?;
    }
    if frozen {
        return frozen_verdict(&project, &report);
    }

    Ok(Exit::Success)
}

/// Every Luau source byte in the project, for the define check.
///
/// This is what `check` costs beyond resolving: one read of the sources, and no darklua.
/// The union across tasks rather than one set per task, because a define read from another
/// profile's tree is a false negative, and a false positive would be worse.
///
/// Two things are left out. Anything that is not Lua source, because a define is only ever
/// substituted into Lua source. And the manifest itself, which lives inside the roots and
/// names every define it declares, so including it would mean a misspelled define always
/// found itself.
fn sources(project: &Project) -> Option<String> {
    let cache = project.cache.relative_to(&project.root);
    let mut seen = std::collections::BTreeSet::new();
    let mut text = String::new();

    for task in &project.plan.tasks {
        let scope = build::inputs::Scope::of(task, &project.plan, cache.as_ref());
        let Ok(paths) = build::inputs::everything(&scope, &project.root) else {
            continue;
        };

        for path in paths {
            if !matches!(path.extension(), Some("luau" | "lua")) || !seen.insert(path.clone()) {
                continue;
            }
            if let Ok(absolute) = project.root.join(path.as_str())
                && absolute != project.manifest_path
                && let Ok(bytes) = crate::vfs::read(&absolute)
                && let Ok(source) = String::from_utf8(bytes)
            {
                text.push_str(&source);
                text.push('\n');
            }
        }
    }

    (!text.is_empty()).then_some(text)
}

fn explain(code: Option<&str>, format: Prose) -> Result<Exit, Failure> {
    if format == Prose::Markdown {
        render::line(report::reference());
        return Ok(Exit::Success);
    }

    let Some(code) = code else {
        for known in report::ALL {
            render::line(known.slug());
        }
        return Ok(Exit::Success);
    };

    let known = Code::parse(code).ok_or_else(|| {
        Diagnostic::new(Code::BadArgument, format!("no diagnostic code `{code}`"))
            .help("run `pcmp explain` with no argument for the list")
    })?;

    render::line(format!("{}\n", known.slug()));
    render::line(known.description());
    Ok(Exit::Success)
}

fn write_lock(project: &Project, report: &build::Report) -> Result<(), Failure> {
    let tasks = report
        .tasks
        .iter()
        .filter_map(|task| {
            let plan = project.plan.get(task.task.as_str())?;
            Some((
                task.task.to_string(),
                build::record::Locked {
                    plan: plan.digest(),
                    artifacts: task.artifacts?,
                },
            ))
        })
        .collect();

    build::record::Lock::new(project.reader.ledger(), tasks).save(&project.root)?;
    Ok(())
}

/// A frozen build has already produced its artifacts, so this is only the comparison.
fn frozen_verdict(project: &Project, report: &build::Report) -> Result<Exit, Failure> {
    let Some(lock) = build::record::Lock::load(&project.root) else {
        return Err(Diagnostic::new(Code::Frozen, "no pcmp.lock to reproduce").into());
    };

    let differing: Vec<&str> = report
        .tasks
        .iter()
        .filter(|task| {
            lock.tasks
                .get(task.task.as_str())
                .is_none_or(|locked| Some(locked.artifacts) != task.artifacts)
        })
        .map(|task| task.task.as_str())
        .collect();

    // A second summary line under the build's own, counted the same way, so the verdict
    // reads as part of the report rather than as an announcement.
    render::line(format!(
        "{} reproduced, {} differing",
        report.tasks.len() - differing.len(),
        differing.len()
    ));

    if differing.is_empty() {
        return Ok(Exit::Success);
    }

    render::line("");
    render::diagnostics(
        &[Diagnostic::new(
            Code::Frozen,
            format!(
                "{} did not reproduce",
                render::count(differing.len(), "task")
            ),
        )
        .help(differing.join(", "))],
        false,
    );

    Ok(Exit::Build)
}

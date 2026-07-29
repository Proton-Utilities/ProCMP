//! The diagnostic catalogue.
//!
//! This is the reference: `pcmp explain <CODE>` prints [`Code::description`], so there is
//! no generated table anywhere that could disagree with it.

use std::fmt::Write as _;

use super::{Exit, Severity};

/// Which stage of a run can report a code, and how the reference groups them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Invocation,
    Manifest,
    Resolution,
    Plan,
    Execution,
    Lint,
}

impl Phase {
    const ORDER: &'static [Self] = &[
        Self::Invocation,
        Self::Manifest,
        Self::Resolution,
        Self::Plan,
        Self::Execution,
        Self::Lint,
    ];

    const fn title(self) -> &'static str {
        match self {
            Self::Invocation => "Reading the command line",
            Self::Manifest => "Reading the manifest",
            Self::Resolution => "Resolving the plan",
            Self::Plan => "Checking the plan",
            Self::Execution => "Building",
            Self::Lint => "Lints",
        }
    }

    const fn summary(self) -> &'static str {
        match self {
            Self::Invocation => {
                "An unknown flag or an unknown value is rejected separately, with a usage message."
            }
            Self::Manifest => "Nothing is built when one of these fires.",
            Self::Resolution => {
                "Every profile is checked before the run gives up, so one edit can fix several at once."
            }
            Self::Plan => {
                "Found once every task is known, so these name two tasks rather than one."
            }
            Self::Execution => "Reported per task, and one failing task does not stop the others.",
            Self::Lint => "Reported by `pcmp check`. `--strict` makes the warnings fail too.",
        }
    }
}

/// Every way a run can go wrong, in the order the phases run.
///
/// A code names one failure and never another, so a message may be reworded but a code
/// may not be reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Code {
    // ── invocation ────────────────────────────────────────────────────────────────
    BadArgument,

    // ── manifest ──────────────────────────────────────────────────────────────────
    NoManifest,
    UnknownFormat,
    Unreadable,
    Syntax,
    NotATable,
    Eval,
    Budget,
    UnsetEnv,

    // ── resolution ────────────────────────────────────────────────────────────────
    UnknownBase,
    CyclicExtends,
    NameCollision,
    BadName,
    MissingEntry,
    MissingOutput,
    BadTemplate,
    BadPath,
    BadDefine,
    BadVar,
    BadRules,
    BadLoader,
    BadLoaderPattern,
    BadGlob,
    EmptyAxis,
    NoTasks,

    // ── plan validation ───────────────────────────────────────────────────────────
    OutputCollision,
    OutputInInputs,
    NoSuchTask,

    // ── execution ─────────────────────────────────────────────────────────────────
    MissingEntryFile,
    UndeclaredInput,
    DarkluaConfig,
    ProcessFailed,
    NoOutput,
    WriteFailed,
    Frozen,

    // ── lints ─────────────────────────────────────────────────────────────────────
    FoldBeforeInject,
    BranchBeforeFold,
    UnreachableDefine,
    UnrecordedReading,
    ShadowedVar,
    OutputOutsideRoot,
    StaleSchema,
    UnusedTemplate,
    IdenticalProfiles,
}

/// Every variant, so `explain` can list them and the CLI can resolve a slug.
pub const ALL: &[Code] = &[
    Code::BadArgument,
    Code::NoManifest,
    Code::UnknownFormat,
    Code::Unreadable,
    Code::Syntax,
    Code::NotATable,
    Code::Eval,
    Code::Budget,
    Code::UnsetEnv,
    Code::UnknownBase,
    Code::CyclicExtends,
    Code::NameCollision,
    Code::BadName,
    Code::MissingEntry,
    Code::MissingOutput,
    Code::BadTemplate,
    Code::BadPath,
    Code::BadDefine,
    Code::BadVar,
    Code::BadRules,
    Code::BadLoader,
    Code::BadLoaderPattern,
    Code::BadGlob,
    Code::EmptyAxis,
    Code::NoTasks,
    Code::OutputCollision,
    Code::OutputInInputs,
    Code::NoSuchTask,
    Code::MissingEntryFile,
    Code::UndeclaredInput,
    Code::DarkluaConfig,
    Code::ProcessFailed,
    Code::NoOutput,
    Code::WriteFailed,
    Code::Frozen,
    Code::FoldBeforeInject,
    Code::BranchBeforeFold,
    Code::UnreachableDefine,
    Code::UnrecordedReading,
    Code::ShadowedVar,
    Code::OutputOutsideRoot,
    Code::StaleSchema,
    Code::UnusedTemplate,
    Code::IdenticalProfiles,
];

impl Code {
    /// The stable kebab-case name, as printed in `error[missing-output]`.
    pub const fn slug(self) -> &'static str {
        match self {
            Self::BadArgument => "bad-argument",
            Self::NoManifest => "no-manifest",
            Self::UnknownFormat => "unknown-format",
            Self::Unreadable => "unreadable",
            Self::Syntax => "syntax",
            Self::NotATable => "not-a-table",
            Self::Eval => "eval",
            Self::Budget => "budget",
            Self::UnsetEnv => "unset-env",
            Self::UnknownBase => "unknown-base",
            Self::CyclicExtends => "cyclic-extends",
            Self::NameCollision => "name-collision",
            Self::BadName => "bad-name",
            Self::MissingEntry => "missing-entry",
            Self::MissingOutput => "missing-output",
            Self::BadTemplate => "bad-template",
            Self::BadPath => "bad-path",
            Self::BadDefine => "bad-define",
            Self::BadVar => "bad-var",
            Self::BadRules => "bad-rules",
            Self::BadLoader => "bad-loader",
            Self::BadLoaderPattern => "bad-loader-pattern",
            Self::BadGlob => "bad-glob",
            Self::EmptyAxis => "empty-axis",
            Self::NoTasks => "no-tasks",
            Self::OutputCollision => "output-collision",
            Self::OutputInInputs => "output-in-inputs",
            Self::NoSuchTask => "no-such-task",
            Self::MissingEntryFile => "missing-entry-file",
            Self::UndeclaredInput => "undeclared-input",
            Self::DarkluaConfig => "darklua-config",
            Self::ProcessFailed => "process-failed",
            Self::NoOutput => "no-output",
            Self::WriteFailed => "write-failed",
            Self::Frozen => "frozen",
            Self::FoldBeforeInject => "fold-before-inject",
            Self::BranchBeforeFold => "branch-before-fold",
            Self::UnreachableDefine => "unreachable-define",
            Self::UnrecordedReading => "unrecorded-reading",
            Self::ShadowedVar => "shadowed-var",
            Self::OutputOutsideRoot => "output-outside-root",
            Self::StaleSchema => "stale-schema",
            Self::UnusedTemplate => "unused-template",
            Self::IdenticalProfiles => "identical-profiles",
        }
    }

    /// Fixed per code, so a finding cannot be reported at the wrong level by accident.
    pub const fn severity(self) -> Severity {
        match self {
            Self::UnreachableDefine
            | Self::UnrecordedReading
            | Self::ShadowedVar
            | Self::OutputOutsideRoot
            | Self::StaleSchema
            | Self::UnusedTemplate
            | Self::IdenticalProfiles => Severity::Warning,
            _ => Severity::Error,
        }
    }

    /// What a run exits with when this is the worst thing that happened.
    ///
    /// Distinct from [`Self::phase`], which is about whether a phase can keep collecting.
    /// Reporting every bad profile in one run and exiting 2 because the manifest did not
    /// resolve are two different questions.
    pub const fn exit(self) -> Exit {
        match self {
            Self::MissingEntryFile
            | Self::UndeclaredInput
            | Self::DarkluaConfig
            | Self::ProcessFailed
            | Self::NoOutput
            | Self::WriteFailed
            | Self::Frozen => Exit::Build,

            Self::FoldBeforeInject
            | Self::BranchBeforeFold
            | Self::UnreachableDefine
            | Self::UnrecordedReading
            | Self::ShadowedVar
            | Self::OutputOutsideRoot
            | Self::StaleSchema
            | Self::UnusedTemplate
            | Self::IdenticalProfiles => Exit::Lint,

            // Everything else is the manifest failing to load or to resolve, which
            // includes a selection naming a task the manifest does not describe.
            _ => Exit::Config,
        }
    }

    /// Where in a run this can come from.
    pub const fn phase(self) -> Phase {
        match self {
            Self::BadArgument => Phase::Invocation,

            Self::NoManifest
            | Self::UnknownFormat
            | Self::Unreadable
            | Self::Syntax
            | Self::NotATable
            | Self::Eval
            | Self::Budget
            | Self::UnsetEnv => Phase::Manifest,

            Self::OutputCollision | Self::OutputInInputs | Self::NoSuchTask => Phase::Plan,

            Self::MissingEntryFile
            | Self::UndeclaredInput
            | Self::DarkluaConfig
            | Self::ProcessFailed
            | Self::NoOutput
            | Self::WriteFailed
            | Self::Frozen => Phase::Execution,

            Self::FoldBeforeInject
            | Self::BranchBeforeFold
            | Self::UnreachableDefine
            | Self::UnrecordedReading
            | Self::ShadowedVar
            | Self::OutputOutsideRoot
            | Self::StaleSchema
            | Self::UnusedTemplate
            | Self::IdenticalProfiles => Phase::Lint,

            _ => Phase::Resolution,
        }
    }

    pub fn parse(slug: &str) -> Option<Self> {
        ALL.iter().copied().find(|code| code.slug() == slug)
    }

    /// The long form, printed by `pcmp explain <CODE>`.
    ///
    /// One match rather than one per phase, and a match rather than a table: the compiler
    /// refusing to build until every code has a description is the entire point.
    #[allow(
        clippy::too_many_lines,
        reason = "a catalogue is as long as the catalogue"
    )]
    pub const fn description(self) -> &'static str {
        match self {
            Self::BadArgument => {
                "\
An argument's value is not the shape its flag takes. `--env`, `--var`, `--define` and
`--axis` each take `KEY=VALUE`, `--now` takes an RFC 3339 instant in UTC to the second,
and `pcmp explain` takes a code from the list `pcmp explain` prints."
            }

            Self::NoManifest => {
                "\
No manifest in the working directory or any directory above it. Run `pcmp init` to write
one, or point at an existing manifest with `-m`."
            }

            Self::UnknownFormat => {
                "\
The extension picks the format, so a JSON manifest called `pcmp.conf` is not read.
Supported: json5, json, jsonc, toml, luau."
            }

            Self::Unreadable => "The operating system refused a read. Its own message follows.",

            Self::Syntax => {
                "\
The manifest is not valid in the format its extension declares. The parser's own message
follows, with a line and column where it reports one."
            }

            Self::NotATable => {
                "\
A Luau manifest must evaluate to a table. The usual cause is a missing `return`."
            }

            Self::Eval => {
                "A Luau manifest raised an error while evaluating. Its traceback follows."
            }

            Self::Budget => {
                "\
A Luau manifest exceeded its evaluation budget or its memory limit. The usual cause is a
loop whose condition never becomes false."
            }

            Self::UnsetEnv => {
                "\
`pcmp.env(name)` was called for a variable that is set neither by `--env` nor in the
process environment. Pass `--env NAME=VALUE`, or use `pcmp.envOr(name, fallback)` when a
default is acceptable."
            }

            Self::UnknownBase => {
                "\
`extends` names something that is in neither `templates` nor `profiles`. The help line
lists what is available."
            }

            Self::CyclicExtends => {
                "\
An `extends` chain returns to a profile it already passed through. The help line lists the
cycle."
            }

            Self::NameCollision => {
                "\
The same name appears in both `templates` and `profiles`. `extends` looks in both, so a
name belongs to one of them. Rename one."
            }

            Self::BadName => {
                "\
A profile or template name contains `[`, `]`, `,` or `=`. Those characters delimit a task
identifier such as `dist[target=roblox]`. Rename it."
            }

            Self::MissingEntry => {
                "\
Neither the profile nor anything it extends declares an `entry`, which is a file to
bundle or a directory to process as a tree."
            }

            Self::MissingOutput => {
                "\
Neither the profile nor anything it extends declares an `output`, which is a template and
so may vary by profile or axis: \"dist/{profile}/app.luau\"."
            }

            Self::BadTemplate => {
                "\
A template names a token that is not a var, not an axis and not `{profile}`, or leaves a
`{` unclosed, or expands to nothing. The help line lists the tokens you can use. Write
`{{` and `}}` for a literal brace.

In a path, a token may not expand to a `.` or `..` segment, so a profile named `..` is
refused. A plain `/` is allowed, and `{outdir}/app.luau` works."
            }

            Self::BadPath => {
                "\
A path is empty, or climbs above the filesystem root with `..`. Paths resolve against
the manifest's own directory, never the working directory."
            }

            Self::BadDefine => {
                "\
A define key must be a Luau identifier, so `my-flag` and `end` are refused. A value must
be finite, and an integer must survive a round trip through an IEEE double, which bounds
it at 2^53."
            }

            Self::BadVar => {
                "\
A var name must be a Luau identifier. Two names may also collide, because the constant is
uppercased, so `channel` and `Channel` both give you `PCMP_CHANNEL`."
            }

            Self::BadRules => {
                "\
`darklua.rules` is not a list darklua could read. Each entry is a rule name, or an object
with a `rule` key and that rule's own settings."
            }

            Self::BadLoader => {
                "\
A loader names a strategy darklua does not have. Valid: copy, skip, luau, json,
json_lines, toml, yaml, string, buffer, bytes, and the encoded forms string/base64,
string/zstd, string/gzip and string/zlib, with buffer and bytes likewise."
            }

            Self::BadLoaderPattern => {
                "\
A loader's `pattern` is not one darklua accepts. A pattern matches a file's path relative
to the entry."
            }

            Self::BadGlob => {
                "\
An `ignore` entry is not a valid glob. Globs match each file's path relative to the root
it was found under."
            }

            Self::EmptyAxis => "An axis lists no values, so its profile expands to zero tasks.",

            Self::NoTasks => {
                "\
The manifest declares no profiles, so there is nothing to build. A `templates` entry is
never built on its own."
            }

            Self::OutputCollision => {
                "\
Two tasks write to the same path, and whichever finished last would win. Give them
distinct `output` templates. `{profile}` and every axis are available as tokens."
            }

            Self::OutputInInputs => {
                "\
A task writes inside a root that a task reads, so the next build would read the artifact
as a source. Move the output outside every root, or exclude it with `ignore`."
            }

            Self::NoSuchTask => {
                "\
No task matched the selection, and the help line lists the ones that exist. A selector is a
profile name or an exact task identifier, and `--axis KEY=VALUE` filters by coordinate.
There is no wildcard."
            }

            Self::MissingEntryFile => {
                "\
The task's `entry` does not exist. The path resolves against the manifest's directory, not
the directory you ran from."
            }

            Self::UndeclaredInput => {
                "\
darklua asked for a file that is under no root this task declares, and the path it tried
is in the help line. Add the directory holding it to `sources`."
            }

            Self::DarkluaConfig => {
                "\
darklua rejected the configuration this task compiles to, which is printed with the error.
`pcmp plan <TASK>` shows the same thing without building."
            }

            Self::ProcessFailed => {
                "\
darklua reported an error while transforming this task's sources. Its own message
follows. When the error is a file darklua could not find, the code is
`undeclared-input` instead."
            }

            Self::NoOutput => {
                "\
The task reported no failure and produced nothing. Check `apply_to_files` and `skip_files`,
which match a path relative to the entry, so `src/**` matches nothing when the entry is
already `src/init.luau`."
            }

            Self::WriteFailed => {
                "\
An artifact could not be written, and the operating system's own message follows. The
artifact from the previous build is left intact."
            }

            Self::Frozen => {
                "\
A `--frozen` build did not reproduce what `pcmp.lock` records, and the help line names the
tasks that differ. Either the manifest changed since the lock was written, or a task
produced different bytes from the same inputs."
            }

            Self::FoldBeforeInject => {
                "\
`compute_expression` is listed before `inject_global_value`, so folding runs first and the
define has no effect. Move every injection ahead of every fold. `pcmp` puts the ones it
generates first, so this only comes from an `inject_global_value` you wrote."
            }

            Self::BranchBeforeFold => {
                "\
`remove_unused_if_branch` is scheduled before `compute_expression`. A branch can only be
removed once its condition has folded to a constant, so the branch survives."
            }

            Self::UnreachableDefine => {
                "\
A define's identifier appears in none of the task's sources, so nothing is substituted.
Check the spelling on both sides."
            }

            Self::UnrecordedReading => {
                "\
The manifest read the clock or the environment, and no pcmp.lock exists, so nothing
records what it read. Run `pcmp build --lock`, and `pcmp build --frozen` then reproduces
the build exactly, timestamps included."
            }

            Self::ShadowedVar => {
                "\
A var is named `profile`, which `pcmp` sets itself. The built-in wins. Rename yours."
            }

            Self::OutputOutsideRoot => {
                "\
A task writes outside the manifest's directory. This builds, and is sometimes what you
want, but `pcmp` is touching files outside the project."
            }

            Self::StaleSchema => {
                "\
A pcmp.schema.json in the project no longer matches this version of `pcmp`, so your editor
is completing against the wrong thing. Regenerate it with `pcmp schema`, or delete it."
            }

            Self::UnusedTemplate => {
                "\
Nothing extends this template, and a template is never built, so it does nothing. Remove
it, or move it to `profiles`."
            }

            Self::IdenticalProfiles => {
                "\
Two profiles resolve to the same task apart from their output. Move what they share into a
template, or give one of them an axis."
            }
        }
    }
}

/// The whole catalogue as a documentation page.
///
/// Written by `pcmp explain --format markdown`, and checked in CI against the copy in the
/// repository. Generating it is the only way a reference of this size stays true: the
/// binary is the source, and a page that disagrees with the binary fails the build.
///
/// The heading of each entry is its slug, so `toc.permalink` gives every code the anchor
/// the rest of the documentation links to, and the site build then checks those anchors,
/// so renaming a code fails CI rather than leaving a dead link behind.
pub fn reference() -> String {
    let mut out = String::from(
        "---\n\
         description: Every code pcmp can report, and what to do about it.\n\
         ---\n\n\
         # Diagnostics\n\n\
         A code never changes meaning, so it is safe to grep for.\n\n\
         ```sh\n\
         pcmp explain missing-output\n\
         ```\n",
    );

    for phase in Phase::ORDER {
        let _ = write!(out, "\n## {}\n\n{}\n", phase.title(), phase.summary());

        for code in ALL.iter().filter(|code| code.phase() == *phase) {
            let severity = match code.severity() {
                Severity::Error => "error",
                Severity::Warning => "warning",
            };

            // One line rather than a definition list. Severity is an attribute of the
            // code and not a term being defined, and forty-four two-line blocks would
            // make the page a third longer for nothing.
            let _ = write!(
                out,
                "\n### {}\n\n`{severity}`, exits `{}`\n\n{}\n",
                code.slug(),
                code.exit() as u8,
                code.description()
            );
        }
    }

    out
}

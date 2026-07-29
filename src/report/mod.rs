//! The crate's one failure type.
//!
//! Fatal and accumulating findings are the same thing reported in different phases:
//! nothing can be collected past a manifest that would not parse, whereas resolution
//! checks every profile before it gives up. [`Code::phase`] carries that distinction so
//! the type does not have to.

mod code;

pub use code::{ALL, Code, reference};

use std::error::Error;
use std::fmt;

/// Ordering is meaningful: the worst finding in a run is a `max` over the set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Warning,
    Error,
}

/// Where in the manifest a finding sits, as a key path such as
/// `profiles.release.darklua.rules[2]`.
///
/// Not a byte span. `json5` reports a position only when parsing fails, `toml` spans
/// would need `Spanned<T>` on every field, and a value a Luau manifest computed has no
/// position at all. A key path is the one representation every format can produce, and
/// it is what points at the field.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(transparent)]
pub struct Location(String);

impl Location {
    /// `profiles` and `release` give `profiles.release`.
    pub fn new(map: &str, key: &str) -> Self {
        Self(format!("{map}.{key}"))
    }

    /// Only the map, for a finding about the map itself.
    pub fn map(map: &str) -> Self {
        Self(map.to_owned())
    }

    #[must_use]
    pub fn field(mut self, field: &str) -> Self {
        self.0.push('.');
        self.0.push_str(field);
        self
    }

    #[must_use]
    pub fn index(mut self, index: usize) -> Self {
        use fmt::Write as _;
        let _ = write!(self.0, "[{index}]");
        self
    }
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One finding. Severity comes from the code, so a finding cannot be reported at the
/// wrong level.
#[derive(Debug)]
pub struct Diagnostic {
    pub code: Code,
    pub at: Option<Location>,
    pub message: String,
    pub help: Option<String>,
    /// The operating system's own words, when something outside `pcmp` refused.
    pub source: Option<String>,
}

impl Diagnostic {
    pub fn new(code: Code, message: impl Into<String>) -> Self {
        Self {
            code,
            at: None,
            message: message.into(),
            help: None,
            source: None,
        }
    }

    #[must_use]
    pub fn at(mut self, at: Location) -> Self {
        self.at = Some(at);
        self
    }

    /// Fills in a location only when the finding does not already know a better one.
    ///
    /// A caller knows the profile. The leaf that failed knows the field. The narrower of
    /// the two is the useful one, so the first location set is the one kept.
    #[must_use]
    pub fn within(mut self, at: Location) -> Self {
        self.at.get_or_insert(at);
        self
    }

    /// Applied twice, both lines survive.
    #[must_use]
    pub fn help(mut self, help: impl Into<String>) -> Self {
        let line = help.into();
        self.help = Some(match self.help {
            Some(existing) => format!("{existing}\n{line}"),
            None => line,
        });
        self
    }

    /// A `String` rather than a boxed error, because every consumer renders it and none
    /// inspects it. Keeping the error would buy an `io::ErrorKind` nothing reads.
    #[must_use]
    pub fn caused_by(mut self, source: impl fmt::Display) -> Self {
        self.source = Some(source.to_string());
        self
    }

    pub fn severity(&self) -> Severity {
        self.code.severity()
    }
}

/// One shape for every diagnostic, wherever it is emitted. Hand-written because severity
/// is derived from the code rather than stored.
impl serde::Serialize for Diagnostic {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct as _;

        let mut shape = serializer.serialize_struct("Diagnostic", 6)?;
        shape.serialize_field("code", self.code.slug())?;
        shape.serialize_field("severity", &self.severity())?;
        shape.serialize_field("at", &self.at)?;
        shape.serialize_field("message", &self.message)?;
        shape.serialize_field("help", &self.help)?;
        shape.serialize_field("source", &self.source)?;
        shape.end()
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Error for Diagnostic {}

/// A run's findings, plus whatever it still managed to produce.
///
/// `T` is `()` for phases that produce nothing but findings.
#[derive(Debug)]
pub struct Outcome<T> {
    pub value: Option<T>,
    pub diagnostics: Vec<Diagnostic>,
}

impl<T> Outcome<T> {
    pub fn new(value: Option<T>, mut diagnostics: Vec<Diagnostic>) -> Self {
        sort(&mut diagnostics);
        Self { value, diagnostics }
    }
}

/// Severity, then code, then message, so two runs print identically.
pub fn sort(diagnostics: &mut [Diagnostic]) {
    diagnostics.sort_by(|a, b| {
        b.severity()
            .cmp(&a.severity())
            .then_with(|| a.code.slug().cmp(b.code.slug()))
            .then_with(|| a.message.cmp(&b.message))
    });
}

pub fn worst(diagnostics: &[Diagnostic]) -> Option<Severity> {
    diagnostics.iter().map(Diagnostic::severity).max()
}

/// Errors and warnings, from one walk.
pub fn tally(diagnostics: &[Diagnostic]) -> (usize, usize) {
    diagnostics
        .iter()
        .fold((0, 0), |(errors, warnings), diagnostic| {
            match diagnostic.severity() {
                Severity::Error => (errors + 1, warnings),
                Severity::Warning => (errors, warnings + 1),
            }
        })
}

/// One per failure class. `main` returns this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    Success = 0,
    /// A task failed, or a `--frozen` build did not reproduce.
    Build = 1,
    /// The manifest could not be loaded or resolved.
    Config = 2,
    /// Linting failed.
    Lint = 3,
}

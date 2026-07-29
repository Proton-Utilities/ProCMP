//! Every value the plan took from outside the manifest's own bytes.
//!
//! Recording an impure read is what makes it safe. A manifest may ask for the clock and
//! still produce a reproducible build, because the answer it got is written down and
//! `--frozen` gives it the same one.
//!
//! The ledger is deliberately *not* a cache key. A resolved task is the result of
//! applying these readings, so a reading that changed nothing in a task cannot have
//! changed its output. Hashing the ledger would rebuild every task whenever any
//! environment variable moved.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::report::{Code, Diagnostic};
use crate::vfs::{Digest, RelPath};

/// One value taken from outside.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Reading {
    Env(String),
    Clock,
    File(RelPath),
    Override(String),
}

/// What a reading yielded.
///
/// A file records its digest rather than its contents.
///
/// A lock is committed and read in review, which a file's bytes would ruin. On replay the
/// right behaviour is to read the file and check it against the digest, because
/// resurrecting a stale copy would hide the very change the check exists to find.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum Recorded {
    Value(String),
    Digest(Digest),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Ledger(BTreeMap<Reading, Recorded>);

impl Reading {
    /// The ledger is serialised as a JSON object, whose keys are strings.
    pub fn key(&self) -> String {
        match self {
            Self::Env(name) => format!("env:{name}"),
            Self::Clock => "clock".to_owned(),
            Self::File(path) => format!("file:{path}"),
            Self::Override(name) => format!("override:{name}"),
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        if key == "clock" {
            return Some(Self::Clock);
        }
        let (tag, rest) = key.split_once(':')?;
        match tag {
            "env" => Some(Self::Env(rest.to_owned())),
            "file" => RelPath::new(rest).ok().map(Self::File),
            "override" => Some(Self::Override(rest.to_owned())),
            _ => None,
        }
    }

    /// Whether this reading makes a build irreproducible unless it is written down.
    fn ambient(&self) -> bool {
        matches!(self, Self::Env(_) | Self::Clock)
    }
}

impl Ledger {
    pub fn get(&self, reading: &Reading) -> Option<&Recorded> {
        self.0.get(reading)
    }

    pub fn insert(&mut self, reading: Reading, recorded: Recorded) {
        self.0.insert(reading, recorded);
    }

    /// Whether anything here would vary between two runs if it were not recorded.
    pub fn has_ambient(&self) -> bool {
        self.0.keys().any(Reading::ambient)
    }
}

impl serde::Serialize for Ledger {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let flat: BTreeMap<String, &Recorded> = self
            .0
            .iter()
            .map(|(key, value)| (key.key(), value))
            .collect();
        flat.serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for Ledger {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let flat = BTreeMap::<String, Recorded>::deserialize(deserializer)?;
        Ok(Self(
            flat.into_iter()
                .filter_map(|(key, value)| Reading::parse(&key).map(|read| (read, value)))
                .collect(),
        ))
    }
}

/// The one channel between the outside world and a manifest.
///
/// Every method both answers and records. In frozen mode it answers from a recorded
/// ledger instead, so a manifest that asks for the clock gets the timestamp the lock
/// pinned.
#[derive(Debug)]
pub struct Reader {
    supplied: BTreeMap<String, String>,
    pinned: Option<String>,
    frozen: Option<Ledger>,
    seen: RefCell<Ledger>,
}

impl Reader {
    /// `supplied` are `--env` pairs, which are consulted ahead of the process
    /// environment. `pinned` is `--now`, falling back to `SOURCE_DATE_EPOCH`.
    ///
    /// A pin is checked by round-tripping it, so `--now` cannot put an instant into a lock
    /// that `pcmp.now()` and `pcmp.epoch()` would then disagree about.
    pub fn new(
        supplied: BTreeMap<String, String>,
        pinned: Option<String>,
    ) -> Result<Self, Diagnostic> {
        if let Some(pinned) = &pinned
            && epoch_of(pinned).map(rfc3339).as_deref() != Some(pinned.as_str())
        {
            return Err(Diagnostic::new(
                Code::BadArgument,
                format!("`{pinned}` is not an RFC 3339 instant"),
            )
            .help("write it as 2026-07-29T00:00:00Z, in UTC and to the second"));
        }

        Ok(Self {
            supplied,
            pinned: pinned.or_else(source_date_epoch),
            frozen: None,
            seen: RefCell::new(Ledger::default()),
        })
    }

    /// Answers from `ledger` rather than from the outside, so a build reproduces what
    /// the ledger describes.
    #[must_use]
    pub fn frozen(mut self, ledger: Ledger) -> Self {
        self.frozen = Some(ledger);
        self
    }

    pub fn env(&self, name: &str) -> Option<String> {
        let reading = Reading::Env(name.to_owned());

        let value = match self.recall(&reading) {
            Some(recalled) => Some(recalled),
            None => self
                .supplied
                .get(name)
                .cloned()
                .or_else(|| std::env::var(name).ok()),
        }?;

        self.note(reading, Recorded::Value(value.clone()));
        Some(value)
    }

    /// An RFC 3339 instant in UTC, to the second.
    pub fn now(&self) -> String {
        let value = self
            .recall(&Reading::Clock)
            .or_else(|| self.pinned.clone())
            .unwrap_or_else(|| rfc3339(unix_now()));

        self.note(Reading::Clock, Recorded::Value(value.clone()));
        value
    }

    /// Seconds since the Unix epoch, consistent with [`Self::now`] within a run.
    pub fn epoch(&self) -> u64 {
        epoch_of(&self.now()).unwrap_or_default()
    }

    /// Records that a file was read, by digest. Returns whether a frozen ledger expected
    /// a different one.
    pub fn file(&self, path: &RelPath, digest: Digest) -> Result<(), Diagnostic> {
        let reading = Reading::File(path.clone());

        if let Some(Recorded::Digest(expected)) =
            self.frozen.as_ref().and_then(|ledger| ledger.get(&reading))
            && *expected != digest
        {
            return Err(Diagnostic::new(
                Code::Frozen,
                format!("`{path}` differs from what pcmp.lock records"),
            )
            .help("the lock describes a build made from different sources"));
        }

        self.note(reading, Recorded::Digest(digest));
        Ok(())
    }

    /// `--var` and `--define`, recorded so a lock describes the whole invocation.
    pub fn note_override(&self, name: &str, value: &str) {
        self.note(
            Reading::Override(name.to_owned()),
            Recorded::Value(value.to_owned()),
        );
    }

    /// What this run actually read.
    pub fn ledger(&self) -> Ledger {
        self.seen.borrow().clone()
    }

    fn recall(&self, reading: &Reading) -> Option<String> {
        match self.frozen.as_ref()?.get(reading) {
            Some(Recorded::Value(value)) => Some(value.clone()),
            _ => None,
        }
    }

    fn note(&self, reading: Reading, recorded: Recorded) {
        self.seen.borrow_mut().insert(reading, recorded);
    }
}

/// The reproducible-builds convention, so `pcmp` honours the same pin every other tool
/// in a release pipeline does.
fn source_date_epoch() -> Option<String> {
    let seconds = std::env::var("SOURCE_DATE_EPOCH").ok()?.parse().ok()?;
    Some(rfc3339(seconds))
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default()
}

fn epoch_of(rendered: &str) -> Option<u64> {
    let (date, rest) = rendered.split_once('T')?;
    let mut parts = date.split('-').map(str::parse::<u64>);
    let (year, month, day) = (
        parts.next()?.ok()?,
        parts.next()?.ok()?,
        parts.next()?.ok()?,
    );

    let mut clock = rest.trim_end_matches('Z').split(':').map(str::parse::<u64>);
    let (hour, minute, second) = (
        clock.next()?.ok()?,
        clock.next()?.ok()?,
        clock.next()?.ok()?,
    );

    Some(days_from_civil(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second)
}

fn rfc3339(seconds: u64) -> String {
    let (year, month, day) = civil_from_days(seconds / 86_400);
    let clock = seconds % 86_400;
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        clock / 3_600,
        (clock % 3_600) / 60,
        clock % 60
    )
}

/// Howard Hinnant's civil-calendar algorithm, in unsigned arithmetic because `pcmp`
/// has no use for instants before 1970.
fn civil_from_days(days: u64) -> (u64, u64, u64) {
    let shifted = days + 719_468;
    let era = shifted / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };

    (if month <= 2 { year + 1 } else { year }, month, day)
}

fn days_from_civil(year: u64, month: u64, day: u64) -> u64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year / 400;
    let year_of_era = year - era * 400;
    let shifted_month = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;

    era * 146_097 + day_of_era - 719_468
}

impl fmt::Display for Reading {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.key())
    }
}

//! Choosing which tasks to act on.
//!
//! A selector is a profile name or an exact task identifier, and `--axis KEY=VALUE`
//! filters an expansion by coordinate. There is no wildcard, because a task identifier
//! contains `[`, `]`, `=` and `,`, which every glob dialect reads as syntax.

use std::collections::BTreeMap;

use crate::plan::Plan;
use crate::report::{Code, Diagnostic};

/// Everything when nothing is named. An explicit selection that matches nothing is an
/// error rather than a quiet success.
pub fn select(
    plan: &Plan,
    selectors: &[String],
    axes: &BTreeMap<String, String>,
) -> Result<Plan, Diagnostic> {
    let tasks: Vec<_> = plan
        .tasks
        .iter()
        .filter(|task| {
            selectors.is_empty()
                || selectors.iter().any(|selector| {
                    selector == task.id.as_str() || selector == task.profile.as_str()
                })
        })
        .filter(|task| {
            axes.iter().all(|(axis, value)| {
                task.axes
                    .iter()
                    .any(|(name, chosen)| name.as_str() == axis && chosen == value)
            })
        })
        .cloned()
        .collect();

    if tasks.is_empty() {
        let named = if selectors.is_empty() {
            axes.iter()
                .map(|(axis, value)| format!("{axis}={value}"))
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            selectors.join(", ")
        };

        return Err(
            Diagnostic::new(Code::NoSuchTask, format!("nothing matched `{named}`"))
                .help(format!("known tasks: {}", plan.known())),
        );
    }

    Ok(Plan { tasks })
}

/// `KEY=VALUE`, repeatable, last occurrence winning.
pub fn pairs(arguments: &[String]) -> Result<BTreeMap<String, String>, Diagnostic> {
    arguments
        .iter()
        .map(|argument| {
            argument
                .split_once('=')
                .map(|(key, value)| (key.to_owned(), value.to_owned()))
                .ok_or_else(|| {
                    Diagnostic::new(Code::BadArgument, format!("`{argument}` is not KEY=VALUE"))
                        .help("write it as `name=value`, with no space around the `=`")
                })
        })
        .collect()
}

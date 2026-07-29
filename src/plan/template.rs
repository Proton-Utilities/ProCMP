//! `{token}` expansion for `entry`, `output` and `header`.

use std::collections::BTreeMap;

use crate::manifest::{Ident, Scalar};
use crate::report::{Code, Diagnostic};
use crate::vfs::RelPath;

/// Whether a substituted value becomes path syntax or stays text.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    Text,
    Path,
}

/// `{{` and `}}` are literal braces. An unknown, empty or unclosed token is an error
/// rather than an empty string: a silently missing path segment is far harder to notice
/// than a refused build.
pub fn expand(template: &str, tokens: &BTreeMap<Ident, Scalar>) -> Result<String, Diagnostic> {
    walk(template, tokens, Shape::Text)
}

/// Expansion into a path, where a value is read as path syntax rather than as text.
///
/// A `.` or `..` inside a value moves the artifact somewhere the template does not
/// describe. `dist/{profile}/app.luau` under a profile named `..` writes `app.luau`,
/// outside `dist/` and possibly outside the project, and the template still reads as
/// though it wrote into `dist/`. A plain `/` is left alone, because `{outdir}/app.luau`
/// is a template worth writing.
pub fn expand_path(
    template: &str,
    tokens: &BTreeMap<Ident, Scalar>,
) -> Result<RelPath, Diagnostic> {
    RelPath::new(walk(template, tokens, Shape::Path)?)
}

fn walk(
    template: &str,
    tokens: &BTreeMap<Ident, Scalar>,
    shape: Shape,
) -> Result<String, Diagnostic> {
    let bytes = template.as_bytes();
    let mut out = String::with_capacity(template.len());
    let mut at = 0;

    while at < bytes.len() {
        match bytes.get(at) {
            Some(b'{') if bytes.get(at + 1) == Some(&b'{') => {
                out.push('{');
                at += 2;
            }
            Some(b'}') if bytes.get(at + 1) == Some(&b'}') => {
                out.push('}');
                at += 2;
            }
            Some(b'{') => {
                let start = at + 1;
                let end = template
                    .get(start..)
                    .and_then(|rest| rest.find('}'))
                    .map(|offset| start + offset)
                    .ok_or_else(|| {
                        Diagnostic::new(
                            Code::BadTemplate,
                            format!("`{template}` leaves a `{{` unclosed"),
                        )
                        .help("write `{{` for a literal brace")
                    })?;

                let name = template.get(start..end).unwrap_or_default();
                let value = tokens
                    .iter()
                    .find(|(token, _)| token.as_str() == name)
                    .map(|(_, value)| value)
                    .ok_or_else(|| unknown(name, tokens))?;

                let text = value.text();
                if text.is_empty() {
                    return Err(Diagnostic::new(
                        Code::BadTemplate,
                        format!("token `{{{name}}}` has no value"),
                    )
                    .help("set it explicitly on the profile"));
                }

                if shape == Shape::Path
                    && let Some(segment) =
                        text.split(['/', '\\']).find(|s| matches!(*s, "." | ".."))
                {
                    return Err(Diagnostic::new(
                        Code::BadTemplate,
                        format!("token `{{{name}}}` expands to `{text}`, which walks the tree"),
                    )
                    .help(format!(
                        "a `{segment}` here moves the file out of the directory `{template}` names"
                    )));
                }

                out.push_str(&text);
                at = end + 1;
            }
            // An unpaired `}` is not ambiguous the way `{` is.
            Some(b'}') => {
                out.push('}');
                at += 1;
            }
            _ => {
                let start = at;
                while at < bytes.len() && !matches!(bytes.get(at), Some(b'{' | b'}')) {
                    at += 1;
                }
                out.push_str(template.get(start..at).unwrap_or_default());
            }
        }
    }

    Ok(out)
}

fn unknown(name: &str, tokens: &BTreeMap<Ident, Scalar>) -> Diagnostic {
    let known = tokens
        .keys()
        .map(|token| format!("{{{token}}}"))
        .collect::<Vec<_>>()
        .join(", ");

    Diagnostic::new(Code::BadTemplate, format!("unknown token `{{{name}}}`")).help(format!(
        "known tokens: {}",
        if known.is_empty() { "<none>" } else { &known }
    ))
}

//! Handing darklua exactly what the manifest declared, and nothing else.
//!
//! The virtual filesystem mirrors the real absolute layout, because darklua resolves a
//! require by doing parent and prefix arithmetic on paths. Mirroring is what makes the
//! in-memory build produce the same bytes as a filesystem one, what makes
//! `sources: ["../shared"]` work without a translation table, and what keeps darklua's own
//! error messages truthful.
//!
//! Absolute paths therefore exist here and nowhere else in the crate.
//!
//! Staging is what makes hermeticity a property rather than a promise: a file that is not
//! staged cannot be opened, so an undeclared dependency is a named error instead of a
//! silent influence on the output.

use std::collections::BTreeMap;

use darklua_core::Resources;

use crate::report::{Code, Diagnostic};
use crate::vfs::{AbsPath, RelPath};

/// The staged input set for one task.
///
/// Each task gets its own, never a clone of another's: `Resources` is `Clone` and its
/// memory variant holds an `Arc<Mutex<_>>`, so a clone would share one store and let one
/// task's output become another's input.
pub fn inputs(
    root: &AbsPath,
    contents: &BTreeMap<RelPath, Vec<u8>>,
) -> Result<Resources, Diagnostic> {
    let resources = Resources::from_memory();

    for (path, bytes) in contents {
        let absolute = root.join(path.as_str())?;
        resources
            .write_bytes(absolute.as_std(), bytes)
            // `ResourceError` implements neither `Display` nor `Error`, so there is
            // nothing to attach beyond the path, and an in-memory write has one way to
            // fail, which the path already names.
            .map_err(|_| Diagnostic::new(Code::WriteFailed, format!("could not stage `{path}`")))?;
    }

    Ok(resources)
}

/// Everything the task produced, named relative to the manifest.
///
/// Reading the artifacts back out of memory is what lets headers be composed, digests be
/// taken and stale files be spotted before anything touches the disk, so a failed task
/// leaves the previous artifact exactly as it was.
pub fn outputs(
    resources: &Resources,
    root: &AbsPath,
    output: &AbsPath,
) -> BTreeMap<RelPath, Vec<u8>> {
    let mut produced = BTreeMap::new();

    for path in resources.walk(output.as_std()) {
        let Some(text) = path.to_str().and_then(|text| AbsPath::new(text).ok()) else {
            continue;
        };
        let Some(relative) = text.relative_to(root) else {
            continue;
        };
        let Ok(bytes) = resources.get_bytes(&path) else {
            continue;
        };

        produced.insert(relative, bytes);
    }

    produced
}

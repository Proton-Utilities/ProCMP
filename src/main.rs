//! `pcmp`, multi-target build composition for Luau projects.
//!
//! One crate, not a library and a binary: nothing outside this tree uses it, and a
//! separate library target would mean every item it touches has to be `pub`, which is
//! exactly the arrangement in which the compiler cannot tell you what is unused.
//!
//! Four stages, in order: [`manifest`] is what the user wrote, [`plan`] is what will be
//! built, [`build`] does it, and [`report`] is how any of them says something went wrong.
//! [`vfs`] is what a file *is* to all of them: a name, an identity, and the one place
//! that touches a disk.

mod build;
mod cli;
mod manifest;
mod plan;
mod report;
mod vfs;

/// The darklua this crate links against.
///
/// Bump it with the `=` requirement in `Cargo.toml`, which is what decides the version and
/// cannot resolve to another. It is part of every cache key, because a darklua patch
/// release can change emitted bytes.
pub const DARKLUA: &str = "0.19.0";

use clap::Parser;

fn main() -> std::process::ExitCode {
    let cli = cli::Cli::parse();
    let code = match cli::run(&cli) {
        Ok(code) => code,
        Err(failure) => cli::fail(&cli, &failure),
    };

    std::process::ExitCode::from(code as u8)
}

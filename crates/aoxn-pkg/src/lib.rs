//! Aoxn package manager.
//!
//! Entry point: [`run`], called by the `aoxn` binary for its package-related
//! subcommands (`aoxn pkg ...` and the direct aliases `aoxn add`, ...).

mod audit;
mod cache;
mod cache_cmd;
mod cli;
mod context;
mod errors;
mod install;
mod lockfile;
mod manage;
mod manifest;
mod npm;
mod outdated;
mod publish;
mod registry;
mod resolve;
mod tarball;
mod tree;
mod ui;
mod workspace;


/// Run a package-manager subcommand. `args` is the argument list *without*
/// the dispatching token (the caller strips `pkg` / `add` / ... so the first
/// element here is always the subcommand itself, e.g. `["add", "http"]`).
/// Returns the process exit code.
pub fn run(args: &[String]) -> i32 {
    match cli::dispatch(args) {
        Ok(code) => code,
        Err(e) => {
            errors::report(&e);
            1
        }
    }
}

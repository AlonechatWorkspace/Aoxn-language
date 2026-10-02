//! Command-line surface (clap). The `aoxn` binary forwards its package
//! subcommands here with the dispatch token stripped.

use clap::Parser as _;

use crate::errors::PkgError;

#[derive(clap::Parser, Debug)]
#[command(
    name = "aoxn-pkg",
    about = "Aoxn package manager",
    disable_help_subcommand = true,
    after_help = "Run `aoxn <command> --help` for per-command flags."
)]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Cmd,

    /// show what would happen without changing anything
    #[arg(long, global = true)]
    pub dry_run: bool,
    /// print the full derivation on resolution conflicts
    #[arg(long, global = true)]
    pub explain: bool,
    /// work only from the local cache; never touch the network
    #[arg(long, global = true)]
    pub offline: bool,
    /// disable colored output (also automatic when not a TTY, or NO_COLOR set)
    #[arg(long, global = true)]
    pub no_color: bool,
    /// machine-readable output where supported
    #[arg(long, global = true)]
    pub json: bool,
    /// suppress progress chatter
    #[arg(long, global = true)]
    pub quiet: bool,
}

#[derive(clap::Subcommand, Debug)]
pub enum Cmd {
    /// create aoxn.json (and a hello-world main) in the current directory
    Init {
        /// default registry URL to record in the manifest
        #[arg(long)]
        registry: Option<String>,
        /// package name (default: current directory name)
        #[arg(long)]
        name: Option<String>,
    },
    /// add registry dependency(ies): <name>[@<req>] (req default: ^latest)
    Add {
        /// packages to add, e.g. `http@^2` or `json`
        pkgs: Vec<String>,
    },
    /// remove dependency(ies) from the manifest
    Remove {
        /// package names to remove
        pkgs: Vec<String>,
    },
    /// install everything per aoxn.json / aoxn.lock
    Install {
        /// re-resolve even when the lockfile matches the manifests
        #[arg(long)]
        force: bool,
        /// CI mode: fail when the lockfile does not exactly satisfy the
        /// manifests instead of re-resolving (no lockfile updates ever)
        #[arg(long)]
        frozen: bool,
    },
    /// upgrade dependencies within their constraints (--latest crosses majors)
    Update {
        /// specific packages to update (default: all)
        pkgs: Vec<String>,
        /// bump the manifest constraint to the newest version
        #[arg(long)]
        latest: bool,
    },
    /// list outdated dependencies
    Outdated,
    /// print the installed dependency tree
    Tree {
        /// limit the depth shown (default: unlimited)
        #[arg(long)]
        depth: Option<usize>,
    },
    /// show why a package is installed (all requirement paths)
    Why { pkg: String },
    /// publish this package to the registry (idempotent, tags `pkg/<name>/v<ver>`)
    Publish {
        /// publish even with uncommitted changes in the package's git repo
        #[arg(long)]
        allow_dirty: bool,
    },
    /// mark a published version as yanked (fresh resolution refuses it;
    /// existing lockfiles keep working). Undo with --undo.
    Yank {
        pkg: String,
        version: String,
        #[arg(long)]
        undo: bool,
    },
    /// check the lockfile against the advisory database
    Audit,
    /// remove dependencies and clean up aox_modules
    Uninstall {
        /// package names to remove
        pkgs: Vec<String>,
    },
    /// build all workspace members in dependency order
    Build,
    /// import an Aoxn package published to an npm-compatible registry
    /// (one-shot bridge; the npm CLI is the transport)
    NpmImport {
        /// npm specs to import: `my-pkg`, `my-pkg@1.2.0`, `@scope/pkg@^1`
        specs: Vec<String>,
        /// import every dependency of an npm package.json that is an Aoxn package
        #[arg(long)]
        from: Option<String>,
    },
    /// manage the global cache
    Cache {
        #[command(subcommand)]
        sub: CacheCmd,
    },
}

#[derive(clap::Subcommand, Debug)]
pub enum CacheCmd {
    /// print the cache directory
    Dir,
    /// delete cached tarballs not referenced by any aoxn.lock under the cwd
    Gc,
    /// drop cached registry snapshots (re-fetched on demand)
    Prune,
    /// delete the entire cache
    Clean,
    /// print total cache size
    Size,
}

/// Parse and run. Returns the process exit code (help/version/errors from
/// clap print themselves and exit via `e.exit()`).
pub fn dispatch(args: &[String]) -> Result<i32, PkgError> {
    // clap treats argv[0] as the program name; the caller strips the
    // dispatch token (`pkg`/`add`/...) so prepend a synthetic one
    let argv = std::iter::once("aoxn".to_string())
        .chain(args.iter().cloned())
        .collect::<Vec<_>>();
    let cli = match Cli::try_parse_from(argv) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };
    crate::ui::init(cli.no_color, cli.json, cli.quiet);
    run_command(cli)
}

fn run_command(cli: Cli) -> Result<i32, PkgError> {
    let explain = cli.explain;
    let res: Result<i32, PkgError> = match cli.cmd {
        Cmd::Init { registry, name } => crate::manage::init(registry, name),
        Cmd::Add { pkgs } => crate::manage::add(&pkgs, cli.dry_run, cli.offline),
        Cmd::Remove { pkgs } => crate::manage::remove(&pkgs, cli.dry_run, cli.offline),
        Cmd::Uninstall { pkgs } => crate::manage::remove(&pkgs, cli.dry_run, cli.offline),
        Cmd::Install { force, frozen } => {
            crate::manage::install_cmd(force, frozen, cli.dry_run, cli.offline)
        }
        Cmd::Update { pkgs, latest } => crate::manage::update(&pkgs, latest, cli.dry_run, cli.offline),
        Cmd::Outdated => crate::outdated::run(cli.offline, cli.json),
        Cmd::Tree { depth } => crate::tree::run_tree(depth, cli.json),
        Cmd::Why { pkg } => crate::tree::run_why(&pkg, cli.json),
        Cmd::Publish { allow_dirty } => crate::publish::run(allow_dirty, cli.dry_run),
        Cmd::Yank { pkg, version, undo } => crate::publish::yank(&pkg, &version, undo, cli.dry_run),
        Cmd::Audit => crate::audit::run(),
        Cmd::Build => crate::publish::build(),
        Cmd::NpmImport { specs, from } => crate::npm::import(&specs, from.as_deref(), cli.dry_run),
        Cmd::Cache { sub } => crate::cache_cmd::run(sub),
    };
    match res {
        Ok(code) => Ok(code),
        Err(e) => {
            if explain {
                if let Some(detail) = e.explain() {
                    eprintln!();
                    eprintln!("{}", crate::ui::style_hint("── explain ─────────────────────────"));
                    eprintln!("{detail}");
                }
            }
            crate::errors::report(&e);
            Ok(1)
        }
    }
}

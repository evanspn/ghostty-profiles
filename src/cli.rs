//! Command line: no arguments opens the TUI, subcommands do one thing and exit.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};

use crate::ghostty::{self, Reloader, SignalReloader};
use crate::paths::Paths;
use crate::store::{INCLUDE_LINE, Store};
use crate::tui;

#[derive(Parser)]
#[command(
    name = "ghostty-profiles",
    version,
    about = "Browse Ghostty themes and save/switch complete look profiles, with hot reload"
)]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// List profiles (● marks the active one)
    List,
    /// Make a profile the active look and reload Ghostty
    Apply {
        name: String,
        /// Write the config but do not signal Ghostty
        #[arg(long)]
        no_reload: bool,
    },
    /// Create a profile (a copy of --from, or empty)
    New {
        name: String,
        #[arg(long)]
        from: Option<String>,
    },
    /// Move the appearance settings of your current Ghostty config into a new profile
    Adopt { name: String },
    /// Write a portable copy of a profile (background images are left out unless --with-images)
    Export {
        name: String,
        /// Destination folder (default: ./NAME)
        dest: Option<PathBuf>,
        #[arg(long)]
        with_images: bool,
        /// Replace the destination if it exists
        #[arg(long)]
        force: bool,
    },
    /// Import a profile folder made by `export`
    Import {
        path: PathBuf,
        #[arg(long)]
        name: Option<String>,
    },
    /// Install the bundled preset profiles
    InstallPresets {
        /// Overwrite presets that are already installed
        #[arg(long)]
        force: bool,
    },
    /// Ask a running Ghostty to reload its config
    Reload,
    /// Show where things are and whether the setup is healthy
    #[command(visible_alias = "doctor")]
    Status,
    /// Remove the include line this tool added to your Ghostty config
    Unlink,
}

pub fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let store = Store::new(Paths::from_env()?);
    let Some(cmd) = cli.command else {
        return tui::run(store, Box::new(SignalReloader));
    };
    match cmd {
        Cmd::List => {
            let active = store.active_name();
            let names = store.list_profiles();
            if names.is_empty() {
                println!("no profiles yet; run `ghostty-profiles install-presets` or `ghostty-profiles adopt NAME`");
            }
            for n in names {
                let mark = if active.as_deref() == Some(n.as_str()) { "●" } else { " " };
                let desc = store.load(&n).map(|p| p.description()).unwrap_or_default();
                println!("{mark} {n:<16} {desc}");
            }
        }
        Cmd::Apply { name, no_reload } => {
            let conf = store.apply(&name)?;
            println!("applied '{name}' ({})", conf.display());
            if !no_reload {
                println!("{}", SignalReloader.reload().detail);
            }
        }
        Cmd::New { name, from } => {
            let p = store.new_profile(&name, from.as_deref())?;
            println!("created '{}' in {}", p.name, p.dir.display());
        }
        Cmd::Adopt { name } => {
            let (p, notes) = store.adopt(&name)?;
            for n in notes {
                println!("{n}");
            }
            println!("created profile '{}'. Apply it with: ghostty-profiles apply {}", p.name, p.name);
        }
        Cmd::Export { name, dest, with_images, force } => {
            let dest = dest.unwrap_or_else(|| PathBuf::from(&name));
            let notes = store.load(&name)?.export(&dest, with_images, force)?;
            for n in notes {
                println!("note: {n}");
            }
            println!("exported '{name}' to {}", dest.display());
        }
        Cmd::Import { path, name } => {
            let n = store.import_profile(&path, name.as_deref())?;
            println!("imported as '{n}'");
        }
        Cmd::InstallPresets { force } => {
            let v = store.install_presets(force)?;
            if v.is_empty() {
                println!("presets are already installed (use --force to overwrite them)");
            } else {
                println!("installed: {}", v.join(", "));
            }
        }
        Cmd::Reload => {
            let r = SignalReloader.reload();
            println!("{}", r.detail);
            if !r.ok {
                bail!("reload did not happen");
            }
        }
        Cmd::Status => status(&store),
        Cmd::Unlink => {
            if store.unlink()? {
                println!("removed the include line; your active look is no longer loaded");
            } else {
                println!("nothing to remove");
            }
        }
    }
    Ok(())
}

fn status(store: &Store) {
    println!("profiles folder : {}", store.profiles_dir().display());
    println!("profiles        : {}", store.list_profiles().len());
    println!("active profile  : {}", store.active_name().unwrap_or_else(|| "none".into()));
    println!("active config   : {}", store.paths.active_conf().display());
    println!("linked          : {} ({INCLUDE_LINE})", if store.is_linked() { "yes" } else { "no" });
    let pids = ghostty::ghostty_pids();
    println!(
        "ghostty running : {}",
        if pids.is_empty() {
            "no".to_string()
        } else {
            format!("yes ({} process{})", pids.len(), if pids.len() > 1 { "es" } else { "" })
        }
    );
    println!(
        "ghostty binary  : {}",
        ghostty::ghostty_exe().map(|p| p.display().to_string()).unwrap_or_else(|| "not found".into())
    );
    let (ok, text) = ghostty::validate_config();
    println!("config valid    : {}", if ok { "yes" } else { "NO" });
    if !ok && !text.is_empty() {
        println!("{text}");
    }
}

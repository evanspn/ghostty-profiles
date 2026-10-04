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
    /// Tune a profile's shader: `gpf shader set PROFILE SHADER NAME VALUE`, `preset`, `reset`, `show`
    Shader {
        #[command(subcommand)]
        action: ShaderCmd,
    },
    /// Tidy profile folders: a profile keeps ONE shader, so unused generated shader files go (backed up first)
    Prune {
        /// Only this profile (default: all)
        profile: Option<String>,
        /// Show what would be removed, change nothing
        #[arg(long)]
        dry_run: bool,
        /// Also remove parameter files with changed values that belong to shaders that are not the profile's shader
        #[arg(long)]
        drop_orphan_params: bool,
    },
    /// Rename a profile (its images and shaders move with it; the active profile stays active)
    Rename {
        old: String,
        new: String,
        /// Do not signal Ghostty (when the renamed profile is the active one)
        #[arg(long)]
        no_reload: bool,
    },
    /// Delete a profile and everything in its folder (images and shaders too)
    #[command(visible_alias = "rm")]
    Delete {
        name: String,
        /// Do not ask for confirmation
        #[arg(long, short = 'y')]
        yes: bool,
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
    /// Turn the active look off: Ghostty keeps your own config only (the include stays, nothing is applied)
    #[command(visible_alias = "none")]
    Off {
        /// Write the config but do not signal Ghostty
        #[arg(long)]
        no_reload: bool,
    },
    /// Remove the include line this tool added to your Ghostty config (stronger than `off`)
    Unlink,
}

#[derive(Subcommand)]
enum ShaderCmd {
    /// Make SHADER the profile's one shader (replacing the current one); `none` removes it
    Use {
        profile: String,
        /// A bundled shader name, a shader file in the profile, or `none`
        shader: String,
        #[arg(long)]
        no_reload: bool,
    },
    /// Set the profile-wide effects opacity (0..1), which multiplies the shader's own opacity
    Opacity {
        profile: String,
        value: f64,
        #[arg(long)]
        no_reload: bool,
    },
    /// Show a shader's parameters and current values
    Show { profile: String, shader: String },
    /// Set one parameter, e.g. `gpf shader set shd xmb-waves wave_a '#2fa8ff'`
    Set {
        profile: String,
        shader: String,
        name: String,
        value: String,
        /// Do not signal Ghostty (when the profile is the active one)
        #[arg(long)]
        no_reload: bool,
    },
    /// Apply one of the shader's named presets (e.g. ocean)
    Preset {
        profile: String,
        shader: String,
        preset: String,
        #[arg(long)]
        no_reload: bool,
    },
    /// Back to the shader's default values
    Reset {
        profile: String,
        shader: String,
        #[arg(long)]
        no_reload: bool,
    },
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
            let ignored = store.load(&name)?.ignored_keys();
            if !ignored.is_empty() {
                println!("note: not applied (not appearance settings): {}", ignored.join(", "));
            }
            if !no_reload {
                println!("{}", SignalReloader.reload().detail);
            }
        }
        Cmd::New { name, from } => {
            let p = store.new_profile(&name, from.as_deref())?;
            println!("created '{}' in {}", p.name, p.dir.display());
        }
        Cmd::Shader { action } => shader_cmd(&store, action)?,
        Cmd::Prune { profile, dry_run, drop_orphan_params } => {
            let names = match profile {
                Some(p) => vec![p],
                None => store.list_profiles(),
            };
            let mut total = 0;
            for name in names {
                let r = store.prune_profile(&name, dry_run, drop_orphan_params)?;
                if r.is_empty() && r.kept_user.is_empty() && r.kept_params.is_empty() {
                    continue;
                }
                println!("{name}:");
                for f in &r.removed {
                    println!("  {} {f}", if dry_run { "would remove" } else { "removed" });
                }
                for f in &r.dropped_extra_lines {
                    println!(
                        "  {} extra custom-shader line: {f} (a profile keeps one shader; the first stays)",
                        if dry_run { "would drop" } else { "dropped" }
                    );
                }
                for f in &r.kept_user {
                    println!("  kept (yours, not a copy of a bundled shader): {f}");
                }
                for f in &r.kept_params {
                    println!(
                        "  kept (changed values for a shader that is not in use): {f}  (--drop-orphan-params removes these)"
                    );
                }
                if let Some(b) = &r.backup {
                    println!("  backup: {}", b.display());
                }
                total += r.removed.len();
            }
            println!(
                "{}",
                if total == 0 {
                    "nothing to tidy"
                } else if dry_run {
                    "dry run: nothing was changed"
                } else {
                    "done"
                }
            );
        }
        Cmd::Rename { old, new, no_reload } => {
            let was_active = store.rename(&old, &new)?;
            println!("renamed '{old}' to '{new}'");
            if crate::presets::profile_names().contains(&old) {
                println!(
                    "note: '{old}' is a bundled preset; `ghostty-profiles install-presets` will install the original again"
                );
            }
            if was_active {
                println!("it is the active profile: the generated config now points at the new folder");
                if !no_reload {
                    println!("{}", SignalReloader.reload().detail);
                }
            }
        }
        Cmd::Delete { name, yes } => {
            if !store.exists(&name) {
                bail!("no profile named '{name}' (see `ghostty-profiles list`)");
            }
            if store.active_name().as_deref() == Some(name.as_str()) {
                bail!("'{name}' is the active profile; apply another one (or `unlink`) first");
            }
            if !yes && !confirm(&delete_prompt(&name))? {
                println!("not deleted");
                return Ok(());
            }
            store.delete(&name)?;
            println!("deleted '{name}'");
            if crate::presets::profile_names().contains(&name) {
                println!("(it is a bundled preset: `ghostty-profiles install-presets` brings it back)");
            }
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
            let (n, removed) = store.import_profile(&path, name.as_deref())?;
            println!("imported as '{n}'");
            if !removed.is_empty() {
                println!("removed settings that are not part of a look (never applied): {}", removed.join(", "));
            }
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
        Cmd::Off { no_reload } => {
            if store.deactivate()? {
                println!("no profile is active now; your own Ghostty config is in effect (the include line stays)");
            } else {
                println!("no profile was active");
            }
            if !no_reload {
                println!("{}", SignalReloader.reload().detail);
            }
        }
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

fn shader_rel(shader: &str) -> String {
    let name = shader.strip_suffix(".glsl").unwrap_or(shader);
    format!("shaders/{name}.glsl")
}

fn shader_cmd(store: &Store, action: ShaderCmd) -> Result<()> {
    let reload_if_active = |store: &Store, profile: &str, no_reload: bool| -> Result<()> {
        if store.active_name().as_deref() == Some(profile) {
            store.rerender_active()?;
            if !no_reload {
                println!("{}", SignalReloader.reload().detail);
            }
        }
        Ok(())
    };
    // changing a shader's values only makes sense for the profile's one active shader
    let require_active = |store: &Store, profile: &str, shader: &str| -> Result<()> {
        let p = store.load(profile)?;
        if p.active_shader().as_deref() != Some(shader_rel(shader).as_str()) {
            bail!(
                "'{shader}' is not the shader of '{profile}' (it uses {}); choose it first with `gpf shader use {profile} {shader}`",
                p.active_shader().unwrap_or_else(|| "no shader".into())
            );
        }
        Ok(())
    };
    match action {
        ShaderCmd::Use { profile, shader, no_reload } => {
            let mut p = store.load(&profile)?;
            if shader == "none" {
                p.clear_shader()?;
            } else {
                p.use_shader(&format!("{}.glsl", shader.strip_suffix(".glsl").unwrap_or(&shader)))?;
            }
            p.save()?;
            println!("'{profile}' now uses {}", if shader == "none" { "no shader".to_string() } else { shader });
            reload_if_active(store, &profile, no_reload)?;
        }
        ShaderCmd::Opacity { profile, value, no_reload } => {
            store.load(&profile)?.set_effects_opacity(value)?;
            println!("effects opacity of '{profile}' is now {}", crate::shaderparams::format_number(value));
            reload_if_active(store, &profile, no_reload)?;
        }
        ShaderCmd::Show { profile, shader } => {
            require_active(store, &profile, &shader)?;
            let p = store.load(&profile)?;
            let rel = shader_rel(&shader);
            let Some(st) = p.shader_param_state(&rel) else { bail!("'{shader}' has no tunable parameters") };
            if let Some(m) = st.schema.motion {
                println!("motion: {}{}", m.name(), if st.schema.coverage_full { " (spans the screen)" } else { "" });
            }
            println!("effects opacity (profile-wide): {}", crate::shaderparams::format_number(p.effects_opacity()));
            for (param, value) in st.schema.params.iter().zip(&st.values) {
                println!("{:<14} {value:<9} {}", param.name, param.label);
            }
            if !st.schema.presets.is_empty() {
                println!(
                    "presets: {}",
                    st.schema.presets.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ")
                );
            }
        }
        ShaderCmd::Set { profile, shader, name, value, no_reload } => {
            require_active(store, &profile, &shader)?;
            store.load(&profile)?.set_shader_params(&shader_rel(&shader), &[(name.clone(), value)])?;
            println!("set {name} on {shader} in '{profile}'");
            reload_if_active(store, &profile, no_reload)?;
        }
        ShaderCmd::Preset { profile, shader, preset, no_reload } => {
            require_active(store, &profile, &shader)?;
            store.load(&profile)?.apply_shader_preset(&shader_rel(&shader), &preset)?;
            println!("applied preset '{preset}' to {shader} in '{profile}'");
            reload_if_active(store, &profile, no_reload)?;
        }
        ShaderCmd::Reset { profile, shader, no_reload } => {
            require_active(store, &profile, &shader)?;
            store.load(&profile)?.reset_shader_params(&shader_rel(&shader))?;
            println!("reset {shader} in '{profile}' to its defaults");
            reload_if_active(store, &profile, no_reload)?;
        }
    }
    Ok(())
}

/// The question asked before deleting; shared wording with the TUI.
fn delete_prompt(name: &str) -> String {
    format!("Delete profile '{name}' and its images and shaders?{} [y/N] ", crate::store::delete_note(name))
}

fn confirm(prompt: &str) -> Result<bool> {
    use std::io::{BufRead, Write};
    print!("{prompt}");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    Ok(matches!(line.trim(), "y" | "Y" | "yes" | "YES"))
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

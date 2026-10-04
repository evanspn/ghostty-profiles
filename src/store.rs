//! Profile storage, activation, and adopting an existing Ghostty config into a profile.
//!
//! The only file this module ever adds to Ghostty's own config is ONE include
//! line, `config-file = ?ghostty-profiles-active.conf` (the `?` makes a missing
//! file harmless). Everything else lives in the profiles folder.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::confparse;
use crate::paths::Paths;
use crate::presets;
use crate::profile::{ASSET_KEYS, AssetKind, CONF_NAME, Profile, SHADER_KEY, is_appearance_key, is_contained_relative};

pub const INCLUDE_LINE: &str = "config-file = ?ghostty-profiles-active.conf";
const MANAGED_COMMENT: &str = "# Managed by ghostty-profiles";
pub const BACKUP_SUFFIX: &str = ".bak-pre-ghostty-profiles";

const LIST_KEYS: [&str; 6] = [
    "font-family",
    "font-family-bold",
    "font-family-italic",
    "font-family-bold-italic",
    "font-feature",
    "custom-shader",
];

fn group(key: &str) -> usize {
    if matches!(key, "background" | "foreground" | "palette" | "theme" | "minimum-contrast" | "bold-color")
        || key.starts_with("cursor-")
        || key.starts_with("selection-")
    {
        0
    } else if key.starts_with("font-") || key.starts_with("adjust-") {
        1
    } else if key.starts_with("background-image") {
        3
    } else if key.starts_with("custom-shader") {
        4
    } else {
        2
    }
}

pub fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        && name.len() <= 64
}

pub(crate) fn atomic_write(path: &Path, text: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, text).with_context(|| format!("writing {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}

/// Recursive copy that skips symlinks (a shared profile must not be able to pull in other files).
fn copy_dir(src: &Path, dest: &Path) -> Result<()> {
    fs::create_dir_all(dest)?;
    for e in fs::read_dir(src)? {
        let e = e?;
        let ty = e.file_type()?;
        if ty.is_symlink() {
            continue;
        }
        let to = dest.join(e.file_name());
        if ty.is_dir() {
            copy_dir(&e.path(), &to)?;
        } else {
            fs::copy(e.path(), &to)?;
        }
    }
    Ok(())
}

pub struct Store {
    pub paths: Paths,
}

impl Store {
    pub fn new(paths: Paths) -> Self {
        Self { paths }
    }

    pub fn profiles_dir(&self) -> PathBuf {
        self.paths.profiles_dir()
    }

    // ---- profiles -----------------------------------------------------------
    pub fn list_profiles(&self) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(self.profiles_dir())
            .map(|rd| {
                rd.flatten()
                    .filter(|e| e.path().join(CONF_NAME).is_file())
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        v.sort();
        v
    }

    pub fn exists(&self, name: &str) -> bool {
        valid_name(name) && self.profiles_dir().join(name).join(CONF_NAME).is_file()
    }

    pub fn load(&self, name: &str) -> Result<Profile> {
        if !valid_name(name) {
            bail!("invalid profile name '{name}'");
        }
        if !self.exists(name) {
            bail!("no profile named '{name}' (see `ghostty-profiles list`)");
        }
        Profile::load(&self.profiles_dir().join(name))
    }

    pub fn active_name(&self) -> Option<String> {
        let n = fs::read_to_string(self.paths.active_file()).ok()?.trim().to_string();
        (!n.is_empty() && self.exists(&n)).then_some(n)
    }

    /// A new profile; a copy of `from` when given, else empty.
    pub fn new_profile(&self, name: &str, from: Option<&str>) -> Result<Profile> {
        if !valid_name(name) {
            bail!("name must be letters, digits, '.', '_' or '-'");
        }
        if self.exists(name) {
            bail!("profile '{name}' already exists");
        }
        let dest = self.profiles_dir().join(name);
        let mut prof = match from {
            Some(src) => {
                if !self.exists(src) {
                    bail!("no profile named '{src}'");
                }
                copy_dir(&self.profiles_dir().join(src), &dest)?;
                let mut p = Profile::load(&dest)?;
                p.set_description(&format!("copy of {src}"));
                p
            }
            None => {
                let mut p = Profile::new(name, &dest);
                p.set_description("new profile");
                p
            }
        };
        prof.name = name.to_string();
        prof.save()?;
        Ok(prof)
    }

    pub fn delete(&self, name: &str) -> Result<()> {
        if !self.exists(name) {
            bail!("no profile named '{name}'");
        }
        if self.active_name().as_deref() == Some(name) {
            bail!("can't delete the active profile; switch to another first");
        }
        fs::remove_dir_all(self.profiles_dir().join(name))?;
        Ok(())
    }

    /// Import a profile folder (e.g. one made by `export`).
    ///
    /// Refuses assets that point outside the folder, and STRIPS every setting that is not an
    /// appearance setting (`command`, `keybind`, `config-file`, ...): a shared profile may change how
    /// Ghostty looks, never what it runs. Returns the name and the keys that were removed.
    pub fn import_profile(&self, src: &Path, name: Option<&str>) -> Result<(String, Vec<String>)> {
        if !src.join(CONF_NAME).is_file() {
            bail!("{} has no {CONF_NAME}", src.display());
        }
        let name = match name {
            Some(n) => n.to_string(),
            None => src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        };
        if !valid_name(&name) {
            bail!("'{name}' is not a valid profile name; pass --name");
        }
        if self.exists(&name) {
            bail!("profile '{name}' already exists");
        }
        let incoming = Profile::load(src)?;
        for l in &incoming.lines {
            if let (Some(k), Some(v)) = (l.key(), l.value())
                && ASSET_KEYS.contains(&k)
                && !is_contained_relative(v)
            {
                bail!("refusing to import: {k} = {v} points outside the profile folder");
            }
        }
        let dest = self.profiles_dir().join(&name);
        copy_dir(src, &dest)?;
        let mut copy = Profile::load(&dest)?;
        let removed = copy.ignored_keys();
        copy.lines.retain(|l| l.key().is_none_or(is_appearance_key));
        copy.save()?;
        Ok((name, removed))
    }

    // ---- presets --------------------------------------------------------------
    /// Install the bundled profiles; existing ones are kept unless `force`. Returns the names written.
    pub fn install_presets(&self, force: bool) -> Result<Vec<String>> {
        let mut installed = Vec::new();
        for name in presets::profile_names() {
            let dest = self.profiles_dir().join(&name);
            if dest.exists() {
                if !force {
                    continue;
                }
                fs::remove_dir_all(&dest)?;
            }
            presets::install_profile(&name, &dest)?;
            installed.push(name);
        }
        Ok(installed)
    }

    // ---- activation -------------------------------------------------------------
    pub fn apply(&self, name: &str) -> Result<PathBuf> {
        let prof = self.load(name)?;
        let conf = self.paths.active_conf();
        atomic_write(&conf, &prof.render())?;
        atomic_write(&self.paths.active_file(), name)?;
        self.ensure_linked()?;
        Ok(conf)
    }

    /// Re-render the active profile after an edit. False if nothing is active.
    pub fn rerender_active(&self) -> Result<bool> {
        match self.active_name() {
            Some(n) => {
                atomic_write(&self.paths.active_conf(), &self.load(&n)?.render())?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    fn main_config(&self) -> PathBuf {
        ["config.ghostty", "config"]
            .iter()
            .map(|n| self.paths.ghostty_dir().join(n))
            .find(|p| p.exists())
            .unwrap_or_else(|| self.paths.ghostty_dir().join("config.ghostty"))
    }

    fn config_files(&self) -> [PathBuf; 2] {
        [self.paths.ghostty_dir().join("config.ghostty"), self.paths.ghostty_dir().join("config")]
    }

    pub fn is_linked(&self) -> bool {
        self.config_files()
            .iter()
            .filter_map(|f| fs::read_to_string(f).ok())
            .any(|t| t.lines().any(|l| l.trim() == INCLUDE_LINE))
    }

    fn backup(&self, path: &Path) -> Result<()> {
        let mut b = path.as_os_str().to_owned();
        b.push(BACKUP_SUFFIX);
        let b = PathBuf::from(b);
        if path.exists() && !b.exists() {
            fs::copy(path, b)?;
        }
        Ok(())
    }

    /// Add the one include line to the main config (idempotent, backed up once). True if it changed.
    pub fn ensure_linked(&self) -> Result<bool> {
        if self.is_linked() {
            return Ok(false);
        }
        let main = self.main_config();
        self.backup(&main)?;
        let mut text = fs::read_to_string(&main).unwrap_or_default();
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&format!("\n{MANAGED_COMMENT}: the active look is loaded from this file.\n{INCLUDE_LINE}\n"));
        atomic_write(&main, &text)?;
        Ok(true)
    }

    /// Remove the include line (and its comment) again. True if anything changed.
    pub fn unlink(&self) -> Result<bool> {
        let mut changed = false;
        for f in self.config_files() {
            let Ok(text) = fs::read_to_string(&f) else { continue };
            let kept: Vec<&str> =
                text.lines().filter(|l| l.trim() != INCLUDE_LINE && !l.starts_with(MANAGED_COMMENT)).collect();
            if kept.len() != text.lines().count() {
                atomic_write(&f, &(kept.join("\n").trim_end().to_string() + "\n"))?;
                changed = true;
            }
        }
        Ok(changed)
    }

    // ---- adopt an existing config as a profile --------------------------------------
    /// Move the appearance keys out of the main Ghostty config files into a new profile.
    ///
    /// Non-appearance settings stay where they are. Referenced shaders and
    /// background images are COPIED into the profile (originals untouched), and
    /// the config files are backed up once. Returns the profile and notes.
    pub fn adopt(&self, name: &str) -> Result<(Profile, Vec<String>)> {
        if !valid_name(name) {
            bail!("invalid profile name '{name}'");
        }
        if self.exists(name) {
            bail!("profile '{name}' already exists");
        }
        let mut notes = Vec::new();
        let mut scalars: Vec<(String, String)> = Vec::new();
        let mut lists: Vec<(String, String)> = Vec::new();
        let mut palette: std::collections::BTreeMap<u8, String> = Default::default();

        let files = [self.paths.ghostty_dir().join("config"), self.paths.ghostty_dir().join("config.ghostty")];
        for f in files.iter().filter(|f| f.is_file()) {
            let lines = confparse::parse(&fs::read_to_string(f)?);
            let (mut kept, mut moved) = (Vec::new(), 0);
            for l in lines {
                match (l.key(), l.value()) {
                    (Some(k), Some(v)) if is_appearance_key(k) => {
                        moved += 1;
                        if k == "palette" {
                            if let Some((i, c)) = v.split_once('=')
                                && let Ok(n) = i.trim().parse::<u8>()
                            {
                                palette.insert(n, c.trim().to_string());
                            }
                        } else if LIST_KEYS.contains(&k) {
                            lists.push((k.to_string(), v.to_string()));
                        } else if let Some(slot) = scalars.iter_mut().find(|(sk, _)| sk == k) {
                            slot.1 = v.to_string();
                        } else {
                            scalars.push((k.to_string(), v.to_string()));
                        }
                    }
                    _ => kept.push(l),
                }
            }
            if moved > 0 {
                self.backup(f)?;
                atomic_write(f, &confparse::dump(&kept))?;
                let fname = f.file_name().unwrap_or_default().to_string_lossy();
                notes.push(format!("moved appearance settings out of {fname} (backup: {fname}{BACKUP_SUFFIX})"));
            }
        }

        let dir = self.profiles_dir().join(name);
        let mut prof = Profile::new(name, &dir);
        prof.set_description("adopted from your Ghostty config");
        let mut entries: Vec<(String, String)> = scalars;
        entries.extend(lists);
        entries.extend(palette.iter().map(|(i, c)| ("palette".to_string(), format!("{i}={c}"))));
        entries.sort_by_key(|(k, _)| group(k));
        for (k, v) in entries {
            prof.lines.push(confparse::entry(&k, &v));
        }
        // make assets self-contained inside the profile
        for i in 0..prof.lines.len() {
            let (Some(k), Some(v)) =
                (prof.lines[i].key().map(str::to_string), prof.lines[i].value().map(str::to_string))
            else {
                continue;
            };
            if !ASSET_KEYS.contains(&k.as_str()) {
                continue;
            }
            let src = prof.resolve_asset(&v);
            if src.is_file() {
                let kind = if k == SHADER_KEY { AssetKind::Shader } else { AssetKind::Image };
                let rel = prof.add_asset(kind, &src, None)?;
                prof.lines[i] = confparse::entry(&k, &rel);
                notes
                    .push(format!("copied {} into the profile", src.file_name().unwrap_or_default().to_string_lossy()));
            } else {
                notes.push(format!("asset not found, left as-is: {v}"));
            }
        }
        prof.save()?;
        Ok((prof, notes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Sandbox {
        _td: tempfile::TempDir,
        store: Store,
    }

    fn sandbox() -> Sandbox {
        let td = tempfile::tempdir().unwrap();
        let store = Store::new(Paths::new(td.path().join("config")));
        Sandbox { _td: td, store }
    }

    fn write_ghostty(sb: &Sandbox, file: &str, text: &str) {
        let d = sb.store.paths.ghostty_dir();
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join(file), text).unwrap();
    }

    fn read_ghostty(sb: &Sandbox, file: &str) -> String {
        fs::read_to_string(sb.store.paths.ghostty_dir().join(file)).unwrap()
    }

    #[test]
    fn names_are_validated() {
        for ok in ["shd", "a.b-c_d", "0x"] {
            assert!(valid_name(ok), "{ok}");
        }
        for bad in ["", ".hidden", "-x", "a/b", "a b", "..", &"x".repeat(65)] {
            assert!(!valid_name(bad), "{bad}");
        }
    }

    #[test]
    fn new_copy_delete_and_active_protection() {
        let sb = sandbox();
        let s = &sb.store;
        s.install_presets(false).unwrap();
        let p = s.new_profile("mine", Some("shd")).unwrap();
        assert_eq!(p.description(), "copy of shd");
        assert!(s.exists("mine") && s.list_profiles().contains(&"mine".to_string()));
        assert!(s.new_profile("mine", None).is_err());
        assert!(s.new_profile("../evil", None).is_err());
        s.apply("mine").unwrap();
        assert!(s.delete("mine").is_err(), "the active profile cannot be deleted");
        s.apply("shd").unwrap();
        s.delete("mine").unwrap();
        assert!(!s.exists("mine"));
    }

    #[test]
    fn ensure_linked_is_idempotent_and_backs_up_once() {
        let sb = sandbox();
        let s = &sb.store;
        write_ghostty(&sb, "config", "font-size = 12\n");
        s.install_presets(false).unwrap();
        s.apply("shd").unwrap();
        let after_first = read_ghostty(&sb, "config");
        assert_eq!(after_first.matches(INCLUDE_LINE).count(), 1);
        assert!(after_first.starts_with("font-size = 12\n"));
        assert_eq!(read_ghostty(&sb, "config.bak-pre-ghostty-profiles"), "font-size = 12\n");
        s.apply("shd").unwrap();
        s.apply("calm-dark").unwrap();
        assert_eq!(read_ghostty(&sb, "config"), after_first, "re-applying must not touch the main config again");
        assert!(s.is_linked());
        // unlink removes exactly what we added
        assert!(s.unlink().unwrap());
        assert_eq!(read_ghostty(&sb, "config").trim_end(), "font-size = 12");
        assert!(!s.is_linked());
        assert!(!s.unlink().unwrap());
    }

    #[test]
    fn applying_or_loading_a_missing_profile_fails_and_writes_nothing() {
        let sb = sandbox();
        let s = &sb.store;
        assert!(s.apply("nope").is_err());
        assert!(s.load("nope").is_err());
        assert!(s.apply("../escape").is_err());
        assert!(!s.paths.active_conf().exists(), "no empty active config was written");
        assert!(!s.paths.active_file().exists());
        assert!(!s.paths.ghostty_dir().exists(), "and Ghostty's config dir was not touched");
    }

    #[test]
    fn apply_writes_the_active_conf_with_absolute_assets() {
        let sb = sandbox();
        let s = &sb.store;
        s.install_presets(false).unwrap();
        let conf = s.apply("shd").unwrap();
        let text = fs::read_to_string(conf).unwrap();
        assert!(
            text.contains(&format!(
                "custom-shader = {}",
                s.profiles_dir().join("shd/shaders/xmb-waves.glsl").display()
            ))
        );
        assert_eq!(s.active_name().as_deref(), Some("shd"));
        assert!(s.rerender_active().unwrap());
    }

    #[test]
    fn every_preset_loads_renders_and_has_its_shaders() {
        let sb = sandbox();
        let s = &sb.store;
        let names = presets::profile_names();
        assert!(names.len() >= 4, "{names:?}");
        assert!(names.contains(&"shd".to_string()));
        s.install_presets(false).unwrap();
        for n in &names {
            let p = s.load(n).unwrap();
            assert!(!p.description().is_empty(), "{n} has no description");
            for k in ["background", "foreground"] {
                assert!(p.color(k).is_some(), "{n}: {k}");
            }
            assert_eq!(p.palette().len(), 16, "{n} must set the full 16-color palette");
            let rendered = p.render();
            assert!(rendered.contains("background = "), "{n}");
            for sh in p.shaders() {
                assert!(is_contained_relative(&sh), "{n}: {sh}");
                let f = p.resolve_asset(&sh);
                let src = fs::read_to_string(&f).unwrap_or_else(|_| panic!("{n}: missing {sh}"));
                assert!(src.contains("mainImage"), "{n}: {sh} is not a Ghostty shader");
            }
            assert!(p.image().is_none(), "{n}: presets ship NO background image");
            assert!(!rendered.contains("background-image"), "{n}");
            assert!(!rendered.contains(concat!("/Us", "ers/")), "{n}: a home path leaked into a preset");
        }
        assert!(s.install_presets(false).unwrap().is_empty(), "second install keeps what exists");
        assert_eq!(s.install_presets(true).unwrap().len(), names.len());
    }

    #[test]
    fn shd_matches_the_operators_look_minus_the_image() {
        let sb = sandbox();
        sb.store.install_presets(false).unwrap();
        let p = sb.store.load("shd").unwrap();
        assert_eq!(p.color("background").as_deref(), Some("#2c2c2c"));
        assert_eq!(p.color("foreground").as_deref(), Some("#d8d8d8"));
        assert_eq!(p.color("cursor-color").as_deref(), Some("#ff6a00"));
        assert_eq!(p.get("background-opacity").as_deref(), Some("0.5"));
        assert_eq!(p.get("background-blur").as_deref(), Some("25"));
        assert_eq!(p.shaders(), vec!["shaders/xmb-waves.glsl"]);
        assert_eq!(p.get("custom-shader-animation").as_deref(), Some("true"));
        assert_eq!(p.palette()[&3], "#ff6a00");
    }

    #[test]
    fn adopt_moves_only_appearance_keys_and_backs_up() {
        let sb = sandbox();
        let s = &sb.store;
        let shader = sb._td.path().join("wave.glsl");
        fs::write(&shader, "void mainImage(){}").unwrap();
        write_ghostty(
            &sb,
            "config",
            "foreground = #111111\nkeybind = alt+t=toggle_quick_terminal\nfont-family = Menlo\nfont-family = Symbols\n",
        );
        write_ghostty(
            &sb,
            "config.ghostty",
            &format!(
                "background = #2c2c2c\nforeground = #d8d8d8\nshell-integration = zsh\npalette = 0=#0a0a0a\ncustom-shader = {}\nwindow-padding-x = 8\nkeybind = ctrl+shift+r=reload_config\n",
                shader.display()
            ),
        );
        let (p, notes) = s.adopt("mine").unwrap();
        assert_eq!(p.get("foreground").as_deref(), Some("#d8d8d8"), "config.ghostty wins over config");
        assert_eq!(p.get_all("font-family"), vec!["Menlo", "Symbols"]);
        assert_eq!(p.get("window-padding-x").as_deref(), Some("8"));
        assert_eq!(p.shaders(), vec!["shaders/wave.glsl"]);
        assert!(p.dir.join("shaders/wave.glsl").is_file());
        assert!(shader.is_file(), "the original shader is untouched");
        let main = read_ghostty(&sb, "config");
        assert_eq!(main, "keybind = alt+t=toggle_quick_terminal\n");
        let second = read_ghostty(&sb, "config.ghostty");
        assert_eq!(second, "shell-integration = zsh\nkeybind = ctrl+shift+r=reload_config\n");
        assert!(read_ghostty(&sb, "config.bak-pre-ghostty-profiles").contains("font-family = Menlo"));
        assert!(read_ghostty(&sb, "config.ghostty.bak-pre-ghostty-profiles").contains("background = #2c2c2c"));
        assert!(notes.iter().any(|n| n.contains("backup")));
        assert!(s.adopt("mine").is_err());
        // the profile is complete enough to be applied, and nothing was applied by adopting
        assert_eq!(s.active_name(), None);
    }

    #[test]
    fn import_rejects_profiles_that_point_outside_their_folder() {
        let sb = sandbox();
        let s = &sb.store;
        let bad = sb._td.path().join("bad");
        fs::create_dir_all(&bad).unwrap();
        for evil in
            ["custom-shader = ../../secret.glsl", "background-image = /etc/hosts", "custom-shader = shaders/../../x"]
        {
            fs::write(bad.join(CONF_NAME), format!("{evil}\n")).unwrap();
            assert!(s.import_profile(&bad, None).is_err(), "{evil}");
        }
        fs::write(bad.join(CONF_NAME), "background = #000000\n").unwrap();
        assert_eq!(s.import_profile(&bad, Some("fine")).unwrap().0, "fine");
        assert!(s.import_profile(&bad, Some("fine")).is_err());
    }

    #[test]
    fn import_strips_everything_that_could_run_or_rebind_and_says_so() {
        let sb = sandbox();
        let s = &sb.store;
        let shared = sb._td.path().join("shared");
        fs::create_dir_all(&shared).unwrap();
        fs::write(
            shared.join(CONF_NAME),
            "# description: nice look\nbackground = #101010\ncommand = /bin/sh -c 'open -a Calculator'\ninitial-command = whoami\nkeybind = ctrl+shift+x=text:evil\nconfig-file = /tmp/evil\nfont-size = 13\n",
        )
        .unwrap();
        let (name, removed) = s.import_profile(&shared, Some("theirs")).unwrap();
        assert_eq!(name, "theirs");
        assert_eq!(removed, vec!["command", "config-file", "initial-command", "keybind"]);
        let on_disk = fs::read_to_string(s.profiles_dir().join("theirs").join(CONF_NAME)).unwrap();
        for bad in ["command", "keybind", "config-file", "evil", "Calculator"] {
            assert!(!on_disk.contains(bad), "{bad} survived import:\n{on_disk}");
        }
        assert!(on_disk.contains("background = #101010") && on_disk.contains("font-size = 13"));
        // and applying it writes none of that into the active Ghostty config either
        s.apply("theirs").unwrap();
        let active = fs::read_to_string(s.paths.active_conf()).unwrap();
        for bad in ["command", "keybind", "evil"] {
            assert!(!active.contains(bad), "{bad} reached the active conf:\n{active}");
        }
        // even a hand-edited profile cannot smuggle them past render
        let mut p = s.load("theirs").unwrap();
        p.set("keybind", "ctrl+a=text:evil");
        p.set("command", "evil");
        p.save().unwrap();
        s.apply("theirs").unwrap();
        let active = fs::read_to_string(s.paths.active_conf()).unwrap();
        assert!(!active.contains("keybind") && !active.contains("command"), "{active}");
    }

    #[test]
    fn export_then_import_round_trips_a_preset() {
        let sb = sandbox();
        let s = &sb.store;
        s.install_presets(false).unwrap();
        let dest = sb._td.path().join("shd-export");
        s.load("shd").unwrap().export(&dest, false, false).unwrap();
        assert!(!dest.join("images").exists());
        assert_eq!(s.import_profile(&dest, Some("shd2")).unwrap(), ("shd2".to_string(), vec![]));
        assert_eq!(s.load("shd2").unwrap().shaders(), vec!["shaders/xmb-waves.glsl"]);
    }
}

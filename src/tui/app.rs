//! TUI state and behavior. No drawing and no terminal here: keys go in through
//! [`App::on_key`], and everything observable (files on disk, the reload hook)
//! can be asserted in tests.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::Result;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::fields::{self, Field, Kind};
use crate::ghostty::{self, Reloader, Theme};
use crate::presets;
use crate::profile::{AssetKind, Profile};
use crate::store::{Store, atomic_write, valid_name};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Profiles,
    Themes,
    Edit,
    Shaders,
}

impl Tab {
    pub const ALL: [Tab; 4] = [Tab::Profiles, Tab::Themes, Tab::Edit, Tab::Shaders];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Profiles => "Profiles",
            Tab::Themes => "Themes",
            Tab::Edit => "Edit",
            Tab::Shaders => "Shaders",
        }
    }

    fn index(self) -> usize {
        Tab::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum InputKind {
    Field(usize),
    /// Renaming the named profile.
    Rename(String),
    NewProfile,
    Export,
    Filter,
}

#[derive(Clone, Debug)]
pub struct Input {
    pub kind: InputKind,
    pub title: String,
    pub buf: String,
    pub error: Option<String>,
}

/// The rows above the profiles in the Profiles list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Top {
    /// "+ New profile".
    New,
    /// "(none)": no profile.
    None,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WizardStep {
    /// Choosing what the new profile starts from (0 current setup, 1 copy, 2 blank).
    Base(usize),
    /// Confirming that the current Ghostty setup will be moved into the profile.
    ConfirmAdopt,
}

/// New-profile flow after the name has been typed: pick a base, then create.
#[derive(Clone, Debug)]
pub struct Wizard {
    pub name: String,
    pub step: WizardStep,
    /// The profile that was selected when the flow started (the "copy of" source).
    pub copy_from: Option<String>,
}

enum Base {
    CurrentSetup,
    Copy(String),
    Blank,
}

#[derive(Clone, Debug)]
pub struct ShaderRow {
    pub name: String,
    pub in_library: bool,
    pub enabled: bool,
}

#[derive(Clone, Debug)]
pub struct Status {
    pub text: String,
    pub ok: bool,
}

pub struct App {
    pub store: Store,
    reloader: Box<dyn Reloader>,
    pub debounce: Duration,
    reload_due: Option<Instant>,

    pub tab: Tab,
    pub quit: bool,
    pub status: Status,
    pub input: Option<Input>,
    pub confirm_delete: Option<String>,

    pub profiles: Vec<String>,
    pub sel: usize,
    /// The cursor is on a row above the profiles ("+ New profile" or "(none)"), not on a profile.
    pub top: Option<Top>,
    pub wizard: Option<Wizard>,
    pub profile: Option<Profile>,
    pub active: Option<String>,

    pub themes: Vec<Theme>,
    pub theme_filter: String,
    pub theme_view: Vec<usize>,
    pub theme_sel: usize,

    pub fields: Vec<Field>,
    pub field_sel: usize,

    pub shader_rows: Vec<ShaderRow>,
    pub shader_sel: usize,
}

impl App {
    pub fn new(store: Store, reloader: Box<dyn Reloader>) -> Result<Self> {
        let themes = ghostty::list_themes(&store.paths);
        let mut app = App {
            store,
            reloader,
            debounce: Duration::from_millis(250),
            reload_due: None,
            tab: Tab::Profiles,
            quit: false,
            status: Status { text: String::new(), ok: true },
            input: None,
            confirm_delete: None,
            profiles: Vec::new(),
            sel: 0,
            top: None,
            wizard: None,
            profile: None,
            active: None,
            themes,
            theme_filter: String::new(),
            theme_view: Vec::new(),
            theme_sel: 0,
            fields: fields::all_fields(),
            field_sel: 0,
            shader_rows: Vec::new(),
            shader_sel: 0,
        };
        if app.store.list_profiles().is_empty() {
            let n = app.store.install_presets(false)?.len();
            app.say(format!("first run: installed {n} preset profiles; Enter applies one"), true);
        }
        app.refresh_profiles(None);
        app.filter_themes();
        Ok(app)
    }

    fn say(&mut self, text: impl Into<String>, ok: bool) {
        self.status = Status { text: text.into(), ok };
    }

    pub fn current_name(&self) -> Option<&str> {
        if self.top.is_some() { None } else { self.profiles.get(self.sel).map(String::as_str) }
    }

    // ---- profile list ---------------------------------------------------------
    /// Reload the list; keep `keep` selected if given, else the previous or the active one.
    pub fn refresh_profiles(&mut self, keep: Option<&str>) {
        let previous = keep.map(str::to_string).or_else(|| self.current_name().map(str::to_string));
        self.top = None;
        self.profiles = self.store.list_profiles();
        self.active = self.store.active_name();
        let want = previous.or_else(|| self.active.clone());
        self.sel = want.and_then(|w| self.profiles.iter().position(|p| *p == w)).unwrap_or(0);
        self.load_selected();
    }

    fn load_selected(&mut self) {
        self.profile = self.current_name().and_then(|n| self.store.load(n).ok());
        self.rebuild_shader_rows();
    }

    // ---- saving and hot reload ------------------------------------------------------
    /// Save the profile; if it is the active one, re-render the active conf and schedule a reload.
    fn commit(&mut self, what: &str) {
        let Some(p) = &self.profile else { return };
        if let Err(e) = p.save() {
            self.say(format!("could not save: {e:#}"), false);
            return;
        }
        if self.active.as_deref() == Some(p.name.as_str()) {
            match self.store.rerender_active() {
                Ok(_) => {
                    self.reload_due = Some(Instant::now() + self.debounce);
                    self.say(format!("{what} · saved, reloading Ghostty"), true);
                }
                Err(e) => self.say(format!("saved, but could not update the active config: {e:#}"), false),
            }
        } else {
            self.say(format!("{what} · saved (not the active profile: Enter on Profiles to apply it)"), true);
        }
    }

    /// Fire a pending reload once its debounce has passed. Call this regularly.
    pub fn tick(&mut self, now: Instant) {
        if self.reload_due.is_some_and(|due| now >= due) {
            self.reload_due = None;
            self.reload_now();
        }
    }

    pub fn reload_pending(&self) -> bool {
        self.reload_due.is_some()
    }

    fn reload_now(&mut self) {
        let r = self.reloader.reload();
        self.say(r.detail, r.ok);
    }

    // ---- keys -----------------------------------------------------------------
    pub fn on_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        if self.input.is_some() {
            self.on_input_key(key);
            return;
        }
        if self.wizard.is_some() {
            self.wizard_key(key);
            return;
        }
        if let Some(name) = self.confirm_delete.take() {
            if matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y')) {
                self.delete_profile(&name);
            } else {
                self.say("delete cancelled", true);
            }
            return;
        }
        if ctrl && key.code == KeyCode::Char('r') {
            if let Err(e) = self.store.rerender_active() {
                self.say(format!("could not re-render: {e:#}"), false);
            }
            self.reload_due = None;
            self.reload_now();
            return;
        }
        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Tab => self.goto_tab(1),
            KeyCode::BackTab => self.goto_tab(-1),
            KeyCode::Char('1') => self.tab = Tab::Profiles,
            KeyCode::Char('2') => self.tab = Tab::Themes,
            KeyCode::Char('3') => self.tab = Tab::Edit,
            KeyCode::Char('4') => self.tab = Tab::Shaders,
            KeyCode::Char('n') => self.begin_new(),
            KeyCode::Char('r') => self.begin_rename(),
            KeyCode::Char('d') => self.begin_delete(),
            KeyCode::Char('e') => self.begin_export(),
            _ => match self.tab {
                Tab::Profiles => self.profiles_key(key),
                Tab::Themes => self.themes_key(key),
                Tab::Edit => self.edit_key(key),
                Tab::Shaders => self.shaders_key(key),
            },
        }
    }

    fn goto_tab(&mut self, by: isize) {
        let n = Tab::ALL.len() as isize;
        self.tab = Tab::ALL[((self.tab.index() as isize + by).rem_euclid(n)) as usize];
    }

    fn step(sel: &mut usize, len: usize, code: KeyCode) {
        if len == 0 {
            *sel = 0;
            return;
        }
        match code {
            KeyCode::Up | KeyCode::Char('k') => *sel = sel.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => *sel = (*sel + 1).min(len - 1),
            KeyCode::PageUp => *sel = sel.saturating_sub(10),
            KeyCode::PageDown => *sel = (*sel + 10).min(len - 1),
            KeyCode::Home => *sel = 0,
            KeyCode::End => *sel = len - 1,
            _ => {}
        }
    }

    // ---- Profiles tab -----------------------------------------------------------
    fn profiles_key(&mut self, key: KeyEvent) {
        let before = (self.sel, self.top);
        match key.code {
            // the rows above the list: "+ New profile", then "(none)", then the profiles
            KeyCode::Up | KeyCode::Char('k') => match self.top {
                Some(Top::New) => {}
                Some(Top::None) => self.top = Some(Top::New),
                None if self.sel == 0 => self.top = Some(Top::None),
                None => Self::step(&mut self.sel, self.profiles.len(), key.code),
            },
            KeyCode::Down | KeyCode::Char('j') => match self.top {
                Some(Top::New) => self.top = Some(Top::None),
                Some(Top::None) => {
                    self.top = None;
                    self.sel = 0;
                }
                None => Self::step(&mut self.sel, self.profiles.len(), key.code),
            },
            KeyCode::Home => self.top = Some(Top::New),
            KeyCode::PageUp | KeyCode::PageDown | KeyCode::End => {
                if self.top.is_some() {
                    self.top = None;
                    self.sel = if key.code == KeyCode::End { self.profiles.len().saturating_sub(1) } else { 0 };
                } else {
                    Self::step(&mut self.sel, self.profiles.len(), key.code);
                }
            }
            _ => {}
        }
        if (self.sel, self.top) != before {
            self.load_selected();
        }
        match key.code {
            KeyCode::Enter => match self.top {
                Some(Top::New) => self.begin_new(),
                Some(Top::None) => self.deactivate(),
                None => self.apply_selected(),
            },
            KeyCode::Char('u') => self.deactivate(),
            KeyCode::Char('p') => match self.store.install_presets(false) {
                Ok(v) if v.is_empty() => self.say("all preset profiles are already installed", true),
                Ok(v) => {
                    self.say(format!("installed presets: {}", v.join(", ")), true);
                    self.refresh_profiles(None);
                }
                Err(e) => self.say(format!("{e:#}"), false),
            },
            _ => {}
        }
    }

    /// Turn the active look off so the user's own Ghostty config shows through.
    pub fn deactivate(&mut self) {
        match self.store.deactivate() {
            Ok(was_active) => {
                self.active = None;
                self.reload_due = Some(Instant::now() + self.debounce);
                let text = if was_active {
                    "no profile active: your own Ghostty config is in effect, reloading Ghostty"
                } else {
                    "no profile was active"
                };
                self.say(text, true);
            }
            Err(e) => self.say(format!("could not turn the profile off: {e:#}"), false),
        }
    }

    pub fn apply_selected(&mut self) {
        let Some(name) = self.current_name().map(str::to_string) else { return };
        match self.store.apply(&name) {
            Ok(_) => {
                self.active = Some(name.clone());
                self.reload_due = Some(Instant::now() + self.debounce);
                self.say(format!("applied '{name}', reloading Ghostty"), true);
            }
            Err(e) => self.say(format!("could not apply: {e:#}"), false),
        }
    }

    /// Start the new-profile flow: ask for a name (the base is chosen next).
    pub fn begin_new(&mut self) {
        self.input = Some(Input {
            kind: InputKind::NewProfile,
            title: "New profile: name".to_string(),
            buf: String::new(),
            error: None,
        });
    }

    fn wizard_key(&mut self, key: KeyEvent) {
        let Some(w) = self.wizard.clone() else { return };
        match w.step {
            WizardStep::Base(sel) => match key.code {
                KeyCode::Esc => {
                    self.wizard = None;
                    self.say("new profile cancelled", true);
                }
                KeyCode::Up | KeyCode::Char('k') => self.set_wizard_step(WizardStep::Base(sel.saturating_sub(1))),
                KeyCode::Down | KeyCode::Char('j') => self.set_wizard_step(WizardStep::Base((sel + 1).min(2))),
                KeyCode::Char('1') => self.choose_base(0),
                KeyCode::Char('2') => self.choose_base(1),
                KeyCode::Char('3') => self.choose_base(2),
                KeyCode::Enter => self.choose_base(sel),
                _ => {}
            },
            WizardStep::ConfirmAdopt => match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => self.create_profile(Base::CurrentSetup),
                KeyCode::Esc => {
                    self.wizard = None;
                    self.say("new profile cancelled", true);
                }
                _ => self.set_wizard_step(WizardStep::Base(0)),
            },
        }
    }

    fn set_wizard_step(&mut self, step: WizardStep) {
        if let Some(w) = self.wizard.as_mut() {
            w.step = step;
        }
    }

    fn choose_base(&mut self, i: usize) {
        match i {
            0 => self.set_wizard_step(WizardStep::ConfirmAdopt),
            1 => match self.wizard.as_ref().and_then(|w| w.copy_from.clone()) {
                Some(src) => self.create_profile(Base::Copy(src)),
                None => self.say("there is no selected profile to copy: pick a profile first", false),
            },
            _ => self.create_profile(Base::Blank),
        }
    }

    /// Create the profile the wizard has been collecting. It is selected afterwards, never applied.
    fn create_profile(&mut self, base: Base) {
        let Some(w) = self.wizard.take() else { return };
        let name = w.name;
        let result = match &base {
            Base::CurrentSetup => self.store.adopt(&name).map(|(_, notes)| notes),
            Base::Copy(src) => self.store.new_profile(&name, Some(src)).map(|_| Vec::new()),
            Base::Blank => self.store.new_profile(&name, None).map(|_| Vec::new()),
        };
        match result {
            Ok(_) => {
                let from = match &base {
                    Base::CurrentSetup => "your current Ghostty setup (originals backed up)".to_string(),
                    Base::Copy(src) => format!("a copy of '{src}'"),
                    Base::Blank => "blank".to_string(),
                };
                self.refresh_profiles(Some(&name));
                self.say(format!("created '{name}' from {from}. Not applied yet: press Enter on it to apply"), true);
            }
            Err(e) => self.say(format!("could not create '{name}': {e:#}"), false),
        }
    }

    fn begin_rename(&mut self) {
        if let Some(n) = self.current_name().map(str::to_string) {
            self.input = Some(Input {
                kind: InputKind::Rename(n.clone()),
                title: format!("Rename '{n}' (Enter confirms, Esc cancels)"),
                buf: n,
                error: None,
            });
        } else {
            self.say("select a profile to rename", false);
        }
    }

    fn begin_delete(&mut self) {
        match self.current_name().map(str::to_string) {
            Some(n) if self.active.as_deref() == Some(n.as_str()) => {
                self.say("can't delete the active profile; apply another first", false)
            }
            Some(n) => {
                let note = crate::store::delete_note(&n);
                self.say(
                    format!("delete '{n}' and its images and shaders?{note} press y to confirm, any other key cancels"),
                    false,
                );
                self.confirm_delete = Some(n);
            }
            None => {}
        }
    }

    fn delete_profile(&mut self, name: &str) {
        match self.store.delete(name) {
            Ok(()) => {
                self.say(format!("deleted '{name}'"), true);
                self.sel = self.sel.saturating_sub(1);
                self.refresh_profiles(None);
            }
            Err(e) => self.say(format!("{e:#}"), false),
        }
    }

    fn begin_export(&mut self) {
        if let Some(n) = self.current_name().map(str::to_string) {
            self.input = Some(Input {
                kind: InputKind::Export,
                title: format!("Export '{n}' to folder (images are left out)"),
                buf: format!("./{n}"),
                error: None,
            });
        }
    }

    // ---- Themes tab ----------------------------------------------------------------
    pub fn filter_themes(&mut self) {
        let needle = self.theme_filter.to_lowercase();
        self.theme_view = self
            .themes
            .iter()
            .enumerate()
            .filter(|(_, t)| needle.is_empty() || t.name.to_lowercase().contains(&needle))
            .map(|(i, _)| i)
            .collect();
        self.theme_sel = self.theme_sel.min(self.theme_view.len().saturating_sub(1));
    }

    pub fn selected_theme(&self) -> Option<&Theme> {
        self.theme_view.get(self.theme_sel).and_then(|i| self.themes.get(*i))
    }

    fn themes_key(&mut self, key: KeyEvent) {
        Self::step(&mut self.theme_sel, self.theme_view.len(), key.code);
        match key.code {
            KeyCode::Char('/') => {
                self.input = Some(Input {
                    kind: InputKind::Filter,
                    title: "Filter themes".into(),
                    buf: self.theme_filter.clone(),
                    error: None,
                })
            }
            KeyCode::Esc => {
                self.theme_filter.clear();
                self.filter_themes();
            }
            KeyCode::Enter => self.bake_theme(),
            _ => {}
        }
    }

    /// Copy the selected theme's colors into the current profile (a baked copy, not a reference).
    pub fn bake_theme(&mut self) {
        let Some(theme) = self.selected_theme().cloned() else { return };
        let Some(p) = self.profile.as_mut() else {
            self.say("no profile selected: move off \"(none)\" on the Profiles tab to edit one", false);
            return;
        };
        let c = ghostty::theme_colors(&theme.text);
        for k in [
            "theme",
            "background",
            "foreground",
            "cursor-color",
            "cursor-text",
            "selection-background",
            "selection-foreground",
        ] {
            p.remove(k);
        }
        let pairs = [
            ("background", &c.background),
            ("foreground", &c.foreground),
            ("cursor-color", &c.cursor_color),
            ("cursor-text", &c.cursor_text),
            ("selection-background", &c.selection_background),
            ("selection-foreground", &c.selection_foreground),
        ];
        for (k, v) in pairs {
            if let Some(v) = v {
                p.set(k, v);
            }
        }
        if !c.palette.is_empty() {
            p.set_palette(&c.palette);
        }
        self.commit(&format!("baked theme '{}'", theme.name));
    }

    // ---- Edit tab -------------------------------------------------------------------
    pub fn field_value(&self, i: usize) -> Option<String> {
        let p = self.profile.as_ref()?;
        fields::current_value(p, self.fields.get(i)?)
    }

    fn edit_key(&mut self, key: KeyEvent) {
        Self::step(&mut self.field_sel, self.fields.len(), key.code);
        let Some(field) = self.fields.get(self.field_sel).copied() else { return };
        match key.code {
            KeyCode::Enter => match field.kind {
                Kind::Enum(_) => self.cycle_field(true),
                _ => {
                    let buf = self.field_value(self.field_sel).unwrap_or_default();
                    self.input = Some(Input {
                        kind: InputKind::Field(self.field_sel),
                        title: format!("{} ({})", field.label, hint(&field)),
                        buf,
                        error: None,
                    });
                }
            },
            KeyCode::Right | KeyCode::Char('l') => self.cycle_field(true),
            KeyCode::Left | KeyCode::Char('h') => self.cycle_field(false),
            KeyCode::Char('x') | KeyCode::Delete => {
                self.set_field(self.field_sel, "");
            }
            _ => {}
        }
    }

    fn cycle_field(&mut self, forward: bool) {
        let Some(field) = self.fields.get(self.field_sel).copied() else { return };
        let cur = self.field_value(self.field_sel);
        if let Some(next) = fields::cycle(&field, cur.as_deref(), forward) {
            self.set_field(self.field_sel, next);
        }
    }

    /// Validate and store a value for field `i`. An invalid value changes nothing and says why.
    pub fn set_field(&mut self, i: usize, text: &str) -> bool {
        let Some(field) = self.fields.get(i).copied() else { return false };
        let value = match fields::validate(&field, text) {
            Ok(v) => v,
            Err(msg) => {
                self.say(format!("{}: {msg}", field.label), false);
                return false;
            }
        };
        let Some(p) = self.profile.as_mut() else {
            self.say("no profile selected: move off \"(none)\" on the Profiles tab to edit one", false);
            return false;
        };
        let value = if field.kind == Kind::Image { value.map(|v| import_image(p, &v)).transpose() } else { Ok(value) };
        let value = match value {
            Ok(v) => v,
            Err(msg) => {
                self.say(format!("{}: {msg}", field.label), false);
                return false;
            }
        };
        fields::store_value(p, &field, value.as_deref());
        self.commit(field.label);
        true
    }

    // ---- Shaders tab ----------------------------------------------------------------
    pub fn rebuild_shader_rows(&mut self) {
        let enabled: Vec<String> = self.profile.as_ref().map(Profile::shaders).unwrap_or_default();
        let is_on = |name: &str| enabled.iter().any(|e| e == &format!("shaders/{name}"));
        let mut rows: Vec<ShaderRow> = presets::shader_names()
            .into_iter()
            .map(|name| ShaderRow { enabled: is_on(&name), name, in_library: true })
            .collect();
        if let Some(p) = &self.profile
            && let Ok(rd) = std::fs::read_dir(p.dir.join("shaders"))
        {
            let mut own: Vec<String> = rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
            own.sort();
            for name in own {
                if !rows.iter().any(|r| r.name == name) {
                    rows.push(ShaderRow { enabled: is_on(&name), name, in_library: false });
                }
            }
        }
        self.shader_sel = self.shader_sel.min(rows.len().saturating_sub(1));
        self.shader_rows = rows;
    }

    fn shaders_key(&mut self, key: KeyEvent) {
        Self::step(&mut self.shader_sel, self.shader_rows.len(), key.code);
        match key.code {
            KeyCode::Enter | KeyCode::Char(' ') => self.toggle_shader(),
            KeyCode::Char('a') => self.toggle_animation(),
            _ => {}
        }
    }

    pub fn toggle_shader(&mut self) {
        let Some(row) = self.shader_rows.get(self.shader_sel).cloned() else { return };
        let Some(p) = self.profile.as_mut() else {
            self.say("no profile selected: move off \"(none)\" on the Profiles tab to edit one", false);
            return;
        };
        let rel = format!("shaders/{}", row.name);
        let mut list = p.shaders();
        if row.enabled {
            list.retain(|s| *s != rel);
            p.set_shaders(&list);
        } else {
            let file = p.dir.join(&rel);
            if !file.exists() {
                match presets::shader_source(&row.name) {
                    Some(src) => {
                        if let Err(e) = atomic_write(&file, src) {
                            self.say(format!("could not copy shader: {e:#}"), false);
                            return;
                        }
                    }
                    None => {
                        self.say(format!("shader file missing: {rel}"), false);
                        return;
                    }
                }
            }
            let animated = std::fs::read_to_string(&file).is_ok_and(|s| s.contains("iTime"));
            if !list.contains(&rel) {
                list.push(rel);
            }
            p.set_shaders(&list);
            if animated && p.get("custom-shader-animation").is_none() {
                p.set("custom-shader-animation", "true");
            }
        }
        let what = format!("shader {} {}", row.name, if row.enabled { "off" } else { "on" });
        self.commit(&what);
        self.rebuild_shader_rows();
    }

    pub fn toggle_animation(&mut self) {
        let Some(p) = self.profile.as_mut() else { return };
        if p.shaders().is_empty() {
            self.say("no shader is enabled, so there is nothing to animate", false);
            return;
        }
        let now = p.get("custom-shader-animation").as_deref() == Some("true");
        p.set("custom-shader-animation", if now { "false" } else { "true" });
        self.commit(if now { "animation off" } else { "animation on" });
    }

    // ---- text input -----------------------------------------------------------------
    fn on_input_key(&mut self, key: KeyEvent) {
        let Some(input) = self.input.as_mut() else { return };
        match key.code {
            KeyCode::Esc => {
                if input.kind == InputKind::Filter {
                    self.theme_filter.clear();
                    self.filter_themes();
                }
                self.input = None;
            }
            KeyCode::Backspace => {
                input.buf.pop();
                input.error = None;
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => input.buf.clear(),
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                input.buf.push(c);
                input.error = None;
            }
            KeyCode::Enter => {
                let done = self.input.take().unwrap_or_else(|| unreachable!());
                self.finish_input(done);
                return;
            }
            _ => {}
        }
        if let Some(i) = &self.input
            && i.kind == InputKind::Filter
        {
            self.theme_filter = i.buf.clone();
            self.filter_themes();
        }
    }

    fn finish_input(&mut self, input: Input) {
        match input.kind {
            InputKind::Filter => {
                self.theme_filter = input.buf;
                self.filter_themes();
            }
            InputKind::Field(i) => {
                if !self.set_field(i, &input.buf) {
                    // keep the box open so the value can be corrected
                    let error = Some(self.status.text.clone());
                    self.input = Some(Input { error, ..input });
                }
            }
            InputKind::NewProfile => {
                let name = input.buf.trim().to_string();
                let problem = if name.is_empty() {
                    Some("type a name for the new profile".to_string())
                } else if !valid_name(&name) {
                    Some("name must be letters, digits, '.', '_' or '-' (no spaces or slashes)".to_string())
                } else if self.store.exists(&name) {
                    Some(format!("a profile named '{name}' already exists: pick another name"))
                } else {
                    None
                };
                if let Some(p) = problem {
                    self.say(p.clone(), false);
                    self.input = Some(Input { error: Some(p), ..input });
                    return;
                }
                let copy_from = self.current_name().map(str::to_string);
                self.wizard = Some(Wizard { name, step: WizardStep::Base(0), copy_from });
            }
            InputKind::Rename(ref old) => {
                let new = input.buf.trim().to_string();
                if new == *old {
                    self.say("name unchanged", true);
                    return;
                }
                match self.store.rename(old, &new) {
                    Ok(was_active) => {
                        let preset = if crate::presets::profile_names().contains(old) {
                            format!(" ('{old}' is a bundled preset: install-presets will bring the original back)")
                        } else {
                            String::new()
                        };
                        self.refresh_profiles(Some(&new));
                        if was_active {
                            self.reload_due = Some(Instant::now() + self.debounce);
                            self.say(format!("renamed '{old}' to '{new}', reloading Ghostty{preset}"), true);
                        } else {
                            self.say(format!("renamed '{old}' to '{new}'{preset}"), true);
                        }
                    }
                    Err(e) => {
                        let msg = format!("{e:#}");
                        self.say(msg.clone(), false);
                        self.input = Some(Input { error: Some(msg), ..input });
                    }
                }
            }
            InputKind::Export => {
                let Some(p) = self.profile.clone() else { return };
                let dest = expand(&input.buf);
                match p.export(&dest, false, false) {
                    Ok(notes) => {
                        let extra = if notes.is_empty() { String::new() } else { format!(" ({})", notes.join("; ")) };
                        self.say(format!("exported to {}{extra}", dest.display()), true);
                    }
                    Err(e) => {
                        self.say(format!("{e:#}"), false);
                        self.input = Some(Input { error: Some(self.status.text.clone()), ..input });
                    }
                }
            }
        }
    }
}

fn expand(s: &str) -> PathBuf {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("~/")
        && let Some(h) = std::env::var_os("HOME")
    {
        return Path::new(&h).join(rest);
    }
    PathBuf::from(s)
}

/// Copy the image the user typed a path for into the profile; returns the relative path to store.
fn import_image(p: &Profile, typed: &str) -> std::result::Result<String, String> {
    let src = expand(typed);
    if !src.is_file() {
        return Err(format!("no such file: {typed}"));
    }
    p.add_asset(AssetKind::Image, &src, None).map_err(|e| format!("{e:#}"))
}

fn hint(f: &Field) -> String {
    match f.kind {
        Kind::Hex | Kind::Palette(_) => "#rrggbb".into(),
        Kind::Text => "text".into(),
        Kind::List => "comma-separated".into(),
        Kind::Number { min, max } => format!("{min}..{max}"),
        Kind::Spacing => "2, -1 or 10%".into(),
        Kind::Padding => "8 or 8,4".into(),
        Kind::Enum(o) => o.join("/"),
        Kind::Image => "path to an image; it is copied into the profile".into(),
    }
}

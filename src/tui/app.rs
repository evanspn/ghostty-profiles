//! TUI state and behavior. No drawing and no terminal here: keys go in through
//! [`App::on_key`], and everything observable (files on disk, the reload hook)
//! can be asserted in tests.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::Result;
use std::cell::RefCell;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use super::color::{self, Hsv, WheelGeom};
use crate::shaderparams as sp;

use super::fields::{self, Field, Kind};
use crate::ghostty::{self, Reloader, Theme};
use crate::presets;
use crate::profile::{AssetKind, Profile};
use crate::store::{Store, valid_name};

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
    /// Setting a parameter (by name) of the selected shader: a color (with the picker) or a number.
    ShaderParam(String),
    /// The profile-wide effects opacity, typed.
    EffectsOpacity,
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

/// One of the three components of a color the picker can set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Comp {
    Hue,
    Sat,
    Val,
}

/// What the mouse is dragging while the button is down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drag {
    Wheel,
    Bar(Comp),
}

/// The color picker: it sits beside the field's hex input box and keeps the box in step with the
/// color while it is dragged. Nothing is saved until the input is accepted.
#[derive(Clone, Debug)]
pub struct Picker {
    pub hsv: Hsv,
    pub drag: Option<Drag>,
}

/// Where the picker was last drawn (set by the drawing code, read by the mouse handler).
#[derive(Clone, Debug, Default)]
pub struct PickerRects {
    pub wheel: Option<Rect>,
    pub bars: Vec<(Comp, Rect)>,
}

fn contains(r: Rect, col: u16, row: u16) -> bool {
    col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height
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
    /// This is the profile's one active shader (radio-style: for the `none` row, no shader is active).
    pub enabled: bool,
    /// A shader file the user wrote, found in the profile folder (not a copy of a bundled one).
    pub user: bool,
    /// The "(none)" row at the top: no shader.
    pub none: bool,
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

    /// The selected shader's tunable parameters, when it is enabled in the profile and has some.
    pub params: Option<crate::profile::ParamState>,
    /// What the cursor's (not active) library shader declares: shown read-only, nothing is written to the profile.
    pub preview: Option<crate::shaderparams::Schema>,
    /// Where the master effects-opacity bar was drawn (clickable).
    pub effects_bar: RefCell<Option<Rect>>,
    /// Profiles already tidied (pruned) this session.
    pruned: std::collections::HashSet<String>,
    /// The cursor is in the parameter list (right pane) of the Shaders tab, not the shader list.
    pub param_focus: bool,
    pub param_sel: usize,
    /// Where the parameter swatches and slider bars were drawn: (area, parameter index).
    pub param_swatches: RefCell<Vec<(Rect, usize)>>,
    pub param_bars: RefCell<Vec<(Rect, usize)>>,
    float_drag: Option<usize>,

    pub picker: Option<Picker>,
    pub picker_rects: RefCell<PickerRects>,
    /// Clickable color swatches in the Edit list: (area, field index). Set by the drawing code.
    pub swatch_rects: RefCell<Vec<(Rect, usize)>>,
    /// Draw exact colors (true) or the nearest of the 256 (false).
    pub truecolor: bool,
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
            params: None,
            preview: None,
            effects_bar: RefCell::new(None),
            pruned: std::collections::HashSet::new(),
            param_focus: false,
            param_sel: 0,
            param_swatches: RefCell::new(Vec::new()),
            param_bars: RefCell::new(Vec::new()),
            float_drag: None,
            picker: None,
            picker_rects: RefCell::new(PickerRects::default()),
            swatch_rects: RefCell::new(Vec::new()),
            truecolor: true,
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
        // a profile keeps ONE shader: tidy the folder of the profile being loaded (once per session)
        if let Some(name) = self.current_name().map(str::to_string)
            && self.pruned.insert(name.clone())
            && let Ok(report) = self.store.prune_profile(&name, false, false)
            && !report.is_empty()
        {
            self.say(prune_message(&name, &report), true);
        }
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
            KeyCode::Char(c @ '1'..='4') => {
                self.tab = Tab::ALL[(c as u8 - b'1') as usize];
                self.entered_tab();
            }
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
        self.entered_tab();
    }

    fn entered_tab(&mut self) {
        if self.tab == Tab::Shaders {
            self.refresh_params(true);
        } else {
            self.param_focus = false;
        }
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
            KeyCode::Char('p') => self.open_picker(self.field_sel),
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
        let active_file = self
            .profile
            .as_ref()
            .and_then(Profile::active_shader)
            .map(|r| r.rsplit('/').next().unwrap_or(&r).to_string());
        let mut rows = vec![ShaderRow {
            name: String::new(),
            in_library: false,
            enabled: active_file.is_none(),
            user: false,
            none: true,
        }];
        let library = presets::shader_names();
        for name in &library {
            rows.push(ShaderRow {
                enabled: active_file.as_deref() == Some(name.as_str()),
                name: name.clone(),
                in_library: true,
                user: false,
                none: false,
            });
        }
        // shader files the user wrote (never copies of bundled ones, never parameter or generated files)
        if let Some(p) = &self.profile {
            for name in p.user_shaders() {
                if !library.contains(&name) {
                    rows.push(ShaderRow {
                        enabled: active_file.as_deref() == Some(name.as_str()),
                        name,
                        in_library: false,
                        user: true,
                        none: false,
                    });
                }
            }
        }
        self.shader_sel = self.shader_sel.min(rows.len().saturating_sub(1));
        self.shader_rows = rows;
        self.refresh_params(false);
    }

    /// Re-read the selected shader's parameters. `write` lets this bring an untouched copy of an older
    /// shader up to the current version (only done on the Shaders tab, never while just browsing).
    fn refresh_params(&mut self, write: bool) {
        self.params = None;
        self.preview = None;
        let (Some(p), Some(row)) = (self.profile.as_ref(), self.shader_rows.get(self.shader_sel)) else { return };
        if row.none || !row.enabled {
            self.param_focus = false;
            if row.in_library && !row.enabled {
                // browsing: read what the bundled shader declares, in memory; nothing is copied into the profile
                self.preview =
                    presets::shader_source(&row.name).and_then(|src| crate::shaderparams::parse_schema(src).ok());
            }
            return;
        }
        if write {
            let _ = p.render_shaders();
        }
        self.params = p.shader_param_state(&format!("shaders/{}", row.name));
        let rows = self.param_row_count();
        self.param_sel = self.param_sel.min(rows.saturating_sub(1));
        if self.params.is_none() {
            self.param_focus = false;
        }
    }

    /// Rows in the parameter list: the preset row (when the shader has presets) and one per parameter.
    pub fn param_row_count(&self) -> usize {
        self.params.as_ref().map(|p| p.schema.params.len() + usize::from(!p.schema.presets.is_empty())).unwrap_or(0)
    }

    fn has_preset_row(&self) -> bool {
        self.params.as_ref().is_some_and(|p| !p.schema.presets.is_empty())
    }

    /// The parameter under the cursor (`None` on the preset row).
    fn param_at_cursor(&self) -> Option<usize> {
        let first = usize::from(self.has_preset_row());
        self.param_sel.checked_sub(first)
    }

    fn shaders_key(&mut self, key: KeyEvent) {
        if self.param_focus && self.params.is_some() {
            self.params_key(key);
            return;
        }
        let before = self.shader_sel;
        Self::step(&mut self.shader_sel, self.shader_rows.len(), key.code);
        if self.shader_sel != before {
            self.param_sel = 0;
            self.refresh_params(true);
        }
        match key.code {
            KeyCode::Enter | KeyCode::Char(' ') => self.select_shader(),
            KeyCode::Char('a') => self.toggle_animation(),
            KeyCode::Char('[') => {
                self.set_effects_opacity(self.effects_opacity() - 0.1);
            }
            KeyCode::Char(']') => {
                self.set_effects_opacity(self.effects_opacity() + 0.1);
            }
            KeyCode::Char('O') => self.begin_effects_opacity(),
            KeyCode::Right | KeyCode::Char('l') => {
                if self.params.is_some() {
                    self.param_focus = true;
                } else if self.shader_rows.get(self.shader_sel).is_some_and(|r| r.enabled && !r.none) {
                    self.say("this shader has no tunable parameters", false);
                } else {
                    self.say("choose the shader first (Enter), then → edits its parameters", false);
                }
            }
            KeyCode::Char('R') => self.reset_params(),
            _ => {}
        }
    }

    fn begin_effects_opacity(&mut self) {
        if self.profile.is_none() {
            self.say("no profile selected", false);
            return;
        }
        self.input = Some(Input {
            kind: InputKind::EffectsOpacity,
            title: "Effects opacity for every shader in this profile (0..1)".into(),
            buf: crate::shaderparams::format_number(self.effects_opacity()),
            error: None,
        });
    }

    fn params_key(&mut self, key: KeyEvent) {
        let rows = self.param_row_count();
        let big = key.modifiers.contains(KeyModifiers::SHIFT);
        match key.code {
            KeyCode::Esc => self.param_focus = false,
            KeyCode::Up | KeyCode::Char('k') => self.param_sel = self.param_sel.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => self.param_sel = (self.param_sel + 1).min(rows.saturating_sub(1)),
            KeyCode::Char('R') => self.reset_params(),
            KeyCode::Char('[') => {
                self.set_effects_opacity(self.effects_opacity() - 0.1);
            }
            KeyCode::Char(']') => {
                self.set_effects_opacity(self.effects_opacity() + 0.1);
            }
            KeyCode::Char('O') => self.begin_effects_opacity(),
            KeyCode::Left | KeyCode::Char('h') => self.adjust_param(false, big),
            KeyCode::Right | KeyCode::Char('l') => self.adjust_param(true, big),
            KeyCode::Char('p') | KeyCode::Enter => self.edit_param(),
            _ => {}
        }
    }

    /// Left/Right on a parameter row: cycle the preset, or nudge a number (a color has no step).
    fn adjust_param(&mut self, forward: bool, big: bool) {
        let Some(state) = self.params.clone() else { return };
        let Some(i) = self.param_at_cursor() else {
            // the preset row
            let n = state.schema.presets.len();
            let cur = sp::matching_preset(&state.schema, &state.values)
                .and_then(|m| state.schema.presets.iter().position(|p| p.name == m.name));
            let next = match (cur, forward) {
                (None, true) => 0,
                (None, false) => n - 1,
                (Some(c), true) => (c + 1) % n,
                (Some(c), false) => (c + n - 1) % n,
            };
            self.apply_preset(&state.schema.presets[next].name.clone());
            return;
        };
        let (Some(param), Some(value)) = (state.schema.params.get(i), state.values.get(i)) else { return };
        if let sp::Kind::Float { min, max } = param.kind {
            let step = (max - min) / 50.0 * if big { 5.0 } else { 1.0 };
            let cur = value.parse::<f64>().unwrap_or(min);
            let next = (cur + if forward { step } else { -step }).clamp(min, max);
            self.set_param(&param.name.clone(), &sp::format_number(next));
        }
    }

    /// Enter / `p` on a parameter row: a color opens the picker, a number opens a typed box, the preset
    /// row moves to the next preset.
    fn edit_param(&mut self) {
        let Some(state) = self.params.clone() else { return };
        let Some(i) = self.param_at_cursor() else {
            self.adjust_param(true, false);
            return;
        };
        let (Some(param), Some(value)) = (state.schema.params.get(i), state.values.get(i)) else { return };
        match param.kind {
            sp::Kind::Color => self.open_shader_color(i),
            sp::Kind::Float { min, max } => {
                self.input = Some(Input {
                    kind: InputKind::ShaderParam(param.name.clone()),
                    title: format!("{} ({min}..{max})", param.label),
                    buf: value.clone(),
                    error: None,
                });
            }
        }
    }

    /// Open the color picker (and its hex box) for color parameter `i` of the selected shader.
    pub fn open_shader_color(&mut self, i: usize) {
        let Some(state) = self.params.clone() else { return };
        let (Some(param), Some(value)) = (state.schema.params.get(i), state.values.get(i)) else { return };
        let hsv = color::hex_to_hsv(value).unwrap_or(Hsv { h: 0.0, s: 0.0, v: 0.5 });
        self.param_focus = true;
        self.param_sel = i + usize::from(self.has_preset_row());
        self.input = Some(Input {
            kind: InputKind::ShaderParam(param.name.clone()),
            title: format!("{} (#rrggbb): wheel, mouse or type; Enter accepts, Esc cancels", param.label),
            buf: value.clone(),
            error: None,
        });
        self.picker = Some(Picker { hsv, drag: None });
    }

    /// Change one parameter of the selected shader: validates, saves the sidecar, regenerates the shader
    /// copy and (when the profile is active) schedules the usual debounced reload.
    pub fn set_param(&mut self, name: &str, value: &str) -> bool {
        let (Some(p), Some(rel)) = (self.profile.as_ref(), self.params.as_ref().map(|s| s.rel.clone())) else {
            return false;
        };
        match p.set_shader_params(&rel, &[(name.to_string(), value.to_string())]) {
            Ok(()) => {
                let label = self
                    .params
                    .as_ref()
                    .and_then(|s| s.schema.params.iter().find(|q| q.name == name))
                    .map(|q| q.label.clone())
                    .unwrap_or_else(|| name.to_string());
                self.params = self.profile.as_ref().and_then(|p| p.shader_param_state(&rel));
                self.commit(&label);
                true
            }
            Err(e) => {
                self.say(format!("{name}: {e:#}"), false);
                false
            }
        }
    }

    fn apply_preset(&mut self, preset: &str) {
        let (Some(p), Some(rel)) = (self.profile.as_ref(), self.params.as_ref().map(|s| s.rel.clone())) else { return };
        match p.apply_shader_preset(&rel, preset) {
            Ok(()) => {
                self.params = self.profile.as_ref().and_then(|p| p.shader_param_state(&rel));
                self.commit(&format!("preset {preset}"));
            }
            Err(e) => self.say(format!("{e:#}"), false),
        }
    }

    fn reset_params(&mut self) {
        let (Some(p), Some(rel)) = (self.profile.as_ref(), self.params.as_ref().map(|s| s.rel.clone())) else {
            self.say("no tunable shader selected", false);
            return;
        };
        match p.reset_shader_params(&rel) {
            Ok(()) => {
                self.params = self.profile.as_ref().and_then(|p| p.shader_param_state(&rel));
                self.commit("shader parameters reset to defaults");
            }
            Err(e) => self.say(format!("{e:#}"), false),
        }
    }

    /// Enter on a row of the Shaders tab: make that shader THE shader of the profile (radio-style: it replaces
    /// the current one and the old one's generated files are removed), or choose "(none)". Only now is a
    /// bundled shader copied into the profile.
    pub fn select_shader(&mut self) {
        let Some(row) = self.shader_rows.get(self.shader_sel).cloned() else { return };
        let Some(p) = self.profile.as_mut() else {
            self.say("no profile selected: move off \"(none)\" on the Profiles tab to edit one", false);
            return;
        };
        if row.enabled {
            let text = if row.none { "this profile has no shader" } else { "that is already this profile's shader" };
            self.say(text, true);
            return;
        }
        let result = if row.none { p.clear_shader() } else { p.use_shader(&row.name) };
        if let Err(e) = result {
            self.say(format!("{e:#}"), false);
            return;
        }
        let what = if row.none { "shader removed".to_string() } else { format!("shader {} selected", row.name) };
        self.commit(&what);
        self.param_focus = false;
        self.rebuild_shader_rows();
        self.refresh_params(true);
    }

    // ---- the profile-wide effects opacity ------------------------------------------------
    pub fn effects_opacity(&self) -> f64 {
        self.profile.as_ref().map(Profile::effects_opacity).unwrap_or(1.0)
    }

    pub fn set_effects_opacity(&mut self, value: f64) -> bool {
        let value = (value.clamp(0.0, 1.0) * 100.0).round() / 100.0;
        let Some(p) = self.profile.as_ref() else {
            self.say("no profile selected", false);
            return false;
        };
        match p.set_effects_opacity(value) {
            Ok(()) => {
                self.commit(&format!("effects opacity {}%", (value * 100.0).round()));
                self.refresh_params(false);
                true
            }
            Err(e) => {
                self.say(format!("{e:#}"), false);
                false
            }
        }
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

    // ---- color picker ---------------------------------------------------------------
    /// Open the picker for color field `i` (also opens its hex input box, which is where the value is accepted).
    pub fn open_picker(&mut self, i: usize) {
        let Some(field) = self.fields.get(i).copied() else { return };
        if !matches!(field.kind, Kind::Hex | Kind::Palette(_)) {
            self.say("the picker is for color fields: move to a color first", false);
            return;
        }
        if self.profile.is_none() {
            self.say("no profile selected: move off \"(none)\" on the Profiles tab to edit one", false);
            return;
        }
        let hex = self
            .field_value(i)
            .and_then(|v| crate::profile::normalize_hex(&v))
            .unwrap_or_else(|| "#808080".to_string());
        let hsv = color::hex_to_hsv(&hex).unwrap_or(Hsv { h: 0.0, s: 0.0, v: 0.5 });
        self.field_sel = i;
        self.input = Some(Input {
            kind: InputKind::Field(i),
            title: format!("{} (#rrggbb): wheel, mouse or type; Enter accepts, Esc cancels", field.label),
            buf: hex,
            error: None,
        });
        self.picker = Some(Picker { hsv, drag: None });
    }

    /// The color changed in the picker: show it as hex in the input box. Nothing is saved or reloaded.
    fn picker_set(&mut self, hsv: Hsv) {
        let Some(p) = self.picker.as_mut() else { return };
        p.hsv = hsv;
        if let Some(input) = self.input.as_mut() {
            input.buf = color::hsv_to_hex(hsv);
            input.error = None;
        }
    }

    /// The hex in the input box was typed or edited: move the wheel's marker to match (when it parses).
    fn picker_follow_input(&mut self) {
        let (Some(p), Some(input)) = (self.picker.as_mut(), self.input.as_ref()) else { return };
        if let Some(h) = color::hsv_from_typed(&input.buf, p.hsv) {
            p.hsv = h;
        }
    }

    /// Keyboard fallback while the picker is open. True if the key was used.
    fn picker_key(&mut self, key: KeyEvent) -> bool {
        let Some(mut hsv) = self.picker.as_ref().map(|p| p.hsv) else { return false };
        let big = key.modifiers.contains(KeyModifiers::SHIFT);
        let (dh, ds, dv) = if big { (20.0, 0.2, 0.12) } else { (5.0, 0.05, 0.03) };
        match key.code {
            KeyCode::Left => hsv.h = (hsv.h - dh).rem_euclid(360.0),
            KeyCode::Right => hsv.h = (hsv.h + dh).rem_euclid(360.0),
            KeyCode::Up => hsv.s = (hsv.s + ds).min(1.0),
            KeyCode::Down => hsv.s = (hsv.s - ds).max(0.0),
            KeyCode::Char('[') | KeyCode::Char('-') | KeyCode::Char('_') => hsv.v = (hsv.v - dv).max(0.0),
            KeyCode::Char(']') | KeyCode::Char('+') | KeyCode::Char('=') => hsv.v = (hsv.v + dv).min(1.0),
            KeyCode::Char('{') => hsv.v = (hsv.v - 0.12).max(0.0),
            KeyCode::Char('}') => hsv.v = (hsv.v + 0.12).min(1.0),
            _ => return false,
        }
        self.picker_set(hsv);
        true
    }

    fn set_component(&mut self, comp: Comp, fraction: f32) {
        let Some(mut hsv) = self.picker.as_ref().map(|p| p.hsv) else { return };
        match comp {
            Comp::Hue => hsv.h = (fraction * 360.0).min(359.999),
            Comp::Sat => hsv.s = fraction,
            Comp::Val => hsv.v = fraction,
        }
        self.picker_set(hsv);
    }

    /// Mouse: click or drag in the picker; click a color swatch on the Edit tab to open it.
    pub fn on_mouse(&mut self, m: MouseEvent) {
        let (col, row) = (m.column, m.row);
        match m.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let idle = self.input.is_none() && self.wizard.is_none() && self.confirm_delete.is_none();
                if self.picker.is_some() {
                    self.picker_press(col, row);
                } else if self.tab == Tab::Shaders && idle {
                    self.shader_press(col, row);
                } else if self.tab == Tab::Edit
                    && self.input.is_none()
                    && self.wizard.is_none()
                    && self.confirm_delete.is_none()
                {
                    let hit = self.swatch_rects.borrow().iter().find(|(r, _)| contains(*r, col, row)).map(|(_, i)| *i);
                    if let Some(i) = hit {
                        self.open_picker(i);
                    }
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if let Some(i) = self.float_drag {
                    if i == usize::MAX {
                        self.drag_effects(col);
                    } else {
                        self.drag_float(i, col);
                    }
                } else {
                    self.picker_drag(col, row);
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.float_drag = None;
                if let Some(p) = self.picker.as_mut() {
                    p.drag = None;
                }
            }
            _ => {}
        }
    }

    /// A click in the Shaders tab's parameter list: a swatch opens the picker, a slider sets the number.
    fn shader_press(&mut self, col: u16, row: u16) {
        let bar = *self.effects_bar.borrow();
        if let Some(bar) = bar
            && contains(bar, col, row)
        {
            self.float_drag = Some(usize::MAX);
            self.drag_effects(col);
            return;
        }
        let swatch = self.param_swatches.borrow().iter().find(|(r, _)| contains(*r, col, row)).map(|(_, i)| *i);
        if let Some(i) = swatch {
            self.open_shader_color(i);
            return;
        }
        let bar = self.param_bars.borrow().iter().find(|(r, _)| contains(*r, col, row)).map(|(_, i)| *i);
        if let Some(i) = bar {
            self.param_focus = true;
            self.param_sel = i + usize::from(self.has_preset_row());
            self.float_drag = Some(i);
            self.drag_float(i, col);
        }
    }

    fn drag_effects(&mut self, col: u16) {
        let Some(bar) = *self.effects_bar.borrow() else { return };
        let frac = color::bar_fraction(col as i32 - bar.x as i32, bar.width) as f64;
        if (self.effects_opacity() - (frac * 100.0).round() / 100.0).abs() > 1e-9 {
            self.set_effects_opacity(frac);
        }
    }

    fn drag_float(&mut self, i: usize, col: u16) {
        let bar = self.param_bars.borrow().iter().find(|(_, p)| *p == i).map(|(r, _)| *r);
        let (Some(bar), Some(state)) = (bar, self.params.clone()) else { return };
        let Some(param) = state.schema.params.get(i) else { return };
        if let sp::Kind::Float { min, max } = param.kind {
            let frac = color::bar_fraction(col as i32 - bar.x as i32, bar.width) as f64;
            let value = sp::format_number(min + frac * (max - min));
            if state.values.get(i) != Some(&value) {
                self.set_param(&param.name.clone(), &value);
            }
        }
    }

    fn picker_press(&mut self, col: u16, row: u16) {
        let rects = self.picker_rects.borrow().clone();
        if let Some(w) = rects.wheel.filter(|w| contains(*w, col, row)) {
            let g = WheelGeom { cols: w.width, rows: w.height };
            // a click outside the circle (the corners of its box) is ignored: no jump
            if let Some((h, s)) = g.hit(col as i32 - w.x as i32, row as i32 - w.y as i32) {
                let mut hsv = self.picker.as_ref().map(|p| p.hsv).unwrap_or(Hsv { h: 0.0, s: 0.0, v: 1.0 });
                hsv.h = h;
                hsv.s = s;
                self.picker_set(hsv);
                if let Some(p) = self.picker.as_mut() {
                    p.drag = Some(Drag::Wheel);
                }
            }
            return;
        }
        for (comp, r) in rects.bars {
            if contains(r, col, row) {
                self.set_component(comp, color::bar_fraction(col as i32 - r.x as i32, r.width));
                if let Some(p) = self.picker.as_mut() {
                    p.drag = Some(Drag::Bar(comp));
                }
                return;
            }
        }
    }

    fn picker_drag(&mut self, col: u16, row: u16) {
        let Some(drag) = self.picker.as_ref().and_then(|p| p.drag) else { return };
        let rects = self.picker_rects.borrow().clone();
        match drag {
            Drag::Wheel => {
                let Some(w) = rects.wheel else { return };
                let g = WheelGeom { cols: w.width, rows: w.height };
                let (h, s) = g.hit_clamped(col as i32 - w.x as i32, row as i32 - w.y as i32);
                let mut hsv = self.picker.as_ref().map(|p| p.hsv).unwrap_or(Hsv { h: 0.0, s: 0.0, v: 1.0 });
                hsv.h = h;
                hsv.s = s;
                self.picker_set(hsv);
            }
            Drag::Bar(comp) => {
                if let Some((_, r)) = rects.bars.iter().find(|(c, _)| *c == comp) {
                    self.set_component(comp, color::bar_fraction(col as i32 - r.x as i32, r.width));
                }
            }
        }
    }

    // ---- text input -----------------------------------------------------------------
    fn on_input_key(&mut self, key: KeyEvent) {
        if self.picker.is_some() && !key.modifiers.contains(KeyModifiers::CONTROL) && self.picker_key(key) {
            return;
        }
        let Some(input) = self.input.as_mut() else { return };
        match key.code {
            KeyCode::Esc => {
                self.picker = None; // cancelling restores the previous value: nothing was saved
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
                let picker = self.picker.take();
                self.finish_input(done);
                if self.input.is_some() {
                    self.picker = picker; // refused (bad hex): keep the picker open to fix it
                }
                return;
            }
            _ => {}
        }
        self.picker_follow_input();
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
            InputKind::EffectsOpacity => match input.buf.trim().parse::<f64>() {
                Ok(v) if v.is_finite() && (0.0..=1.0).contains(&v) => {
                    self.set_effects_opacity(v);
                }
                _ => {
                    self.say("effects opacity must be a number between 0 and 1", false);
                    self.input = Some(Input { error: Some(self.status.text.clone()), ..input });
                }
            },
            InputKind::ShaderParam(ref name) => {
                if !self.set_param(name, &input.buf) {
                    let error = Some(self.status.text.clone());
                    self.input = Some(Input { error, ..input });
                }
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

/// The status line after a tidy-up (also used by the CLI).
pub fn prune_message(name: &str, r: &crate::profile::PruneReport) -> String {
    let mut parts = Vec::new();
    if !r.removed.is_empty() {
        parts.push(format!(
            "removed {} unused shader file{}",
            r.removed.len(),
            if r.removed.len() == 1 { "" } else { "s" }
        ));
    }
    if !r.dropped_extra_lines.is_empty() {
        parts.push(format!("kept only the first shader (dropped {})", r.dropped_extra_lines.join(", ")));
    }
    let mut msg = format!("tidied '{name}': {}", parts.join("; "));
    if let Some(b) = &r.backup {
        msg.push_str(&format!(" (backup: {})", b.display()));
    }
    msg
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

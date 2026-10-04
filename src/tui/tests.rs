//! TUI behavior tests. Everything runs in a temp config home with a recording
//! reloader: no real Ghostty is ever signalled and the real ~/.config is never read.

use std::cell::Cell;
use std::fs;
use std::rc::Rc;
use std::time::{Duration, Instant};

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::app::{App, Tab};
use super::ui;
use crate::ghostty::{ReloadResult, Reloader};
use crate::paths::Paths;
use crate::store::Store;

struct Recorder(Rc<Cell<usize>>);

impl Reloader for Recorder {
    fn reload(&mut self) -> ReloadResult {
        self.0.set(self.0.get() + 1);
        ReloadResult { ok: true, detail: "reload signal sent (test)".into() }
    }
}

struct Harness {
    td: tempfile::TempDir,
    app: App,
    reloads: Rc<Cell<usize>>,
}

fn harness() -> Harness {
    let td = tempfile::tempdir().unwrap();
    let store = Store::new(Paths::new(td.path().join("config")));
    let reloads = Rc::new(Cell::new(0));
    let mut app = App::new(store, Box::new(Recorder(reloads.clone()))).unwrap();
    app.debounce = Duration::ZERO;
    Harness { td, app, reloads }
}

impl Harness {
    fn press(&mut self, code: KeyCode) {
        self.app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            self.press(KeyCode::Char(c));
        }
    }

    fn tick(&mut self) {
        self.app.tick(Instant::now() + Duration::from_millis(1));
    }

    fn select_profile(&mut self, name: &str) {
        let i = self.app.profiles.iter().position(|p| p == name).unwrap();
        self.app.tab = Tab::Profiles;
        if self.app.none_selected {
            self.press(KeyCode::Down);
        }
        while self.app.sel < i {
            self.press(KeyCode::Down);
        }
        while self.app.sel > i {
            self.press(KeyCode::Up);
        }
    }

    /// Apply a profile as the user would: select it, press Enter, let the reload fire.
    fn apply(&mut self, name: &str) {
        self.select_profile(name);
        self.press(KeyCode::Enter);
        self.tick();
    }

    fn go_to_field(&mut self, key: &str) {
        let want = self.app.fields.iter().position(|f| f.key == key).unwrap();
        self.app.tab = Tab::Edit;
        while self.app.field_sel < want {
            self.press(KeyCode::Down);
        }
        while self.app.field_sel > want {
            self.press(KeyCode::Up);
        }
    }

    fn conf(&self, profile: &str) -> String {
        fs::read_to_string(self.app.store.profiles_dir().join(profile).join("profile.conf")).unwrap()
    }

    fn active_conf(&self) -> String {
        fs::read_to_string(self.app.store.paths.active_conf()).unwrap()
    }

    fn screen(&self, w: u16, h: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| ui::draw(f, &self.app)).unwrap();
        text_of(t.backend().buffer())
    }
}

fn text_of(buf: &Buffer) -> String {
    let w = buf.area.width as usize;
    buf.content().chunks(w).map(|row| row.iter().map(|c| c.symbol()).collect::<String>()).collect::<Vec<_>>().join("\n")
}

#[test]
fn first_run_installs_the_presets_and_touches_nothing_else() {
    let h = harness();
    assert_eq!(h.app.profiles, vec!["aurora-glass", "calm-dark", "crt-green", "shd"]);
    assert!(h.app.status.text.contains("first run"));
    assert_eq!(h.app.active, None, "nothing is applied until the user says so");
    assert!(!h.app.store.paths.active_conf().exists());
    assert!(!h.app.store.paths.ghostty_dir().exists(), "Ghostty's own config dir is not even created");
}

#[test]
fn editing_a_color_saves_rerenders_and_hot_reloads() {
    let mut h = harness();
    h.apply("shd");
    assert_eq!(h.reloads.get(), 1, "applying reloads");

    h.go_to_field("cursor-color");
    h.press(KeyCode::Enter);
    assert!(h.app.input.is_some(), "Enter opens the editor");
    for _ in 0..7 {
        h.press(KeyCode::Backspace);
    }
    h.type_text("#00ff00");
    h.press(KeyCode::Enter);

    assert!(h.app.input.is_none());
    assert!(h.conf("shd").contains("cursor-color = #00ff00"), "profile.conf changed");
    assert!(h.active_conf().contains("cursor-color = #00ff00"), "the active conf Ghostty reads changed");
    assert!(h.app.reload_pending());
    h.tick();
    assert_eq!(h.reloads.get(), 2, "the injected reload hook fired");

    // and the screen shows it: value text and a swatch painted in that exact color
    let mut t = Terminal::new(TestBackend::new(110, 40)).unwrap();
    t.draw(|f| ui::draw(f, &h.app)).unwrap();
    let buf = t.backend().buffer().clone();
    assert!(text_of(&buf).contains("#00ff00"));
    let green = ratatui::style::Color::Rgb(0, 255, 0);
    assert!(buf.content().iter().any(|c| c.symbol() == "█" && c.fg == green), "a green swatch is drawn");
}

#[test]
fn invalid_input_changes_nothing_and_keeps_the_box_open() {
    let mut h = harness();
    h.apply("shd");
    let before = h.conf("shd");
    let reloads = h.reloads.get();

    h.go_to_field("background");
    h.press(KeyCode::Enter);
    h.type_text("zzz");
    h.press(KeyCode::Enter);
    assert!(h.app.input.is_some(), "stays open so it can be corrected");
    assert!(h.app.input.as_ref().unwrap().error.as_deref().unwrap().contains("not a color"));
    assert_eq!(h.conf("shd"), before);
    h.tick();
    assert_eq!(h.reloads.get(), reloads, "no reload for an invalid edit");
    assert!(h.screen(100, 30).contains("not a color"));

    h.press(KeyCode::Esc);
    assert!(h.app.input.is_none());
    assert_eq!(h.conf("shd"), before);
}

#[test]
fn a_burst_of_edits_makes_one_reload_after_the_debounce() {
    let mut h = harness();
    h.apply("shd");
    let base = h.reloads.get();
    h.app.debounce = Duration::from_millis(250);
    h.go_to_field("cursor-style");
    for _ in 0..3 {
        h.press(KeyCode::Right);
    }
    let t0 = Instant::now();
    h.app.tick(t0 + Duration::from_millis(10));
    assert_eq!(h.reloads.get(), base, "still inside the debounce window");
    h.app.tick(t0 + Duration::from_millis(400));
    assert_eq!(h.reloads.get(), base + 1, "one reload for the whole burst");
    h.app.tick(t0 + Duration::from_millis(800));
    assert_eq!(h.reloads.get(), base + 1);
}

#[test]
fn enum_fields_cycle_and_x_unsets() {
    let mut h = harness();
    h.apply("shd");
    h.go_to_field("cursor-style");
    h.press(KeyCode::Right);
    assert!(h.conf("shd").contains("cursor-style = block"));
    h.press(KeyCode::Right);
    assert!(h.conf("shd").contains("cursor-style = bar"));
    h.press(KeyCode::Left);
    assert!(h.conf("shd").contains("cursor-style = block"));
    h.press(KeyCode::Char('x'));
    assert!(!h.conf("shd").contains("cursor-style"));
}

#[test]
fn editing_a_profile_that_is_not_active_saves_without_reloading() {
    let mut h = harness();
    h.apply("shd");
    let base = h.reloads.get();
    h.select_profile("calm-dark");
    h.go_to_field("font-size");
    h.press(KeyCode::Enter);
    h.type_text("15");
    h.press(KeyCode::Enter);
    assert!(h.conf("calm-dark").contains("font-size = 15"));
    assert!(!h.active_conf().contains("font-size"), "the live look is untouched");
    h.tick();
    assert_eq!(h.reloads.get(), base);
    assert!(h.app.status.text.contains("not the active profile"));
}

#[test]
fn ctrl_r_reloads_immediately() {
    let mut h = harness();
    h.apply("shd");
    let base = h.reloads.get();
    h.app.on_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL));
    assert_eq!(h.reloads.get(), base + 1);
}

#[test]
fn theme_filter_and_bake_copy_the_colors_into_the_profile() {
    let mut h = harness();
    h.apply("shd");
    h.app.tab = Tab::Themes;
    h.press(KeyCode::Char('/'));
    h.type_text("dracula");
    h.press(KeyCode::Enter);
    assert!(!h.app.theme_view.is_empty() && h.app.theme_view.len() < 10, "{}", h.app.theme_view.len());
    let name = h.app.selected_theme().unwrap().name.clone();
    assert!(name.to_lowercase().contains("dracula"));
    h.press(KeyCode::Enter);
    let conf = h.conf("shd");
    assert_eq!(conf.matches("palette = ").count(), 16);
    assert!(!conf.contains("theme ="), "baked, not referenced");
    let theme = crate::ghostty::theme_colors(&h.app.selected_theme().unwrap().text);
    assert!(conf.contains(&format!("background = {}", theme.background.unwrap())));
    // unrelated settings survive
    assert!(conf.contains("background-opacity = 0.5") && conf.contains("custom-shader = shaders/xmb-waves.glsl"));
    assert!(h.active_conf().contains("palette = 0="));
    h.tick();
    assert!(h.screen(110, 30).contains(&name));
    // Esc clears the filter
    h.press(KeyCode::Esc);
    assert!(h.app.theme_view.len() > 400);
}

#[test]
fn shaders_toggle_copies_into_the_profile_and_manages_animation() {
    let mut h = harness();
    h.select_profile("calm-dark");
    h.app.tab = Tab::Shaders;
    let row = |h: &Harness, n: &str| h.app.shader_rows.iter().position(|r| r.name == n).unwrap();
    assert!(h.app.shader_rows.iter().any(|r| r.name == "soft-glow.glsl" && r.enabled));
    // turn the preset's shader off: entry gone, animation key gone with it
    let i = row(&h, "soft-glow.glsl");
    while h.app.shader_sel < i {
        h.press(KeyCode::Down);
    }
    h.press(KeyCode::Enter);
    assert!(!h.conf("calm-dark").contains("custom-shader"));
    // turn an animated one on: copied into the profile, animation turned on
    let i = row(&h, "aurora.glsl");
    h.app.shader_sel = i;
    h.press(KeyCode::Enter);
    let conf = h.conf("calm-dark");
    assert!(conf.contains("custom-shader = shaders/aurora.glsl"), "{conf}");
    assert!(conf.contains("custom-shader-animation = true"));
    let dir = h.app.store.profiles_dir().join("calm-dark");
    assert!(dir.join("shaders/aurora.glsl").is_file());
    // animation toggle
    h.press(KeyCode::Char('a'));
    assert!(h.conf("calm-dark").contains("custom-shader-animation = false"));
    h.press(KeyCode::Char('a'));
    assert!(h.conf("calm-dark").contains("custom-shader-animation = true"));
    // a second shader stacks
    h.app.shader_sel = row(&h, "crt-scanlines.glsl");
    h.press(KeyCode::Char(' '));
    assert_eq!(h.conf("calm-dark").matches("custom-shader = ").count(), 2);
    assert!(h.screen(100, 24).contains("[x] aurora.glsl"));
}

#[test]
fn the_background_image_is_copied_into_the_profile_and_dropped_on_export() {
    let mut h = harness();
    h.apply("shd");
    let img = h.td.path().join("wallpaper.png");
    fs::write(&img, "not really a png").unwrap();

    h.go_to_field("background-image");
    h.press(KeyCode::Enter);
    h.type_text(&img.display().to_string());
    h.press(KeyCode::Enter);
    let dir = h.app.store.profiles_dir().join("shd");
    assert!(dir.join("images/wallpaper.png").is_file(), "copied in");
    assert!(h.conf("shd").contains("background-image = images/wallpaper.png"), "stored relative");
    assert!(h.active_conf().contains(&format!("background-image = {}", dir.join("images/wallpaper.png").display())));

    h.go_to_field("background-image-fit");
    h.press(KeyCode::Right);
    h.select_profile("shd");
    h.press(KeyCode::Char('e'));
    let dest = h.td.path().join("shared");
    for _ in 0..40 {
        h.press(KeyCode::Backspace);
    }
    h.type_text(&dest.display().to_string());
    h.press(KeyCode::Enter);
    assert!(h.app.status.text.contains("exported"), "{}", h.app.status.text);
    assert!(!dest.join("images").exists(), "the image is not in the export");
    let exported = fs::read_to_string(dest.join("profile.conf")).unwrap();
    assert!(!exported.contains("background-image"), "{exported}");
    assert!(dest.join("shaders/xmb-waves.glsl").is_file(), "shaders stay self-contained");

    // a path that does not exist is refused and leaves the profile alone
    let before = h.conf("shd");
    h.go_to_field("background-image");
    h.press(KeyCode::Enter);
    for _ in 0..80 {
        h.press(KeyCode::Backspace);
    }
    h.type_text("/definitely/not/here.png");
    h.press(KeyCode::Enter);
    assert_eq!(h.conf("shd"), before);
    assert!(h.app.input.as_ref().unwrap().error.as_deref().unwrap().contains("no such file"));
}

#[test]
fn new_delete_and_the_active_profile_is_protected() {
    let mut h = harness();
    h.apply("shd");
    h.press(KeyCode::Char('n'));
    h.type_text("mine");
    h.press(KeyCode::Enter);
    assert!(h.app.profiles.contains(&"mine".to_string()));
    assert_eq!(h.app.current_name(), Some("mine"), "the new profile is selected");
    assert!(h.conf("mine").contains("copy of shd"));
    assert!(h.conf("mine").contains("custom-shader"), "copied from the selected profile");

    // bad names are refused with the box still open
    h.press(KeyCode::Char('n'));
    h.type_text("a/b");
    h.press(KeyCode::Enter);
    assert!(h.app.input.is_some());
    h.press(KeyCode::Esc);

    // delete needs a y
    h.select_profile("mine");
    h.press(KeyCode::Char('d'));
    h.press(KeyCode::Char('n'));
    assert!(h.app.profiles.contains(&"mine".to_string()), "any key but y cancels");
    h.press(KeyCode::Char('d'));
    h.press(KeyCode::Char('y'));
    assert!(!h.app.profiles.contains(&"mine".to_string()));

    h.select_profile("shd");
    h.press(KeyCode::Char('d'));
    assert!(h.app.profiles.contains(&"shd".to_string()), "the active profile cannot be deleted");
    assert!(h.app.confirm_delete.is_none());
}

#[test]
fn every_tab_draws_at_phone_and_desktop_sizes() {
    let mut h = harness();
    h.apply("shd");
    for tab in Tab::ALL {
        h.app.tab = tab;
        for (w, hh) in [(30, 10), (60, 20), (120, 40)] {
            let s = h.screen(w, hh);
            assert!(s.contains("ghostty-profiles"), "{tab:?} {w}x{hh}");
        }
    }
    let s = h.screen(120, 40);
    h.app.tab = Tab::Profiles;
    assert!(h.screen(120, 40).contains("● shd"), "the active profile is marked");
    assert!(s.contains("Shaders") && h.screen(120, 40).contains("Profiles"));
    // no profile at all: still draws
    h.app.profile = None;
    h.app.tab = Tab::Edit;
    assert!(h.screen(80, 24).contains("Select a profile"));
}

#[test]
fn quit_keys() {
    let mut h = harness();
    h.press(KeyCode::Char('q'));
    assert!(h.app.quit);
    let mut h = harness();
    h.app.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert!(h.app.quit);
    // typing a q into an input box does not quit
    let mut h = harness();
    h.press(KeyCode::Char('n'));
    h.type_text("quiet");
    assert!(!h.app.quit);
}

#[test]
fn none_row_is_always_there_and_marks_the_active_state() {
    let mut h = harness();
    let screen = h.screen(110, 30);
    assert!(screen.contains("● (none)"), "nothing applied yet, so (none) is the marked row:\n{screen}");
    h.apply("shd");
    let screen = h.screen(110, 30);
    assert!(screen.contains("● shd") && !screen.contains("● (none)"), "{screen}");
    assert!(screen.contains("u off"), "the key is on the footer");
    // the (none) row is reachable with Up from the first profile, and Down comes back
    h.select_profile("aurora-glass");
    h.press(KeyCode::Up);
    assert!(h.app.none_selected && h.app.current_name().is_none());
    assert!(h.screen(110, 30).contains("Enter turns the active profile off"));
    h.press(KeyCode::Down);
    assert!(!h.app.none_selected && h.app.current_name() == Some("aurora-glass"));
}

#[test]
fn enter_on_none_and_the_u_key_turn_the_profile_off_and_back_on() {
    let mut h = harness();
    h.apply("shd");
    let reloads = h.reloads.get();
    h.select_profile("aurora-glass");
    h.press(KeyCode::Up); // onto (none)
    h.press(KeyCode::Enter);
    assert_eq!(h.app.active, None);
    assert_eq!(h.app.store.active_name(), None);
    assert_eq!(h.active_conf(), crate::store::OFF_CONF, "the generated file applies nothing");
    assert!(h.app.store.is_linked(), "the include stays");
    assert!(h.app.reload_pending());
    h.tick();
    assert_eq!(h.reloads.get(), reloads + 1, "Ghostty is reloaded so the user's own config shows through");
    let screen = h.screen(110, 30);
    assert!(screen.contains("● (none)") && screen.contains("No profile is active"), "{screen}");

    // off again is harmless and idempotent
    h.press(KeyCode::Enter);
    assert_eq!(h.active_conf(), crate::store::OFF_CONF);
    assert!(h.app.status.text.contains("no profile was active"));

    // re-selecting a profile applies it again
    h.apply("calm-dark");
    assert_eq!(h.app.active.as_deref(), Some("calm-dark"));
    assert!(h.active_conf().contains("background = #1b1e24"));

    // u works from any row
    h.press(KeyCode::Char('u'));
    assert_eq!(h.app.active, None);
    assert_eq!(h.active_conf(), crate::store::OFF_CONF);
}

#[test]
fn edits_while_none_is_selected_change_nothing_and_say_why() {
    let mut h = harness();
    h.apply("shd");
    h.press(KeyCode::Char('u'));
    h.tick();
    let shd_before = h.conf("shd");
    let reloads = h.reloads.get();

    h.select_profile("aurora-glass");
    h.press(KeyCode::Up);
    assert!(h.app.none_selected);
    h.go_to_field("background");
    h.press(KeyCode::Enter);
    h.type_text("#123456");
    h.press(KeyCode::Enter);
    assert!(h.app.status.text.contains("no profile selected"), "{}", h.app.status.text);
    assert_eq!(h.conf("shd"), shd_before, "no profile was edited");
    assert_eq!(h.active_conf(), crate::store::OFF_CONF, "and nothing was silently re-activated");
    h.tick();
    assert_eq!(h.reloads.get(), reloads);
    assert_eq!(h.app.store.active_name(), None);
    assert!(h.app.input.is_some(), "the box stays open so nothing typed is lost");
    h.press(KeyCode::Esc);
    assert!(h.screen(100, 24).contains("Select a profile"));

    // editing a selected-but-inactive profile saves it without applying it, and says so
    h.select_profile("shd");
    h.go_to_field("font-size");
    h.press(KeyCode::Enter);
    h.type_text("15");
    h.press(KeyCode::Enter);
    assert!(h.conf("shd").contains("font-size = 15"));
    assert_eq!(h.active_conf(), crate::store::OFF_CONF);
    assert!(h.app.status.text.contains("not the active profile"));
}

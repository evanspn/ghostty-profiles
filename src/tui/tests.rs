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

use super::app::{App, Tab, Top};
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
        while self.app.top.is_some() {
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
    h.press(KeyCode::Char('2')); // a copy of the selected profile
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
    assert!(h.app.top == Some(Top::None) && h.app.current_name().is_none());
    assert!(h.screen(110, 30).contains("Enter turns the active profile off"));
    h.press(KeyCode::Down);
    assert!(h.app.top.is_none() && h.app.current_name() == Some("aurora-glass"));
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
    assert!(h.app.top == Some(Top::None));
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

// ---- creating profiles -------------------------------------------------------------

impl Harness {
    fn write_ghostty(&self, file: &str, text: &str) {
        let d = self.app.store.paths.ghostty_dir();
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join(file), text).unwrap();
    }

    fn read_ghostty(&self, file: &str) -> String {
        fs::read_to_string(self.app.store.paths.ghostty_dir().join(file)).unwrap_or_default()
    }

    /// Start the flow with `n`, type the name and press Enter: the base chooser is now up.
    fn start_new(&mut self, name: &str) {
        self.app.tab = Tab::Profiles;
        self.press(KeyCode::Char('n'));
        self.type_text(name);
        self.press(KeyCode::Enter);
    }
}

#[test]
fn a_new_profile_row_is_first_in_the_list_and_the_footer_names_the_keys() {
    let mut h = harness();
    let screen = h.screen(120, 30);
    assert!(screen.contains("+ New profile"), "{screen}");
    let new_row = screen.lines().position(|l| l.contains("+ New profile")).unwrap();
    let none_row = screen.lines().position(|l| l.contains("(none)")).unwrap();
    let first_profile = screen.lines().position(|l| l.contains("│  aurora-glass")).unwrap();
    assert!(new_row < none_row && none_row < first_profile, "it is the first row:\n{screen}");
    for hint in ["n new profile", "d delete", "e export", "u off", "Enter apply"] {
        assert!(screen.contains(hint), "footer is missing '{hint}':\n{screen}");
    }
    // Up from the first profile reaches (none), then + New profile
    h.select_profile("aurora-glass");
    h.press(KeyCode::Up);
    assert_eq!(h.app.top, Some(Top::None));
    h.press(KeyCode::Up);
    assert_eq!(h.app.top, Some(Top::New));
    assert!(h.screen(120, 30).contains("then choose what it starts from"));
    // Enter on the row starts the flow, same as n
    h.press(KeyCode::Enter);
    assert!(h.app.input.is_some());
    assert!(h.screen(120, 30).contains("New profile: name"));
}

#[test]
fn name_validation_gives_a_clear_inline_error_and_keeps_the_box_open() {
    let mut h = harness();
    for (typed, expect) in [
        ("", "type a name"),
        ("   ", "type a name"),
        ("a/b", "letters, digits"),
        ("has space", "letters, digits"),
        (".hidden", "letters, digits"),
        ("shd", "already exists"),
    ] {
        h.app.tab = Tab::Profiles;
        h.press(KeyCode::Char('n'));
        h.type_text(typed);
        h.press(KeyCode::Enter);
        assert!(h.app.wizard.is_none(), "{typed:?} must not start the flow");
        let err = h.app.input.as_ref().unwrap().error.clone().unwrap();
        assert!(err.contains(expect), "{typed:?}: {err}");
        assert!(h.screen(120, 30).contains(expect), "the error is on screen for {typed:?}");
        h.press(KeyCode::Esc);
    }
    assert_eq!(h.app.profiles.len(), 4, "nothing was created");
    // a good name moves on to choosing the base
    h.start_new("fresh");
    assert!(h.app.wizard.is_some() && h.app.input.is_none());
    let screen = h.screen(120, 30);
    for line in ["your current Ghostty setup", "a copy of", "blank", "NOT applied"] {
        assert!(screen.contains(line), "{line}:\n{screen}");
    }
    h.press(KeyCode::Esc);
    assert!(h.app.wizard.is_none() && !h.app.profiles.contains(&"fresh".to_string()), "Esc cancels");
}

#[test]
fn blank_base_makes_an_empty_profile_that_is_selected_and_not_applied() {
    let mut h = harness();
    h.apply("shd");
    let (active_before, reloads) = (h.active_conf(), h.reloads.get());
    h.start_new("blank1");
    h.press(KeyCode::Char('3'));
    assert!(h.app.wizard.is_none());
    assert_eq!(h.app.current_name(), Some("blank1"), "it appears in the list and is selected");
    assert!(h.conf("blank1").lines().all(|l| l.starts_with('#')), "blank means no settings: {}", h.conf("blank1"));
    assert_eq!(h.app.active.as_deref(), Some("shd"), "the active profile did not change");
    assert_eq!(h.active_conf(), active_before, "nothing was applied");
    h.tick();
    assert_eq!(h.reloads.get(), reloads, "and Ghostty was not reloaded");
    assert!(h.app.status.text.contains("Not applied yet"));
    assert!(h.screen(120, 30).contains("blank1"));
}

#[test]
fn copy_base_duplicates_the_selected_profile_and_does_not_apply_it() {
    let mut h = harness();
    h.select_profile("crt-green");
    h.start_new("my-crt");
    h.press(KeyCode::Char('2'));
    assert_eq!(h.app.current_name(), Some("my-crt"));
    assert!(h.conf("my-crt").contains("background = #040a05"), "same settings as crt-green");
    assert!(h.conf("my-crt").contains("copy of crt-green"));
    assert!(h.app.store.active_name().is_none(), "nothing is applied");
    assert!(!h.app.store.paths.active_conf().exists(), "no generated config was written");
}

#[test]
fn copy_is_refused_when_no_profile_was_selected() {
    let mut h = harness();
    h.select_profile("aurora-glass");
    h.press(KeyCode::Up); // onto (none)
    h.start_new("c");
    h.press(KeyCode::Char('2'));
    assert!(h.app.wizard.is_some(), "stays in the chooser");
    assert!(h.app.status.text.contains("no selected profile"));
    assert!(!h.app.profiles.contains(&"c".to_string()));
    h.press(KeyCode::Char('3')); // blank still works
    assert!(h.app.profiles.contains(&"c".to_string()));
}

#[test]
fn current_setup_base_warns_then_moves_the_setup_into_the_profile_and_does_not_apply() {
    let mut h = harness();
    h.write_ghostty("config", "foreground = #111111\nkeybind = alt+t=toggle_quick_terminal\nfont-family = Menlo\n");
    h.write_ghostty(
        "config.ghostty",
        "background = #2c2c2c\nfont-size = 14\nshell-integration = zsh\npalette = 0=#0a0a0a\n",
    );
    let main_before = h.read_ghostty("config");

    h.start_new("mine");
    h.press(KeyCode::Char('1'));
    // the one-line warning is on screen before anything happens
    let screen = h.screen(120, 30);
    assert!(screen.contains("MOVES your appearance settings"), "{screen}");
    assert!(screen.contains("bak-pre-ghostty-profiles") && screen.contains("shows defaults"), "{screen}");
    assert_eq!(h.read_ghostty("config"), main_before, "nothing moved yet");
    assert!(!h.app.profiles.contains(&"mine".to_string()));

    // any other key goes back to the chooser, still nothing done
    h.press(KeyCode::Char('x'));
    assert!(h.screen(120, 30).contains("1  your current Ghostty setup"));
    assert_eq!(h.read_ghostty("config"), main_before);

    // confirm
    h.press(KeyCode::Char('1'));
    h.press(KeyCode::Char('y'));
    assert!(h.app.wizard.is_none());
    assert_eq!(h.app.current_name(), Some("mine"));
    let conf = h.conf("mine");
    assert!(conf.contains("background = #2c2c2c") && conf.contains("foreground = #111111"));
    assert!(
        conf.contains("font-family = Menlo") && conf.contains("font-size = 14") && conf.contains("palette = 0=#0a0a0a")
    );
    assert!(!conf.contains("keybind") && !conf.contains("shell-integration"), "only appearance moved: {conf}");
    // the user's files lost only appearance settings, and were backed up first
    assert_eq!(h.read_ghostty("config"), "keybind = alt+t=toggle_quick_terminal\n");
    assert_eq!(h.read_ghostty("config.ghostty"), "shell-integration = zsh\n");
    assert_eq!(h.read_ghostty("config.bak-pre-ghostty-profiles"), main_before);
    assert!(h.read_ghostty("config.ghostty.bak-pre-ghostty-profiles").contains("background = #2c2c2c"));
    // not applied: no active profile, no generated config, no include line, no reload
    assert_eq!(h.app.store.active_name(), None);
    assert!(!h.app.store.paths.active_conf().exists());
    assert!(!h.app.store.is_linked());
    h.tick();
    assert_eq!(h.reloads.get(), 0);
    assert!(h.app.status.text.contains("Not applied yet"));
}

#[test]
fn the_new_profile_is_edited_on_the_edit_tab_as_usual_and_apply_stays_explicit() {
    let mut h = harness();
    h.start_new("brand-new");
    h.press(KeyCode::Char('3'));
    h.go_to_field("background");
    h.press(KeyCode::Enter);
    h.type_text("#223344");
    h.press(KeyCode::Enter);
    assert!(h.conf("brand-new").contains("background = #223344"));
    assert!(h.app.status.text.contains("not the active profile"), "editing does not apply it");
    assert!(!h.app.store.paths.active_conf().exists());
    // Enter on it is what applies
    h.select_profile("brand-new");
    h.press(KeyCode::Enter);
    h.tick();
    assert_eq!(h.app.active.as_deref(), Some("brand-new"));
    assert!(h.active_conf().contains("background = #223344"));
}

#[test]
fn the_wizard_draws_at_small_sizes_without_panicking() {
    let mut h = harness();
    h.start_new("small");
    for (w, hh) in [(30, 10), (60, 14), (120, 40)] {
        assert!(h.screen(w, hh).contains("New profile") || w < 40, "{w}x{hh}");
    }
    h.press(KeyCode::Char('1'));
    for (w, hh) in [(30, 10), (60, 16), (120, 40)] {
        let _ = h.screen(w, hh);
    }
}

#[test]
fn delete_prompt_names_the_profile_and_says_images_go_too_and_presets_come_back() {
    let mut h = harness();
    h.select_profile("shd");
    h.apply("calm-dark"); // shd is not active now
    h.select_profile("shd");
    h.press(KeyCode::Char('d'));
    let text = h.app.status.text.clone();
    assert!(text.contains("'shd'") && text.contains("images and shaders"), "{text}");
    assert!(text.contains("bundled preset") && text.contains("install-presets"), "{text}");
    assert!(h.screen(140, 30).contains("images and shaders"), "and it is on screen");
    h.press(KeyCode::Char('y'));
    assert!(!h.app.profiles.contains(&"shd".to_string()));
    assert!(!h.app.store.profiles_dir().join("shd").exists(), "the folder, shaders included, is gone");

    // a user-made profile: no preset note, and its picture goes with it
    h.select_profile("crt-green");
    h.start_new("mine");
    h.press(KeyCode::Char('2'));
    let dir = h.app.store.profiles_dir().join("mine");
    fs::create_dir_all(dir.join("images")).unwrap();
    fs::write(dir.join("images/p.png"), "x").unwrap();
    h.select_profile("mine");
    h.press(KeyCode::Char('d'));
    assert!(!h.app.status.text.contains("bundled preset"), "{}", h.app.status.text);
    h.press(KeyCode::Char('y'));
    assert!(!dir.exists(), "images deleted too");
    // the New-profile help mentions delete
    h.select_profile("aurora-glass");
    h.press(KeyCode::Up);
    h.press(KeyCode::Up);
    assert!(h.screen(140, 30).contains("d deletes the selected profile"));
}

#[test]
fn copying_a_profile_in_the_tui_copies_its_picture_and_shaders_too() {
    let mut h = harness();
    // give shd a background picture the way the Edit tab does
    let img = h.td.path().join("wallpaper.png");
    fs::write(&img, "PIXELS").unwrap();
    h.apply("shd");
    h.go_to_field("background-image");
    h.press(KeyCode::Enter);
    h.type_text(&img.display().to_string());
    h.press(KeyCode::Enter);
    h.go_to_field("background-image-opacity");
    h.press(KeyCode::Enter);
    h.type_text("0.3");
    h.press(KeyCode::Enter);

    h.select_profile("shd");
    h.start_new("shd-copy");
    h.press(KeyCode::Char('2'));
    let copy = h.app.store.profiles_dir().join("shd-copy");
    assert_eq!(fs::read_to_string(copy.join("images/wallpaper.png")).unwrap(), "PIXELS", "the picture came along");
    assert!(copy.join("shaders/xmb-waves.glsl").is_file(), "and the shader");
    let conf = h.conf("shd-copy");
    assert!(
        conf.contains("background-image = images/wallpaper.png") && conf.contains("background-image-opacity = 0.3"),
        "{conf}"
    );
    assert_eq!(h.app.store.active_name().as_deref(), Some("shd"), "still not applied");
    // the copy is independent of the original
    h.select_profile("calm-dark");
    h.apply("calm-dark");
    h.select_profile("shd");
    h.press(KeyCode::Char('d'));
    h.press(KeyCode::Char('y'));
    assert!(copy.join("images/wallpaper.png").is_file());
}

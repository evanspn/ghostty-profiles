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
    assert_eq!(h.app.profiles, crate::presets::profile_names());
    assert!(h.app.profiles.len() >= 11, "{:?}", h.app.profiles);
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

fn shader_files(h: &Harness, profile: &str) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(h.app.store.profiles_dir().join(profile).join("shaders"))
        .map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    v.sort();
    v
}

#[test]
fn a_profile_has_one_shader_selecting_replaces_it_and_none_removes_it() {
    let mut h = harness();
    h.select_profile("calm-dark");
    h.app.tab = Tab::Shaders;
    let row = |h: &Harness, n: &str| h.app.shader_rows.iter().position(|r| r.name == n).unwrap();
    assert!(h.app.shader_rows[0].none && !h.app.shader_rows[0].enabled, "the (none) row is first");
    assert!(h.app.shader_rows.iter().any(|r| r.name == "soft-glow.glsl" && r.enabled), "calm-dark uses soft-glow");
    assert_eq!(h.app.shader_rows.iter().filter(|r| r.enabled).count(), 1, "radio: exactly one is selected");
    assert_eq!(shader_files(&h, "calm-dark"), vec!["soft-glow.glsl"]);

    // Enter on the one in use changes nothing
    h.app.shader_sel = row(&h, "soft-glow.glsl");
    h.press(KeyCode::Enter);
    assert!(h.app.status.text.contains("already"));

    // selecting another REPLACES it: the line, and the old one's files
    h.app.shader_sel = row(&h, "aurora.glsl");
    h.press(KeyCode::Enter);
    let conf = h.conf("calm-dark");
    assert_eq!(conf.matches("custom-shader = ").count(), 1, "{conf}");
    assert!(
        conf.contains("custom-shader = shaders/aurora.glsl") && conf.contains("custom-shader-animation = true"),
        "{conf}"
    );
    assert_eq!(shader_files(&h, "calm-dark"), vec!["aurora.glsl"], "the old rendered copy is gone");
    let screen = h.screen(110, 30);
    assert!(
        screen.contains("(*) aurora.glsl") && screen.contains("( ) soft-glow.glsl") && screen.contains("( ) (none)"),
        "{screen}"
    );

    // animation toggle belongs to the active shader
    h.press(KeyCode::Char('a'));
    assert!(h.conf("calm-dark").contains("custom-shader-animation = false"));
    h.press(KeyCode::Char('a'));
    assert!(h.conf("calm-dark").contains("custom-shader-animation = true"));

    // another replaces again, and (none) removes the shader and the animation flag
    h.app.shader_sel = row(&h, "crt-scanlines.glsl");
    h.press(KeyCode::Char(' '));
    assert_eq!(h.conf("calm-dark").matches("custom-shader = ").count(), 1);
    assert_eq!(shader_files(&h, "calm-dark"), vec!["crt-scanlines.glsl"]);
    h.app.shader_sel = 0;
    h.press(KeyCode::Enter);
    let conf = h.conf("calm-dark");
    assert!(!conf.contains("custom-shader"), "{conf}");
    assert!(shader_files(&h, "calm-dark").is_empty(), "nothing left in shaders/: {:?}", shader_files(&h, "calm-dark"));
    assert!(h.app.shader_rows[0].enabled, "(none) is now the selected one");
}

#[test]
fn browsing_shaders_creates_no_files_in_the_profile() {
    let mut h = harness();
    h.select_profile("calm-dark");
    h.app.tab = Tab::Shaders;
    let before = shader_files(&h, "calm-dark");
    let conf = h.conf("calm-dark");
    h.app.shader_sel = 0;
    h.press(KeyCode::Char('4'));
    for _ in 0..h.app.shader_rows.len() + 3 {
        h.press(KeyCode::Down);
        let _ = h.screen(120, 30);
        // a bundled shader that is not in use shows its parameters read-only
        if h.app.shader_rows[h.app.shader_sel].in_library && !h.app.shader_rows[h.app.shader_sel].enabled {
            assert!(h.app.preview.is_some(), "{}", h.app.shader_rows[h.app.shader_sel].name);
            assert!(h.screen(120, 30).contains("Enter selects it"));
        }
    }
    assert_eq!(shader_files(&h, "calm-dark"), before, "browsing wrote nothing");
    assert_eq!(h.conf("calm-dark"), conf);
    assert!(!h.app.store.profiles_dir().join("calm-dark/shaders/aurora.params").exists());
    // moving the cursor onto the preview and pressing the parameter keys changes nothing either
    for k in [KeyCode::Right, KeyCode::Char('R')] {
        h.press(k);
    }
    assert_eq!(shader_files(&h, "calm-dark"), before);
}

#[test]
fn only_shaders_the_user_wrote_are_listed_under_your_shaders_never_params_or_generated_files() {
    let mut h = harness();
    let dir = h.app.store.profiles_dir().join("shd/shaders");
    // stale generated stuff and a params file with defaults, plus a shader the user wrote
    fs::write(dir.join("snow.glsl"), include_str!("../../presets/shaders/snow.glsl")).unwrap();
    fs::write(dir.join("snow.params"), "opacity = 0.6\n").unwrap();
    fs::write(dir.join("mine.glsl"), "void mainImage(out vec4 c, in vec2 p) { c = vec4(1.0); }\n").unwrap();
    h.select_profile("shd"); // loading the profile tidies its folder
    h.app.tab = Tab::Shaders;
    let names: Vec<_> = h.app.shader_rows.iter().map(|r| r.name.clone()).collect();
    assert!(names.contains(&"mine.glsl".to_string()), "{names:?}");
    assert!(!names.iter().any(|n| n.ends_with(".params")), "{names:?}");
    assert_eq!(h.app.shader_rows.iter().filter(|r| r.user).count(), 1);
    let screen = h.screen(120, 40);
    assert!(
        screen.contains("── Your shaders") && screen.contains("mine.glsl") && !screen.contains(".params"),
        "{screen}"
    );
    assert!(dir.join("mine.glsl").exists(), "what the user wrote is never deleted");
    assert!(
        !dir.join("snow.glsl").exists() && !dir.join("snow.params").exists(),
        "the generated copy and the default params were tidied away"
    );
    // the user's own shader can be selected like any other, and replaces the active one
    h.app.shader_sel = h.app.shader_rows.iter().position(|r| r.name == "mine.glsl").unwrap();
    h.press(KeyCode::Enter);
    assert!(h.conf("shd").contains("custom-shader = shaders/mine.glsl"));
    assert!(dir.join("mine.glsl").exists());
    assert!(!dir.join("xmb-waves.glsl").exists(), "the replaced generated shader is gone");
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
    for hint in ["n new profile", "r rename", "d delete", "e export", "u off", "Enter apply"] {
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
    assert_eq!(h.app.profiles, crate::presets::profile_names(), "nothing was created");
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
    assert!(h.screen(140, 30).contains("r renames, d deletes"));
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

// ---- renaming ---------------------------------------------------------------------

#[test]
fn r_renames_the_selected_profile_and_keeps_it_selected() {
    let mut h = harness();
    h.select_profile("crt-green");
    assert!(h.screen(140, 30).contains("r rename"), "the footer lists the key");
    h.press(KeyCode::Char('r'));
    let input = h.app.input.as_ref().unwrap();
    assert_eq!(input.buf, "crt-green", "pre-filled with the current name");
    assert!(h.screen(140, 30).contains("Rename 'crt-green'"));
    for _ in 0..9 {
        h.press(KeyCode::Backspace);
    }
    h.type_text("my-crt");
    h.press(KeyCode::Enter);
    assert!(h.app.input.is_none());
    assert!(h.app.profiles.contains(&"my-crt".to_string()) && !h.app.profiles.contains(&"crt-green".to_string()));
    assert_eq!(h.app.current_name(), Some("my-crt"), "the renamed profile stays selected");
    assert!(h.app.status.text.contains("renamed 'crt-green' to 'my-crt'"));
    assert!(h.screen(140, 30).contains("my-crt"));
    assert!(h.conf("my-crt").contains("background = #040a05"));
    assert!(!h.app.store.paths.active_conf().exists(), "renaming an inactive profile applies nothing");
}

#[test]
fn renaming_the_active_profile_keeps_the_marker_rerenders_and_reloads() {
    let mut h = harness();
    h.apply("shd");
    let reloads = h.reloads.get();
    h.press(KeyCode::Char('r'));
    for _ in 0..3 {
        h.press(KeyCode::Backspace);
    }
    h.type_text("ember");
    h.press(KeyCode::Enter);
    assert_eq!(h.app.active.as_deref(), Some("ember"));
    assert_eq!(h.app.store.active_name().as_deref(), Some("ember"));
    let screen = h.screen(140, 30);
    assert!(screen.contains("● ember") && !screen.contains("● shd"), "the marker is on the renamed profile:\n{screen}");
    let shader = h.app.store.profiles_dir().join("ember/shaders/xmb-waves.glsl");
    assert!(h.active_conf().contains(&format!("custom-shader = {}", shader.display())), "re-rendered");
    assert!(shader.is_file());
    assert!(h.app.status.text.contains("bundled preset"), "{}", h.app.status.text);
    assert!(h.app.reload_pending());
    h.tick();
    assert_eq!(h.reloads.get(), reloads + 1, "the reload hook fired");
    // edits keep going to the renamed profile and still hot-reload
    h.go_to_field("cursor-style");
    h.press(KeyCode::Right);
    h.tick();
    assert!(h.conf("ember").contains("cursor-style = block"));
    assert_eq!(h.reloads.get(), reloads + 2);
}

#[test]
fn rename_errors_are_inline_and_esc_cancels() {
    let mut h = harness();
    h.select_profile("calm-dark");
    for (typed, expect) in [("shd", "already exists"), ("a/b", "letters, digits"), ("has space", "letters, digits")] {
        h.press(KeyCode::Char('r'));
        for _ in 0..9 {
            h.press(KeyCode::Backspace);
        }
        h.type_text(typed);
        h.press(KeyCode::Enter);
        let err = h.app.input.as_ref().expect("the box stays open").error.clone().unwrap();
        assert!(err.contains(expect), "{typed}: {err}");
        assert!(h.screen(140, 30).contains(expect));
        h.press(KeyCode::Esc);
        assert!(h.app.input.is_none());
    }
    // an empty name
    h.press(KeyCode::Char('r'));
    for _ in 0..9 {
        h.press(KeyCode::Backspace);
    }
    h.press(KeyCode::Enter);
    assert!(h.app.input.as_ref().unwrap().error.as_deref().unwrap().contains("type a name"));
    h.press(KeyCode::Esc);
    // unchanged name just closes
    h.press(KeyCode::Char('r'));
    h.press(KeyCode::Enter);
    assert!(h.app.input.is_none() && h.app.status.text.contains("unchanged"));
    assert_eq!(h.app.profiles, crate::presets::profile_names(), "nothing was renamed");
    // on the New-profile / (none) rows there is nothing to rename
    h.press(KeyCode::Up);
    h.press(KeyCode::Up);
    assert!(h.app.top.is_some());
    h.press(KeyCode::Char('r'));
    assert!(h.app.input.is_none() && h.app.status.text.contains("select a profile"));
}

#[test]
fn renaming_moves_the_picture_and_shaders_with_the_profile_in_the_tui() {
    let mut h = harness();
    let img = h.td.path().join("wallpaper.png");
    fs::write(&img, "PIXELS").unwrap();
    h.select_profile("calm-dark");
    h.go_to_field("background-image");
    h.press(KeyCode::Enter);
    h.type_text(&img.display().to_string());
    h.press(KeyCode::Enter);
    h.select_profile("calm-dark");
    h.press(KeyCode::Char('r'));
    for _ in 0..9 {
        h.press(KeyCode::Backspace);
    }
    h.type_text("calm-pic");
    h.press(KeyCode::Enter);
    let dir = h.app.store.profiles_dir().join("calm-pic");
    assert_eq!(fs::read_to_string(dir.join("images/wallpaper.png")).unwrap(), "PIXELS");
    assert!(dir.join("shaders/soft-glow.glsl").is_file());
    assert!(!h.app.store.profiles_dir().join("calm-dark").exists());
}

// ---- the color picker ----------------------------------------------------------------------

use super::app::Comp;
use super::color::{self, Hsv, WheelGeom};
use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::style::Color;

impl Harness {
    fn buffer(&self, w: u16, h: u16) -> Buffer {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| ui::draw(f, &self.app)).unwrap();
        t.backend().buffer().clone()
    }

    fn mouse(&mut self, kind: MouseEventKind, col: u16, row: u16) {
        self.app.on_mouse(MouseEvent { kind, column: col, row, modifiers: KeyModifiers::NONE });
    }

    fn down(&mut self, col: u16, row: u16) {
        self.mouse(MouseEventKind::Down(MouseButton::Left), col, row);
    }

    fn drag(&mut self, col: u16, row: u16) {
        self.mouse(MouseEventKind::Drag(MouseButton::Left), col, row);
    }

    fn up(&mut self, col: u16, row: u16) {
        self.mouse(MouseEventKind::Up(MouseButton::Left), col, row);
    }

    /// Apply shd, open the picker on `key` and draw it once so its areas are known.
    fn open_picker_on(&mut self, key: &str) -> Rect {
        self.apply("shd");
        self.go_to_field(key);
        self.press(KeyCode::Char('p'));
        assert!(self.app.picker.is_some(), "the picker opened");
        let _ = self.screen(110, 40);
        self.app.picker_rects.borrow().wheel.expect("the wheel was drawn")
    }

    fn buf_text(&self) -> String {
        self.app.input.as_ref().unwrap().buf.clone()
    }

    fn picker_hsv(&self) -> Hsv {
        self.app.picker.as_ref().unwrap().hsv
    }
}

fn geom(w: Rect) -> WheelGeom {
    WheelGeom { cols: w.width, rows: w.height }
}

#[test]
fn p_opens_the_picker_with_the_current_hex_and_the_marker_in_the_right_place() {
    let mut h = harness();
    let wheel = h.open_picker_on("cursor-color"); // shd's cursor color is #ff6a00
    assert_eq!(h.buf_text(), "#ff6a00", "the hex is shown in the field's input box");
    let hsv = color::hex_to_hsv("#ff6a00").unwrap();
    assert!((h.picker_hsv().h - hsv.h).abs() < 0.01);
    assert_eq!(wheel.width, wheel.height * 2, "a round wheel: twice as many cells wide as tall");

    let buf = h.buffer(110, 40);
    let (_, _, mc, mr) = geom(wheel).marker(hsv.h, hsv.s);
    let cell = &buf[(wheel.x + mc, wheel.y + mr)];
    assert_eq!(cell.symbol(), "+", "the crosshair is on the current color");
    let (r, g, b) = color::hsv_to_rgb(Hsv { h: hsv.h, s: hsv.s, v: 1.0 });
    assert_eq!(cell.bg, Color::Rgb(r, g, b), "and sits on the wheel color for that hue/saturation");
    // exactly one marker
    let crosses = (0..wheel.height)
        .flat_map(|y| (0..wheel.width).map(move |x| (x, y)))
        .filter(|(x, y)| buf[(wheel.x + x, wheel.y + y)].symbol() == "+")
        .count();
    assert_eq!(crosses, 1);

    // the wheel is a real circle drawn with half blocks in truecolor; its corners are empty
    let painted = |x: u16, y: u16| matches!(buf[(wheel.x + x, wheel.y + y)].symbol(), "▀" | "▄" | "+");
    assert!(painted(wheel.width / 2, wheel.height / 2) && painted(wheel.width - 3, wheel.height / 2));
    for (x, y) in [(0, 0), (wheel.width - 1, 0), (0, wheel.height - 1), (wheel.width - 1, wheel.height - 1)] {
        assert!(!painted(x, y), "corner ({x},{y}) is outside the circle");
    }
    let rgb_cells = (0..wheel.height)
        .flat_map(|y| (0..wheel.width).map(move |x| (x, y)))
        .filter(|(x, y)| matches!(buf[(wheel.x + x, wheel.y + y)].fg, Color::Rgb(..)))
        .count();
    assert!(rgb_cells > 100, "truecolor cells: {rgb_cells}");
    // brightness bar + preview + hint are on screen
    let text = text_of(&buf);
    assert!(text.contains("brightness") && text.contains("#ff6a00") && text.contains("Enter accept"), "{text}");
    // the footer on the Edit tab names the picker key
    h.press(KeyCode::Esc);
    assert!(h.screen(140, 30).contains("p color picker"));
}

#[test]
fn p_on_a_field_that_is_not_a_color_says_so_and_opens_nothing() {
    let mut h = harness();
    h.apply("shd");
    h.go_to_field("font-size");
    h.press(KeyCode::Char('p'));
    assert!(h.app.picker.is_none() && h.app.input.is_none());
    assert!(h.app.status.text.contains("color fields"));
}

#[test]
fn clicking_the_wheel_picks_hue_and_saturation_and_updates_the_hex_live() {
    let mut h = harness();
    let wheel = h.open_picker_on("cursor-color");
    let g = geom(wheel);
    let v = h.picker_hsv().v;
    let (col, row) = (wheel.width - 4, wheel.height / 2); // right of center: red-ish, fairly saturated
    let (hue, sat) = g.hit(col as i32, row as i32).unwrap();
    h.down(wheel.x + col, wheel.y + row);
    assert_eq!(h.buf_text(), color::hsv_to_hex(Hsv { h: hue, s: sat, v }), "the input box follows the click");
    assert!(h.app.input.is_some(), "still open: nothing is accepted by a click");
    // the marker moved with it
    let buf = h.buffer(110, 40);
    let (_, _, mc, mr) = g.marker(hue, sat);
    assert_eq!(buf[(wheel.x + mc, wheel.y + mr)].symbol(), "+");
    // a click far left gives a different hue
    let before = h.buf_text();
    h.up(wheel.x + col, wheel.y + row);
    h.down(wheel.x + 3, wheel.y + wheel.height / 2);
    assert_ne!(h.buf_text(), before);
    assert!((h.picker_hsv().h - 180.0).abs() < 25.0, "{}", h.picker_hsv().h);
}

#[test]
fn dragging_follows_the_mouse_clamps_at_the_rim_and_stops_on_release() {
    let mut h = harness();
    let wheel = h.open_picker_on("background");
    let (cx, cy) = (wheel.x + wheel.width / 2, wheel.y + wheel.height / 2);
    h.down(cx + 2, cy);
    let first = h.buf_text();
    h.drag(cx, cy - 3);
    let second = h.buf_text();
    h.drag(cx - 6, cy);
    let third = h.buf_text();
    assert!(first != second && second != third, "{first} {second} {third}");
    // dragging out of the circle (and out of the popup) clamps to the rim: full saturation, no jump
    h.drag(wheel.x + wheel.width + 30, cy);
    assert!(h.picker_hsv().s == 1.0 && (h.picker_hsv().h < 12.0 || h.picker_hsv().h > 348.0), "{:?}", h.picker_hsv());
    h.drag(cx, wheel.y.saturating_sub(5));
    assert!((h.picker_hsv().h - 90.0).abs() < 10.0 && h.picker_hsv().s == 1.0);
    // release ends the drag: later motion is ignored
    h.up(cx, cy);
    let settled = h.buf_text();
    h.drag(cx - 6, cy);
    assert_eq!(h.buf_text(), settled);
}

#[test]
fn clicks_outside_the_circle_are_ignored_and_never_jump_the_color() {
    let mut h = harness();
    let wheel = h.open_picker_on("cursor-color");
    let before = (h.buf_text(), h.picker_hsv());
    // the corners of the wheel's box, just outside the circle
    for (x, y) in [(0, 0), (wheel.width - 1, 0), (0, wheel.height - 1), (wheel.width - 1, wheel.height - 1)] {
        h.down(wheel.x + x, wheel.y + y);
        assert_eq!((h.buf_text(), h.picker_hsv()), before, "corner ({x},{y})");
        assert!(h.app.picker.as_ref().unwrap().drag.is_none(), "no drag started from outside");
        h.drag(wheel.x + wheel.width / 2, wheel.y + wheel.height / 2);
        assert_eq!(h.buf_text(), before.0, "a drag that began outside does nothing");
        h.up(wheel.x + x, wheel.y + y);
    }
    // clicks nowhere near the picker (the title bar, the far corner)
    for (x, y) in [(0, 0), (109, 39), (50, 1)] {
        h.down(x, y);
        assert_eq!(h.buf_text(), before.0);
    }
}

#[test]
fn the_brightness_bar_sets_value_by_click_and_drag() {
    let mut h = harness();
    let _ = h.open_picker_on("cursor-color");
    let (comp, bar) = h.app.picker_rects.borrow().bars[0];
    assert_eq!(comp, Comp::Val);
    let hue_before = h.picker_hsv().h;
    h.down(bar.x, bar.y);
    assert_eq!(h.buf_text(), "#000000", "the left end is black");
    assert!((h.picker_hsv().h - hue_before).abs() < 0.001, "hue is kept");
    h.drag(bar.x + bar.width - 1, bar.y);
    let full = color::hsv_to_hex(Hsv { v: 1.0, ..h.picker_hsv() });
    assert_eq!(h.buf_text(), full, "the right end is full brightness");
    h.drag(bar.x + bar.width / 2, bar.y + 1);
    assert!((h.picker_hsv().v - 0.5).abs() < 0.06, "{}", h.picker_hsv().v);
    h.drag(bar.x + bar.width + 40, bar.y);
    assert_eq!(h.picker_hsv().v, 1.0, "past the end clamps");
    h.drag(0, bar.y);
    assert_eq!(h.picker_hsv().v, 0.0, "past the start clamps");
    // brightness drags never moved the marker's hue/saturation
    assert!((h.picker_hsv().h - hue_before).abs() < 0.001);
}

#[test]
fn nothing_saves_or_reloads_while_dragging_only_on_accept() {
    let mut h = harness();
    let wheel = h.open_picker_on("cursor-color");
    let saved = h.conf("shd");
    let active = h.active_conf();
    let reloads = h.reloads.get();
    h.down(wheel.x + 3, wheel.y + 4);
    for i in 0..30 {
        h.drag(wheel.x + 2 + i % 9, wheel.y + 3 + i % 5);
        h.tick();
    }
    h.up(wheel.x + 4, wheel.y + 4);
    h.tick();
    assert_eq!(h.reloads.get(), reloads, "no hot reload for any of those drag events");
    assert_eq!(h.conf("shd"), saved, "and nothing was written");
    assert_eq!(h.active_conf(), active);
    assert!(!h.app.reload_pending());

    // Enter accepts: saved, re-rendered, and the normal debounced reload happens
    let accepted = h.buf_text();
    h.press(KeyCode::Enter);
    assert!(h.app.input.is_none() && h.app.picker.is_none(), "the picker closes");
    assert!(h.conf("shd").contains(&format!("cursor-color = {accepted}")), "{accepted}");
    assert!(h.active_conf().contains(&format!("cursor-color = {accepted}")));
    assert!(h.app.reload_pending());
    h.tick();
    assert_eq!(h.reloads.get(), reloads + 1, "exactly one reload, after accept");
}

#[test]
fn esc_cancels_and_leaves_the_previous_value() {
    let mut h = harness();
    let wheel = h.open_picker_on("cursor-color");
    let saved = h.conf("shd");
    let reloads = h.reloads.get();
    h.down(wheel.x + 3, wheel.y + 5);
    assert_ne!(h.buf_text(), "#ff6a00");
    h.press(KeyCode::Esc);
    assert!(h.app.picker.is_none() && h.app.input.is_none());
    assert_eq!(h.conf("shd"), saved, "the previous value is still there");
    h.tick();
    assert_eq!(h.reloads.get(), reloads);
    assert_eq!(h.app.field_value(h.app.field_sel).as_deref(), Some("#ff6a00"));
    // mouse events after closing do nothing: the wheel is gone (re-draw first, as the real loop does)
    let _ = h.screen(110, 40);
    assert!(h.app.picker_rects.borrow().wheel.is_none(), "no stale wheel area is left behind");
    h.down(100, 30);
    assert!(h.app.picker.is_none() && h.app.input.is_none());
    let _ = wheel;
}

#[test]
fn the_keyboard_works_without_a_mouse() {
    let mut h = harness();
    let _ = h.open_picker_on("cursor-color");
    let (h0, s0, v0) = (h.picker_hsv().h, h.picker_hsv().s, h.picker_hsv().v);
    h.press(KeyCode::Right);
    assert!((h.picker_hsv().h - (h0 + 5.0)).abs() < 0.01);
    h.press(KeyCode::Left);
    h.press(KeyCode::Left);
    assert!((h.picker_hsv().h - (h0 - 5.0)).abs() < 0.01);
    h.app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT));
    assert!((h.picker_hsv().h - (h0 + 15.0)).abs() < 0.01, "Shift makes bigger steps");
    h.press(KeyCode::Down);
    assert!((h.picker_hsv().s - (s0 - 0.05)).abs() < 0.001);
    h.press(KeyCode::Up);
    h.press(KeyCode::Up);
    assert_eq!(h.picker_hsv().s, 1.0, "saturation stops at 1");
    h.press(KeyCode::Char('['));
    assert!((h.picker_hsv().v - (v0 - 0.03)).abs() < 0.001);
    h.press(KeyCode::Char('-'));
    h.press(KeyCode::Char(']'));
    h.press(KeyCode::Char('+'));
    h.press(KeyCode::Char('='));
    assert!((h.picker_hsv().v - (v0 + 0.03).min(1.0)).abs() < 0.001);
    h.press(KeyCode::Char('{'));
    assert!((h.picker_hsv().v - (1.0 - 0.12)).abs() < 0.001, "{}", h.picker_hsv().v);
    assert_eq!(h.buf_text(), color::hsv_to_hex(h.picker_hsv()), "the box always shows the color");
    // hue wraps around
    for _ in 0..80 {
        h.press(KeyCode::Right);
    }
    assert!((0.0..360.0).contains(&h.picker_hsv().h));
    let chosen = color::hsv_to_hex(h.picker_hsv());
    h.press(KeyCode::Enter);
    assert!(h.conf("shd").contains(&format!("cursor-color = {chosen}")), "accepted from the keyboard alone");
}

#[test]
fn typing_a_hex_moves_the_marker_and_a_bad_hex_is_refused_with_the_picker_kept() {
    let mut h = harness();
    let wheel = h.open_picker_on("cursor-color");
    for _ in 0..7 {
        h.press(KeyCode::Backspace);
    }
    h.type_text("#0000ff");
    assert!((h.picker_hsv().h - 240.0).abs() < 0.01 && h.picker_hsv().s == 1.0, "{:?}", h.picker_hsv());
    let buf = h.buffer(110, 40);
    let (_, _, mc, mr) = geom(wheel).marker(240.0, 1.0);
    assert_eq!(buf[(wheel.x + mc, wheel.y + mr)].symbol(), "+", "the crosshair moved to blue");
    // half-typed hex does not move it
    let at = h.picker_hsv();
    h.press(KeyCode::Backspace);
    h.press(KeyCode::Backspace);
    assert_eq!(h.picker_hsv(), at, "an incomplete hex leaves the marker alone");
    h.type_text("zz");
    h.press(KeyCode::Enter);
    assert!(h.app.input.is_some() && h.app.picker.is_some(), "refused: both stay open");
    assert!(h.app.input.as_ref().unwrap().error.as_deref().unwrap().contains("not a color"));
    // fix it and accept
    for _ in 0..8 {
        h.press(KeyCode::Backspace);
    }
    h.type_text("#00ff00");
    h.press(KeyCode::Enter);
    assert!(h.conf("shd").contains("cursor-color = #00ff00"));
    // a typed gray keeps the hue instead of snapping to red
    h.press(KeyCode::Char('p'));
    for _ in 0..7 {
        h.press(KeyCode::Backspace);
    }
    h.type_text("#80808"); // (passing through valid 3-digit hexes on the way moves the marker; that is fine)
    let hue = h.picker_hsv().h;
    h.type_text("0");
    assert_eq!(h.buf_text(), "#808080");
    assert_eq!(h.picker_hsv().s, 0.0);
    assert_eq!(h.picker_hsv().h, hue, "a typed gray keeps the hue instead of snapping to red");
}

#[test]
fn clicking_a_color_swatch_in_the_edit_list_opens_the_picker_for_that_field() {
    let mut h = harness();
    h.apply("shd");
    h.app.tab = Tab::Edit;
    let _ = h.screen(110, 40);
    let rects = h.app.swatch_rects.borrow().clone();
    // 6 colors + 16 palette entries are clickable
    assert_eq!(rects.len(), 22, "{rects:?}");
    let cursor = h.app.fields.iter().position(|f| f.key == "cursor-color").unwrap();
    let (r, _) = rects.iter().find(|(_, i)| *i == cursor).copied().unwrap();
    // the swatch itself is at the right end of the hit area
    let buf = h.buffer(110, 40);
    assert_eq!(buf[(r.x + r.width - 1, r.y)].symbol(), "█", "the hit area ends on the swatch");
    h.down(r.x + r.width - 1, r.y);
    assert!(h.app.picker.is_some());
    assert_eq!(h.app.field_sel, cursor);
    assert_eq!(h.buf_text(), "#ff6a00");
    h.press(KeyCode::Esc);

    // a click on a non-color row or on the label does nothing
    let font = h.app.fields.iter().position(|f| f.key == "font-size").unwrap();
    assert!(!rects.iter().any(|(_, i)| *i == font));
    h.down(r.x - 5, r.y);
    assert!(h.app.picker.is_none());
    // clicks on other tabs do nothing
    h.app.tab = Tab::Shaders;
    h.down(r.x + 1, r.y);
    assert!(h.app.picker.is_none());
}

#[test]
fn a_palette_swatch_opens_the_picker_and_accepting_writes_that_palette_entry() {
    let mut h = harness();
    h.apply("shd");
    h.app.tab = Tab::Edit;
    let _ = h.screen(110, 60);
    let idx = h.app.fields.iter().position(|f| f.kind == super::fields::Kind::Palette(3)).unwrap();
    let (r, _) = h.app.swatch_rects.borrow().iter().find(|(_, i)| *i == idx).copied().unwrap();
    h.down(r.x + r.width - 1, r.y);
    assert!(h.app.picker.is_some());
    for _ in 0..7 {
        h.press(KeyCode::Backspace);
    }
    h.type_text("#123456");
    h.press(KeyCode::Enter);
    assert!(h.conf("shd").contains("palette = 3=#123456"));
}

#[test]
fn without_truecolor_the_picker_uses_the_256_color_palette() {
    let mut h = harness();
    h.app.truecolor = false;
    let wheel = h.open_picker_on("cursor-color");
    let buf = h.buffer(110, 40);
    let mut indexed = 0;
    for y in 0..wheel.height {
        for x in 0..wheel.width {
            let c = &buf[(wheel.x + x, wheel.y + y)];
            assert!(
                !matches!(c.fg, Color::Rgb(..)) && !matches!(c.bg, Color::Rgb(..)),
                "no 24-bit color anywhere on the wheel"
            );
            if matches!(c.fg, Color::Indexed(_)) {
                indexed += 1;
            }
        }
    }
    assert!(indexed > 100, "{indexed}");
    // the whole screen has no 24-bit colors either, swatches included
    assert!(buf.content().iter().all(|c| !matches!(c.fg, Color::Rgb(..)) && !matches!(c.bg, Color::Rgb(..))));
    // and it still works
    h.down(wheel.x + 3, wheel.y + 5);
    assert_ne!(h.buf_text(), "#ff6a00");
}

#[test]
fn small_terminals_shrink_the_wheel_then_fall_back_to_sliders_and_never_panic() {
    let mut h = harness();
    h.apply("shd");
    h.go_to_field("cursor-color");
    h.press(KeyCode::Char('p'));
    for (w, hh) in [
        (110, 40),
        (80, 24),
        (60, 20),
        (50, 18),
        (40, 16),
        (36, 14),
        (30, 12),
        (24, 10),
        (20, 8),
        (14, 7),
        (12, 6),
        (10, 5),
        (8, 4),
        (3, 3),
        (1, 1),
    ] {
        let _ = h.screen(w, hh);
        let r = h.app.picker_rects.borrow().clone();
        if let Some(wheel) = r.wheel {
            assert_eq!(wheel.width, wheel.height * 2, "{w}x{hh}: still round");
            assert!(wheel.height >= 5);
        }
    }
    // a short terminal gets sliders instead of a wheel: the three components, each clickable
    let _ = h.screen(50, 16);
    let r = h.app.picker_rects.borrow().clone();
    assert!(r.wheel.is_none(), "too small for a wheel");
    assert_eq!(r.bars.iter().map(|(c, _)| *c).collect::<Vec<_>>(), vec![Comp::Hue, Comp::Sat, Comp::Val]);
    let hue_bar = r.bars[0].1;
    h.down(hue_bar.x + hue_bar.width - 1, hue_bar.y);
    assert!(h.picker_hsv().h > 350.0, "the right end of the hue bar is the end of the spectrum");
    h.down(hue_bar.x, hue_bar.y);
    assert_eq!(h.picker_hsv().h, 0.0);
    let sat_bar = r.bars[1].1;
    h.down(sat_bar.x, sat_bar.y);
    assert_eq!(h.picker_hsv().s, 0.0);
    h.drag(sat_bar.x + sat_bar.width / 2, sat_bar.y);
    assert!((h.picker_hsv().s - 0.5).abs() < 0.1);
    let val_bar = r.bars[2].1;
    h.up(0, 0);
    h.down(val_bar.x + val_bar.width - 1, val_bar.y);
    assert_eq!(h.picker_hsv().v, 1.0);
    assert_eq!(h.buf_text(), color::hsv_to_hex(h.picker_hsv()));
    // the hex box still works at any size, and accepting works
    h.press(KeyCode::Enter);
    assert!(h.app.picker.is_none());
}

// ---- shader parameters in the Shaders tab ----------------------------------------------------

impl Harness {
    /// Apply `profile`, open the Shaders tab with `shader` selected (so its parameters are showing).
    fn open_shader(&mut self, profile: &str, shader: &str) {
        self.apply(profile);
        self.app.tab = Tab::Shaders;
        self.app.shader_sel = self.app.shader_rows.iter().position(|r| r.name == format!("{shader}.glsl")).unwrap();
        self.press(KeyCode::Char('4'));
        assert!(self.app.params.is_some(), "{shader} is enabled in {profile} and has parameters");
    }

    fn shader_text(&self, profile: &str, shader: &str) -> String {
        fs::read_to_string(self.app.store.profiles_dir().join(profile).join(format!("shaders/{shader}.glsl"))).unwrap()
    }

    fn param_value(&self, name: &str) -> String {
        let st = self.app.params.as_ref().unwrap();
        let i = st.schema.params.iter().position(|p| p.name == name).unwrap();
        st.values[i].clone()
    }

    fn go_to_param(&mut self, name: &str) {
        let st = self.app.params.clone().unwrap();
        let want =
            st.schema.params.iter().position(|p| p.name == name).unwrap() + usize::from(!st.schema.presets.is_empty());
        if !self.app.param_focus {
            self.press(KeyCode::Right);
        }
        while self.app.param_sel < want {
            self.press(KeyCode::Down);
        }
        while self.app.param_sel > want {
            self.press(KeyCode::Up);
        }
    }
}

#[test]
fn the_shader_list_shows_an_enabled_shaders_parameters_with_values_and_swatches() {
    let mut h = harness();
    h.open_shader("shd", "xmb-waves");
    let screen = h.screen(140, 30);
    for want in [
        "xmb-waves: parameters",
        "Wave color",
        "#ff6b1a",
        "Accent",
        "#e31a24",
        "Strength",
        "0.16",
        "Speed",
        "preset",
        "◂ ember ▸",
    ] {
        assert!(screen.contains(want), "{want}:\n{screen}");
    }
    // a swatch for each color, a bar for each number
    assert_eq!(h.app.param_swatches.borrow().len(), 2);
    assert_eq!(h.app.param_bars.borrow().len(), 3, "opacity, strength and speed");
    let buf = h.buffer(140, 30);
    let (r, _) = h.app.param_swatches.borrow()[0];
    assert_eq!(buf[(r.x, r.y)].fg, Color::Rgb(0xff, 0x6b, 0x1a), "the swatch is painted in the parameter's color");
    // a shader that is off shows no parameters, and a shader without any says so
    let off = h.app.shader_rows.iter().position(|r| r.name == "aurora.glsl").unwrap();
    h.app.shader_sel = off;
    h.press(KeyCode::Char('4'));
    assert!(h.app.params.is_none());
    h.press(KeyCode::Right);
    assert!(h.app.status.text.contains("choose the shader first"));
    // a shader the user wrote has no annotations, so nothing to tune
    let dir = h.app.store.profiles_dir().join("shd/shaders");
    fs::write(
        dir.join("mine.glsl"),
        "void mainImage(out vec4 c, in vec2 p) { c = texture(iChannel0, p / iResolution.xy); }\n",
    )
    .unwrap();
    h.select_profile("aurora-glass");
    h.select_profile("shd"); // reload so the file is found
    h.app.refresh_profiles(Some("shd"));
    h.app.tab = Tab::Shaders;
    h.app.shader_sel = h.app.shader_rows.iter().position(|r| r.name == "mine.glsl").unwrap();
    h.press(KeyCode::Char(' ')); // select it
    h.press(KeyCode::Right);
    assert!(h.app.status.text.contains("no tunable parameters"), "{}", h.app.status.text);
}

#[test]
fn changing_a_color_with_the_picker_rewrites_the_shader_copy_and_reloads_once() {
    let mut h = harness();
    h.open_shader("shd", "xmb-waves");
    let reloads = h.reloads.get();
    let before = h.shader_text("shd", "xmb-waves");
    h.go_to_param("wave_a");
    h.press(KeyCode::Char('p'));
    assert!(h.app.picker.is_some(), "the hue wheel opens for a color parameter");
    assert_eq!(h.buf_text(), "#ff6b1a");
    for _ in 0..7 {
        h.press(KeyCode::Backspace);
    }
    h.type_text("#00aaff");
    h.press(KeyCode::Enter);
    assert!(h.app.picker.is_none() && h.app.input.is_none());
    let after = h.shader_text("shd", "xmb-waves");
    assert_ne!(after, before, "the rendered shader copy changed");
    assert!(after.contains("P_wave_a = vec3(0.000000, 0.666667, 1.000000)"), "{}", &after[..300]);
    assert_eq!(h.param_value("wave_a"), "#00aaff");
    assert!(
        fs::read_to_string(h.app.store.profiles_dir().join("shd/shaders/xmb-waves.params"))
            .unwrap()
            .contains("wave_a = #00aaff")
    );
    assert!(
        h.screen(140, 30).contains("#00aaff") && h.screen(140, 30).contains("custom"),
        "it no longer matches a preset"
    );
    h.tick();
    assert_eq!(h.reloads.get(), reloads + 1, "one debounced reload");
    // Esc in the picker leaves everything as it was
    h.press(KeyCode::Char('p'));
    h.press(KeyCode::Right);
    h.press(KeyCode::Esc);
    assert_eq!(h.param_value("wave_a"), "#00aaff");
    h.tick();
    assert_eq!(h.reloads.get(), reloads + 1);
    // a bad hex is refused, the box stays open
    h.press(KeyCode::Char('p'));
    h.type_text("zz");
    h.press(KeyCode::Enter);
    assert!(h.app.input.is_some() && h.app.picker.is_some());
    h.press(KeyCode::Esc);
}

#[test]
fn numbers_adjust_with_arrows_and_typing_and_a_burst_reloads_once() {
    let mut h = harness();
    h.open_shader("shd", "xmb-waves");
    h.go_to_param("strength");
    let reloads = h.reloads.get();
    h.app.debounce = Duration::from_millis(250);
    let t0 = Instant::now();
    // range 0..0.5 -> a step of 0.01
    h.press(KeyCode::Right);
    assert_eq!(h.param_value("strength"), "0.17");
    h.app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT));
    assert_eq!(h.param_value("strength"), "0.22", "Shift steps five times as far");
    h.press(KeyCode::Left);
    h.press(KeyCode::Left);
    assert_eq!(h.param_value("strength"), "0.2");
    assert!(h.shader_text("shd", "xmb-waves").contains("P_strength = 0.200000"));
    h.app.tick(t0 + Duration::from_millis(10));
    assert_eq!(h.reloads.get(), reloads, "still inside the debounce");
    h.app.tick(t0 + Duration::from_millis(500));
    assert_eq!(h.reloads.get(), reloads + 1, "one reload for the whole burst");
    // clamps at the ends
    for _ in 0..100 {
        h.press(KeyCode::Right);
    }
    assert_eq!(h.param_value("strength"), "0.5");
    // typing a number
    h.press(KeyCode::Enter);
    assert!(h.app.input.is_some() && h.app.picker.is_none(), "a number opens a plain box, not the picker");
    for _ in 0..4 {
        h.press(KeyCode::Backspace);
    }
    h.type_text("0.33");
    h.press(KeyCode::Enter);
    assert_eq!(h.param_value("strength"), "0.33");
    h.press(KeyCode::Enter);
    for _ in 0..4 {
        h.press(KeyCode::Backspace);
    }
    h.type_text("7");
    h.press(KeyCode::Enter);
    assert!(h.app.input.as_ref().unwrap().error.as_deref().unwrap().contains("between"), "out of range is refused");
    assert_eq!(h.param_value("strength"), "0.33");
    h.press(KeyCode::Esc);
}

#[test]
fn presets_cycle_on_the_preset_row_and_r_resets_to_the_defaults() {
    let mut h = harness();
    h.open_shader("shd", "xmb-waves");
    h.press(KeyCode::Right); // into the parameter list, on the preset row
    assert_eq!(h.app.param_sel, 0);
    h.press(KeyCode::Right);
    assert_eq!(h.param_value("wave_a"), "#2fa8ff", "ember -> ocean");
    assert!(h.screen(140, 30).contains("◂ ocean ▸"));
    h.press(KeyCode::Right);
    assert_eq!(h.param_value("wave_a"), "#3ddc84", "-> forest");
    h.press(KeyCode::Left);
    h.press(KeyCode::Left);
    assert_eq!(h.param_value("wave_a"), "#ff6b1a", "<- back to ember");
    // tweak, then reset
    h.go_to_param("speed");
    h.press(KeyCode::Right);
    assert_ne!(h.param_value("speed"), "0.35");
    h.press(KeyCode::Char('R'));
    assert_eq!(h.param_value("speed"), "0.35");
    assert!(!h.app.store.profiles_dir().join("shd/shaders/xmb-waves.params").exists(), "the sidecar is gone");
    assert!(h.shader_text("shd", "xmb-waves").contains("P_speed = 0.350000"));
    // Esc leaves the parameter list, and the shader list works again
    h.press(KeyCode::Esc);
    assert!(!h.app.param_focus);
    h.press(KeyCode::Down);
    assert!(h.app.shader_sel > 0 || h.app.shader_rows.len() == 1);
}

#[test]
fn mouse_clicks_swatches_and_drags_bars_in_the_parameter_list() {
    let mut h = harness();
    h.open_shader("shd", "xmb-waves");
    let _ = h.screen(140, 30);
    // click a swatch -> the picker for that parameter
    let (r, i) = h.app.param_swatches.borrow()[1]; // wave_b
    assert_eq!(h.app.params.as_ref().unwrap().schema.params[i].name, "wave_b");
    h.down(r.x, r.y);
    assert!(h.app.picker.is_some());
    assert_eq!(h.buf_text(), "#e31a24");
    let wheel = h.app.picker_rects.borrow().wheel;
    let _ = h.screen(140, 40);
    let wheel = h.app.picker_rects.borrow().wheel.or(wheel).expect("wheel");
    h.down(wheel.x + wheel.width * 4 / 5, wheel.y + wheel.height / 2);
    h.press(KeyCode::Enter);
    assert_ne!(h.param_value("wave_b"), "#e31a24", "the wheel click was accepted into the shader");
    assert!(h.shader_text("shd", "xmb-waves").contains("P_wave_b"));
    h.tick();
    // click and drag a bar: strength spans 0..0.5 over the bar's cells
    let _ = h.screen(140, 30);
    let bars = h.app.param_bars.borrow().clone();
    let (bar, bi) = bars
        .iter()
        .copied()
        .find(|(_, i)| h.app.params.as_ref().unwrap().schema.params[*i].name == "strength")
        .unwrap();
    h.down(bar.x, bar.y);
    assert_eq!(h.param_value("strength"), "0", "the left end is the minimum");
    h.drag(bar.x + bar.width - 1, bar.y);
    assert_eq!(h.param_value("strength"), "0.5", "the right end is the maximum");
    h.drag(bar.x + bar.width / 2, bar.y);
    let v: f64 = h.param_value("strength").parse().unwrap();
    assert!((v - 0.25).abs() < 0.03, "{v}");
    h.drag(bar.x + bar.width + 30, bar.y);
    assert_eq!(h.param_value("strength"), "0.5", "past the end clamps");
    h.up(bar.x, bar.y);
    let settled = h.param_value("strength");
    h.drag(bar.x, bar.y);
    assert_eq!(h.param_value("strength"), settled, "after the button is released a drag does nothing");
    assert!(h.app.param_focus && h.app.param_sel == bi + 1);
    // clicking elsewhere on the tab changes nothing
    h.down(0, 0);
    assert_eq!(h.param_value("strength"), settled);
}

#[test]
fn selecting_a_shader_writes_its_header_and_its_parameters_appear_with_the_profiles_values() {
    let mut h = harness();
    h.apply("calm-dark"); // uses soft-glow only
    h.app.tab = Tab::Shaders;
    let i = h.app.shader_rows.iter().position(|r| r.name == "enchant-glyphs.glsl").unwrap();
    h.app.shader_sel = i;
    h.press(KeyCode::Char('4'));
    assert!(h.app.params.is_none(), "not selected yet, so no parameters (only a read-only preview)");
    h.press(KeyCode::Enter);
    let text = h.shader_text("calm-dark", "enchant-glyphs");
    assert!(
        text.contains("const vec3 P_glyph_a") && text.contains("const float P_density"),
        "the header is written the moment it is selected"
    );
    assert!(h.app.params.is_some() && h.screen(140, 30).contains("Glyph color"));
    assert!(h.conf("calm-dark").contains("custom-shader = shaders/enchant-glyphs.glsl"));
    assert!(h.conf("calm-dark").contains("custom-shader-animation = true"), "animated shaders turn animation on");
    // a changed value is kept in a sidecar...
    h.go_to_param("density");
    h.press(KeyCode::Right);
    let side = h.app.store.profiles_dir().join("calm-dark/shaders/enchant-glyphs.params");
    assert!(side.is_file(), "the operator changed something, so the values are saved");
    // ...and a value changed back to its default leaves no sidecar at all
    h.press(KeyCode::Left);
    assert!(
        !side.exists(),
        "every value equals its default again: no sidecar is kept: {}",
        fs::read_to_string(&side).unwrap_or_default()
    );
    h.press(KeyCode::Right);
    assert!(side.is_file());
    h.press(KeyCode::Char('R'));
    assert!(!side.exists());
    // picking (none) removes the shader and everything generated for it
    h.press(KeyCode::Esc);
    h.app.shader_sel = 0;
    h.press(KeyCode::Enter);
    assert!(!h.conf("calm-dark").contains("enchant-glyphs"));
    assert!(shader_files(&h, "calm-dark").is_empty());
}

#[test]
fn the_parameter_list_draws_at_small_sizes_without_panicking() {
    let mut h = harness();
    h.open_shader("shd", "xmb-waves");
    h.press(KeyCode::Right);
    for (w, hh) in [(140, 40), (80, 24), (60, 16), (44, 12), (30, 10), (12, 5), (3, 3)] {
        let _ = h.screen(w, hh);
    }
    // open a picker in a cramped terminal too
    h.press(KeyCode::Down);
    h.press(KeyCode::Char('p'));
    for (w, hh) in [(80, 24), (50, 16), (30, 10), (10, 5)] {
        let _ = h.screen(w, hh);
    }
}

//! End-to-end through the real binaries, in a sandboxed config home.
//! `reload` is deliberately never run here: it would signal a real Ghostty.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn run(bin: &str, home: &Path, args: &[&str]) -> Output {
    Command::new(bin)
        .args(args)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .output()
        .expect("binary runs")
}

fn run_stdin(bin: &str, home: &Path, args: &[&str], input: &str) -> Output {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = Command::new(bin)
        .args(args)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("binary runs");
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    child.wait_with_output().unwrap()
}

fn ok(bin: &str, home: &Path, args: &[&str]) -> String {
    let o = run(bin, home, args);
    assert!(o.status.success(), "{args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stdout).into_owned()
}

const BIN: &str = env!("CARGO_BIN_EXE_ghostty-profiles");
const GPF: &str = env!("CARGO_BIN_EXE_gpf");

#[test]
fn install_list_apply_export_import_unlink() {
    let td = tempfile::tempdir().unwrap();
    let home = td.path();
    let ghostty = home.join("config/ghostty");
    fs::create_dir_all(&ghostty).unwrap();
    fs::write(ghostty.join("config"), "keybind = alt+t=toggle_quick_terminal\n").unwrap();

    assert!(ok(BIN, home, &["list"]).contains("no profiles yet"));
    let out = ok(BIN, home, &["install-presets"]);
    for n in ["shd", "calm-dark", "crt-green", "aurora-glass"] {
        assert!(out.contains(n), "{out}");
    }
    assert!(ok(GPF, home, &["install-presets"]).contains("already installed"), "the short name is the same program");

    // applying (without signalling anything) links exactly one include line
    assert!(ok(BIN, home, &["apply", "shd", "--no-reload"]).contains("applied 'shd'"));
    let list = ok(BIN, home, &["list"]);
    assert!(list.contains("● shd"), "{list}");
    let main = fs::read_to_string(ghostty.join("config")).unwrap();
    assert_eq!(main.matches("config-file = ?ghostty-profiles-active.conf").count(), 1);
    assert!(main.starts_with("keybind = alt+t=toggle_quick_terminal\n"));
    let active = fs::read_to_string(ghostty.join("ghostty-profiles-active.conf")).unwrap();
    assert!(active.contains("cursor-color = #ff6a00"));
    assert!(!active.contains("background-image"));

    // export -> import
    let dest = home.join("out/shd");
    ok(BIN, home, &["export", "shd", dest.to_str().unwrap()]);
    assert!(dest.join("profile.conf").is_file() && dest.join("shaders/xmb-waves.glsl").is_file());
    assert!(!dest.join("images").exists());
    assert!(ok(BIN, home, &["import", dest.to_str().unwrap(), "--name", "shd-copy"]).contains("shd-copy"));
    assert!(ok(BIN, home, &["new", "scratch", "--from", "shd-copy"]).contains("created"));
    assert!(ok(BIN, home, &["list"]).contains("scratch"));

    // errors are plain and non-zero
    let bad = run(BIN, home, &["apply", "nope", "--no-reload"]);
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).starts_with("error:"));
    let dup = run(BIN, home, &["new", "scratch"]);
    assert!(!dup.status.success());

    // unlink restores the user's config exactly
    assert!(ok(BIN, home, &["unlink"]).contains("removed"));
    assert_eq!(fs::read_to_string(ghostty.join("config")).unwrap(), "keybind = alt+t=toggle_quick_terminal\n");
    assert!(ok(BIN, home, &["unlink"]).contains("nothing to remove"));
}

#[test]
fn adopt_a_config_then_the_look_is_a_profile() {
    let td = tempfile::tempdir().unwrap();
    let home = td.path();
    let ghostty = home.join("config/ghostty");
    fs::create_dir_all(&ghostty).unwrap();
    fs::write(
        ghostty.join("config.ghostty"),
        "background = #101010\nfont-size = 14\nkeybind = ctrl+shift+r=reload_config\n",
    )
    .unwrap();
    let out = ok(BIN, home, &["adopt", "mine"]);
    assert!(out.contains("backup"), "{out}");
    assert_eq!(fs::read_to_string(ghostty.join("config.ghostty")).unwrap(), "keybind = ctrl+shift+r=reload_config\n");
    let conf = fs::read_to_string(home.join("config/ghostty-profiles/profiles/mine/profile.conf")).unwrap();
    assert!(conf.contains("background = #101010") && conf.contains("font-size = 14") && !conf.contains("keybind"));
    // adopting does not activate anything
    assert!(!ghostty.join("ghostty-profiles-active.conf").exists());
}

#[test]
fn help_and_version_work() {
    let td = tempfile::tempdir().unwrap();
    let help = ok(BIN, td.path(), &["--help"]);
    for c in ["list", "apply", "new", "adopt", "export", "import", "install-presets", "reload", "status", "unlink"] {
        assert!(help.contains(c), "help is missing {c}");
    }
    assert!(ok(BIN, td.path(), &["--version"]).contains("ghostty-profiles"));
}

#[test]
fn reload_can_be_redirected_to_a_custom_command() {
    let td = tempfile::tempdir().unwrap();
    let marker = td.path().join("reloaded");
    let o = Command::new(BIN)
        .arg("reload")
        .env("HOME", td.path())
        .env("XDG_CONFIG_HOME", td.path().join("config"))
        .env("GHOSTTY_PROFILES_RELOAD_CMD", format!("touch {}", marker.display()))
        .output()
        .unwrap();
    assert!(o.status.success());
    assert!(marker.exists(), "the custom command ran instead of signalling any Ghostty");
    let failing = Command::new(BIN)
        .arg("reload")
        .env("HOME", td.path())
        .env("XDG_CONFIG_HOME", td.path().join("config"))
        .env("GHOSTTY_PROFILES_RELOAD_CMD", "false")
        .output()
        .unwrap();
    assert!(!failing.status.success());
}

#[test]
fn a_shared_profile_cannot_make_ghostty_run_anything() {
    let td = tempfile::tempdir().unwrap();
    let home = td.path();
    let shared = home.join("shared");
    fs::create_dir_all(&shared).unwrap();
    fs::write(
        shared.join("profile.conf"),
        "background = #101010\ncommand = /bin/sh -c 'touch /tmp/pwned'\ninitial-command = whoami\nkeybind = ctrl+a=text:evil\nconfig-file = /tmp/evil.conf\n",
    )
    .unwrap();
    let out = ok(BIN, home, &["import", shared.to_str().unwrap(), "--name", "theirs"]);
    assert!(out.contains("removed settings"), "{out}");
    for k in ["command", "initial-command", "keybind", "config-file"] {
        assert!(out.contains(k), "{k} should be reported: {out}");
    }
    ok(BIN, home, &["apply", "theirs", "--no-reload"]);
    let active = fs::read_to_string(home.join("config/ghostty/ghostty-profiles-active.conf")).unwrap();
    assert!(active.contains("background = #101010"));
    for bad in ["command", "keybind", "pwned", "evil"] {
        assert!(!active.contains(bad), "{bad} reached the active Ghostty config:\n{active}");
    }
}

#[test]
fn off_and_on_again_leave_the_users_config_alone_and_status_says_none() {
    let td = tempfile::tempdir().unwrap();
    let home = td.path();
    let ghostty = home.join("config/ghostty");
    fs::create_dir_all(&ghostty).unwrap();
    let mine = "font-size = 12\nbackground = #111111\n";
    fs::write(ghostty.join("config"), mine).unwrap();
    ok(BIN, home, &["install-presets"]);

    ok(BIN, home, &["apply", "shd", "--no-reload"]);
    let linked = fs::read_to_string(ghostty.join("config")).unwrap();
    assert!(linked.starts_with(mine) && linked.contains("config-file = ?ghostty-profiles-active.conf"));
    assert!(ok(BIN, home, &["status"]).contains("active profile  : shd"));

    // off: nothing applied, include kept, main config byte-for-byte unchanged
    assert!(ok(BIN, home, &["off", "--no-reload"]).contains("no profile is active now"));
    let status = ok(BIN, home, &["status"]);
    assert!(status.contains("active profile  : none"), "{status}");
    assert!(status.contains("linked          : yes"), "{status}");
    assert!(ok(BIN, home, &["list"]).lines().all(|l| !l.contains('●')), "no profile is marked");
    let active = fs::read_to_string(ghostty.join("ghostty-profiles-active.conf")).unwrap();
    assert!(active.starts_with('#') && !active.contains("palette") && !active.contains("shader"), "{active}");
    assert_eq!(fs::read_to_string(ghostty.join("config")).unwrap(), linked);

    // idempotent, and the alias works
    assert!(ok(BIN, home, &["off", "--no-reload"]).contains("no profile was active"));
    assert!(ok(GPF, home, &["none", "--no-reload"]).contains("no profile was active"));

    // on again applies; the include is not duplicated
    ok(BIN, home, &["apply", "calm-dark", "--no-reload"]);
    assert!(ok(BIN, home, &["status"]).contains("active profile  : calm-dark"));
    assert_eq!(fs::read_to_string(ghostty.join("config")).unwrap(), linked);

    // unlink is the stronger step and still works
    assert!(ok(BIN, home, &["unlink"]).contains("removed the include line"));
    assert_eq!(fs::read_to_string(ghostty.join("config")).unwrap(), mine);
    assert!(ok(BIN, home, &["status"]).contains("linked          : no"));
}

#[test]
fn delete_asks_refuses_the_active_profile_and_removes_the_whole_folder() {
    let td = tempfile::tempdir().unwrap();
    let home = td.path();
    ok(BIN, home, &["install-presets"]);
    ok(BIN, home, &["new", "mine", "--from", "shd"]);
    let dir = home.join("config/ghostty-profiles/profiles/mine");
    fs::create_dir_all(dir.join("images")).unwrap();
    fs::write(dir.join("images/pic.png"), "x").unwrap();
    assert!(dir.join("shaders/xmb-waves.glsl").is_file());

    // asks first: an empty answer or "n" keeps it
    for answer in ["\n", "n\n", ""] {
        let o = run_stdin(BIN, home, &["delete", "mine"], answer);
        assert!(o.status.success());
        let out = String::from_utf8_lossy(&o.stdout);
        assert!(
            out.contains("Delete profile 'mine' and its images and shaders?") && out.contains("not deleted"),
            "{out}"
        );
        assert!(dir.is_dir(), "kept after {answer:?}");
    }

    // the active profile is refused, even with --yes
    ok(BIN, home, &["apply", "mine", "--no-reload"]);
    let o = run(BIN, home, &["delete", "mine", "--yes"]);
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("active profile"));
    assert!(dir.is_dir());
    ok(BIN, home, &["apply", "shd", "--no-reload"]);

    // y deletes everything in the folder
    let o = run_stdin(BIN, home, &["rm", "mine"], "y\n");
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(String::from_utf8_lossy(&o.stdout).contains("deleted 'mine'"));
    assert!(!dir.exists(), "folder, images and shaders are gone");
    assert!(!ok(BIN, home, &["list"]).contains("mine"));

    // --yes skips the question; a missing profile is a clear error
    let out = ok(BIN, home, &["delete", "calm-dark", "-y"]);
    assert!(
        out.contains("deleted 'calm-dark'") && out.contains("bundled preset") && out.contains("install-presets"),
        "{out}"
    );
    let missing = run(BIN, home, &["delete", "nope", "--yes"]);
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("no profile named 'nope'"));
    // and a bundled preset really does come back
    assert!(ok(BIN, home, &["install-presets"]).contains("calm-dark"));
    assert!(ok(BIN, home, &["--help"]).contains("delete"));
}

#[test]
fn new_from_copies_images_and_shaders_too() {
    let td = tempfile::tempdir().unwrap();
    let home = td.path();
    ok(BIN, home, &["install-presets"]);
    ok(BIN, home, &["new", "pic", "--from", "shd"]);
    let src = home.join("config/ghostty-profiles/profiles/pic");
    fs::create_dir_all(src.join("images")).unwrap();
    fs::write(src.join("images/wall.png"), "PIXELS").unwrap();
    let mut conf = fs::read_to_string(src.join("profile.conf")).unwrap();
    conf.push_str("background-image = images/wall.png\nbackground-image-opacity = 0.4\n");
    fs::write(src.join("profile.conf"), conf).unwrap();

    ok(BIN, home, &["new", "pic2", "--from", "pic"]);
    let copy = home.join("config/ghostty-profiles/profiles/pic2");
    assert_eq!(fs::read_to_string(copy.join("images/wall.png")).unwrap(), "PIXELS", "the picture was copied");
    assert!(copy.join("shaders/xmb-waves.glsl").is_file(), "and the shader");
    let conf = fs::read_to_string(copy.join("profile.conf")).unwrap();
    assert!(conf.contains("background-image = images/wall.png") && conf.contains("background-image-opacity = 0.4"));
    // independent files: deleting the original leaves the copy's picture
    ok(BIN, home, &["delete", "pic", "--yes"]);
    assert!(copy.join("images/wall.png").is_file());
}

#[test]
fn rename_via_the_cli_including_the_active_profile() {
    let td = tempfile::tempdir().unwrap();
    let home = td.path();
    ok(BIN, home, &["install-presets"]);
    ok(BIN, home, &["apply", "shd", "--no-reload"]);
    let out = ok(BIN, home, &["rename", "shd", "ember", "--no-reload"]);
    assert!(
        out.contains("renamed 'shd' to 'ember'") && out.contains("bundled preset") && out.contains("active profile"),
        "{out}"
    );
    assert!(ok(BIN, home, &["list"]).contains("● ember"));
    assert!(ok(BIN, home, &["status"]).contains("active profile  : ember"));
    let active = fs::read_to_string(home.join("config/ghostty/ghostty-profiles-active.conf")).unwrap();
    assert!(active.contains("profiles/ember/shaders/xmb-waves.glsl") && !active.contains("profiles/shd/"), "{active}");
    assert!(home.join("config/ghostty-profiles/profiles/ember/shaders/xmb-waves.glsl").is_file());

    // errors are plain and non-zero, and nothing changes
    for args in [["rename", "ember", "calm-dark"], ["rename", "ember", "a/b"], ["rename", "nope", "x"]] {
        let o = run(BIN, home, &args);
        assert!(!o.status.success(), "{args:?}");
        assert!(String::from_utf8_lossy(&o.stderr).starts_with("error:"));
    }
    assert!(ok(BIN, home, &["list"]).contains("● ember"));
    assert!(ok(BIN, home, &["--help"]).contains("rename"));
}

#[test]
fn shader_show_set_preset_and_reset_from_the_cli() {
    let td = tempfile::tempdir().unwrap();
    let home = td.path();
    ok(BIN, home, &["install-presets"]);
    let out = ok(BIN, home, &["shader", "show", "shd", "xmb-waves"]);
    assert!(out.contains("wave_a") && out.contains("#ff6b1a") && out.contains("presets: ember, ocean"), "{out}");
    assert!(ok(BIN, home, &["shader", "set", "shd", "xmb-waves", "wave_a", "#2FA8FF"]).contains("set wave_a"));
    assert!(ok(BIN, home, &["shader", "show", "shd", "xmb-waves"]).contains("#2fa8ff"));
    let shader = fs::read_to_string(home.join("config/ghostty-profiles/profiles/shd/shaders/xmb-waves.glsl")).unwrap();
    assert!(shader.contains("P_wave_a = vec3(0.184314"), "{}", &shader[..400]);
    ok(BIN, home, &["shader", "preset", "shd", "xmb-waves.glsl", "forest"]);
    assert!(ok(BIN, home, &["shader", "show", "shd", "xmb-waves"]).contains("#3ddc84"));
    ok(BIN, home, &["shader", "reset", "shd", "xmb-waves"]);
    assert!(ok(BIN, home, &["shader", "show", "shd", "xmb-waves"]).contains("#ff6b1a"));
    // errors are plain: bad value, unknown param, unknown preset, a shader that is not in the profile
    for args in [
        vec!["shader", "set", "shd", "xmb-waves", "wave_a", "nope"],
        vec!["shader", "set", "shd", "xmb-waves", "bogus", "1"],
        vec!["shader", "preset", "shd", "xmb-waves", "bogus"],
        vec!["shader", "show", "shd", "aurora"],
    ] {
        let o = run(BIN, home, &args);
        assert!(!o.status.success(), "{args:?}");
        assert!(String::from_utf8_lossy(&o.stderr).starts_with("error:"), "{args:?}");
    }
    // changing the ACTIVE profile's shader re-renders without signalling anything (--no-reload)
    ok(BIN, home, &["apply", "shd", "--no-reload"]);
    assert!(
        ok(BIN, home, &["shader", "set", "shd", "xmb-waves", "strength", "0.4", "--no-reload"])
            .contains("set strength")
    );
}

#[test]
fn prune_use_and_opacity_from_the_cli() {
    let td = tempfile::tempdir().unwrap();
    let home = td.path();
    ok(BIN, home, &["install-presets"]);
    let dir = home.join("config/ghostty-profiles/profiles/shd/shaders");
    // browse-like clutter: a bundled shader's copy and a defaults-only params file
    fs::write(dir.join("snow.glsl"), include_str!("../presets/shaders/snow.glsl")).unwrap();
    fs::write(dir.join("snow.params"), "opacity = 0.6\n").unwrap();
    fs::write(dir.join("mine.glsl"), "void mainImage(out vec4 c, in vec2 p) {}\n").unwrap();

    let out = ok(BIN, home, &["prune", "shd", "--dry-run"]);
    assert!(
        out.contains("would remove shaders/snow.glsl") && out.contains("kept (yours") && out.contains("dry run"),
        "{out}"
    );
    assert!(dir.join("snow.glsl").exists(), "a dry run deletes nothing");
    let out = ok(BIN, home, &["prune"]);
    assert!(
        out.contains("removed shaders/snow.glsl") && out.contains("backup:") && out.contains("shaders/mine.glsl"),
        "{out}"
    );
    assert!(!dir.join("snow.glsl").exists() && dir.join("mine.glsl").exists() && dir.join("xmb-waves.glsl").exists());
    assert!(home.join("config/ghostty-profiles/backups/shd.bak-pre-prune/shaders/snow.glsl").is_file());
    assert!(ok(BIN, home, &["prune"]).contains("nothing to tidy"));

    // use: replaces the shader; the old one's files go
    assert!(ok(BIN, home, &["shader", "use", "shd", "snow", "--no-reload"]).contains("now uses snow"));
    assert!(dir.join("snow.glsl").exists() && !dir.join("xmb-waves.glsl").exists());
    // changing values needs the shader to be the profile's one
    let o = run(BIN, home, &["shader", "set", "shd", "xmb-waves", "strength", "0.2"]);
    assert!(!o.status.success() && String::from_utf8_lossy(&o.stderr).contains("not the shader of 'shd'"));
    assert!(
        ok(BIN, home, &["shader", "set", "shd", "snow", "strength", "0.5", "--no-reload"]).contains("set strength")
    );
    assert!(dir.join("snow.params").exists());
    ok(BIN, home, &["shader", "reset", "shd", "snow", "--no-reload"]);
    assert!(!dir.join("snow.params").exists(), "defaults only: no sidecar");
    // effects opacity
    assert!(ok(BIN, home, &["shader", "opacity", "shd", "0.5", "--no-reload"]).contains("0.5"));
    assert!(ok(BIN, home, &["shader", "show", "shd", "snow"]).contains("effects opacity (profile-wide): 0.5"));
    assert!(!run(BIN, home, &["shader", "opacity", "shd", "2"]).status.success());
    // none
    assert!(ok(BIN, home, &["shader", "use", "shd", "none", "--no-reload"]).contains("no shader"));
    assert!(!dir.join("snow.glsl").exists());
    assert!(
        !fs::read_to_string(home.join("config/ghostty-profiles/profiles/shd/profile.conf"))
            .unwrap()
            .contains("custom-shader")
    );
}

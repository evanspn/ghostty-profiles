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

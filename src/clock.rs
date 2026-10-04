//! The palette clock: how a shader learns the time of day.
//!
//! Ghostty gives custom shaders no wall-clock time (`iDate` is always zero, and `iTime` counts from each window's own
//! first frame), but it does pass the terminal's live 256-color palette (`iPalette`). So while the active profile's
//! shader calls `gp_clockStamp()`, every Ghostty shell (zsh) keeps color 254 of its own terminal set to the current
//! time, once a second, with an invisible OSC 4 sequence: red = 200 + hour, green = 192 + minute, blue = 192 + second.
//! The shader's generated header decodes it ([`crate::shaderparams`]); any other value of color 254 reads as "no time".
//!
//! Pieces: `clock.zsh` (written here, sourced from `~/.zshrc` by one marked line) installs a prompt hook that starts one
//! small background loop per shell when the flag file exists. The loop uses zsh builtins only (no process per tick), and
//! stops by itself within a second when the flag goes (`gpf apply` of another look, `gpf off`), when its shell exits,
//! or on `gpf_clock_stop`; on the way out it gives color 254 back to the terminal's own palette (OSC 104).

use std::fs;
use std::path::Path;

use anyhow::Result;

use crate::paths::Paths;
use crate::store::atomic_write;

/// The comment that marks the one line this tool adds to the shell startup file.
pub const RC_MARKER: &str = "# ghostty-profiles palette clock (does nothing unless a clock look is active)";

/// The palette entry that carries the time.
pub const PALETTE_INDEX: u8 = 254;

/// zsh single-quoted literal of a path (`'` closed, escaped and reopened).
fn zsh_quote(p: &Path) -> String {
    format!("'{}'", p.to_string_lossy().replace('\'', r"'\''"))
}

/// The line added to the shell startup file.
pub fn rc_line(paths: &Paths) -> String {
    let script = zsh_quote(&paths.clock_script());
    format!("[[ -r {script} ]] && source {script}")
}

/// The zsh snippet, with the flag file's path filled in.
pub fn script(paths: &Paths) -> String {
    SCRIPT_TEMPLATE.replace("@FLAG@", &zsh_quote(&paths.clock_flag()))
}

const SCRIPT_TEMPLATE: &str = r#"# ghostty-profiles palette clock (written by ghostty-profiles; edits are overwritten).
# While a clock look is active (the flag file below exists), each Ghostty shell keeps color 254 of its OWN terminal set
# to the current time, once a second (an invisible OSC 4 escape), so the clock shader can show it. One small background
# loop per shell, zsh builtins only. It stops by itself within a second when the look changes (gpf apply / gpf off) or
# the shell exits, and gives color 254 back. Stop it by hand in this shell with: gpf_clock_stop
[[ -n $ZSH_VERSION ]] || return 0
typeset -g _gpf_clock_flag=@FLAG@
typeset -g _gpf_clock_pid=

_gpf_clock_loop() {
  emulate -L zsh
  local shell_pid=$1 tty=$2 t tick=sleep
  zmodload zsh/datetime 2>/dev/null || return
  # a builtin one-second wait (zselect returns non-zero when it times out, so it is not chained with ||)
  zmodload zsh/zselect 2>/dev/null && tick=zselect
  while kill -0 $shell_pid 2>/dev/null && [[ -e $_gpf_clock_flag ]]; do
    strftime -s t '%H %M %S' $EPOCHSECONDS
    t=(${=t})
    printf '\e]4;254;rgb:%02x/%02x/%02x\e\\' $(( 200 + 10#$t[1] )) $(( 192 + 10#$t[2] )) $(( 192 + 10#$t[3] )) \
      2>/dev/null >| $tty || break
    if [[ $tick == zselect ]]; then zselect -t 100 2>/dev/null; else sleep 1; fi
  done
  printf '\e]104;254\e\\' 2>/dev/null >| $tty
}

_gpf_clock_start() {
  [[ -e $_gpf_clock_flag && $TERM_PROGRAM == ghostty && -n $TTY ]] || return 0
  [[ -n $_gpf_clock_pid ]] && kill -0 $_gpf_clock_pid 2>/dev/null && return 0
  _gpf_clock_loop $$ $TTY &!
  _gpf_clock_pid=$!
}

gpf_clock_stop() {
  [[ -n $_gpf_clock_pid ]] && kill $_gpf_clock_pid 2>/dev/null
  _gpf_clock_pid=
  [[ -n $TTY ]] && printf '\e]104;254\e\\' >| $TTY
}

autoload -Uz add-zsh-hook && add-zsh-hook precmd _gpf_clock_start
"#;

/// Is the shell startup file hooked (the marked line present)?
pub fn is_hooked(rc: &Path) -> bool {
    fs::read_to_string(rc).is_ok_and(|t| t.lines().any(|l| l.trim() == RC_MARKER))
}

/// Bring the clock in line with the active look: when `on`, write the script and the flag and hook the shell startup
/// file once (backed up first); when off, remove the flag (running loops then stop by themselves). Returns notes.
pub fn sync(paths: &Paths, on: bool) -> Result<Vec<String>> {
    let mut notes = Vec::new();
    let flag = paths.clock_flag();
    if !on {
        match fs::remove_file(&flag) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        return Ok(notes);
    }
    atomic_write(&paths.clock_script(), &script(paths))?;
    atomic_write(&flag, "the active look's shader is a palette clock\n")?;
    match paths.shell_rc() {
        None => notes.push(format!(
            "the clock needs zsh: add this line to your zsh startup file to make it run: {}",
            rc_line(paths)
        )),
        Some(rc) if is_hooked(rc) => {}
        Some(rc) if fs::symlink_metadata(rc).is_ok_and(|m| m.file_type().is_symlink()) => notes.push(format!(
            "{} is a symlink, so it was left alone: add this line to it for the clock to run: {}",
            rc.display(),
            rc_line(paths)
        )),
        Some(rc) => {
            let mut backup = rc.as_os_str().to_owned();
            backup.push(crate::store::BACKUP_SUFFIX);
            if rc.exists() && !Path::new(&backup).exists() {
                fs::copy(rc, &backup)?;
            }
            let mut text = fs::read_to_string(rc).unwrap_or_default();
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str(&format!("\n{RC_MARKER}\n{}\n", rc_line(paths)));
            atomic_write(rc, &text)?;
            notes.push(format!(
                "the clock is hooked into {} (one line, backed up first): open a new tab, or run `source {}` in open ones",
                rc.display(),
                rc.display()
            ));
        }
    }
    Ok(notes)
}

/// Undo everything: the flag, the script and the marked line in the shell startup file. True if anything changed.
pub fn unhook(paths: &Paths) -> Result<bool> {
    let mut changed = false;
    for f in [paths.clock_flag(), paths.clock_script()] {
        match fs::remove_file(&f) {
            Ok(()) => changed = true,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    if let Some(rc) = paths.shell_rc()
        && is_hooked(rc)
        && !fs::symlink_metadata(rc).is_ok_and(|m| m.file_type().is_symlink())
    {
        let text = fs::read_to_string(rc)?;
        let line = rc_line(paths);
        let kept: Vec<&str> = text.lines().filter(|l| l.trim() != RC_MARKER && l.trim() != line).collect();
        atomic_write(rc, &(kept.join("\n").trim_end().to_string() + "\n"))?;
        changed = true;
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sandbox() -> (tempfile::TempDir, Paths) {
        let td = tempfile::tempdir().unwrap();
        let paths = Paths::new(td.path().join("config")).with_shell_rc(td.path().join(".zshrc"));
        (td, paths)
    }

    #[test]
    fn on_writes_the_script_and_flag_and_hooks_the_rc_once_with_a_backup() {
        let (_td, paths) = sandbox();
        let rc = paths.shell_rc().unwrap().to_path_buf();
        fs::write(&rc, "export EDITOR=vi").unwrap();
        let notes = sync(&paths, true).unwrap();
        assert!(notes[0].contains("hooked"), "{notes:?}");
        assert!(paths.clock_flag().exists() && paths.clock_script().exists());
        let text = fs::read_to_string(&rc).unwrap();
        assert!(text.starts_with("export EDITOR=vi\n") && text.contains(&rc_line(&paths)), "{text}");
        let mut backup = rc.clone().into_os_string();
        backup.push(crate::store::BACKUP_SUFFIX);
        assert_eq!(fs::read_to_string(backup).unwrap(), "export EDITOR=vi");
        // idempotent: a second apply adds nothing
        assert!(sync(&paths, true).unwrap().is_empty());
        assert_eq!(fs::read_to_string(&rc).unwrap(), text);
    }

    #[test]
    fn off_removes_only_the_flag_and_unhook_removes_everything() {
        let (_td, paths) = sandbox();
        let rc = paths.shell_rc().unwrap().to_path_buf();
        fs::write(&rc, "alias ll='ls -l'\n").unwrap();
        sync(&paths, true).unwrap();
        sync(&paths, false).unwrap();
        assert!(!paths.clock_flag().exists() && paths.clock_script().exists() && is_hooked(&rc));
        assert!(unhook(&paths).unwrap());
        assert!(!paths.clock_script().exists());
        assert_eq!(fs::read_to_string(&rc).unwrap(), "alias ll='ls -l'\n");
        assert!(!unhook(&paths).unwrap(), "nothing left to undo");
    }

    #[test]
    fn without_a_zsh_rc_or_with_a_symlinked_one_nothing_is_edited_and_the_line_is_shown() {
        let td = tempfile::tempdir().unwrap();
        let paths = Paths::new(td.path().join("config"));
        let notes = sync(&paths, true).unwrap();
        assert!(notes[0].contains("needs zsh") && notes[0].contains(&rc_line(&paths)), "{notes:?}");
        #[cfg(unix)]
        {
            let real = td.path().join("dotfiles-zshrc");
            fs::write(&real, "# mine\n").unwrap();
            let link = td.path().join(".zshrc");
            std::os::unix::fs::symlink(&real, &link).unwrap();
            let paths = paths.with_shell_rc(&link);
            let notes = sync(&paths, true).unwrap();
            assert!(notes[0].contains("symlink"), "{notes:?}");
            assert_eq!(fs::read_to_string(&real).unwrap(), "# mine\n");
            assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        }
    }

    #[test]
    fn the_script_names_the_flag_and_paths_with_quotes_stay_quoted() {
        let td = tempfile::tempdir().unwrap();
        let paths = Paths::new(td.path().join("it's config"));
        let s = script(&paths);
        assert!(s.contains(r"it'\''s config"), "{s}");
        assert!(!s.contains("@FLAG@"));
        assert!(rc_line(&paths).contains(r"it'\''s config"));
    }
}

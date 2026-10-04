//! Filesystem locations. Everything hangs off one config home so tests can
//! sandbox it: nothing in this crate reads `$HOME` outside [`Paths::from_env`]
//! (which is also the only place the user's shell startup file is found).

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

pub const ACTIVE_CONF_NAME: &str = "ghostty-profiles-active.conf";

#[derive(Clone, Debug)]
pub struct Paths {
    config_home: PathBuf,
    /// The interactive shell's startup file (`~/.zshrc`), where the palette clock is hooked in. Only
    /// [`Paths::from_env`] sets it, and only for zsh; `None` means: never touch a shell startup file.
    shell_rc: Option<PathBuf>,
}

impl Paths {
    /// `$XDG_CONFIG_HOME`, else `$HOME/.config`.
    pub fn from_env() -> Result<Self> {
        let var = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty());
        let home = var("HOME");
        let paths = if let Some(x) = var("XDG_CONFIG_HOME") {
            Self::new(x)
        } else {
            match &home {
                Some(h) => Self::new(Path::new(h).join(".config")),
                None => bail!("neither XDG_CONFIG_HOME nor HOME is set"),
            }
        };
        // the palette clock's hook goes in the zsh startup file ($ZDOTDIR/.zshrc, else ~/.zshrc); other shells get none
        let zsh = var("SHELL").is_some_and(|s| Path::new(&s).file_name().is_some_and(|n| n == "zsh"));
        let rc = var("ZDOTDIR").or(home).map(|d| Path::new(&d).join(".zshrc"));
        Ok(match rc {
            Some(rc) if zsh => paths.with_shell_rc(rc),
            _ => paths,
        })
    }

    pub fn new(config_home: impl Into<PathBuf>) -> Self {
        Self { config_home: config_home.into(), shell_rc: None }
    }

    /// The same locations, with a shell startup file the palette clock may hook into.
    pub fn with_shell_rc(mut self, rc: impl Into<PathBuf>) -> Self {
        self.shell_rc = Some(rc.into());
        self
    }

    pub fn shell_rc(&self) -> Option<&Path> {
        self.shell_rc.as_deref()
    }

    pub fn config_home(&self) -> &Path {
        &self.config_home
    }

    pub fn app_dir(&self) -> PathBuf {
        self.config_home.join("ghostty-profiles")
    }

    pub fn profiles_dir(&self) -> PathBuf {
        self.app_dir().join("profiles")
    }

    pub fn active_file(&self) -> PathBuf {
        self.app_dir().join("active")
    }

    pub fn ghostty_dir(&self) -> PathBuf {
        self.config_home.join("ghostty")
    }

    pub fn user_themes_dir(&self) -> PathBuf {
        self.ghostty_dir().join("themes")
    }

    pub fn active_conf(&self) -> PathBuf {
        self.ghostty_dir().join(ACTIVE_CONF_NAME)
    }

    /// The zsh snippet that runs the palette clock (sourced from the shell startup file).
    pub fn clock_script(&self) -> PathBuf {
        self.app_dir().join("clock.zsh")
    }

    /// Present while the active profile's shader is a palette clock: shells stamp the time only while it exists.
    pub fn clock_flag(&self) -> PathBuf {
        self.app_dir().join("clock-on")
    }
}

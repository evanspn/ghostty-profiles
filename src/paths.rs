//! Filesystem locations. Everything hangs off one config home so tests can
//! sandbox it: nothing in this crate reads `$HOME` outside [`Paths::from_env`].

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

pub const ACTIVE_CONF_NAME: &str = "ghostty-profiles-active.conf";

#[derive(Clone, Debug)]
pub struct Paths {
    config_home: PathBuf,
}

impl Paths {
    /// `$XDG_CONFIG_HOME`, else `$HOME/.config`.
    pub fn from_env() -> Result<Self> {
        if let Some(x) = std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
            return Ok(Self::new(x));
        }
        match std::env::var_os("HOME").filter(|v| !v.is_empty()) {
            Some(h) => Ok(Self::new(Path::new(&h).join(".config"))),
            None => bail!("neither XDG_CONFIG_HOME nor HOME is set"),
        }
    }

    pub fn new(config_home: impl Into<PathBuf>) -> Self {
        Self { config_home: config_home.into() }
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
}

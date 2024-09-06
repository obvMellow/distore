use std::{
    env::{self},
    fmt::Display,
    fs::File,
    io,
    path::{Path, PathBuf},
};

use ini::{Ini, Properties};
use thiserror::Error;

#[derive(Debug)]
pub enum ConfigValue {
    Token(String),
    Channel(String),
}

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Invalid key: {0}")]
    InvalidKey(String),

    #[error("Non Unicode character in {0}")]
    NonUnicodePath(PathBuf),

    #[error("Config directory couldn't found. Please specify one.")]
    NoConfigDir,

    #[error(transparent)]
    Ini(#[from] ini::Error),

    #[error(transparent)]
    Io(#[from] io::Error),

    #[error("No token set")]
    NoToken,

    #[error("No channel set")]
    NoChannel,
}

type Result<T> = std::result::Result<T, ConfigError>;

impl Display for ConfigValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self._pairs().0, self.inner())
    }
}

impl ConfigValue {
    pub fn parse<S: Into<String>>(key: S, val: S) -> Result<ConfigValue> {
        let key = key.into();
        match key.as_str() {
            "token" => Ok(ConfigValue::Token(val.into())),
            "channel" => Ok(ConfigValue::Channel(val.into())),
            _ => Err(ConfigError::InvalidKey(key)),
        }
    }

    pub fn inner(&self) -> &str {
        self._pairs().1
    }

    fn _pairs(&self) -> (&str, &str) {
        match self {
            Self::Token(s) => ("Token", s),
            Self::Channel(s) => ("Channel", s),
        }
    }

    pub fn write_to_path(
        path: &Path,
        value: &ConfigValue,
        scope: Option<impl Into<String>>,
    ) -> Result<()> {
        if !path.exists() {
            _ = File::create(path)?;
        }
        let mut f = Ini::load_from_file(path)?;
        f.with_section(scope)
            .set(value._pairs().0.to_lowercase(), value.inner());
        f.write_to_file(path)?;
        Ok(())
    }

    pub fn get_current_config(path: &Path) -> Result<(ConfigValue, ConfigValue)> {
        let current_dir = env::current_dir()?;
        let conf = Ini::load_from_file(path)?;

        let section = match conf.section(current_dir.to_str()) {
            Some(s) => s,
            None => conf.general_section(),
        };

        Self::_get_config(section)
    }

    pub fn get_global_config(path: &Path) -> Result<(ConfigValue, ConfigValue)> {
        let conf = Ini::load_from_file(path)?;
        Self::_get_config(conf.general_section())
    }

    fn _get_config(section: &Properties) -> Result<(ConfigValue, ConfigValue)> {
        let token = section.get("token").ok_or(ConfigError::NoToken)?;
        let channel = section.get("channel").ok_or(ConfigError::NoChannel)?;

        Ok((
            ConfigValue::parse("token", token)?,
            ConfigValue::parse("channel", channel)?,
        ))
    }
}

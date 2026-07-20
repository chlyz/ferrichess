//! Shared, non-secret user configuration for Ferrichess tools.

use std::{collections::BTreeMap, env, error::Error, fmt, fs, io, path::PathBuf};

use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
pub struct Config {
    #[serde(default)]
    pub lichess: ServiceConfig,
    #[serde(default)]
    pub chesscom: ServiceConfig,
    /// Named, private Lichess studies used as repertoire sources of truth.
    #[serde(default)]
    pub studies: BTreeMap<String, StudyConfig>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
pub struct ServiceConfig {
    pub username: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct StudyConfig {
    /// The eight-character Lichess study identifier.
    pub study_id: String,
    /// Local directory receiving `study.pgn` and `study.fen.sqlite3`.
    pub directory: PathBuf,
    /// Optional read-only course directory used as reference material.
    #[serde(default)]
    pub course_directory: Option<PathBuf>,
}

impl Config {
    /// Loads `config.toml` from the Ferrichess XDG configuration directory.
    /// A missing file is treated as an empty configuration.
    pub fn load_default() -> Result<Self, ConfigError> {
        let Some(path) = default_config_path() else {
            return Ok(Self::default());
        };
        match fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text).map_err(|source| ConfigError::Toml { path, source }),
            Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(ConfigError::Io { path, source }),
        }
    }
}

#[must_use]
pub fn default_config_path() -> Option<PathBuf> {
    if let Some(config_home) = env::var_os("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(config_home).join("ferrichess/config.toml"));
    }
    env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".config/ferrichess/config.toml"))
}

#[derive(Debug)]
pub enum ConfigError {
    Io {
        path: PathBuf,
        source: io::Error,
    },
    Toml {
        path: PathBuf,
        source: toml::de::Error,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(formatter, "cannot read {}: {source}", path.display())
            }
            Self::Toml { path, source } => {
                write!(
                    formatter,
                    "invalid Ferrichess config {}: {source}",
                    path.display()
                )
            }
        }
    }
}

impl Error for ConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Toml { source, .. } => Some(source),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn parses_service_usernames_without_secrets() {
        let config: Config = toml::from_str(
            r#"
[lichess]
username = "example-lichess"

[chesscom]
username = "example-chesscom"

[studies.white-1-e4]
study_id = "abcdefgh"
directory = "/path/to/repertoires/white-1-e4/lichess"
course_directory = "/path/to/courses/example-course"
"#,
        )
        .unwrap();
        assert_eq!(config.lichess.username.as_deref(), Some("example-lichess"));
        assert_eq!(
            config.chesscom.username.as_deref(),
            Some("example-chesscom")
        );
        assert_eq!(config.studies["white-1-e4"].study_id, "abcdefgh");
        assert_eq!(
            config.studies["white-1-e4"].directory,
            std::path::Path::new("/path/to/repertoires/white-1-e4/lichess")
        );
        assert_eq!(
            config.studies["white-1-e4"].course_directory.as_deref(),
            Some(std::path::Path::new("/path/to/courses/example-course"))
        );
    }
}

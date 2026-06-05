use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs, io,
    net::SocketAddr,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

const DEFAULT_CONFIG_FILE: &str = "config.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    pub proxy_listen: SocketAddr,
    pub admin_listen: SocketAddr,
    pub database: PathBuf,
    pub payload_dir: PathBuf,
    pub master_key: PathBuf,
    pub default_route: String,
    pub routes: BTreeMap<String, RouteConfig>,
    #[serde(default)]
    pub models: BTreeMap<String, ModelRoute>,
    #[serde(default)]
    pub payload_capture: PayloadCaptureConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteConfig {
    pub base_url: Url,
    pub upstream_api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelRoute {
    pub route: String,
    pub upstream_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PayloadCaptureConfig {
    pub default_enabled: bool,
    pub compression: Compression,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Compression {
    Zstd,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LoggingConfig {
    pub format: LogFormat,
    pub level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    Json,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigPaths {
    pub config: PathBuf,
    pub created: bool,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("could not determine current executable path")]
    MissingExecutablePath,
    #[error("could not determine home directory")]
    MissingHomeDirectory,
    #[error("invalid config: {0}")]
    Invalid(String),
    #[error("failed to read config {path}: {source}")]
    Read { path: PathBuf, source: io::Error },
    #[error("failed to write config {path}: {source}")]
    Write { path: PathBuf, source: io::Error },
    #[error("failed to parse config {path}: {source}")]
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("failed to serialize default config: {0}")]
    Serialize(serde_json::Error),
}

impl Default for Config {
    fn default() -> Self {
        let mut routes = BTreeMap::new();
        routes.insert(
            "local".to_owned(),
            RouteConfig {
                base_url: Url::parse("http://localhost:1234").expect("default route URL is valid"),
                upstream_api_key: None,
            },
        );

        Self {
            proxy_listen: "0.0.0.0:8080"
                .parse()
                .expect("default proxy address is valid"),
            admin_listen: "0.0.0.0:8081"
                .parse()
                .expect("default admin address is valid"),
            database: PathBuf::from("./data/llm-proxy.sqlite"),
            payload_dir: PathBuf::from("./data/payloads"),
            master_key: PathBuf::from("./data/master.key"),
            default_route: "local".to_owned(),
            routes,
            models: BTreeMap::new(),
            payload_capture: PayloadCaptureConfig::default(),
            logging: LoggingConfig::default(),
        }
    }
}

impl Default for PayloadCaptureConfig {
    fn default() -> Self {
        Self {
            default_enabled: true,
            compression: Compression::Zstd,
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            format: LogFormat::Json,
            level: "info".to_owned(),
        }
    }
}

impl Config {
    pub fn load_or_create(
        explicit_path: Option<&Path>,
    ) -> Result<(Self, ConfigPaths), ConfigError> {
        let paths = config_path(explicit_path)?;
        if !paths.config.exists() {
            write_default_config(&paths.config)?;
        }

        let contents = fs::read_to_string(&paths.config).map_err(|source| ConfigError::Read {
            path: paths.config.clone(),
            source,
        })?;
        let mut config: Self =
            serde_json::from_str(&contents).map_err(|source| ConfigError::Parse {
                path: paths.config.clone(),
                source,
            })?;
        config.resolve_relative_paths(&paths.config);
        config.validate()?;

        Ok((config, paths))
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.routes.is_empty() {
            return Err(ConfigError::Invalid(
                "at least one route is required".to_owned(),
            ));
        }

        if !self.routes.contains_key(&self.default_route) {
            return Err(ConfigError::Invalid(format!(
                "default_route '{}' does not exist",
                self.default_route
            )));
        }

        for (name, route) in &self.routes {
            if name.trim().is_empty() {
                return Err(ConfigError::Invalid(
                    "route names cannot be empty".to_owned(),
                ));
            }
            match route.base_url.scheme() {
                "http" | "https" => {}
                scheme => {
                    return Err(ConfigError::Invalid(format!(
                        "route '{name}' has unsupported URL scheme '{scheme}'"
                    )));
                }
            }
        }

        for (model, mapping) in &self.models {
            if model.trim().is_empty() {
                return Err(ConfigError::Invalid(
                    "model names cannot be empty".to_owned(),
                ));
            }
            if !self.routes.contains_key(&mapping.route) {
                return Err(ConfigError::Invalid(format!(
                    "model '{model}' references unknown route '{}'",
                    mapping.route
                )));
            }
        }

        Ok(())
    }

    pub fn referenced_upstream_secrets(&self) -> BTreeSet<String> {
        self.routes
            .values()
            .filter_map(|route| route.upstream_api_key.clone())
            .collect()
    }

    fn resolve_relative_paths(&mut self, config_path: &Path) {
        let base_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
        self.database = resolve_path_from_base(base_dir, &self.database);
        self.payload_dir = resolve_path_from_base(base_dir, &self.payload_dir);
        self.master_key = resolve_path_from_base(base_dir, &self.master_key);
    }
}

pub fn default_config_json() -> Result<String, ConfigError> {
    serde_json::to_string_pretty(&Config::default()).map_err(ConfigError::Serialize)
}

fn config_path(explicit_path: Option<&Path>) -> Result<ConfigPaths, ConfigError> {
    if let Some(path) = explicit_path {
        return Ok(ConfigPaths {
            config: path.to_path_buf(),
            created: !path.exists(),
        });
    }

    let exe = env::current_exe().map_err(|_| ConfigError::MissingExecutablePath)?;
    let exe_dir = exe.parent().ok_or(ConfigError::MissingExecutablePath)?;
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or(ConfigError::MissingHomeDirectory)?;
    config_path_from_dirs(exe_dir, &home)
}

fn config_path_from_dirs(exe_dir: &Path, home: &Path) -> Result<ConfigPaths, ConfigError> {
    let neighbor = exe_dir.join(DEFAULT_CONFIG_FILE);
    if neighbor.exists() {
        return Ok(ConfigPaths {
            created: false,
            config: neighbor,
        });
    }

    let config = home
        .join(".config")
        .join("llm-proxy")
        .join(DEFAULT_CONFIG_FILE);
    if config.exists() {
        return Ok(ConfigPaths {
            created: false,
            config,
        });
    }

    if directory_writable(exe_dir) {
        return Ok(ConfigPaths {
            created: true,
            config: neighbor,
        });
    }

    Ok(ConfigPaths {
        created: true,
        config,
    })
}

fn directory_writable(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| !metadata.permissions().readonly())
        .unwrap_or(false)
}

fn resolve_path_from_base(base_dir: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir.join(path)
    }
}

fn write_default_config(path: &Path) -> Result<(), ConfigError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| ConfigError::Write {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    let json = format!("{}\n", default_config_json()?);
    fs::write(path, json).map_err(|source| ConfigError::Write {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        Config::default()
            .validate()
            .expect("default config should validate");
    }

    #[test]
    fn rejects_missing_default_route() {
        let config = Config {
            default_route: "missing".to_owned(),
            ..Config::default()
        };

        assert!(matches!(config.validate(), Err(ConfigError::Invalid(_))));
    }

    #[test]
    fn rejects_model_with_unknown_route() {
        let mut config = Config::default();
        config.models.insert(
            "gpt-5.5".to_owned(),
            ModelRoute {
                route: "openai".to_owned(),
                upstream_model: None,
            },
        );

        assert!(matches!(config.validate(), Err(ConfigError::Invalid(_))));
    }

    #[test]
    fn load_or_create_resolves_relative_paths_from_config_directory() {
        let dir = tempfile::tempdir().expect("config dir");
        let config_path = dir.path().join(DEFAULT_CONFIG_FILE);
        let json = default_config_json().expect("default config json");
        fs::write(&config_path, json).expect("write config");

        let (config, paths) = Config::load_or_create(Some(&config_path)).expect("load config");

        assert_eq!(paths.config, config_path);
        assert_eq!(config.database, dir.path().join("./data/llm-proxy.sqlite"));
        assert_eq!(config.payload_dir, dir.path().join("./data/payloads"));
        assert_eq!(config.master_key, dir.path().join("./data/master.key"));
    }

    #[test]
    fn load_or_create_preserves_absolute_paths() {
        let dir = tempfile::tempdir().expect("config dir");
        let config_path = dir.path().join(DEFAULT_CONFIG_FILE);
        let database = dir.path().join("absolute.sqlite");
        let payload_dir = dir.path().join("absolute-payloads");
        let master_key = dir.path().join("absolute.key");
        let mut config = Config::default();
        config.database = database.clone();
        config.payload_dir = payload_dir.clone();
        config.master_key = master_key.clone();
        let json = serde_json::to_string_pretty(&config).expect("config json");
        fs::write(&config_path, json).expect("write config");

        let (config, _) = Config::load_or_create(Some(&config_path)).expect("load config");

        assert_eq!(config.database, database);
        assert_eq!(config.payload_dir, payload_dir);
        assert_eq!(config.master_key, master_key);
    }

    #[test]
    fn config_path_prefers_existing_neighbor() {
        let exe_dir = tempfile::tempdir().expect("exe dir");
        let home = tempfile::tempdir().expect("home dir");
        let neighbor = exe_dir.path().join(DEFAULT_CONFIG_FILE);
        let home_config = home
            .path()
            .join(".config")
            .join("llm-proxy")
            .join(DEFAULT_CONFIG_FILE);
        fs::create_dir_all(home_config.parent().expect("home config parent"))
            .expect("create home config dir");
        fs::write(&neighbor, "{}").expect("write neighbor config");
        fs::write(&home_config, "{}").expect("write home config");

        let paths = config_path_from_dirs(exe_dir.path(), home.path()).expect("config paths");

        assert_eq!(paths.config, neighbor);
        assert!(!paths.created);
    }

    #[test]
    fn config_path_prefers_existing_home_config_before_creating_neighbor() {
        let exe_dir = tempfile::tempdir().expect("exe dir");
        let home = tempfile::tempdir().expect("home dir");
        let home_config = home
            .path()
            .join(".config")
            .join("llm-proxy")
            .join(DEFAULT_CONFIG_FILE);
        fs::create_dir_all(home_config.parent().expect("home config parent"))
            .expect("create home config dir");
        fs::write(&home_config, "{}").expect("write home config");

        let paths = config_path_from_dirs(exe_dir.path(), home.path()).expect("config paths");

        assert_eq!(paths.config, home_config);
        assert!(!paths.created);
    }

    #[test]
    fn config_path_creates_neighbor_when_no_config_exists_and_exe_dir_is_writable() {
        let exe_dir = tempfile::tempdir().expect("exe dir");
        let home = tempfile::tempdir().expect("home dir");

        let paths = config_path_from_dirs(exe_dir.path(), home.path()).expect("config paths");

        assert_eq!(paths.config, exe_dir.path().join(DEFAULT_CONFIG_FILE));
        assert!(paths.created);
    }
}

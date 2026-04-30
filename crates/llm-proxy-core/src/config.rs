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
        let config: Self =
            serde_json::from_str(&contents).map_err(|source| ConfigError::Parse {
                path: paths.config.clone(),
                source,
            })?;
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
    let neighbor = exe_dir.join(DEFAULT_CONFIG_FILE);
    if neighbor.exists() || directory_writable(exe_dir) {
        return Ok(ConfigPaths {
            created: !neighbor.exists(),
            config: neighbor,
        });
    }

    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or(ConfigError::MissingHomeDirectory)?;
    let config = home
        .join(".config")
        .join("llm-proxy")
        .join(DEFAULT_CONFIG_FILE);
    Ok(ConfigPaths {
        created: !config.exists(),
        config,
    })
}

fn directory_writable(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| !metadata.permissions().readonly())
        .unwrap_or(false)
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
}

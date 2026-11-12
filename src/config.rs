use crate::{Result, Error};
use std::path::{Path, PathBuf};
use serde::Deserialize;
use toml;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub paths: PathsConfig,
    pub repositories: Vec<RepositoryConfig>,
}

#[derive(Debug, Deserialize)]
pub struct PathsConfig {
    pub cache_dir: PathBuf,
    pub store_dir: PathBuf,
    pub profile_dir: PathBuf,
    pub database_path: PathBuf,
}

#[derive(Debug, Deserialize)]
pub struct RepositoryConfig {
    pub name: String,
    pub mirrors: Vec<String>,
    pub index_path: String,
}

impl Config {
    pub fn load<P: AsRef<Path>>(config_path: P) -> Result<Self> {
        let contents = std::fs::read_to_string(config_path).map_err(|e| Error::Configuration(e.to_string()))?;
        let config: Config = toml::from_str(&contents).map_err(|e| Error::Configuration(e.to_string()))?;
        Ok(config)
    }
}
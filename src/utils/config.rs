use crate::{Result, Error};
use std::path::{Path, PathBuf};
use serde::Deserialize;
use toml;
use std::env;

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

#[cfg(test)]
mod tests {
    use super::Config;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn write_config(contents: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("quilla-config-{}-{nonce}.toml", std::process::id()));
        std::fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn loads_paths_and_repository_settings() {
        let path = write_config(r#"
            [paths]
            cache_dir = "/tmp/cache"
            store_dir = "/tmp/store"
            profile_dir = "/tmp/profile"
            database_path = "/tmp/db.json"

            [[repositories]]
            name = "main"
            mirrors = ["https://mirror.test/$repo/$arch"]
            index_path = "core.db"
        "#);
        let config = Config::load(&path).unwrap();
        assert_eq!(config.paths.cache_dir, std::path::Path::new("/tmp/cache"));
        assert_eq!(config.repositories[0].name, "main");
        assert_eq!(config.repositories[0].mirrors[0], "https://mirror.test/$repo/$arch");
        assert_eq!(config.repositories[0].index_path, "core.db");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_invalid_toml_and_missing_file() {
        let path = write_config("not = [valid");
        assert!(Config::load(&path).is_err());
        std::fs::remove_file(&path).unwrap();
        assert!(Config::load(path).is_err());
    }
}

pub fn parse_config() -> Result<PathBuf> {
    let config_path = match env::var("QUILLA_CONFIG") {
        Ok(s) => PathBuf::from(s),
        Err(_) => {
            let home = match env::var("HOME") {
                Ok(s) => s,
                Err(_) => {
                    eprintln!("Could not retrieve the user's home directory");
                    return Err(Error::Configuration("Could not retrieve the user's home directory".to_string()));
                }
            };
            PathBuf::from(format!("{home}/.config/quilla.config.toml"))
        },
    };
    Ok(config_path)
}
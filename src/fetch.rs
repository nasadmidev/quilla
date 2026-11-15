use crate::{Error, Result};
use reqwest::blocking::{Client, Response};
use std::fs::{self, File};
use std::io;
use std::path::{Component, Path, PathBuf};
pub mod verify;

pub struct Fetcher {
    client: Client,
    cache_dir: PathBuf,
}

impl Fetcher {
    pub fn new<P: AsRef<Path>>(cache_dir: P) -> Result<Self> {
        let cache_dir = cache_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&cache_dir)
            .map_err(|e| FetchError::CacheDirCreation(e.to_string()))?;
        Ok(Self {
            client: Client::new(),
            cache_dir,
        })
    }

    pub fn cache_dir(&self) -> &PathBuf {
        &self.cache_dir
    }

    pub fn download(&self, url: &str, file_name: &str) -> Result<(PathBuf, PathBuf)> {
        let mut components = Path::new(file_name).components();
        if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
            return Err(FetchError::InvalidFileName.into());
        }

        let mut response = self
            .client
            .get(url)
            .send()
            .and_then(Response::error_for_status)
            .map_err(|e| FetchError::RequestFailed(e.to_string()))?;

        let temp_path = self.cache_dir.join(format!("{file_name}.part"));
        let final_path = self.cache_dir.join(format!("{file_name}"));
        let mut file =
            File::create(&temp_path).map_err(|e| FetchError::SavingFile(e.to_string()))?;

        match io::copy(&mut response, &mut file) {
            Ok(_) => {}
            Err(e) => {
                let _ = fs::remove_file(&temp_path);
                return Err(FetchError::SavingFile(e.to_string()).into());
            }
        };
        Ok((temp_path, final_path))
    }

    pub fn download_bytes(&self, url: &str) -> Result<Vec<u8>> {
        let response = self
            .client
            .get(url)
            .send()
            .and_then(Response::error_for_status)
            .map_err(|e| FetchError::RequestFailed(e.to_string()))?;
        let bytes = response
            .bytes()
            .map_err(|e| FetchError::RequestFailed(e.to_string()))?;
        Ok(bytes.to_vec())
    }
}

#[derive(Debug)]
pub enum FetchError {
    CacheDirCreation(String),
    SavingFile(String),
    InvalidFileName,
    RequestFailed(String),
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FetchError::CacheDirCreation(s) => write!(f, "Failed in creating cache dir: {s}"),
            FetchError::SavingFile(s) => {
                write!(f, "Could not create the file in cache_dir: {s}")
            }
            FetchError::InvalidFileName => write!(f, "Validation requires a simple name"),
            FetchError::RequestFailed(e) => write!(f, "Request failed: {e}"),
        }
    }
}

impl From<FetchError> for Error {
    fn from(value: FetchError) -> Self {
        Error::Fetch(value)
    }
}

impl std::error::Error for FetchError {}

#[cfg(test)]
mod tests {
    use super::{FetchError, Fetcher};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn rejects_file_names_that_could_escape_the_cache() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let cache =
            std::env::temp_dir().join(format!("quilla-fetch-{}-{nonce}", std::process::id()));
        let fetcher = Fetcher::new(&cache).unwrap();
        for name in ["../outside", "/absolute", "nested/file", ""] {
            assert!(matches!(
                fetcher.download("http://127.0.0.1:1", name),
                Err(crate::Error::Fetch(FetchError::InvalidFileName))
            ));
        }
        std::fs::remove_dir_all(cache).unwrap();
    }
}

use std::fs;
use std::path::{Path, PathBuf};
pub mod error;
pub mod cli;
pub mod generations;
pub mod package;
pub mod store;
pub mod db;
pub mod repo;
pub mod resolver;
pub mod fetch;
pub mod verify;
pub mod transaction;
pub mod config;
use store::StoreError;
pub use error::{Error, Result};


pub fn walkdir(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let root_metadata = match fs::symlink_metadata(dir) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(files),
        Err(error) => return Err(StoreError::DirectoryWalkError(error).into()),
    };

    if !root_metadata.file_type().is_dir() {
        return Ok(files);
    }

    for entry in fs::read_dir(dir).map_err(StoreError::DirectoryWalkError)? {
        let entry = entry.map_err(StoreError::DirectoryWalkError)?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(StoreError::DirectoryWalkError)?;
        if metadata.file_type().is_dir() {
            files.extend(walkdir(&path)?);
        }
        files.push(path);
    }
    Ok(files)
}

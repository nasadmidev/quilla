use std::fs;
use std::path::{Path, PathBuf};
pub mod error;
pub mod cli;
pub mod generations;
pub mod package;
pub mod store;
pub mod db;
use store::StoreError;
pub use error::{Error, Result};


pub fn walkdir(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if dir.is_dir() {
        for entry in fs::read_dir(dir).map_err(|e| StoreError::DirectoryWalkError(e))? {
            let entry = entry.map_err(|e| StoreError::DirectoryWalkError(e))?;
            let path = entry.path();
            if path.is_dir() {
                files.extend(walkdir(&path)?);
            }
            files.push(path);
        }
    }
    Ok(files)
}

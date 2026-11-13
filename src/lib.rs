use std::fs;
use std::path::{Path, PathBuf};
pub mod core;
pub mod fetch;
pub mod utils;
use core::store::StoreError;
pub use utils::error::{Error, Result};


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

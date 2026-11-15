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

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn walkdir_lists_nested_entries_without_following_symlinks() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("quilla-walk-{}-{nonce}", std::process::id()));
        std::fs::create_dir_all(root.join("nested")).unwrap();
        std::fs::write(root.join("nested/file"), "data").unwrap();
        let outside = root.with_extension("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("not-followed"), "data").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();

        let paths = crate::walkdir(&root).unwrap();
        assert!(paths.contains(&root.join("nested/file")));
        #[cfg(unix)]
        assert!(!paths.iter().any(|path| path.starts_with(&outside)));
        assert!(
            crate::walkdir(root.join("missing").as_path())
                .unwrap()
                .is_empty()
        );

        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(outside).unwrap();
    }
}

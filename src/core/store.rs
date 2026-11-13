use crate::core::package::{Arch, PackageMeta};
use crate::{Error, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

pub struct Store {
    store_dir: PathBuf,
}

impl Store {
    pub fn new<P: AsRef<Path>>(store_dir: P) -> Result<Self> {
        let store_dir = store_dir.as_ref().to_path_buf();
        match fs::create_dir_all(&store_dir) {
            Ok(_) => Ok(Self { store_dir }),
            Err(e) => Err(StoreError::CreateStorageError(store_dir, e).into()),
        }
    }

    pub fn get_package_path(&self, meta: &PackageMeta) -> PathBuf {
        let metadata = format!(
            "{}-{}-{}",
            meta.name,
            meta.version,
            match meta.arch {
                Arch::Aarch64 => "aarch64",
                Arch::X86_64 => "x86_64",
                Arch::Any => "any",
            }
        );
        let hash = Sha256::digest(metadata.as_bytes());
        let hex: String = hash.into_iter().map(|b| format!("{b:02x}")).collect();
        self.store_dir.join(format!("{hex}-{metadata}"))
    }

    pub fn store_package(&self, meta: &PackageMeta, temp_extracted_dir: &Path) -> Result<PathBuf> {
        let target_path = self.get_package_path(meta);
        if target_path.exists() {
            return Ok(target_path);
        }

        fs::rename(temp_extracted_dir, &target_path).map_err(|e| StoreError::RenameError {
            from: temp_extracted_dir.to_path_buf(),
            to: target_path.clone(),
            source: e,
        })?;
        self.make_immutable(&target_path)?;
        Ok(target_path)
    }

    fn make_immutable(&self, path: &Path) -> Result<()> {
        for entry in crate::walkdir(path)? {
            let metadata = fs::symlink_metadata(&entry)
                .map_err(|e| StoreError::MetadataError(entry.clone(), e))?;
            if metadata.file_type().is_symlink() {
                continue;
            }

            let mut perms = metadata.permissions();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = if metadata.is_dir() {
                    0o555
                } else if perms.mode() & 0o111 != 0 {
                    // Keep executables runnable while removing write access.
                    0o555
                } else {
                    0o444
                };
                perms.set_mode(mode);
            }
            #[cfg(not(unix))]
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Unsupported,
                    "immutabily store currently require a Unix-like operating system",
                ));
            }
            fs::set_permissions(&entry, perms)
                .map_err(|e| StoreError::SetPermissionsError(entry, e))?;
        }

        Ok(())
    }
}

#[derive(Debug)]
pub enum StoreError {
    CreateStorageError(PathBuf, std::io::Error),
    DirectoryWalkError(std::io::Error),
    MetadataError(PathBuf, std::io::Error),
    SetPermissionsError(PathBuf, std::io::Error),
    RenameError {
        from: PathBuf,
        to: PathBuf,
        source: std::io::Error,
    },
    HashCalculationError(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CreateStorageError(path, e) => {
                write!(f, "Error creating storage path in {path:?}: {e}")
            }
            Self::DirectoryWalkError(e) => write!(f, "Error while iterating through storage: {e}"),
            Self::MetadataError(path, e) => {
                write!(f, "Error while reading metadata for {path:?}: {e}")
            }
            Self::SetPermissionsError(path, e) => {
                write!(f, "Cannot be applied immutable permission in {path:?}: {e}")
            }
            Self::RenameError { from, to, source } => write!(
                f,
                "Error on moving the temp package from {from:?} to {to:?}: {source}"
            ),
            Self::HashCalculationError(msg) => write!(f, "Error on calculation hash: {msg}"),
        }
    }
}

impl From<StoreError> for Error {
    fn from(e: StoreError) -> Self {
        Error::Store(e)
    }
}

use crate::core::repo::RepositoryRecord;
use crate::{Error, Result};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Compare the hash of the temporary file with the one obtained from the repository.
pub fn check_hash<P: AsRef<Path>>(path: P, record: &RepositoryRecord) -> Result<()> {
    let mut file = File::open(&path).map_err(|e| VerifyError::FileOpening(e.to_string()))?;
    let mut hasher = Sha256::new();
    let mut buffer: [u8; 8192] = [0u8; 8192];

    loop {
        let bytes_read = file
            .read(&mut buffer)
            .map_err(|e| VerifyError::Hashing(e.to_string()))?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    let result: String = hasher
        .finalize()
        .into_iter()
        .map(|b| format!("{:02x}", b))
        .collect();

    if result.to_ascii_lowercase() == record.sha256.to_ascii_lowercase() {
        Ok(())
    } else {
        Err(VerifyError::InvalidHash.into())
    }
}

#[derive(Debug)]
pub enum VerifyError {
    Hashing(String),
    FileOpening(String),
    InvalidHash,
}

impl From<VerifyError> for Error {
    fn from(value: VerifyError) -> Self {
        Error::Verify(value)
    }
}

impl std::fmt::Display for VerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerifyError::Hashing(e) => write!(f, "Hashing temporary file error: {e}"),
            VerifyError::FileOpening(e) => write!(f, "Failed when try to open the file: {e}"),
            VerifyError::InvalidHash => write!(f, "The hashes do not match"),
        }
    }
}

impl std::error::Error for VerifyError {}

#[cfg(test)]
mod tests {
    use super::check_hash;
    use crate::core::package::{self, Arch, PackageMeta};
    use crate::core::repo::RepositoryRecord;
    use sha2::{Digest, Sha256};
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_file() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("quilla-verify-{}-{nonce}", std::process::id()))
    }

    fn record(hash: String) -> RepositoryRecord {
        RepositoryRecord {
            package: PackageMeta {
                name: "demo".into(),
                version: package::tests::v("1.0-1"),
                provides: vec![],
                arch: Arch::Any,
                description: "demo".into(),
                depends: vec![],
                licenses: vec![],
                url: None,
                installed_size: 0,
                conflicts: vec![],
                replaces: vec![],
            },
            archive_path: "demo.pkg.tar.zst".into(),
            sha256: hash,
        }
    }

    #[test]
    fn accepts_matching_sha256_case_insensitively() {
        let path = temp_file();
        std::fs::write(&path, b"package bytes").unwrap();
        let hash = hex::encode(Sha256::digest(b"package bytes")).to_uppercase();

        assert!(check_hash(&path, &record(hash)).is_ok());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_mismatched_sha256_and_missing_file() {
        let path = temp_file();
        std::fs::write(&path, b"package bytes").unwrap();
        assert!(check_hash(&path, &record("00".repeat(32))).is_err());
        std::fs::remove_file(&path).unwrap();
        assert!(check_hash(&path, &record("00".repeat(32))).is_err());
    }
}

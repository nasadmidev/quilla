use crate::package::{Dependency, PackageMeta};
use crate::{Error, Result};
use hex;
use serde::{Deserialize, Serialize};
use std::path::{Component, Path};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryRecord {
    pub package: PackageMeta,
    pub archive_path: String,
    pub sha256: String,
}

#[derive(Debug, Clone)]
pub struct Repository {
    pub name: String,
    pub mirrors: Vec<String>,
    pub records: Vec<RepositoryRecord>,
}

impl Repository {
    pub fn find_records(&self, package_name: &str) -> Vec<&RepositoryRecord> {
        let mut package_vec: Vec<&RepositoryRecord> = Vec::new();
        for r in &self.records {
            if r.package.name == package_name {
                package_vec.push(r);
            }
        }

        package_vec
    }

    pub fn find_satisfying(&self, dep: &Dependency) -> Vec<&RepositoryRecord> {
        self.records
            .iter()
            .filter(|r| r.package.satisfies(dep))
            .collect()
    }

    pub fn download_urls(&self, record: &RepositoryRecord) -> Vec<String> {
        self.mirrors
            .iter()
            .map(|s| {
                format!(
                    "{}/{}",
                    s.trim_end_matches('/'),
                    record.archive_path.trim_start_matches('/')
                )
            })
            .collect()
    }

    pub fn parse_index(&mut self, bytes: &[u8]) -> Result<()> {
        let records: Vec<RepositoryRecord> = serde_json::from_slice(bytes)
            .map_err(|e| RepositoryError::ParsingError(e.to_string()))?;
        validate(&records)?;
        self.records = records;
        Ok(())
    }
}

fn validate(records: &[RepositoryRecord]) -> Result<()> {
    for r in records {
        let bytes = hex::decode(&r.sha256).map_err(|e| {
            RepositoryError::ValidationError(format!(
                "Failed in hexadecimal decodification: {}",
                e.to_string()
            ))
        })?;

        if bytes.len() != 32 {
            return Err(RepositoryError::ValidationError(format!(
                "The checksum must be 32 bytes (established by SHA-256): {}",
                r.sha256
            ))
            .into());
        }

        let path = Path::new(&r.archive_path);
        if r.archive_path.is_empty() || !path.is_relative() {
            return Err(RepositoryError::ValidationError(format!(
                "Failed in relative path validation: {}",
                r.archive_path
            ))
            .into());
        }

        for component in path.components() {
            match component {
                Component::ParentDir => {
                    return Err(RepositoryError::ValidationError(format!(
                        "Failed in relative path validation: {}",
                        r.archive_path
                    ))
                    .into());
                }
                _ => (),
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct Catalog {
    pub repositories: Vec<Repository>,
}

impl Catalog {
    pub fn find_satisfying<'a>(
        &'a self,
        dependency: &Dependency,
    ) -> Vec<(&'a Repository, &'a RepositoryRecord)> {
        let mut candidates = Vec::new();
        for repository in &self.repositories {
            for record in repository.find_satisfying(dependency) {
                candidates.push((repository, record));
            }
        }
        candidates
    }

    pub fn add_repository(&mut self, repository: Repository) -> Result<()> {
        if self
            .repositories
            .iter()
            .any(|existing| existing.name == repository.name)
        {
            return Err(RepositoryError::ValidationError(format!(
                "Repository `{}` is already registered",
                repository.name
            ))
            .into());
        }

        self.repositories.push(repository);
        Ok(())
    }
}

#[derive(Debug)]
pub enum RepositoryError {
    ParsingError(String),
    ValidationError(String),
}

impl std::fmt::Display for RepositoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RepositoryError::ParsingError(e) => write!(f, "Error on parsing JSON: {e}"),
            RepositoryError::ValidationError(e) => write!(f, "Error on validation data: {e}"),
        }
    }
}

impl From<RepositoryError> for Error {
    fn from(value: RepositoryError) -> Self {
        Error::Repository(value)
    }
}

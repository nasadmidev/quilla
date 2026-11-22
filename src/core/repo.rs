use crate::core::package::{Dependency, PackageMeta};
use crate::{Error, Result};
use flate2::read::GzDecoder;
use hex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::{Component, Path};
use std::str::FromStr;
use tar::Archive;

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
        let decoder = GzDecoder::new(Cursor::new(bytes));
        let mut archive = Archive::new(decoder);
        let entries = archive.entries().map_err(|e| {
            RepositoryError::ParsingError(format!("cannot read database archive: {e}"))
        })?;

        let mut records = Vec::new();
        for entry in entries {
            let mut entry = entry.map_err(|e| {
                RepositoryError::ParsingError(format!("cannot read database entry: {e}"))
            })?;
            let entry_path = entry.path().map_err(|e| {
                RepositoryError::ParsingError(format!("invalid database entry path: {e}"))
            })?;

            if !entry.header().entry_type().is_file()
                || entry_path.file_name() != Some(std::ffi::OsStr::new("desc"))
            {
                continue;
            }

            let mut desc = String::new();
            entry.read_to_string(&mut desc).map_err(|e| {
                RepositoryError::ParsingError(format!("cannot read package description: {e}"))
            })?;
            records.push(parse_repository_record(&desc)?);
        }

        if records.is_empty() {
            return Err(RepositoryError::ParsingError(
                "database archive contains no package descriptions".to_string(),
            )
            .into());
        }

        validate(&records)?;
        self.records = records;
        Ok(())
    }
}

fn parse_repository_record(desc: &str) -> Result<RepositoryRecord> {
    let mut fields: HashMap<String, Vec<String>> = HashMap::new();
    let mut current_field: Option<String> = None;

    for line in desc.lines() {
        if line.is_empty() {
            continue;
        }

        if line.starts_with('%') && line.ends_with('%') && line.len() > 2 {
            let field = line[1..line.len() - 1].to_string();
            fields.entry(field.clone()).or_default();
            current_field = Some(field);
        } else if let Some(field) = &current_field {
            fields
                .entry(field.clone())
                .or_default()
                .push(line.to_string());
        } else {
            return Err(RepositoryError::ParsingError(format!(
                "value appears before a field header: {line}"
            ))
            .into());
        }
    }

    let name = required_field(&fields, "NAME")?.to_string();
    let version = required_field(&fields, "VERSION")?.parse()?;
    use crate::core::package::Arch;
    let arch = match required_field(&fields, "ARCH")? {
        "x86_64" => Arch::X86_64,
        "aarch64" => Arch::Aarch64,
        "any" => Arch::Any,
        other => {
            return Err(RepositoryError::ValidationError(format!(
                "unsupported package architecture `{other}` for `{name}`"
            ))
            .into());
        }
    };
    let description = optional_field(&fields, "DESC")
        .unwrap_or_default()
        .to_string();
    let url = optional_field(&fields, "URL")
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let licenses = field_values(&fields, "LICENSE").to_vec();
    let installed_size = required_field(&fields, "ISIZE")?
        .parse::<u64>()
        .map_err(|e| RepositoryError::ParsingError(format!("invalid ISIZE for `{name}`: {e}")))?;

    let package = PackageMeta {
        name,
        version,
        arch,
        description,
        url,
        licenses,
        installed_size,
        depends: parse_dependencies(&fields, "DEPENDS")?,
        conflicts: parse_dependencies(&fields, "CONFLICTS")?,
        provides: parse_dependencies(&fields, "PROVIDES")?,
        replaces: parse_dependencies(&fields, "REPLACES")?,
    };

    Ok(RepositoryRecord {
        package,
        archive_path: required_field(&fields, "FILENAME")?.to_string(),
        sha256: required_field(&fields, "SHA256SUM")?.to_string(),
    })
}

fn required_field<'a>(fields: &'a HashMap<String, Vec<String>>, key: &str) -> Result<&'a str> {
    optional_field(fields, key).ok_or_else(|| {
        RepositoryError::ParsingError(format!("missing required %{key}% field")).into()
    })
}

fn optional_field<'a>(fields: &'a HashMap<String, Vec<String>>, key: &str) -> Option<&'a str> {
    fields
        .get(key)
        .and_then(|values| values.first())
        .map(String::as_str)
}

fn field_values<'a>(fields: &'a HashMap<String, Vec<String>>, key: &str) -> &'a [String] {
    fields.get(key).map(Vec::as_slice).unwrap_or(&[])
}

fn parse_dependencies(fields: &HashMap<String, Vec<String>>, key: &str) -> Result<Vec<Dependency>> {
    field_values(fields, key)
        .iter()
        .map(|value| {
            Dependency::from_str(value).map_err(|e| {
                RepositoryError::ParsingError(format!("invalid %{key}% value `{value}`: {e}"))
                    .into()
            })
        })
        .collect()
}

fn validate(records: &[RepositoryRecord]) -> Result<()> {
    for r in records {
        let bytes = hex::decode(&r.sha256).map_err(|e| {
            RepositoryError::ValidationError(format!("Failed in hexadecimal decodification: {}", e))
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
            if component == Component::ParentDir {
                return Err(RepositoryError::ValidationError(format!(
                    "Failed in relative path validation: {}",
                    r.archive_path
                ))
                .into());
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
    pub fn new() -> Self {
        Self {
            repositories: Vec::new(),
        }
    }

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
            RepositoryError::ParsingError(e) => {
                write!(f, "Error on parsing Pacman repository database: {e}")
            }
            RepositoryError::ValidationError(e) => write!(f, "Error on validation data: {e}"),
        }
    }
}

impl From<RepositoryError> for Error {
    fn from(value: RepositoryError) -> Self {
        Error::Repository(value)
    }
}

#[cfg(test)]
mod tests {
    use super::{Catalog, Repository, RepositoryRecord, parse_repository_record, validate};
    use crate::core::package::{Arch, Dependency};

    fn description() -> &'static str {
        "%NAME%\ndemo\n%VERSION%\n1.2-3\n%ARCH%\nx86_64\n%DESC%\nDemo package\n%URL%\nhttps://example.test\n%LICENSE%\nMIT\n%DEPENDS%\nlibc>=2.0\n%ISIZE%\n42\n%FILENAME%\ndemo.pkg.tar.zst\n%SHA256SUM%\n0000000000000000000000000000000000000000000000000000000000000000\n"
    }

    fn record(name: &str) -> RepositoryRecord {
        let mut record = parse_repository_record(description()).unwrap();
        record.package.name = name.into();
        record
    }

    #[test]
    fn parses_package_description_fields_and_dependencies() {
        let record = parse_repository_record(description()).unwrap();
        assert_eq!(record.package.name, "demo");
        assert_eq!(record.package.version.to_string(), "1.2-3");
        assert_eq!(record.package.arch, Arch::X86_64);
        assert_eq!(record.package.description, "Demo package");
        assert_eq!(record.package.url.as_deref(), Some("https://example.test"));
        assert_eq!(record.package.licenses, ["MIT"]);
        assert_eq!(record.package.installed_size, 42);
        assert_eq!(record.package.depends[0].name, "libc");
    }

    #[test]
    fn rejects_missing_fields_and_invalid_architecture() {
        assert!(parse_repository_record("%NAME%\ndemo\n").is_err());
        let invalid_arch = description().replace("x86_64", "mips");
        assert!(parse_repository_record(&invalid_arch).is_err());
    }

    #[test]
    fn rejects_invalid_checksums_and_unsafe_archive_paths() {
        let mut invalid_hash = record("demo");
        invalid_hash.sha256 = "not-hex".into();
        assert!(validate(&[invalid_hash]).is_err());

        let mut traversal = record("demo");
        traversal.archive_path = "../demo.pkg.tar.zst".into();
        assert!(validate(&[traversal]).is_err());
    }

    #[test]
    fn repository_finds_matching_packages_and_builds_mirror_urls() {
        let repo = Repository {
            name: "main".into(),
            mirrors: vec!["https://one.test/".into(), "https://two.test".into()],
            records: vec![record("demo"), record("other")],
        };
        let dependency: Dependency = "demo>=1".parse().unwrap();
        assert_eq!(repo.find_records("demo").len(), 1);
        assert_eq!(repo.find_satisfying(&dependency).len(), 1);
        assert_eq!(
            repo.download_urls(&repo.records[0]),
            [
                "https://one.test/demo.pkg.tar.zst",
                "https://two.test/demo.pkg.tar.zst"
            ]
        );
    }

    #[test]
    fn catalog_rejects_duplicate_repository_names() {
        let repo = Repository {
            name: "main".into(),
            mirrors: vec![],
            records: vec![],
        };
        let mut catalog = Catalog {
            repositories: vec![],
        };
        catalog.add_repository(repo.clone()).unwrap();
        assert!(catalog.add_repository(repo).is_err());
    }
}

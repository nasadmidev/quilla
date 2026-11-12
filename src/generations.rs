use crate::{Error, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub struct GenerationManager {
    profile_dir: PathBuf,
    generations_dir: PathBuf,
}

pub type Profile = GenerationManager;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Generation {
    pub number: u64,
    pub path: PathBuf,
    pub is_current: bool,
}

impl GenerationManager {
    pub fn new<P: AsRef<Path>>(profile_dir: P) -> Result<Self> {
        let profile_dir = profile_dir.as_ref().to_path_buf();
        let generations_dir = profile_dir.join("generations");

        fs::create_dir_all(&generations_dir).map_err(|source| GenerationError::CreateProfile {
            path: generations_dir.clone(),
            source,
        })?;

        Ok(Self {
            profile_dir,
            generations_dir,
        })
    }

    pub fn create_generation<P: AsRef<Path>>(&self, package_paths: &[P]) -> Result<Generation> {
        let number = self.next_generation_number()?;
        let final_path = self.generations_dir.join(number.to_string());
        let staging_path = self.generations_dir.join(format!(".{number}.staging"));

        if staging_path.exists() || final_path.exists() {
            return Err(GenerationError::GenerationAlreadyExists(number).into());
        }

        fs::create_dir(&staging_path).map_err(|source| GenerationError::CreateGeneration {
            path: staging_path.clone(),
            source,
        })?;

        let build_result = (|| {
            for package_path in package_paths {
                let package_path = package_path.as_ref();
                let absolute_package_path = fs::canonicalize(package_path).map_err(|source| {
                    GenerationError::ReadPackage {
                        path: package_path.to_path_buf(),
                        source,
                    }
                })?;
                let metadata = fs::metadata(&absolute_package_path).map_err(|source| {
                    GenerationError::ReadPackage {
                        path: absolute_package_path.clone(),
                        source,
                    }
                })?;
                if !metadata.is_dir() {
                    return Err(GenerationError::PackageNotDirectory(absolute_package_path).into());
                }
                merge_package(
                    &absolute_package_path,
                    &absolute_package_path,
                    &staging_path,
                )?;
            }
            Ok(())
        })();

        if let Err(error) = build_result {
            let _ = fs::remove_dir_all(&staging_path);
            return Err(error);
        }

        fs::rename(&staging_path, &final_path).map_err(|source| {
            let _ = fs::remove_dir_all(&staging_path);
            GenerationError::FinalizeGeneration {
                from: staging_path,
                to: final_path.clone(),
                source,
            }
        })?;

        Ok(Generation {
            number,
            path: final_path,
            is_current: false,
        })
    }

    pub fn activate(&self, number: u64) -> Result<()> {
        let generation_path = self.generations_dir.join(number.to_string());
        if !generation_path.is_dir() {
            return Err(GenerationError::GenerationNotFound(number).into());
        }

        let current = self.profile_dir.join("current");
        let temporary_link = self
            .profile_dir
            .join(format!(".current-{}", std::process::id()));
        let _ = fs::remove_file(&temporary_link);
        create_symlink(
            Path::new("generations").join(number.to_string()),
            &temporary_link,
        )
        .map_err(|source| GenerationError::CreateActiveLink {
            path: temporary_link.clone(),
            source,
        })?;

        fs::rename(&temporary_link, &current).map_err(|source| {
            let _ = fs::remove_file(&temporary_link);
            GenerationError::Activate {
                generation: number,
                source,
            }
        })?;
        Ok(())
    }

    pub fn deactivate(&self) -> Result<()> {
        let current = self.profile_dir.join("current");

        match fs::read_link(&current) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(source) => {
                return Err(GenerationError::ReadActiveLink {
                    path: current,
                    source,
                }
                .into());
            }
        }

        fs::remove_file(&current).map_err(|source| GenerationError::Deactivate {
            path: current,
            source,
        })?;

        Ok(())
    }

    pub fn list_generations(&self) -> Result<Vec<Generation>> {
        let current_number = self.current_generation_number()?;
        let mut generations = Vec::new();

        for entry in
            fs::read_dir(&self.generations_dir).map_err(|source| GenerationError::ReadProfile {
                path: self.generations_dir.clone(),
                source,
            })?
        {
            let entry = entry.map_err(|source| GenerationError::ReadProfile {
                path: self.generations_dir.clone(),
                source,
            })?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(number) = entry
                .file_name()
                .to_str()
                .and_then(|name| name.parse().ok())
            else {
                continue;
            };
            generations.push(Generation {
                number,
                path,
                is_current: current_number == Some(number),
            });
        }

        generations.sort_by_key(|generation| generation.number);
        Ok(generations)
    }

    pub fn rollback(&self) -> Result<Generation> {
        let current = self
            .current_generation_number()?
            .ok_or(GenerationError::NoCurrentGeneration)?;
        let previous = self
            .list_generations()?
            .into_iter()
            .filter(|generation| generation.number < current)
            .max_by_key(|generation| generation.number)
            .ok_or(GenerationError::NoPreviousGeneration(current))?;

        self.activate(previous.number)?;
        Ok(Generation {
            is_current: true,
            ..previous
        })
    }

    fn next_generation_number(&self) -> Result<u64> {
        self.list_generations()?
            .iter()
            .map(|generation| generation.number)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| GenerationError::GenerationNumberOverflow.into())
    }

    pub fn current_generation_number(&self) -> Result<Option<u64>> {
        let current = self.profile_dir.join("current");
        let target = match fs::read_link(&current) {
            Ok(target) => target,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => {
                return Err(GenerationError::ReadActiveLink {
                    path: current,
                    source,
                }
                .into());
            }
        };

        let number = target
            .strip_prefix("generations")
            .ok()
            .and_then(Path::file_name)
            .and_then(|value| value.to_str())
            .and_then(|value| value.parse::<u64>().ok());
        Ok(number)
    }
}

fn merge_package(package_root: &Path, source_dir: &Path, generation_root: &Path) -> Result<()> {
    for entry in fs::read_dir(source_dir).map_err(|source| GenerationError::ReadPackage {
        path: source_dir.to_path_buf(),
        source,
    })? {
        let entry = entry.map_err(|source| GenerationError::ReadPackage {
            path: source_dir.to_path_buf(),
            source,
        })?;
        let source = entry.path();
        let relative = source
            .strip_prefix(package_root)
            .expect("walked package path must remain beneath its root");
        let destination = generation_root.join(relative);
        let metadata =
            fs::symlink_metadata(&source).map_err(|source_error| GenerationError::ReadPackage {
                path: source.clone(),
                source: source_error,
            })?;

        if metadata.file_type().is_symlink() {
            add_file_link(&source, &destination)?;
        } else if metadata.is_dir() {
            match fs::create_dir(&destination) {
                Ok(()) => merge_package(package_root, &source, generation_root)?,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    if !destination.is_dir() {
                        return Err(GenerationError::PathConflict(destination).into());
                    }
                    merge_package(package_root, &source, generation_root)?;
                }
                Err(source) => {
                    return Err(GenerationError::CreateGeneration {
                        path: destination,
                        source,
                    }
                    .into());
                }
            }
        } else {
            add_file_link(&source, &destination)?;
        }
    }
    Ok(())
}

fn add_file_link(source: &Path, destination: &Path) -> Result<()> {
    if destination.exists() || fs::symlink_metadata(destination).is_ok() {
        return Err(GenerationError::PathConflict(destination.to_path_buf()).into());
    }
    create_symlink(source, destination).map_err(|source_error| {
        GenerationError::CreatePackageLink {
            path: destination.to_path_buf(),
            source: source_error,
        }
        .into()
    })
}

#[cfg(unix)]
fn create_symlink<P: AsRef<Path>, Q: AsRef<Path>>(target: P, link: Q) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(not(unix))]
fn create_symlink<P: AsRef<Path>, Q: AsRef<Path>>(_target: P, _link: Q) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "generation profiles currently require a Unix-like operating system",
    ))
}

#[derive(Debug)]
pub enum GenerationError {
    CreateProfile {
        path: PathBuf,
        source: std::io::Error,
    },
    CreateGeneration {
        path: PathBuf,
        source: std::io::Error,
    },
    ReadProfile {
        path: PathBuf,
        source: std::io::Error,
    },
    ReadPackage {
        path: PathBuf,
        source: std::io::Error,
    },
    PackageNotDirectory(PathBuf),
    CreatePackageLink {
        path: PathBuf,
        source: std::io::Error,
    },
    CreateActiveLink {
        path: PathBuf,
        source: std::io::Error,
    },
    FinalizeGeneration {
        from: PathBuf,
        to: PathBuf,
        source: std::io::Error,
    },
    Activate {
        generation: u64,
        source: std::io::Error,
    },
    ReadActiveLink {
        path: PathBuf,
        source: std::io::Error,
    },
    GenerationAlreadyExists(u64),
    GenerationNotFound(u64),
    PathConflict(PathBuf),
    NoCurrentGeneration,
    NoPreviousGeneration(u64),
    GenerationNumberOverflow,
    Deactivate {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl std::fmt::Display for GenerationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CreateProfile { path, source } => {
                write!(f, "cannot create profile at {path:?}: {source}")
            }
            Self::CreateGeneration { path, source } => {
                write!(f, "cannot create generation at {path:?}: {source}")
            }
            Self::ReadProfile { path, source } => {
                write!(f, "cannot read profile at {path:?}: {source}")
            }
            Self::ReadPackage { path, source } => {
                write!(f, "cannot read package at {path:?}: {source}")
            }
            Self::PackageNotDirectory(path) => {
                write!(f, "package path is not a directory: {path:?}")
            }
            Self::CreatePackageLink { path, source } => {
                write!(f, "cannot create package link at {path:?}: {source}")
            }
            Self::CreateActiveLink { path, source } => {
                write!(f, "cannot create active profile link at {path:?}: {source}")
            }
            Self::FinalizeGeneration { from, to, source } => write!(
                f,
                "cannot finalize generation from {from:?} to {to:?}: {source}"
            ),
            Self::Activate { generation, source } => {
                write!(f, "cannot activate generation {generation}: {source}")
            }
            Self::ReadActiveLink { path, source } => {
                write!(f, "cannot read active profile link at {path:?}: {source}")
            }
            Self::GenerationAlreadyExists(number) => {
                write!(f, "generation {number} already exists")
            }
            Self::GenerationNotFound(number) => write!(f, "generation {number} does not exist"),
            Self::PathConflict(path) => {
                write!(f, "multiple packages provide the same path: {path:?}")
            }
            Self::NoCurrentGeneration => write!(f, "profile has no active generation"),
            Self::NoPreviousGeneration(number) => {
                write!(f, "generation {number} has no previous generation")
            }
            Self::GenerationNumberOverflow => write!(f, "generation number overflow"),
            Self::Deactivate { path, source } => {
                write!(f, "cannot deactivate profile link at {path:?}: {source}")
            }
        }
    }
}

impl From<GenerationError> for Error {
    fn from(error: GenerationError) -> Self {
        Error::Generation(error)
    }
}

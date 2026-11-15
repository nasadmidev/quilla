use crate::core::db::{Database, GenerationRecord, InstalledPackage};
use crate::core::generations::GenerationManager;
use crate::core::package::{Arch, Dependency, PackageMeta};
use crate::core::repo::RepositoryRecord;
use crate::core::repo::{Catalog, Repository};
use crate::core::resolver::Resolver;
use crate::core::store::Store;
use crate::fetch::verify;
use crate::fetch::{FetchError, Fetcher};
use crate::{Error, Result};
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Component, Path, PathBuf};
use tar::Archive;
use zstd::stream::read::Decoder;

pub struct Transaction<'a> {
    resolver: Resolver<'a>,
    fetcher: Fetcher,
    store: Store,
    generations: GenerationManager,
}

pub type PackagesPaths = Vec<(PackageMeta, PathBuf)>;

impl<'a> Transaction<'a> {
    pub fn new<P: AsRef<Path>>(
        catalog: &'a Catalog,
        target_arch: Arch,
        cache_dir: P,
        store_dir: P,
        profile_dir: P,
    ) -> Result<Self> {
        Ok(Self {
            resolver: Resolver::new(catalog, target_arch),
            fetcher: Fetcher::new(cache_dir)?,
            store: Store::new(store_dir)?,
            generations: GenerationManager::new(profile_dir)?,
        })
    }

    pub fn install<P: AsRef<Path>>(
        &self,
        dependencies: &[Dependency],
        upgrade_packages: &[String],
        db: &mut Database,
        db_path: P,
    ) -> Result<()> {
        let previous_db = db.clone();
        let previous_generation = db.get_current_generation().map(|g| g.number());
        let packages = self.fetch_resolution_plan(dependencies, upgrade_packages, db)?;
        let stored_packages = self.save_store(packages)?;
        self.save_generation(stored_packages, db)?;
        if let Err(error) = db.save(&db_path) {
            *db = previous_db;

            match previous_generation {
                Some(number) => self.generations.activate(number)?,
                None => self.generations.deactivate()?,
            }

            return Err(error);
        }
        Ok(())
    }

    fn fetch_verified_mirror(&self, url: &str, record: &RepositoryRecord) -> Result<PathBuf> {
        let Some(Component::Normal(name)) = Path::new(&record.archive_path).components().last()
        else {
            return Err(FetchError::InvalidFileName.into());
        };

        let name = name.to_str().ok_or(FetchError::InvalidFileName)?;
        let (temp_path, final_path) = self.fetcher.download(url, name)?;
        match verify::check_hash(&temp_path, record) {
            Ok(_) => {}
            Err(e) => {
                let _ = fs::remove_file(temp_path);
                return Err(e);
            }
        };
        match fs::rename(&temp_path, &final_path) {
            Ok(_) => Ok(final_path),
            Err(e) => {
                let _ = fs::remove_file(temp_path);
                return Err(Error::Io(e));
            }
        }
    }

    fn fetch_verified_package(
        &self,
        repository: &Repository,
        record: &RepositoryRecord,
    ) -> Result<PathBuf> {
        let mut last_error: Option<Error> = None;

        for url in repository.download_urls(record) {
            match self.fetch_verified_mirror(&url, record) {
                Ok(path) => return Ok(path),
                Err(error) => last_error = Some(error),
            }
        }

        match last_error {
            Some(e) => {
                return Err(e);
            }
            None => {
                return Err(Error::Configuration("No mirrors configured".to_string()));
            }
        };
    }

    fn fetch_resolution_plan<'b>(
        &'b self,
        dependencies: &[Dependency],
        upgrade_packages: &[String],
        db: &Database,
    ) -> Result<PackagesPaths> {
        let mut paths: PackagesPaths = Vec::new();
        let plan = self
            .resolver
            .resolution_plan(dependencies, upgrade_packages, db)?;
        for (_, (repository, record)) in plan.packages {
            let path = self.fetch_verified_package(repository, record)?;
            paths.push((record.package.clone(), path));
        }
        Ok(paths)
    }

    fn save_store(&self, packages: PackagesPaths) -> Result<PackagesPaths> {
        let mut stored_packages = Vec::new();
        let uncompressed_packages = unpack_tar(packages, self.fetcher.cache_dir())?;
        for (meta, path) in uncompressed_packages {
            let stored_path = self.store.store_package(&meta, &path)?;
            stored_packages.push((meta, stored_path));
        }
        Ok(stored_packages)
    }

    fn save_generation(&self, stored_packages: PackagesPaths, db: &mut Database) -> Result<()> {
        let mut packages = db
            .get_current_generation()
            .map(|g| g.packages().clone())
            .unwrap_or_default();
        for (meta, store_path) in stored_packages {
            let name = meta.name.clone();
            packages.insert(name, InstalledPackage::new(meta, store_path));
        }
        let stored_paths: Vec<PathBuf> = packages
            .iter()
            .map(|(_, p)| p.store_path().clone())
            .collect();
        let generation = self.generations.create_generation(&stored_paths)?;
        let number = generation.number;
        db.register(GenerationRecord::new(number, packages))?;
        self.generations.activate(number)?;
        db.set_current_generation(number)?;
        Ok(())
    }
}

fn unpack_tar(packages: PackagesPaths, cache_dir: &PathBuf) -> Result<PackagesPaths> {
    let mut uncompressed_packages: PackagesPaths = Vec::new();
    let extracted_root = cache_dir.join("extracted");
    for (meta, path) in packages.into_iter() {
        let file = File::open(&path).map_err(|e| Error::Io(e))?;
        let archive_name = path
            .file_name()
            .ok_or_else(|| Error::Configuration("Invalid archive path".to_string()))?;

        let extracted_folder = extracted_root.join(archive_name);
        fs::create_dir_all(&extracted_folder).map_err(|e| Error::Io(e))?;
        let reader = BufReader::new(file);
        let zstd_decoder =
            Decoder::new(reader).map_err(|_| TransactionError::UnpackFail(meta.name.clone()))?;
        let mut archive = Archive::new(zstd_decoder);
        archive
            .unpack(&extracted_folder)
            .map_err(|e| TransactionError::UnpackFail(e.to_string()))?;
        uncompressed_packages.push((meta, extracted_folder));
    }
    Ok(uncompressed_packages)
}

#[derive(Debug)]
pub enum TransactionError {
    UnpackFail(String),
}

impl std::fmt::Display for TransactionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnpackFail(name) => write!(f, "Failed on unpack package: {name}"),
        }
    }
}

impl std::error::Error for TransactionError {}

impl From<TransactionError> for Error {
    fn from(value: TransactionError) -> Self {
        Error::Transaction(value)
    }
}

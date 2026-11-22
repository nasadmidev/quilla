use crate::core::db::{Database, GenerationRecord, InstalledPackage};
use crate::core::generations::{GenerationError, GenerationManager};
use crate::core::package::{Arch, Dependency, PackageMeta};
use crate::core::repo::RepositoryRecord;
use crate::core::repo::{Catalog, Repository};
use crate::core::resolver::Resolver;
use crate::core::store::Store;
use crate::fetch::verify;
use crate::fetch::{FetchError, Fetcher};
use crate::{Error, Result};
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader};
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

    pub fn install<P: AsRef<Path>, R: io::Read + BufRead>(
        &self,
        mut reader: R,
        dependencies: &[Dependency],
        upgrade_packages: &[String],
        db: &mut Database,
        db_path: P,
    ) -> Result<()> {
        let previous_db = db.clone();
        let previous_generation = db.get_current_generation().map(|g| g.number());
        let packages = self.fetch_resolution_plan(dependencies, upgrade_packages, db)?;
        let stored_packages = self.save_store(packages)?;
        let new_generation = self.save_generation(stored_packages, db)?;
        if let Err(error) = db.save(&db_path) {
            let mut option = String::new();
            println!(
                "Failed on saving database, select one of the following operations\n\t - revert\n\t - active\n\t - inactive"
            );
            let Ok(_) = reader.read_line(&mut option).map_err(Error::Io) else {
                eprintln!("Failed to read buffer, reverting");
                self.revert(
                    db,
                    previous_db,
                    previous_generation,
                    new_generation,
                    db_path,
                )?;
                return Err(error);
            };

            match option.trim() {
                "active" => {
                    eprintln!(
                        "Warning: the database could not be saved to the default path; a backup will be used, and the packages from the current generation will remain active.
                    ");
                    let backup_path = format!("{:?}.bak", db_path.as_ref());
                    let _ = File::create(&backup_path).map_err(Error::Io)?;
                    if let Err(backup_err) = db.save(&backup_path) {
                        eprintln!(
                            "The database backup could not be created in {}: {}",
                            backup_path, backup_err
                        );
                    } else {
                        println!(
                            "The database backup created successfully in {}",
                            backup_path
                        );
                    }
                    return Err(error);
                }
                "inactive" => {
                    match previous_generation {
                        Some(n) => {
                            db.set_current_generation(n)?;
                            self.generations.activate(n)?;
                        }
                        None => {
                            eprintln!("No previous generation found");
                            self.generations.deactivate()?;
                        }
                    }
                    return Err(error);
                }
                choice => {
                    if choice != "revert" {
                        eprintln!(
                            "Invalid option ('{}'), applying 'revert' as default.",
                            option
                        );
                    }
                    self.revert(
                        db,
                        previous_db,
                        previous_generation,
                        new_generation,
                        db_path,
                    )?;
                    return Err(error);
                }
            }
        }
        reconcile_database_with_current(&self.generations, self.generations.profile_dir(), db)?;
        Ok(())
    }

    fn revert<P: AsRef<Path>>(
        &self,
        db: &mut Database,
        previous_database: Database,
        previous_generation: Option<u64>,
        new_generation: u64,
        db_path: P,
    ) -> Result<()> {
        *db = previous_database;
        match previous_generation {
            Some(n) => {
                self.generations.activate(n)?;
                db.set_current_generation(n)?;
            }
            None => {
                self.generations.deactivate()?;
            }
        }
        self.generations.remove_generation(new_generation)?;
        db.save(db_path)?;
        Ok(())
    }

    fn fetch_verified_mirror(&self, url: &str, record: &RepositoryRecord) -> Result<PathBuf> {
        let Some(Component::Normal(name)) =
            Path::new(&record.archive_path).components().next_back()
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
                Err(Error::Io(e))
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
            Some(e) => Err(e),
            None => Err(Error::Configuration("No mirrors configured".to_string())),
        }
    }

    fn fetch_resolution_plan(
        &self,
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

    fn save_generation(&self, stored_packages: PackagesPaths, db: &mut Database) -> Result<u64> {
        let mut packages = db
            .get_current_generation()
            .map(|g| g.packages().clone())
            .unwrap_or_default();
        for (meta, store_path) in stored_packages {
            let name = meta.name.clone();
            packages.insert(name, InstalledPackage::new(meta, store_path));
        }
        let stored_paths: Vec<PathBuf> =
            packages.values().map(|p| p.store_path().clone()).collect();
        let generation = self.generations.create_generation(&stored_paths)?;
        let number = generation.number;
        db.register(GenerationRecord::new(number, packages))?;
        self.generations.activate(number)?;
        db.set_current_generation(number)?;
        Ok(number)
    }
}

/// Rollback has been ruled out, as many of the operations within the transaction do not require by rollback.
pub fn rollback<P: AsRef<Path>, R: io::Read + BufRead>(
    generations: GenerationManager,
    mut reader: R,
    db: &mut Database,
    db_path: P,
) -> Result<()> {
    let mut selected_generation = String::new();
    if let Ok(list) = &generations.list_generations() {
        println!(
            "Generations list:{}",
            list.iter()
                .map(|generation| { format!("\n\t- {}", generation.number) })
                .collect::<String>()
        );
    }
    println!("Select previous generation: ");
    reader
        .read_line(&mut selected_generation)
        .map_err(|e| Error::Io(e))?;

    let Ok(selected_generation) = selected_generation.trim().parse::<u64>() else {
        return Err(Error::Custom(
            "Invalid number in selected generation".to_string(),
        ));
    };

    reconcile_database_with_current(&generations, &generations.profile_dir(), db)?;
    if let None = db.get_current_generation() {
        return Err(TransactionError::Rollback(
            "No current generation found in database".to_string(),
        )
        .into());
    };

    if selected_generation == 0 || !db.is_generation_exists(selected_generation) {
        return Err(TransactionError::Rollback(
            "There is no previous generation or generation number less than or equal to zero"
                .to_string(),
        )
        .into());
    }

    generations.activate(selected_generation).map_err(|e| {
        TransactionError::Rollback(format!(
            "Failed while activating generation: {}",
            e.to_string()
        ))
    })?;
    db.set_current_generation(selected_generation)?;
    db.save(&db_path)?;

    println!("Activate previous generation, rollback to: {selected_generation}");
    Ok(())
}

fn clean_interrupted_staging(generations_dir: &Path) -> Result<()> {
    if generations_dir.exists() {
        for entry in fs::read_dir(generations_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir()
                && let Some(name) = path.file_name().and_then(|n| n.to_str())
                && name.ends_with("staging")
            {
                println!("Cleaning interrupted staging directory: {:?}", path);
                fs::remove_dir_all(&path)?;
            }
        }
    }
    Ok(())
}

fn reconcile_database_with_current(
    generation_manager: &GenerationManager,
    profile_dir: &Path,
    db: &mut Database,
) -> Result<()> {
    let current = profile_dir.join("current");
    let generation = fs::read_link(&current).map_err(|e| {
        Error::Generation(GenerationError::ReadActiveLink {
            path: current,
            source: e,
        })
    })?;
    let current_generation: Option<u64> = match generation.components().next_back() {
        Some(component) => {
            if let Some(component_as_str) = component.as_os_str().to_str() {
                component_as_str.parse::<u64>().ok()
            } else {
                None
            }
        }
        None => None,
    };
    let current_generation_database = db.get_current_generation();
    match (current_generation, current_generation_database) {
        (Some(generation), Some(record)) => {
            if generation == record.number() {
                Ok(())
            } else {
                generation_manager.activate(record.number())?;
                Ok(())
            }
        }
        (None, None) => Ok(()),
        (Some(generation), None) => {
            db.set_current_generation(generation)?;
            Ok(())
        }
        (None, Some(record)) => {
            generation_manager.activate(record.number())?;
            Ok(())
        }
    }
}

impl<'a> Drop for Transaction<'a> {
    fn drop(&mut self) {
        if let Err(e) = clean_interrupted_staging(self.generations.generations_dir()) {
            eprintln!("Warning: failed to clean interrupted staging directories: {e}");
        }
    }
}

fn unpack_tar(packages: PackagesPaths, cache_dir: &Path) -> Result<PackagesPaths> {
    let mut uncompressed_packages: PackagesPaths = Vec::new();
    let extracted_root = cache_dir.join("extracted");
    for (meta, path) in packages.into_iter() {
        let file = File::open(&path).map_err(Error::Io)?;
        let archive_name = path
            .file_name()
            .ok_or_else(|| Error::Configuration("Invalid archive path".to_string()))?;

        let extracted_folder = extracted_root.join(archive_name);
        fs::create_dir_all(&extracted_folder).map_err(Error::Io)?;
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
    ReconcileFail(String),
    Rollback(String),
}

impl std::fmt::Display for TransactionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnpackFail(name) => write!(f, "Failed on unpack package: {name}"),
            Self::ReconcileFail(e) => write!(f, "Failed on reconcile current with database: {e}"),
            Self::Rollback(e) => write!(f, "Rollback error: {e}"),
        }
    }
}

impl std::error::Error for TransactionError {}

impl From<TransactionError> for Error {
    fn from(value: TransactionError) -> Self {
        Error::Transaction(value)
    }
}

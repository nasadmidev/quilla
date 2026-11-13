use crate::core::package::PackageMeta;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPackage {
    metadata: PackageMeta,
    store_path: PathBuf,
}

impl InstalledPackage {
    pub fn new(metadata: PackageMeta, store_path: PathBuf) -> Self {
        Self {
            metadata,
            store_path,
        }
    }

    pub fn metadata(&self) -> &PackageMeta {
        &self.metadata
    }
    pub fn store_path(&self) -> &PathBuf {
        &self.store_path
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationRecord {
    number: u64,
    packages: BTreeMap<String, InstalledPackage>,
}

impl GenerationRecord {
    pub fn new(number: u64, packages: BTreeMap<String, InstalledPackage>) -> Self {
        Self { number, packages }
    }

    pub fn number(&self) -> u64 {
        self.number
    }
    pub fn packages(&self) -> &BTreeMap<String, InstalledPackage> {
        &self.packages
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Database {
    current_generation: Option<u64>,
    generations: BTreeMap<u64, GenerationRecord>,
}

impl Database {
    pub fn new() -> Self {
        Self {
            current_generation: None,
            generations: BTreeMap::new(),
        }
    }

    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let path = path.as_ref();
        let temp_path = path.with_extension(format!("{}.tmp", std::process::id()));

        let write_result = (|| {
            let mut file = File::create(&temp_path).map_err(|e| Error::Io(e))?;
            serde_json::to_writer_pretty(&mut file, self)
                .map_err(|e| Error::Database(e.to_string()))?;
            file.sync_all().map_err(|e| Error::Io(e))?;
            fs::rename(&temp_path, path)?;
            Ok(())
        })();

        if write_result.is_err() {
            let _ = fs::remove_file(&temp_path);
        }
        write_result
    }

    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file =
            File::open(path.as_ref().to_path_buf()).map_err(|e| Error::Database(e.to_string()))?;
        let reader = BufReader::new(file);
        let db: Self =
            serde_json::from_reader(reader).map_err(|e| Error::Database(e.to_string()))?;
        db.validate()?;
        Ok(db)
    }

    pub fn register(&mut self, record: GenerationRecord) -> Result<()> {
        if self.generations.contains_key(&record.number) {
            return Err(Error::Database("Generation already saved".to_string()));
        }
        self.generations.insert(record.number, record);
        Ok(())
    }

    pub fn set_current_generation(&mut self, number: u64) -> Result<()> {
        if !self.generations.contains_key(&number) {
            return Err(Error::Database(format!(
                "Generation {number} does not exist"
            )));
        }
        self.current_generation = Some(number);
        Ok(())
    }

    pub fn get_current_generation(&self) -> Option<&GenerationRecord> {
        self.current_generation
            .and_then(|number| self.generations.get(&number))
    }

    fn validate(&self) -> Result<()> {
        if let Some(number) = self.current_generation {
            if !self.generations.contains_key(&number) {
                return Err(Error::Database(format!(
                    "Current generation {number} is not in generations"
                )));
            }
        }

        for (key, record) in &self.generations {
            if *key != record.number {
                return Err(Error::Database(format!(
                    "Generation key {key} does not match record number {}",
                    record.number()
                )));
            }
        }

        Ok(())
    }
}

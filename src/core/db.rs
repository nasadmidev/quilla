use crate::core::package::PackageMeta;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPackage {
    pub metadata: PackageMeta,
    pub store_path: PathBuf,
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
    pub number: u64,
    pub packages: BTreeMap<String, InstalledPackage>,
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
    pub current_generation: Option<u64>,
    pub generations: BTreeMap<u64, GenerationRecord>,
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
            let mut file = File::create(&temp_path).map_err(Error::Io)?;
            serde_json::to_writer_pretty(&mut file, self)
                .map_err(|e| Error::Database(e.to_string()))?;
            file.sync_all().map_err(Error::Io)?;
            fs::rename(&temp_path, path)?;
            Ok(())
        })();

        if write_result.is_err() {
            let _ = fs::remove_file(&temp_path);
        }
        write_result
    }

    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(path.as_ref()).map_err(|e| Error::Database(e.to_string()))?;
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
        if let Some(number) = self.current_generation
            && !self.generations.contains_key(&number)
        {
            return Err(Error::Database(format!(
                "Current generation {number} is not in generations"
            )));
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

impl Default for Database {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{Database, GenerationRecord, InstalledPackage};
    use crate::core::package::{Arch, PackageMeta};
    use std::{
        collections::BTreeMap,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temporary_database_path() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("quilla-db-{}-{nonce}.json", std::process::id()))
    }

    fn bash_generation(number: u64) -> GenerationRecord {
        let bash = PackageMeta {
            name: "bash".into(),
            version: crate::core::package::tests::v("5.2.37-1"),
            provides: vec![crate::core::package::tests::d("sh=5.2")],
            arch: Arch::X86_64,
            description: "GNU Shell".into(),
            depends: vec![],
            licenses: vec![],
            url: None,
            installed_size: 3600,
            conflicts: vec![],
            replaces: vec![],
        };
        let packages = BTreeMap::from([(
            "bash".to_string(),
            InstalledPackage::new(bash, PathBuf::from("/store/bash")),
        )]);
        GenerationRecord::new(number, packages)
    }

    #[test]
    fn should_save_database() {
        let path = temporary_database_path();
        let db = Database::new();
        let result = db.save(&path);
        assert!(result.is_ok());
        assert!(path.is_file());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn should_load_saved_database_with_current_generation() {
        let path = temporary_database_path();
        let mut db = Database::new();
        db.register(bash_generation(1)).unwrap();
        db.set_current_generation(1).unwrap();
        db.save(&path).unwrap();

        let loaded = Database::load(&path).unwrap();

        assert_eq!(loaded.current_generation, Some(1));
        assert_eq!(loaded.get_current_generation().unwrap().number(), 1);
        assert_eq!(
            loaded.get_current_generation().unwrap().packages()["bash"].store_path(),
            &PathBuf::from("/store/bash")
        );
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn should_reject_loaded_database_with_missing_current_generation() {
        let path = temporary_database_path();
        let db = Database {
            current_generation: Some(1),
            generations: BTreeMap::new(),
        };
        db.save(&path).unwrap();

        assert!(Database::load(&path).is_err());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn should_reject_loaded_database_with_mismatched_generation_key() {
        let path = temporary_database_path();
        let db = Database {
            current_generation: None,
            generations: BTreeMap::from([(1, bash_generation(2))]),
        };
        db.save(&path).unwrap();

        assert!(Database::load(&path).is_err());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn should_register_generation() {
        let bash = PackageMeta {
            name: "bash".into(),
            version: crate::core::package::tests::v("5.2.37-1"),
            provides: vec![crate::core::package::tests::d("sh=5.2")],
            arch: Arch::X86_64,
            description: "GNU Shell".into(),
            depends: vec![],
            licenses: vec![],
            url: None,
            installed_size: 3600,
            conflicts: vec![],
            replaces: vec![],
        };
        let mut db = Database::new();
        let mut packages: BTreeMap<String, InstalledPackage> = BTreeMap::new();
        packages.insert(
            "bash".to_string(),
            InstalledPackage::new(bash, PathBuf::new()),
        );
        let record = GenerationRecord {
            number: 1,
            packages,
        };
        let result = db.register(record);
        assert!(result.is_ok());
    }

    #[test]
    fn should_fail_in_register_generation() {
        let bash = PackageMeta {
            name: "bash".into(),
            version: crate::core::package::tests::v("5.2.37-1"),
            provides: vec![crate::core::package::tests::d("sh=5.2")],
            arch: Arch::X86_64,
            description: "GNU Shell".into(),
            depends: vec![],
            licenses: vec![],
            url: None,
            installed_size: 3600,
            conflicts: vec![],
            replaces: vec![],
        };
        let mut db = Database::new();
        let mut packages: BTreeMap<String, InstalledPackage> = BTreeMap::new();
        packages.insert(
            "bash".to_string(),
            InstalledPackage::new(bash, PathBuf::new()),
        );
        let record = GenerationRecord {
            number: 1,
            packages,
        };
        let _ = db.register(record.clone()).unwrap();
        let result = db.register(record);
        assert!(result.is_err());
    }

    #[test]
    fn should_set_current_generation() {
        let bash = PackageMeta {
            name: "bash".into(),
            version: crate::core::package::tests::v("5.2.37-1"),
            provides: vec![crate::core::package::tests::d("sh=5.2")],
            arch: Arch::X86_64,
            description: "GNU Shell".into(),
            depends: vec![],
            licenses: vec![],
            url: None,
            installed_size: 3600,
            conflicts: vec![],
            replaces: vec![],
        };
        let mut db = Database::new();
        let mut packages: BTreeMap<String, InstalledPackage> = BTreeMap::new();
        packages.insert(
            "bash".to_string(),
            InstalledPackage::new(bash, PathBuf::new()),
        );
        let record = GenerationRecord {
            number: 1,
            packages,
        };
        let _ = db.register(record).unwrap();
        let result = db.set_current_generation(1);
        assert!(result.is_ok());
    }

    #[test]
    fn should_fail_in_set_current_generation() {
        let bash = PackageMeta {
            name: "bash".into(),
            version: crate::core::package::tests::v("5.2.37-1"),
            provides: vec![crate::core::package::tests::d("sh=5.2")],
            arch: Arch::X86_64,
            description: "GNU Shell".into(),
            depends: vec![],
            licenses: vec![],
            url: None,
            installed_size: 3600,
            conflicts: vec![],
            replaces: vec![],
        };
        let mut db = Database::new();
        let mut packages: BTreeMap<String, InstalledPackage> = BTreeMap::new();
        packages.insert(
            "bash".to_string(),
            InstalledPackage::new(bash, PathBuf::new()),
        );
        let record = GenerationRecord {
            number: 2,
            packages,
        };
        let _ = db.register(record).unwrap();
        let result = db.set_current_generation(1);
        assert!(result.is_err());
    }
}

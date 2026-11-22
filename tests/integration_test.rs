use quilla::core::db::Database;
use quilla::core::generations::GenerationManager;
use quilla::core::package::{Arch, Constraint, Dependency};
use quilla::core::repo::{Catalog, Repository};
use quilla::core::transaction::{self, Transaction};
use quilla::fetch::Fetcher;
use quilla::utils::{config::Config, operations};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

mod common;
// Install Testing
#[tokio::test]
async fn installation_downloads_verifies_and_activates_a_generation() {
    let workspace = TestWorkspace::new();
    let server_url = common::run_server(&workspace.0).await;

    run_install(&workspace.0, &server_url, vec!["pkg_test"])
        .await
        .expect("package installation failed");

    assert_package_is_installed(&workspace.0);
}

#[tokio::test]
async fn checksum_failure_does_not_activate_or_register_a_generation() {
    let workspace = TestWorkspace::new();
    let server_url = common::run_server(&workspace.0).await;
    common::make_checksum_invalid(&workspace.0);

    let error = run_install(&workspace.0, &server_url, vec!["pkg_test"])
        .await
        .expect_err("installation unexpectedly accepted a bad checksum");

    assert!(
        error.to_lowercase().contains("hash"),
        "unexpected error: {error}"
    );
    assert_no_committed_generation(&workspace.0);
}

#[tokio::test]
async fn invalid_archive_does_not_activate_or_register_a_generation() {
    let workspace = TestWorkspace::new();
    let server_url = common::run_server(&workspace.0).await;
    common::replace_with_invalid_archive(&workspace.0);

    let error = run_install(&workspace.0, &server_url, vec!["pkg_test"])
        .await
        .expect_err("installation unexpectedly accepted an invalid archive");

    assert!(
        error.to_lowercase().contains("unpack"),
        "unexpected error: {error}"
    );
    assert_no_committed_generation(&workspace.0);
}

#[tokio::test]
async fn database_save_failure_apply_inactive() {
    let input = "inactive\n";
    database_save_failure(input).await;
}

#[tokio::test]
async fn database_save_failure_apply_revert() {
    let input = "revert\n";
    database_save_failure(input).await;
}

#[tokio::test]
async fn database_save_failure_apply_active() {
    let input = "active\n";
    database_save_failure(input).await;
}

// Rollback Testing
#[tokio::test]
async fn should_run_rollback() {
    let workspace = TestWorkspace::new();
    let server_url = common::run_server(&workspace.0).await;
    run_install(&workspace.0, &server_url, vec!["pkg_test"])
        .await
        .expect("First package installation failed");
    assert_package_is_installed(&workspace.0);

    run_install(&workspace.0, &server_url, vec!["pkg_test"])
        .await
        .expect("Second package installation failed");
    let config = Config::load(&format!("{}/quilla.config.toml", workspace.0.display()))
        .expect("Failed loading configuration");
    let mut db = Database::load(&config.paths.database_path).expect("Failed loading database");
    let generations =
        GenerationManager::new(config.paths.profile_dir).expect("Failed loading generations");
    let input = Cursor::new("1\n".as_bytes().to_owned());
    let result = transaction::rollback(generations, input, &mut db, config.paths.database_path);
    assert!(result.is_ok());
}

#[tokio::test]
async fn should_fail_on_rollback_invalid_number() {
    let workspace = TestWorkspace::new();
    let server_url = common::run_server(&workspace.0).await;
    run_install(&workspace.0, &server_url, vec!["pkg_test"])
        .await
        .expect("First package installation failed");
    assert_package_is_installed(&workspace.0);

    run_install(&workspace.0, &server_url, vec!["pkg_test"])
        .await
        .expect("Second package installation failed");
    let config = Config::load(&format!("{}/quilla.config.toml", workspace.0.display()))
        .expect("Failed loading configuration");
    let mut db = Database::load(&config.paths.database_path).expect("Failed loading database");
    let generations =
        GenerationManager::new(config.paths.profile_dir).expect("Failed loading generations");
    let input = Cursor::new("s\n".as_bytes().to_owned());
    let result = transaction::rollback(generations, input, &mut db, config.paths.database_path);
    assert!(
        result.is_err(),
        "Expected rollback to failed on invalid number"
    );
}

#[tokio::test]
async fn should_fail_on_rollback_generation_not_exists() {
    let workspace = TestWorkspace::new();
    let server_url = common::run_server(&workspace.0).await;
    run_install(&workspace.0, &server_url, vec!["pkg_test"])
        .await
        .expect("First package installation failed");
    assert_package_is_installed(&workspace.0);

    run_install(&workspace.0, &server_url, vec!["pkg_test"])
        .await
        .expect("Second package installation failed");
    let config = Config::load(&format!("{}/quilla.config.toml", workspace.0.display()))
        .expect("Failed loading configuration");
    let mut db = Database::load(&config.paths.database_path).expect("Failed loading database");
    let generations =
        GenerationManager::new(config.paths.profile_dir).expect("Failed loading generations");
    let input = Cursor::new("3\n".as_bytes().to_owned());
    let result = transaction::rollback(generations, input, &mut db, config.paths.database_path);
    assert!(
        result.is_err(),
        "Expected rollback to failed when generation does not exist"
    );
}

async fn database_save_failure(input: &str) {
    let workspace = TestWorkspace::new();
    let server_url = common::run_server(&workspace.0).await;
    run_install(&workspace.0, &server_url, vec!["pkg_test"])
        .await
        .expect("initial installation failed");

    let database_path = workspace.0.join("database.json");
    let saved_database_path = workspace.0.join("database-before-failure.json");
    std::fs::rename(&database_path, &saved_database_path).unwrap();
    std::fs::create_dir(&database_path).unwrap();

    let cache_dir = workspace.0.join("cache");
    let store_dir = workspace.0.join("store");
    let profile_dir = workspace.0.join("profile");
    let server_for_worker = server_url.clone();
    let saved_database_for_worker = saved_database_path.clone();
    let input_data = input;
    let input = Cursor::new(input.as_bytes().to_owned());
    let (error, in_memory_generation) = tokio::task::spawn_blocking(move || {
        let fetcher = Fetcher::new(&cache_dir)?;
        let index_url = format!("{server_for_worker}/core/os/x86_64/core.db");
        let index = fetcher.download_bytes(&index_url)?;
        let mut repository = Repository {
            name: "core".to_string(),
            mirrors: vec![format!("{server_for_worker}/core/os/x86_64")],
            records: Vec::new(),
        };
        repository.parse_index(&index)?;
        let catalog = Catalog {
            repositories: vec![repository],
        };

        let mut database = Database::load(&saved_database_for_worker)?;
        let transaction = Transaction::new(
            &catalog,
            Arch::get_architecture_by_consts(),
            cache_dir,
            store_dir,
            profile_dir,
        )?;
        let dependency = Dependency {
            name: "pkg_test".to_string(),
            constraint: Constraint::Any,
        };

        let result = transaction.install(input, &[dependency], &[], &mut database, &database_path);
        let current = database
            .get_current_generation()
            .map(|generation| generation.number());
        Ok::<_, quilla::Error>((result, current))
    })
    .await
    .expect("transaction worker panicked")
    .expect("could not prepare the database-save failure case");

    assert!(error.is_err(), "database save unexpectedly succeeded");
    let error_message = error.unwrap_err().to_string();
    assert!(
        error_message.to_lowercase().contains("i/o error"),
        "expected a database persistence error, got: {error_message}"
    );
    let previous_database = Database::load(&saved_database_path).unwrap();
    let previous_generation = previous_database.get_current_generation().unwrap().number();
    if input_data != "active\n" {
        assert_eq!(in_memory_generation, Some(previous_generation));
    }

    let current_link = std::fs::read_link(workspace.0.join("profile/current")).unwrap();
    assert_eq!(
        current_link,
        PathBuf::from(format!(
            "generations/{}",
            if input_data != "active\n" {
                previous_generation
            } else {
                in_memory_generation.unwrap_or(2)
            }
        ))
    );

    // The failed transaction must leave the old generation as the committed
    // profile, even though its database save was rejected.
    assert!(
        Database::load(&saved_database_path)
            .unwrap()
            .get_current_generation()
            .unwrap()
            .packages()
            .contains_key("pkg_test")
    );
}

#[tokio::test]
async fn repeated_installation_keeps_the_installed_state_consistent() {
    let workspace = TestWorkspace::new();
    let server_url = common::run_server(&workspace.0).await;
    run_install(&workspace.0, &server_url, vec!["pkg_test"])
        .await
        .expect("first installation failed");
    let first = installed_snapshot(&workspace.0);

    run_install(&workspace.0, &server_url, vec!["pkg_test"])
        .await
        .expect("repeated installation failed");
    let second = installed_snapshot(&workspace.0);

    assert_eq!(first, second);
    assert_eq!(
        std::fs::read_dir(workspace.0.join("store"))
            .unwrap()
            .count(),
        1
    );
    assert_package_is_installed(&workspace.0);
}

async fn run_install(root: &Path, server_url: &str, packages: Vec<&str>) -> Result<(), String> {
    let config = create_config(root, server_url)?;
    let packages = packages.into_iter().map(str::to_string).collect();

    tokio::task::spawn_blocking(move || operations::install(config, packages))
        .await
        .map_err(|error| format!("installation worker panicked: {error}"))?
        .map_err(|error| error.to_string())
}

fn create_config(root: &Path, server_url: &str) -> Result<Config, String> {
    let config_path = root.join("quilla.config.toml");
    let config_content = format!(
        r#"
        [paths]
        cache_dir = "{root}/cache"
        store_dir = "{root}/store"
        profile_dir = "{root}/profile"
        database_path = "{root}/database.json"

        [[repositories]]
        name = "core"
        index_path = "core.db"
        mirrors = ["{server_url}/$repo/os/$arch"]
        "#,
        root = root.display(),
    );
    std::fs::write(&config_path, config_content).map_err(|error| error.to_string())?;
    Config::load(config_path).map_err(|error| error.to_string())
}

fn assert_package_is_installed(root: &Path) {
    let installed_binary = root.join("profile/current/usr/bin/test");
    assert!(installed_binary.is_file(), "installed binary is missing");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&installed_binary)
            .unwrap()
            .permissions()
            .mode();
        assert_ne!(mode & 0o111, 0, "installed binary is not executable");
    }

    let database = Database::load(root.join("database.json")).unwrap();
    let generation = database
        .get_current_generation()
        .expect("database has no active generation");
    assert!(generation.packages().contains_key("pkg_test"));
}

fn assert_no_committed_generation(root: &Path) {
    let current = root.join("profile/current");
    assert!(
        std::fs::symlink_metadata(&current).is_err(),
        "a generation was activated despite the failed installation"
    );
    assert!(
        !root.join("database.json").exists(),
        "database was written despite the failed installation"
    );
    let generations = root.join("profile/generations");
    if generations.exists() {
        assert_eq!(
            std::fs::read_dir(generations).unwrap().count(),
            0,
            "an incomplete generation was left behind"
        );
    }
}

#[derive(Debug, PartialEq, Eq)]
struct InstalledSnapshot {
    packages: Vec<(String, String, PathBuf)>,
    binary_contents: Vec<u8>,
    binary_mode: u32,
}

fn installed_snapshot(root: &Path) -> InstalledSnapshot {
    let database = Database::load(root.join("database.json")).unwrap();
    let generation = database
        .get_current_generation()
        .expect("database has no active generation");
    let packages = generation
        .packages()
        .iter()
        .map(|(name, package)| {
            (
                name.clone(),
                package.metadata.version.to_string(),
                package.store_path.clone(),
            )
        })
        .collect();
    let binary = root.join("profile/current/usr/bin/test");
    let binary_contents = std::fs::read(&binary).unwrap();
    #[cfg(unix)]
    let binary_mode = {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(&binary).unwrap().permissions().mode() & 0o777
    };
    #[cfg(not(unix))]
    let binary_mode = 0;

    InstalledSnapshot {
        packages,
        binary_contents,
        binary_mode,
    }
}

struct TestWorkspace(PathBuf);

impl TestWorkspace {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is before UNIX epoch")
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("quilla-integration-{}-{nonce}", std::process::id()));
        std::fs::create_dir_all(&path).expect("could not create test workspace");
        Self(path)
    }
}

impl Drop for TestWorkspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

use quilla::Result;
use quilla::cli::{self, Command};
use quilla::config::Config;
use quilla::db::Database;
use quilla::fetch::Fetcher;
use quilla::package::{Arch, Constraint, Dependency};
use quilla::repo::{Catalog, Repository};
use quilla::transaction::Transaction;
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> quilla::Result<()> {
    let config_path = match env::var("QUILLA_CONFIG") {
        Ok(s) => PathBuf::from(s),
        Err(_) => PathBuf::from("/home/nasadmidev/.config/quilla.config.toml"),
    };
    let config = Config::load(config_path)?;

    match cli::parse()? {
        Command::Install(pkgs) => {
            install(config, pkgs)?;
        }
        Command::Remove(pkgs) => println!("Remove: {:?}", pkgs),
        Command::Upgrade => println!("Update system"),
        Command::Rollback => println!("Return to previous generation"),
    };
    Ok(())
}

fn install(config: Config, pkgs: Vec<String>) -> Result<()> {
    let target_arch = Arch::get_architecture_by_consts();
    let arch_name = match &target_arch {
        Arch::X86_64 => "x86_64",
        Arch::Aarch64 => "aarch64",
        Arch::Any => {
            return Err(quilla::Error::Configuration(
                "Could not detect a supported target architecture".to_string(),
            ));
        }
    };

    let fetcher = Fetcher::new(&config.paths.cache_dir)?;
    let mut catalog = Catalog {
        repositories: Vec::new(),
    };
    for config_repo in config.repositories {
        let repository_name = config_repo.name;
        let mirrors: Vec<String> = config_repo
            .mirrors
            .iter()
            .map(|mirror| {
                mirror
                    .replace("$repo", &repository_name)
                    .replace("$arch", arch_name)
            })
            .collect();
        let mut repository = Repository {
            name: repository_name,
            mirrors: mirrors.clone(),
            records: Vec::new(),
        };
        let mut index_loaded = false;
        let mut last_index_error = None;
        for mirror in &mirrors {
            let index_url = format!(
                "{}/{}",
                mirror.trim_end_matches('/'),
                config_repo.index_path.trim_start_matches('/')
            );
            match fetcher.download_bytes(&index_url) {
                Ok(bytes) => match repository.parse_index(&bytes) {
                    Ok(()) => {
                        index_loaded = true;
                        break;
                    }
                    Err(error) => last_index_error = Some(error.to_string()),
                },
                Err(error) => last_index_error = Some(error.to_string()),
            }
        }

        if !index_loaded {
            return Err(quilla::Error::Configuration(format!(
                "Could not load the repository index {}: {}",
                repository.name,
                last_index_error.unwrap_or_else(|| "no mirrors configured".to_string())
            ))
            .into());
        }

        catalog.add_repository(repository)?;
    }

    let dependencies: Vec<Dependency> = pkgs
        .into_iter()
        .map(|pkg| Dependency {
            name: pkg,
            constraint: Constraint::Any,
        })
        .collect();
    let mut db = match Database::load(&config.paths.database_path) {
        Ok(f) => f,
        Err(e) => {
            if config.paths.database_path.exists() {
                return Err(e);
            } else {
                Database::new()
            }
        }
    };
    let transaction = Transaction::new(
        &catalog,
        target_arch,
        config.paths.cache_dir,
        config.paths.store_dir,
        config.paths.profile_dir,
    )?;
    transaction.install(
        &dependencies,
        &[],
        &mut db,
        config.paths.database_path,
    )?;
    Ok(())
}

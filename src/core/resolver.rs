use crate::core::db::{Database, GenerationRecord};
use crate::core::package::{Arch, Constraint, Dependency, PackageMeta};
use crate::core::repo::{Catalog, Repository, RepositoryRecord};
use crate::{Error, Result};
use std::cmp::Ordering;
use std::collections::BTreeMap;

#[derive(Debug, Default)]
pub struct ResolutionPlan<'a> {
    pub packages: BTreeMap<String, (&'a Repository, &'a RepositoryRecord)>,
}

pub struct Resolver<'a> {
    catalog: &'a Catalog,
    target_arch: Arch,
}

impl<'a> Resolver<'a> {
    pub fn new(catalog: &'a Catalog, target_arch: Arch) -> Self {
        Self {
            catalog,
            target_arch,
        }
    }

    pub fn resolution_plan<'b>(
        &'b self,
        dependencies: &[Dependency],
        upgrade_packages: &[String],
        db: &Database,
    ) -> Result<ResolutionPlan<'b>> {
        let mut plan = ResolutionPlan::default();
        let mut pending = dependencies.to_vec();
        let db_generation = db.get_current_generation();

        if db_generation.is_none()
            && !upgrade_packages.is_empty() {
                return Err(ResolverError::UpdatingAnEmptyGeneration.into());
            }

        if let Some(generation) = db_generation
            && let Some(package) = upgrade_packages
                .iter()
                .find(|name| !generation.packages().contains_key(*name))
            {
                return Err(ResolverError::PackageNotFoundOnUpdating {
                    package: package.clone(),
                    generation: generation.number(),
                }
                .into());
            }

        pending.extend(upgrade_packages.iter().map(|name| Dependency {
            name: name.clone(),
            constraint: Constraint::Any,
        }));
        while let Some(dependency) = pending.pop() {
            if plan
                .packages
                .values()
                .any(|(_, record)| record.package.satisfies(&dependency))
            {
                continue;
            }

            if !upgrade_packages.contains(&dependency.name)
                && let Some(generation) = db_generation
                    && generation
                        .packages()
                        .values()
                        .any(|installed| installed.metadata().satisfies(&dependency))
                    {
                        continue;
                    }

            let (repository, record) = self.select_candidate(&dependency)?;
            let package_name = record.package.name.clone();

            if upgrade_packages.contains(&package_name)
                && let Some(generation) = db_generation
                    && let Some(installed) = generation.packages().get(&package_name) {
                        match record.package.version.cmp(&installed.metadata().version) {
                            Ordering::Less => {
                                return Err(ResolverError::DowngradeNotAllowed {
                                    package: package_name,
                                }
                                .into());
                            }
                            Ordering::Equal => continue,
                            Ordering::Greater => {}
                        }
                    }

            if let Some((_, existing)) = plan.packages.get(&package_name) {
                if existing.package.satisfies(&dependency) {
                    continue;
                }

                return Err(ResolverError::ConflictingRequirements {
                    package: package_name,
                }
                .into());
            }

            find_conflict(&plan, &record.package)?;

            pending.extend(record.package.depends.iter().cloned());
            plan.packages.insert(package_name, (repository, record));
        }

        if let Some(generation) = db_generation {
            find_generation_conflict(generation, &plan)?;
        }

        Ok(plan)
    }

    fn select_candidate<'b>(
        &'b self,
        dependency: &Dependency,
    ) -> Result<(&'b Repository, &'b RepositoryRecord)> {
        let candidates = self
            .find_candidates(dependency)
            .into_iter()
            .max_by(|(_, left), (_, right)| left.package.version.cmp(&right.package.version));

        match candidates {
            Some(c) => Ok(c),
            None => Err(ResolverError::NoCandidate {
                dependency: dependency.name.clone(),
            }
            .into()),
        }
    }

    fn find_candidates<'b>(
        &'b self,
        dependency: &Dependency,
    ) -> Vec<(&'b Repository, &'b RepositoryRecord)> {
        self.catalog
            .find_satisfying(dependency)
            .into_iter()
            .filter(|(_, record)| {
                record.package.arch == self.target_arch || record.package.arch == Arch::Any
            })
            .collect()
    }
}

fn find_conflict(plan: &ResolutionPlan, candidate: &PackageMeta) -> Result<()> {
    for (_, record) in plan.packages.values() {
        if candidate
            .conflicts
            .iter()
            .any(|conflict| record.package.satisfies(conflict))
        {
            return Err(ResolverError::DependenciesConflict {
                candidate: candidate.name.clone(),
                record: record.package.name.clone(),
            }
            .into());
        }

        if record
            .package
            .conflicts
            .iter()
            .any(|conflict| candidate.satisfies(conflict))
        {
            return Err(ResolverError::DependenciesConflict {
                candidate: candidate.name.clone(),
                record: record.package.name.clone(),
            }
            .into());
        }
    }
    Ok(())
}

fn find_generation_conflict(generation: &GenerationRecord, plan: &ResolutionPlan) -> Result<()> {
    for (installed_name, installed_package) in generation.packages() {
        if plan.packages.contains_key(installed_name) {
            continue;
        }

        let installed = installed_package.metadata();

        for (_, record) in plan.packages.values() {
            let candidate = &record.package;

            let candidate_conflicts = candidate
                .conflicts
                .iter()
                .any(|conflict| installed.satisfies(conflict));

            let installed_conflicts = installed
                .conflicts
                .iter()
                .any(|conflict| candidate.satisfies(conflict));

            if candidate_conflicts || installed_conflicts {
                return Err(ResolverError::DependenciesConflict {
                    candidate: candidate.name.clone(),
                    record: installed.name.clone(),
                }
                .into());
            }
        }
    }

    Ok(())
}

#[derive(Debug)]
pub enum ResolverError {
    NoCandidate { dependency: String },
    ConflictingRequirements { package: String },
    DependenciesConflict { candidate: String, record: String },
    PackageNotFoundOnUpdating { package: String, generation: u64 },
    UpdatingAnEmptyGeneration,
    DowngradeNotAllowed { package: String },
}

impl std::error::Error for ResolverError {}

impl std::fmt::Display for ResolverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoCandidate { dependency } => {
                write!(f, "No package satisfies dependency `{dependency}`")
            }
            Self::ConflictingRequirements { package } => {
                write!(f, "Incompatible requirements for package `{package}`")
            }
            Self::DependenciesConflict { candidate, record } => {
                write!(f, "Dependencies in conflict: {candidate} with {record}")
            }
            Self::PackageNotFoundOnUpdating {
                package,
                generation,
            } => {
                write!(
                    f,
                    "Package {package} marked for update is not installed in the current generation: {generation}"
                )
            }
            Self::UpdatingAnEmptyGeneration => {
                write!(f, "Trying to update packages in empty generation")
            }
            Self::DowngradeNotAllowed { package } => {
                write!(f, "Downgrade not allowed for package: {package}")
            }
        }
    }
}

impl From<ResolverError> for Error {
    fn from(value: ResolverError) -> Self {
        Error::Resolver(value)
    }
}

#[cfg(test)]
mod test {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use crate::core::db::{Database, GenerationRecord, InstalledPackage};
    use crate::core::package::{self, Arch, Constraint, Dependency, PackageMeta};
    use crate::core::repo::{Catalog, Repository, RepositoryRecord};

    fn create_catalog() -> (PackageMeta, RepositoryRecord, Repository, Catalog) {
        let bash = PackageMeta {
            name: "bash".into(),
            version: package::tests::v("5.2.37-1"),
            provides: vec![package::tests::d("sh=5.2")],
            arch: Arch::X86_64,
            description: "GNU Shell".into(),
            depends: vec![],
            licenses: vec![],
            url: None,
            installed_size: 3600,
            conflicts: vec![],
            replaces: vec![],
        };

        let record = RepositoryRecord {
            package: bash.clone(),
            archive_path: "/bash".to_string(),
            sha256: "1234".to_string(),
        };

        let repository = Repository {
            name: "main".to_string(),
            mirrors: vec!["mirror.com".to_string()],
            records: vec![record.clone()],
        };

        let catalog: Catalog = Catalog {
            repositories: vec![repository.clone()],
        };

        (bash, record, repository, catalog)
    }

    #[test]
    fn should_get_resolution_plan() {
        let (_, record, repository, catalog) = create_catalog();
        let db = Database::new();

        let resolver = super::Resolver::new(&catalog, Arch::X86_64);
        let dependency = Dependency {
            name: "bash".to_string(),
            constraint: Constraint::Any,
        };
        let plan = resolver.resolution_plan(&[dependency.clone()], &[], &db);

        match plan {
            Ok(p) => {
                for (k, (repo, rec)) in p.packages {
                    assert_eq!(k, "bash", "Resolution plan name failed");
                    assert_eq!(repo.name, repository.name);
                    assert_eq!(repo.mirrors, repository.mirrors);
                    assert!(rec.package.satisfies(&dependency));
                    assert_eq!(rec.archive_path, record.archive_path);
                    assert_eq!(rec.sha256, record.sha256);
                }
            }
            Err(e) => panic!("Error: {e}"),
        }
    }

    #[test]
    fn should_fail_with_no_generation_and_upgrade_packages_defined() {
        let (_, _, _, catalog) = create_catalog();
        let db = Database::new();
        let resolver = super::Resolver::new(&catalog, Arch::X86_64);
        let dependency = Dependency {
            name: "bash".to_string(),
            constraint: Constraint::Any,
        };
        let plan = resolver.resolution_plan(&[dependency.clone()], &["bash".to_string()], &db);

        assert!(plan.is_err());
    }

    #[test]
    fn should_fail_when_cant_find_the_package_to_update() {
        let (bash, _, _, catalog) = create_catalog();
        let mut installed_package: BTreeMap<String, InstalledPackage> = BTreeMap::new();
        let mut generation_map: BTreeMap<u64, GenerationRecord> = BTreeMap::new();
        installed_package.insert(
            "other_package".to_string(),
            InstalledPackage {
                metadata: bash,
                store_path: PathBuf::new(),
            },
        );
        generation_map.insert(
            1,
            GenerationRecord {
                number: 1,
                packages: installed_package,
            },
        );
        let db = Database {
            current_generation: Some(1),
            generations: generation_map,
        };
        let resolver = super::Resolver::new(&catalog, Arch::X86_64);
        let dependency = Dependency {
            name: "bash".to_string(),
            constraint: Constraint::Any,
        };
        let plan = resolver.resolution_plan(&[dependency.clone()], &["bash".to_string()], &db);

        assert!(plan.is_err());
    }

    #[test]
    fn should_select_the_highest_version_candidate() {
        let (mut older, _, _, _) = create_catalog();
        older.version = package::tests::v("5.2.36-1");
        let newer = create_catalog().0;
        let repository = Repository {
            name: "main".to_string(),
            mirrors: vec!["mirror.com".to_string()],
            records: vec![
                RepositoryRecord {
                    package: older,
                    archive_path: "/bash-old".to_string(),
                    sha256: "old".to_string(),
                },
                RepositoryRecord {
                    package: newer,
                    archive_path: "/bash-new".to_string(),
                    sha256: "new".to_string(),
                },
            ],
        };
        let catalog = Catalog {
            repositories: vec![repository],
        };
        let resolver = super::Resolver::new(&catalog, Arch::X86_64);
        let dependency = Dependency {
            name: "bash".to_string(),
            constraint: Constraint::Any,
        };

        let (_, candidate) = resolver.select_candidate(&dependency).unwrap();

        assert_eq!(candidate.archive_path, "/bash-new");
    }

    #[test]
    fn should_ignore_candidates_for_another_architecture() {
        let (mut incompatible, _, _, _) = create_catalog();
        incompatible.arch = Arch::Aarch64;
        let repository = Repository {
            name: "main".to_string(),
            mirrors: vec!["mirror.com".to_string()],
            records: vec![RepositoryRecord {
                package: incompatible,
                archive_path: "/bash-aarch64".to_string(),
                sha256: "hash".to_string(),
            }],
        };
        let catalog = Catalog {
            repositories: vec![repository],
        };
        let resolver = super::Resolver::new(&catalog, Arch::X86_64);
        let dependency = Dependency {
            name: "bash".to_string(),
            constraint: Constraint::Any,
        };

        let result = resolver.select_candidate(&dependency);

        assert!(result.is_err());
    }
}

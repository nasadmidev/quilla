use crate::db::{Database, GenerationRecord};
use crate::package::{Arch, Constraint, Dependency, PackageMeta};
use crate::repo::{Catalog, Repository, RepositoryRecord};
use crate::{Error, Result};
use std::collections::BTreeMap;
use std::cmp::Ordering;

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

        if let None = db_generation {
            if upgrade_packages.len() > 0 {
                return Err(ResolverError::UpdatingAnEmptyGeneration.into());
            }
        }

        if let Some(generation) = db_generation {
            if let Some(package) = upgrade_packages
                .iter()
                .find(|name| !generation.packages().contains_key(*name))
            {
                return Err(ResolverError::PackageNotFoundOnUpdating {
                    package: package.clone(),
                    generation: generation.number(),
                }
                .into());
            }
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

            if !upgrade_packages.contains(&dependency.name) {
                if let Some(generation) = db_generation {
                    if generation
                        .packages()
                        .values()
                        .any(|installed| installed.metadata().satisfies(&dependency))
                    {
                        continue;
                    }
                }
            }

            let (repository, record) = self.select_candidate(&dependency)?;
            let package_name = record.package.name.clone();

            if upgrade_packages.contains(&package_name) {
                if let Some(generation) = db_generation {
                    if let Some(installed) = generation.packages().get(&package_name) {
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
    DowngradeNotAllowed { package: String }
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

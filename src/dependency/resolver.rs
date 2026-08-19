use crate::{
    aur::{self, rpc::RpcError},
    dependency::{self, Dependency, DependencyGraph, PackageNode, PacmanError},
    ui,
};
use std::{
    collections::HashMap,
    process::{Command, Stdio},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ResolveError {
    #[error(transparent)]
    Rpc(#[from] RpcError),

    #[error(transparent)]
    Pacman(#[from] PacmanError),

    #[error("provider not found: {0}")]
    NotFound(String),

    #[error("package '{0}' does not satisfy the requirement (found '{1}')")]
    VersionMismatch(String, String),
}

pub struct Resolver {
    graph: DependencyGraph,
    installed: HashMap<String, String>,
    providers: HashMap<String, String>,
}

impl Resolver {
    pub fn new() -> Result<Self, PacmanError> {
        Ok(Self {
            graph: DependencyGraph::default(),
            installed: dependency::installed_packages()?,
            providers: HashMap::new(),
        })
    }

    pub fn resolve<'a, T: IntoIterator<Item = &'a str>>(
        mut self,
        packages: T,
        loading: &ui::Loading,
    ) -> Result<DependencyGraph, ResolveError> {
        packages
            .into_iter()
            .map(|package| Dependency::new(package, dependency::DependencyKind::Runtime))
            .try_for_each(|package| self.package(&package, loading).map(|_| ()))
            .map(|_| self.graph)
    }

    fn package(
        &mut self,
        package: &Dependency,
        loading: &ui::Loading,
    ) -> Result<String, ResolveError> {
        let name = package.name.as_str();

        if package.requirement.is_none()
            && let Ok(provider) = self.cached_provider(name)
        {
            loading.set_message(format!("Using cached package: {}", package.spec));
            return Ok(provider);
        }

        loading.set_message(format!("Checking installed package: {}", package.spec));
        if let Some(provider) = self.resolve_installed(package)? {
            return Ok(provider);
        }

        loading.set_message(format!("Checking repository: {}", package.spec));
        let repo_error = match self.resolve_repo(package) {
            Ok(provider) => return Ok(provider),
            Err(error) => error,
        };

        if matches!(&repo_error, ResolveError::VersionMismatch(..)) {
            loading.set_message(format!("Fetching AUR metadata: {}", package.spec));
            return self.resolve_aur(package, loading);
        }

        loading.set_message(format!("Checking providers: {}", package.spec));
        let provider_error = match Self::resolve_provider(self, package, loading) {
            Ok(provider) => return Ok(provider),
            Err(error) => error,
        };

        if matches!(&provider_error, ResolveError::VersionMismatch(..)) {
            return Err(provider_error);
        }

        loading.set_message(format!("Fetching AUR metadata: {}", package.spec));
        self.resolve_aur(package, loading)
    }

    fn cached_provider(&self, name: &str) -> Result<String, ResolveError> {
        if let Some(provider) = self.providers.get(name) {
            Ok(provider.clone())
        } else {
            Err(ResolveError::NotFound(name.into()))
        }
    }

    fn resolve_installed(&mut self, package: &Dependency) -> Result<Option<String>, ResolveError> {
        if !self.installed.contains_key(&package.name) {
            return Ok(None);
        }

        let installed = PackageNode::from_installed(&package.name)?;
        if !package_satisfies(&installed, package)? {
            return Ok(None);
        }

        self.graph.insert(installed);
        self.remember_provider(&package.name, &package.name);
        Ok(Some(package.name.clone()))
    }

    fn resolve_repo(&mut self, package: &Dependency) -> Result<String, ResolveError> {
        let result = PackageNode::from_pacman(&package.name)?;

        if !package_satisfies(&result, package)? {
            return Err(ResolveError::VersionMismatch(
                package.spec.clone(),
                result.version.clone().unwrap_or_default(),
            ));
        }

        self.graph.insert(result);
        self.remember_provider(&package.name, &package.name);
        Ok(package.name.clone())
    }

    fn resolve_provider(
        &mut self,
        package: &Dependency,
        loading: &ui::Loading,
    ) -> Result<String, ResolveError> {
        let provider = match Self::provider_of(&package.spec) {
            Ok(provider) => provider,
            Err(error) => return Err(ResolveError::Pacman(error)),
        };

        self.package(&package.for_provider(&provider), loading)
            .inspect(|provider| self.remember_provider(&package.name, provider.clone()))
    }

    fn resolve_aur(
        &mut self,
        package: &Dependency,
        loading: &ui::Loading,
    ) -> Result<String, ResolveError> {
        let aur_info = match aur::rpc::fetch_package_info(&package.name) {
            Ok(info) => info,
            Err(error) => return Err(ResolveError::Rpc(error)),
        };
        Self::warn_about_aur(&aur_info);

        let aur_name = aur_info.name.clone();
        let mut node = PackageNode::from_rpc(&aur_info);

        if !package_satisfies(&node, package)? {
            return Err(ResolveError::VersionMismatch(
                package.spec.clone(),
                aur_info.version,
            ));
        }

        self.remember_provider(&package.name, aur_name.clone());

        self.resolve_dependencies(&mut node, loading)?;
        self.graph.insert(node);
        Ok(aur_name)
    }

    fn warn_about_aur(package: &aur::rpc::RpcPackage) {
        if package.orphan() {
            ui::warn(format_args!("{} has no maintainer", package.name));
        }

        if package.is_outdated()
            && let Some(flagged) = package.out_of_date
        {
            ui::warn(format_args!(
                "{} was flagged out of date {}",
                package.name,
                ui::relative_time(flagged),
            ));
        }
    }

    fn resolve_dependencies(
        &mut self,
        node: &mut PackageNode,
        loading: &ui::Loading,
    ) -> Result<(), ResolveError> {
        node.dependencies
            .iter_mut()
            .filter(|dependency| dependency.kind.is_resolvable())
            .try_for_each(|dependency| {
                loading.set_message(format!("Resolving dependency: {}", dependency.spec));
                dependency.name = self.package(dependency, loading)?;
                Ok(())
            })
    }

    fn remember_provider(&mut self, requested: impl Into<String>, provider: impl Into<String>) {
        self.providers.insert(requested.into(), provider.into());
    }

    pub fn provider_of(name: &str) -> Result<String, PacmanError> {
        let package = dependency::normalize_name(name);
        let provider = Self::provider_output(package)?
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && *line != package)
            .map(str::to_owned)
            .next();

        if let Some(provider) = provider {
            Ok(provider)
        } else {
            Err(PacmanError::ProviderNotFound(name.into()))
        }
    }

    fn provider_output(name: &str) -> Result<String, PacmanError> {
        let output = Command::new("pacman")
            .args(["-Sp", "--nodeps", "--noconfirm", "--print-format=%n", name])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .map_err(PacmanError::spawn)?;

        if !output.status.success() {
            Err(PacmanError::ProviderNotFound(name.into()))
        } else {
            Ok(String::from_utf8(output.stdout)?)
        }
    }
}

fn package_satisfies(package: &PackageNode, dependency: &Dependency) -> Result<bool, ResolveError> {
    let Some(requirement) = &dependency.requirement else {
        return Ok(true);
    };

    if package.name == dependency.requirement_name {
        return match package.version.as_deref() {
            Some(version) => Ok(requirement.matches(version)?),
            None => Ok(false),
        };
    }

    for provided in &package.provides {
        let provided = Dependency::new(provided, dependency.kind);

        if provided.name == dependency.requirement_name {
            return match provided.requirement {
                Some(provided_requirement) => {
                    Ok(requirement.matches(&provided_requirement.version)?)
                }
                None => Ok(false),
            };
        }
    }

    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dependency::{DependencyKind, PackageSource};

    fn package(name: &str, version: Option<&str>, provides: &[&str]) -> PackageNode {
        PackageNode {
            name: name.to_owned(),
            version: version.map(str::to_owned),
            source: PackageSource::Repo,
            dependencies: Vec::new(),
            size: None,
            download_size: None,
            provides: provides
                .iter()
                .map(|provide| (*provide).to_owned())
                .collect(),
            packager: None,
            aur: None,
        }
    }

    #[test]
    fn accepts_unversioned_and_matching_versioned_packages() {
        let candidate = package("demo", Some("2.1-1"), &[]);

        assert!(
            package_satisfies(
                &candidate,
                &Dependency::new("demo", DependencyKind::Runtime)
            )
            .expect("unversioned comparison")
        );
        assert!(
            package_satisfies(
                &candidate,
                &Dependency::new("demo>=2", DependencyKind::Runtime)
            )
            .expect("version comparison")
        );
        assert!(
            !package_satisfies(
                &candidate,
                &Dependency::new("demo>3", DependencyKind::Runtime)
            )
            .expect("version comparison")
        );
    }

    #[test]
    fn checks_versioned_virtual_provides() {
        let candidate = package("provider", Some("9"), &["virtual-api=3"]);

        assert!(
            package_satisfies(
                &candidate,
                &Dependency::new("virtual-api>=2", DependencyKind::Build)
            )
            .expect("provided version comparison")
        );
        assert!(
            !package_satisfies(
                &candidate,
                &Dependency::new("virtual-api>3", DependencyKind::Build)
            )
            .expect("provided version comparison")
        );
    }

    #[test]
    fn cached_provider_round_trips_requested_name() {
        let mut resolver = Resolver {
            graph: DependencyGraph::default(),
            installed: HashMap::new(),
            providers: HashMap::new(),
        };
        resolver.remember_provider("virtual", "real");

        assert_eq!(resolver.cached_provider("virtual").unwrap(), "real");
        assert!(matches!(
            resolver.cached_provider("missing"),
            Err(ResolveError::NotFound(_))
        ));
    }
}

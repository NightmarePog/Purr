use std::{
    collections::BTreeSet,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

use crate::{
    aur, build,
    cli::CliError,
    config,
    dependency::{self, InstallPlan, PackageNode, PackageSource},
    ui::{self, prompt},
};

pub fn install<'a, T: IntoIterator<Item = &'a str>>(
    package_names: T,
    verbose: bool,
) -> Result<(), CliError> {
    let loading = ui::loading("Reading installed package database")?;
    let resolver = dependency::Resolver::new()?;
    loading.set_message("Resolving package sources".to_owned());
    let graph = resolver.resolve(package_names, &loading)?;
    drop(loading);

    let plan = InstallPlan::from_graph(&graph);
    ui::header("Install plan");
    ui::install_plan(&plan);

    if verbose {
        ui::aur_details(&plan);
    }
    ui::question("Continue with installation? [y/N]");
    ui::prompt_marker();

    if !confirm()? {
        return Err(CliError::UserCancelled);
    }

    ui::step("Fetching sources");
    let build_root = prepare_build_root()?;
    let sources = fetch_sources(&plan, &build_root)?;
    let repo_packages = plan
        .packages
        .iter()
        .filter(|package| matches!(package.source, PackageSource::Repo))
        .map(|package| package.name.clone())
        .collect();
    let build_plan = build::BuildPlan::new(sources, repo_packages);

    build_plan.install_repo_packages()?;
    ui::step("Build");
    let environment = build::Environment::new(build::SandboxFiles::new()?, &build_root)?;
    let result = build_plan.execute(&environment)?;
    ui::success(format_args!("built {} artifact(s)", result.artifacts()));
    ui::step("Installing application wrappers");
    result.install_wrappers()?;
    ui::success("installed packages");
    Ok(())
}

pub fn confirm() -> Result<bool, CliError> {
    let answer = prompt()?;
    Ok(answer.trim().eq_ignore_ascii_case("y") || answer.trim().eq_ignore_ascii_case("yes"))
}

fn prepare_build_root() -> Result<PathBuf, CliError> {
    let root = config::build_path().ok_or(build::SandboxError::MissingCacheDir)?;
    secure_build_root(&root)?;
    Ok(root)
}

fn secure_build_root(root: &Path) -> Result<(), CliError> {
    fs::create_dir_all(root).map_err(build::SandboxError::from)?;
    fs::set_permissions(root, fs::Permissions::from_mode(0o700))
        .map_err(build::SandboxError::from)?;
    Ok(())
}

fn fetch_sources(plan: &InstallPlan, build_root: &Path) -> Result<Vec<PathBuf>, CliError> {
    unique_aur_packages(plan)
        .into_iter()
        .map(|package| {
            let repository = repository_of(package, build_root)?;
            fetch_source(repository)
        })
        .collect()
}

fn unique_aur_packages(plan: &InstallPlan) -> Vec<&PackageNode> {
    let mut fetched_bases = BTreeSet::new();
    plan.packages
        .iter()
        .filter(|package| matches!(package.source, PackageSource::Aur))
        .filter(|package| {
            let base = package
                .aur
                .as_ref()
                .map(|aur| aur.base.as_str())
                .unwrap_or(&package.name);
            fetched_bases.insert(base.to_owned())
        })
        .collect()
}

fn fetch_source(repository: aur::Package<'_>) -> Result<PathBuf, CliError> {
    repository.refresh_repository()?;
    ui::success(format_args!("refreshed {}", repository.base()));

    Ok(repository.directory().to_path_buf())
}

fn repository_of<'a>(
    package: &'a PackageNode,
    build_root: &Path,
) -> Result<aur::Package<'a>, aur::PackageNameParseError> {
    let base = package
        .aur
        .as_ref()
        .map(|aur| aur.base.as_str())
        .unwrap_or(&package.name);

    aur::Package::new(&package.name, base, build_root.join(base))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        dependency::{AurMeta, PackageSource},
        test_support::TempDir,
    };

    fn aur_package(name: &str, base: &str) -> PackageNode {
        PackageNode {
            name: name.to_owned(),
            version: Some("1".to_owned()),
            source: PackageSource::Aur,
            dependencies: Vec::new(),
            size: None,
            download_size: None,
            provides: Vec::new(),
            packager: None,
            aur: Some(AurMeta {
                base: base.to_owned(),
                maintainer: None,
                submitter: None,
                description: None,
                url: None,
                votes: 0,
                popularity: 0.0,
                out_of_date: None,
                last_modified: 0,
            }),
        }
    }

    #[test]
    fn split_packages_share_one_source_checkout() {
        let plan = InstallPlan {
            packages: vec![
                aur_package("demo-cli", "demo"),
                aur_package("demo-gui", "demo"),
            ],
        };

        let packages = unique_aur_packages(&plan);
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].aur.as_ref().unwrap().base, "demo");
    }

    #[test]
    fn repository_path_uses_package_base() {
        let root = Path::new("/cache/build");
        let package = aur_package("demo-cli", "demo");
        let repository = repository_of(&package, root).unwrap();

        assert_eq!(repository.directory(), Path::new("/cache/build/demo"));
    }

    #[test]
    fn build_root_is_created_with_private_permissions() {
        let directory = TempDir::new("build-root");
        let root = directory.path().join("nested/build");

        secure_build_root(&root).unwrap();

        assert!(root.is_dir());
        assert_eq!(
            fs::metadata(root).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
}

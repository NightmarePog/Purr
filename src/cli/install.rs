use std::{path::Path, path::PathBuf};

use crate::{
    aur, build,
    cli::{CliError, InstalledPackages},
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
    let build_plan = build::BuildPlan::from(fetch_sources(&plan)?);

    ui::step("Build");
    let environment = build::Environment::new(build::SandboxFiles::new()?)?;
    let result = build_plan.execute(&environment)?;
    ui::success(format_args!("built {} artifact(s)", result.artifacts()));
    ui::step("Installing packages");
    result.install()?;
    ui::step("Installing application wrappers");
    result.install_wrappers()?;
    ui::success("installed packages");
    Ok(())
}

pub fn confirm() -> Result<bool, CliError> {
    let answer = prompt()?;
    Ok(answer.trim().eq_ignore_ascii_case("y") || answer.trim().eq_ignore_ascii_case("yes"))
}

fn fetch_sources(plan: &InstallPlan) -> Result<InstalledPackages, CliError> {
    let packages = plan
        .packages
        .iter()
        .filter(|p| matches!(p.source, PackageSource::Aur))
        .map(fetch_source)
        .collect::<Result<Vec<_>, CliError>>()?;

    Ok(InstalledPackages(packages))
}

fn fetch_source(package: &PackageNode) -> Result<PathBuf, CliError> {
    let repo = repository_of(package)?;

    if repo.directory().exists() {
        ui::info(format_args!("{} already cloned", repo.name()));
    } else {
        repo.clone_repository()?;
        ui::success(format_args!("cloned {}", repo.base()));
    }

    Ok(repo.directory().to_path_buf())
}

fn repository_of(package: &PackageNode) -> Result<aur::Package<'_>, aur::PackageNameParseError> {
    let base = package
        .aur
        .as_ref()
        .map(|aur| aur.base.as_str())
        .unwrap_or(&package.name);

    aur::Package::new(
        &package.name,
        base,
        Path::new(config::BUILD_PATH).join(&package.name),
    )
}

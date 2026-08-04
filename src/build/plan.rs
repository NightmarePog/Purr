use std::{
    ffi::OsStr,
    fmt::{self, Display},
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use crate::{cli::InstalledPackages, ui};

use crate::build::{Artifact, BuildError, Database, Environment};

pub struct BuildPlan {
    paths: Vec<PathBuf>,
}

pub struct BuildResult {
    paths: Vec<PathBuf>,
}

impl BuildResult {
    pub fn artifacts(&self) -> ArtifactCount {
        ArtifactCount(self.paths.len())
    }

    pub fn install(&self) -> Result<(), BuildError> {
        let status = Command::new("sudo")
            .args(["pacman", "-U", "--noconfirm"])
            .args(&self.paths)
            .status()?;

        if status.success() {
            Ok(())
        } else {
            Err(BuildError::InstallFailed)
        }
    }

    pub fn install_wrappers(&self) -> Result<(), BuildError> {
        self.paths
            .iter()
            .try_for_each(|artifact| crate::launcher::install_for_artifact(artifact).map(|_| ()))?;

        Ok(())
    }

    fn append(&mut self, mut result: Self) {
        self.paths.append(&mut result.paths);
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ArtifactCount(usize);

impl Display for ArtifactCount {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl From<InstalledPackages> for BuildPlan {
    fn from(value: InstalledPackages) -> Self {
        Self { paths: value.0 }
    }
}

impl BuildPlan {
    pub fn execute(&self, environment: &Environment) -> Result<BuildResult, BuildError> {
        self.paths.iter().try_fold(
            BuildResult { paths: Vec::new() },
            |mut result, path| -> Result<BuildResult, BuildError> {
                result.append(build_package(environment, path)?);
                Ok(result)
            },
        )
    }
}

fn build_package(environment: &Environment, path: &Path) -> Result<BuildResult, BuildError> {
    let name = dir_name(path)?;
    ui::step(format_args!("Building {name}"));

    run_makepkg(environment, name)?;

    let artifacts = artifact_paths(path)?.try_fold(
        BuildResult { paths: Vec::new() },
        |mut result, artifact| -> Result<BuildResult, BuildError> {
            result
                .paths
                .push(register_built_artifact(environment, artifact)?);
            Ok(result)
        },
    )?;

    if artifacts.paths.is_empty() {
        Err(BuildError::NoArtifacts(name.into()))
    } else {
        Ok(artifacts)
    }
}

fn run_makepkg(environment: &Environment, name: &str) -> Result<(), BuildError> {
    let (success, output) = environment.makepkg(OsStr::new(name))?;

    if success {
        Ok(())
    } else {
        if !output.is_empty() {
            eprintln!("{output}");
        }
        Err(BuildError::Failed(name.into()))
    }
}

fn register_built_artifact(
    environment: &Environment,
    artifact: PathBuf,
) -> Result<PathBuf, BuildError> {
    let name = artifact.file_name().unwrap_or_default().to_string_lossy();
    ui::step(format_args!("Registering {name}"));
    Database::new(environment.pacman_db()).push(Artifact::new(&artifact))?;
    ui::info(format_args!("built {name}"));
    Ok(artifact)
}

fn dir_name(path: &Path) -> Result<&str, BuildError> {
    path.file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| BuildError::InvalidPath(path.to_path_buf()))
}

fn artifact_paths(dir: &Path) -> Result<impl Iterator<Item = PathBuf>, BuildError> {
    Ok(fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| is_artifact(path)))
}

fn is_artifact(path: &Path) -> bool {
    path.is_file()
        && path
            .file_name()
            .and_then(OsStr::to_str)
            .is_some_and(|name| name.contains(".pkg.tar."))
}

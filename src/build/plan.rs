use std::{
    ffi::OsStr,
    fmt::{self, Display},
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use crate::ui;

use crate::build::{Artifact, BuildError, Database, Environment};

pub struct BuildPlan {
    paths: Vec<PathBuf>,
    repo_packages: Vec<String>,
}

pub struct BuildResult {
    paths: Vec<PathBuf>,
}

impl BuildResult {
    pub fn artifacts(&self) -> ArtifactCount {
        ArtifactCount(self.paths.len())
    }

    fn install(&self) -> Result<(), BuildError> {
        let status = artifact_install_command(&self.paths).status()?;

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

impl BuildPlan {
    pub fn new(paths: Vec<PathBuf>, repo_packages: Vec<String>) -> Self {
        Self {
            paths,
            repo_packages,
        }
    }

    pub fn install_repo_packages(&self) -> Result<(), BuildError> {
        install_repo_packages(&self.repo_packages)
    }

    pub fn execute(&self, environment: &Environment) -> Result<BuildResult, BuildError> {
        self.paths.iter().try_fold(
            BuildResult { paths: Vec::new() },
            |mut result, path| -> Result<BuildResult, BuildError> {
                let built = build_package(environment, path)?;
                ui::step("Installing packages needed by subsequent builds");
                built.install()?;
                result.append(built);
                Ok(result)
            },
        )
    }
}

fn install_repo_packages(packages: &[String]) -> Result<(), BuildError> {
    if packages.is_empty() {
        return Ok(());
    }

    ui::step("Installing repository dependencies");
    let status = repo_install_command(packages).status()?;

    if status.success() {
        Ok(())
    } else {
        Err(BuildError::RepoInstallFailed)
    }
}

fn artifact_install_command(paths: &[PathBuf]) -> Command {
    let mut command = Command::new("sudo");
    command
        .args(["pacman", "-U", "--noconfirm", "--"])
        .args(paths);
    command
}

fn repo_install_command(packages: &[String]) -> Command {
    let mut command = Command::new("sudo");
    command
        .args(["pacman", "-S", "--needed", "--noconfirm", "--"])
        .args(packages);
    command
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
    let paths = fs::read_dir(dir)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(paths.into_iter().filter(|path| is_artifact(path)))
}

fn is_artifact(path: &Path) -> bool {
    path.is_file()
        && path
            .file_name()
            .and_then(OsStr::to_str)
            .is_some_and(is_artifact_name)
}

fn is_artifact_name(name: &str) -> bool {
    name.ends_with(".pkg.tar")
        || name
            .rsplit_once(".pkg.tar.")
            .is_some_and(|(_, compression)| !compression.is_empty() && !compression.contains('.'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempDir;

    #[test]
    fn recognizes_package_archives() {
        for name in [
            "demo-1.0-1-x86_64.pkg.tar",
            "demo-1.0-1-x86_64.pkg.tar.zst",
            "demo-1.0-1-x86_64.pkg.tar.xz",
        ] {
            assert!(is_artifact_name(name), "{name}");
        }
    }

    #[test]
    fn rejects_signatures_and_unrelated_files() {
        for name in [
            "demo-1.0-1-x86_64.pkg.tar.zst.sig",
            "demo.pkg.tar.zst.old",
            "demo.src.tar.zst",
            "PKGBUILD",
        ] {
            assert!(!is_artifact_name(name), "{name}");
        }
    }

    #[test]
    fn discovers_only_regular_package_archives() {
        let directory = TempDir::new("artifact-paths");
        let package = directory.path().join("demo.pkg.tar.zst");
        let signature = directory.path().join("demo.pkg.tar.zst.sig");
        fs::write(&package, b"package").unwrap();
        fs::write(&signature, b"signature").unwrap();
        fs::create_dir(directory.path().join("directory.pkg.tar.zst")).unwrap();

        let artifacts = artifact_paths(directory.path())
            .unwrap()
            .collect::<Vec<_>>();

        assert_eq!(artifacts, [package]);
        assert!(artifact_paths(&directory.path().join("missing")).is_err());
    }

    #[test]
    fn constructs_noninteractive_pacman_commands_with_option_terminators() {
        let packages = vec!["alpha".to_owned(), "beta".to_owned()];
        let repo = repo_install_command(&packages);
        assert_eq!(repo.get_program(), "sudo");
        assert_eq!(
            repo.get_args().collect::<Vec<_>>(),
            [
                "pacman",
                "-S",
                "--needed",
                "--noconfirm",
                "--",
                "alpha",
                "beta"
            ]
        );

        let paths = vec![PathBuf::from("/tmp/-package.pkg.tar.zst")];
        let artifact = artifact_install_command(&paths);
        assert_eq!(
            artifact.get_args().collect::<Vec<_>>(),
            [
                "pacman",
                "-U",
                "--noconfirm",
                "--",
                "/tmp/-package.pkg.tar.zst"
            ]
        );
    }
}

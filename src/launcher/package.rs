use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Component, Path, PathBuf},
};

use alpm::{Alpm, File, SigLevel};
use thiserror::Error;

use super::wrapper;

#[derive(Debug, Error)]
pub enum PackageError {
    #[error("invalid package name '{0}'")]
    InvalidName(String),

    #[error("package '{0}' has no executable entry point")]
    NoExecutable(String),

    #[error("package '{0}' has multiple executable entry points: {1}")]
    AmbiguousExecutable(String, String),

    #[error("failed to query the local package database")]
    Alpm(#[from] alpm::Error),
}

pub struct PackageFiles {
    pub name: String,
    pub paths: Vec<PathBuf>,
}

pub fn validate_name(package: &str) -> Result<(), PackageError> {
    if !is_single_component(package) {
        Err(PackageError::InvalidName(package.to_owned()))
    } else {
        Ok(())
    }
}

fn is_single_component(package: &str) -> bool {
    let mut components = Path::new(package).components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}

pub fn installed_entry(package: &str) -> Result<PathBuf, PackageError> {
    let alpm = alpm()?;
    let installed = alpm.localdb().pkg(package)?;

    select_executable(
        package,
        package_paths(installed.files().files())
            .into_iter()
            .filter(|path| is_executable_entry(path) && is_executable_file(path)),
    )
}

pub fn load_artifact(artifact: &Path) -> Result<PackageFiles, PackageError> {
    let alpm = alpm()?;
    let package = alpm.pkg_load(
        artifact.to_string_lossy().into_owned(),
        true,
        SigLevel::NONE,
    )?;

    Ok(PackageFiles {
        name: package.name().to_owned(),
        paths: package_paths(package.files().files()),
    })
}

fn alpm() -> Result<Alpm, PackageError> {
    Ok(Alpm::new("/", "/var/lib/pacman")?)
}

fn package_paths(files: &[File]) -> Vec<PathBuf> {
    files
        .iter()
        .filter_map(|file| std::str::from_utf8(file.name()).ok())
        .filter_map(package_file_path)
        .collect()
}

impl PackageFiles {
    pub fn executable_entries(self) -> impl Iterator<Item = PathBuf> {
        let name = self.name;
        self.paths
            .into_iter()
            .filter(move |path| wrapper::should_wrap(&name, path))
            .filter(|path| is_executable_file(path))
    }
}

fn package_file_path(name: &str) -> Option<PathBuf> {
    let name = name.strip_prefix("./").unwrap_or(name);
    (!name.is_empty() && !name.ends_with('/')).then(|| Path::new("/").join(name))
}

pub fn is_executable_entry(path: &Path) -> bool {
    ["/bin", "/sbin", "/usr/bin", "/usr/sbin"]
        .iter()
        .any(|directory| path.parent() == Some(Path::new(directory)))
        && path.file_name().is_some()
}

fn is_executable_file(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| {
        metadata.file_type().is_file() && metadata.permissions().mode() & 0o111 != 0
    })
}

fn select_executable(
    package: &str,
    candidates: impl IntoIterator<Item = PathBuf>,
) -> Result<PathBuf, PackageError> {
    match unique(candidates).as_slice() {
        [] => Err(PackageError::NoExecutable(package.to_owned())),
        [only] => Ok(only.clone()),
        many => match preferred(package, many) {
            Some(binary) => Ok(binary),
            None => Err(ambiguous(package, many)),
        },
    }
}

fn unique(candidates: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut candidates = candidates.into_iter().collect::<Vec<_>>();
    candidates.sort();
    candidates.dedup();
    candidates
}

fn preferred(package: &str, candidates: &[PathBuf]) -> Option<PathBuf> {
    let binary = PathBuf::from(format!("/usr/bin/{package}"));
    candidates.iter().find(|path| **path == binary).cloned()
}

fn ambiguous(package: &str, candidates: &[PathBuf]) -> PackageError {
    PackageError::AmbiguousExecutable(
        package.to_owned(),
        candidates
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", "),
    )
}

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};

    use super::{
        PackageError, is_executable_entry, is_executable_file, package_file_path,
        select_executable, unique, validate_name,
    };

    #[test]
    fn prefers_the_package_named_binary() {
        let executable = select_executable(
            "demo",
            vec![
                PathBuf::from("/usr/bin/demo-helper"),
                PathBuf::from("/usr/bin/demo"),
            ],
        )
        .expect("package binary should be selected");

        assert_eq!(executable, PathBuf::from("/usr/bin/demo"));
    }

    #[test]
    fn rejects_ambiguous_packages() {
        let error = select_executable(
            "demo",
            vec![
                PathBuf::from("/usr/bin/first"),
                PathBuf::from("/usr/bin/second"),
            ],
        )
        .expect_err("ambiguous package should fail");

        assert!(matches!(error, PackageError::AmbiguousExecutable(_, _)));
    }

    #[test]
    fn rejects_path_traversal_in_package_names() {
        assert!(validate_name("../demo").is_err());
        assert!(validate_name("demo/helper").is_err());
        assert!(validate_name("demo").is_ok());
    }

    #[test]
    fn limits_executable_entries_to_system_binary_directories() {
        assert!(is_executable_entry(
            PathBuf::from("/usr/bin/demo").as_path()
        ));
        assert!(!is_executable_entry(
            PathBuf::from("/usr/lib/demo/helper").as_path()
        ));
    }

    #[test]
    fn normalizes_package_archive_paths() {
        assert_eq!(
            package_file_path("./usr/bin/demo"),
            Some(PathBuf::from("/usr/bin/demo"))
        );
        assert_eq!(package_file_path("usr/bin/"), None);
        assert_eq!(package_file_path(""), None);
    }

    #[test]
    fn removes_duplicate_candidate_paths() {
        assert_eq!(
            unique([
                PathBuf::from("/usr/bin/b"),
                PathBuf::from("/usr/bin/a"),
                PathBuf::from("/usr/bin/a"),
            ]),
            [PathBuf::from("/usr/bin/a"), PathBuf::from("/usr/bin/b")]
        );
    }

    #[test]
    fn requires_a_regular_file_with_an_execute_bit() {
        let directory = crate::test_support::TempDir::new("executable-file");
        let executable = directory.path().join("executable");
        let plain = directory.path().join("plain");
        fs::write(&executable, b"data").unwrap();
        fs::write(&plain, b"data").unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        fs::set_permissions(&plain, fs::Permissions::from_mode(0o600)).unwrap();

        assert!(is_executable_file(&executable));
        assert!(!is_executable_file(&plain));
        assert!(!is_executable_file(directory.path()));
    }
}

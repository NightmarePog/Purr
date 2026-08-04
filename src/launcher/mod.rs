mod environment;
mod package;
mod wrapper;

use std::path::{Path, PathBuf};

use thiserror::Error;

pub use wrapper::{restore_all_as_root, Wrapper, WrapperError};

#[derive(Debug, Error)]
pub enum LauncherError {
    #[error(transparent)]
    Package(#[from] package::PackageError),

    #[error(transparent)]
    Environment(#[from] environment::EnvironmentError),

    #[error(transparent)]
    Wrapper(#[from] WrapperError),

    #[error("the application entry point is not an absolute standard binary path: {0}")]
    InvalidEntry(PathBuf),
}

pub fn launch(
    package: Option<&str>,
    entry: Option<&Path>,
    args: &[String],
) -> Result<(), LauncherError> {
    let package = package
        .map(|name| package::validate_name(name).map(|_| name))
        .transpose()?;
    let entry = resolve_entry(package, entry)?;
    let real_entry = resolve_real_entry(&entry)?;

    if environment::in_sandbox() {
        environment::exec_preserved(&real_entry, args)?;
    }

    crate::ui::step("Launching");
    environment::launch(&entry, &real_entry, app_name(package, &entry)?, args)?;

    Ok(())
}

fn app_name<'a>(package: Option<&'a str>, entry: &'a Path) -> Result<&'a str, LauncherError> {
    if let Some(app_name) = package {
        return Ok(app_name);
    }
    if let Some(app_name) = entry.file_stem().and_then(|name| name.to_str()) {
        return Ok(app_name);
    }

    Err(LauncherError::InvalidEntry(entry.to_path_buf()))
}

pub fn install_for_artifact(
    artifact: &Path,
) -> Result<impl Iterator<Item = PathBuf>, LauncherError> {
    Ok(package::load_artifact(artifact)?
        .executable_entries()
        .map(|entry| {
            wrapper::Wrapper::new(&entry)?.install()?;
            Ok(entry)
        })
        .collect::<Result<Vec<_>, WrapperError>>()?
        .into_iter())
}

fn resolve_entry(package: Option<&str>, entry: Option<&Path>) -> Result<PathBuf, LauncherError> {
    match (package, entry) {
        (_, Some(entry)) if package::is_executable_entry(entry) && entry.is_absolute() => {
            Ok(entry.to_path_buf())
        }
        (Some(package), None) => Ok(package::installed_entry(package)?),
        (_, Some(entry)) => Err(LauncherError::InvalidEntry(entry.to_path_buf())),
        (None, None) => Err(LauncherError::InvalidEntry(PathBuf::new())),
    }
}

fn resolve_real_entry(entry: &Path) -> Result<PathBuf, LauncherError> {
    let stored = wrapper::Wrapper::new(entry)?.stored().to_path_buf();
    if stored.exists() {
        Ok(stored)
    } else if entry.is_file() {
        Ok(entry.to_path_buf())
    } else {
        Err(LauncherError::InvalidEntry(entry.to_path_buf()))
    }
}

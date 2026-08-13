use std::path::PathBuf;

use crate::cli::CliError;

pub fn run(
    package: Option<String>,
    entry: Option<PathBuf>,
    args: Vec<String>,
) -> Result<(), CliError> {
    match crate::launcher::launch(package.as_deref(), entry.as_deref(), &args) {
        Ok(()) => Ok(()),
        Err(crate::launcher::LauncherError::Application(status)) => {
            std::process::exit(status.code().unwrap_or(1));
        }
        Err(error) => Err(error.into()),
    }
}

pub fn wrapper_install(original: PathBuf, stored: PathBuf) -> Result<(), CliError> {
    crate::launcher::Wrapper::new(&original)
        .and_then(|wrapper| wrapper.install_as_root(&stored))
        .map_err(crate::launcher::LauncherError::from)?;
    Ok(())
}

pub fn restore_all() -> Result<(), CliError> {
    crate::launcher::restore_all_as_root().map_err(crate::launcher::LauncherError::from)?;
    Ok(())
}

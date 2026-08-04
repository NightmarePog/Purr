use std::path::PathBuf;

pub fn run(
    package: Option<String>,
    entry: Option<PathBuf>,
    args: Vec<String>,
) -> Result<(), crate::cli::CliError> {
    crate::launcher::launch(package.as_deref(), entry.as_deref(), &args)?;
    Ok(())
}

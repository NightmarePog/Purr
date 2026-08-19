pub fn remove(package: String) -> Result<(), crate::cli::CliError> {
    crate::ui::step(format!("Preparing to remove {package}"));
    crate::ui::info("Remove support is not implemented yet");

    Ok(())
}

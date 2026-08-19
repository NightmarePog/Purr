mod aur;
mod build;
mod cli;
mod config;
mod dependency;
mod launcher;
mod sandbox;
#[cfg(test)]
mod test_support;
mod ui;

use std::{error::Error, process::ExitCode};

use clap::Parser;
use cli::Cli;

fn main() -> ExitCode {
    ui::configure();

    match dispatch() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            report(&error);

            ExitCode::FAILURE
        }
    }
}

fn dispatch() -> Result<(), cli::CliError> {
    let cli = Cli::parse();

    match cli.command {
        cli::Command::Install { packages } => {
            ui::command("install", &packages.join(", "));
            cli::install::install(packages.iter().map(String::as_str), cli.verbose)
        }
        cli::Command::Run {
            package,
            entry,
            args,
        } => {
            ui::command(
                "run",
                package
                    .as_deref()
                    .or_else(|| entry.as_deref().and_then(|path| path.to_str()))
                    .unwrap_or("entry point"),
            );
            cli::run::run(package, entry, args)
        }
        cli::Command::WrapperInstall { original, stored } => {
            crate::launcher::Wrapper::new(&original)
                .and_then(|wrapper| wrapper.install_as_root(&stored))
                .map_err(crate::launcher::LauncherError::from)
                .map_err(cli::CliError::from)
        }
        cli::Command::WrapperRestoreAll => crate::launcher::restore_all_as_root()
            .map_err(crate::launcher::LauncherError::from)
            .map_err(cli::CliError::from),
        cli::Command::Remove { package } => {
            ui::command("remove", &package);
            cli::remove::remove(package)
        }
    }
}

fn report(error: &dyn Error) {
    ui::error(error.to_string());

    let mut source = error.source();

    while let Some(cause) = source {
        ui::error(format!("caused by: {cause}"));

        source = cause.source();
    }
}

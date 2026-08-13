#![deny(unsafe_code)]

mod aur;
mod build;
mod cli;
mod config;
mod dependency;
mod launcher;
mod sandbox;
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
        } => cli::run::run(package, entry, args),
        cli::Command::Remove { package } => {
            ui::command("remove", &package);
            cli::remove::remove(package)
        }
        cli::Command::List {
            packages,
            managed,
            hide,
        } => {
            let target = if packages.is_empty() {
                if managed {
                    "managed packages"
                } else {
                    "all packages"
                }
                .to_owned()
            } else {
                packages.join(", ")
            };
            ui::command("list", &target);
            cli::list::list(&packages, managed, &hide)
        }
        other => cli::dispatch_hidden(other),
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

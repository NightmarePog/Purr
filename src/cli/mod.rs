use std::path::PathBuf;

use clap::{Parser, Subcommand};
use thiserror::Error;

use crate::{
    aur,
    build::{BuildError, SandboxError},
    dependency::{PacmanError, ResolveError},
    ui,
};

pub mod install;
pub mod remove;
pub mod run;

#[derive(Debug, Error)]
pub enum CliError {
    #[error(transparent)]
    Resolve(#[from] ResolveError),

    #[error(transparent)]
    Pacman(#[from] PacmanError),

    #[error(transparent)]
    PackageName(#[from] aur::PackageNameParseError),

    #[error(transparent)]
    Clone(#[from] aur::CloneError),

    #[error(transparent)]
    Sandbox(#[from] SandboxError),

    #[error(transparent)]
    Build(#[from] BuildError),

    #[error(transparent)]
    Ui(#[from] ui::UiError),

    #[error(transparent)]
    Launcher(#[from] crate::launcher::LauncherError),

    #[error("user cancelled")]
    UserCancelled,
}

#[derive(Parser)]
#[command(name = "aur-pkg-manager")]
#[command(version)]
#[command(about = "Sandboxed AUR package manager")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    #[arg(short, long, global = true)]
    pub verbose: bool,
}

#[derive(Subcommand)]
pub enum Command {
    Install {
        #[arg(value_name = "PACKAGE", required = true)]
        packages: Vec<String>,
    },
    Run {
        #[arg(value_name = "PACKAGE")]
        package: Option<String>,

        #[arg(long, value_name = "PATH", conflicts_with = "package", hide = true)]
        entry: Option<PathBuf>,

        #[arg(trailing_var_arg = true, value_name = "ARG")]
        args: Vec<String>,
    },

    #[command(name = "__wrapper-install", hide = true)]
    WrapperInstall { original: PathBuf, stored: PathBuf },

    #[command(name = "__wrapper-restore-all", hide = true)]
    WrapperRestoreAll,
    Remove {
        #[arg(value_name = "PACKAGE")]
        package: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multiple_install_targets_and_global_verbose_flag() {
        let cli = Cli::try_parse_from(["aur-pkg-manager", "--verbose", "install", "alpha", "beta"])
            .unwrap();

        assert!(cli.verbose);
        match cli.command {
            Command::Install { packages } => assert_eq!(packages, ["alpha", "beta"]),
            _ => panic!("expected install command"),
        }
    }

    #[test]
    fn rejects_run_package_combined_with_hidden_entry() {
        let result =
            Cli::try_parse_from(["aur-pkg-manager", "run", "demo", "--entry", "/usr/bin/demo"]);

        assert!(result.is_err());
    }

    #[test]
    fn preserves_hyphenated_application_arguments() {
        let cli =
            Cli::try_parse_from(["aur-pkg-manager", "run", "demo", "--", "--application-flag"])
                .unwrap();

        match cli.command {
            Command::Run { args, .. } => assert_eq!(args, ["--application-flag"]),
            _ => panic!("expected run command"),
        }
    }
}

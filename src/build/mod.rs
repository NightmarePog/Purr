mod bridge;
mod environment;
mod files;
mod plan;

use std::path::PathBuf;

use thiserror::Error;

use crate::sandbox;

pub use bridge::{Artifact, Database};
pub use environment::Environment;
pub use files::SandboxFiles;
pub use plan::BuildPlan;

#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("failed to prepare sandbox")]
    Io(#[from] std::io::Error),

    #[error("failed to locate user data directory")]
    MissingDataDir,

    #[error("failed to locate user cache directory")]
    MissingCacheDir,

    #[error(transparent)]
    Spawn(#[from] sandbox::SpawnError),
}

#[derive(Debug, Error)]
pub enum BuildError {
    #[error(transparent)]
    Sandbox(#[from] SandboxError),

    #[error(transparent)]
    Spawn(#[from] sandbox::SpawnError),

    #[error("makepkg failed for '{0}'")]
    Failed(String),

    #[error("no package artifacts found for '{0}'")]
    NoArtifacts(String),

    #[error("pacman failed to install built packages")]
    InstallFailed,

    #[error("pacman failed to install repository packages")]
    RepoInstallFailed,

    #[error("invalid build directory: {0}")]
    InvalidPath(PathBuf),

    #[error(transparent)]
    Utf8(#[from] std::string::FromUtf8Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Alpm(#[from] ::alpm::Error),

    #[error(transparent)]
    Launcher(#[from] crate::launcher::LauncherError),
}

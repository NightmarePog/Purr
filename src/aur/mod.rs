pub mod rpc;

use std::{
    fmt, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use thiserror::Error;

use crate::config;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Package<'a> {
    name: &'a str,
    base: &'a str,
    directory: PathBuf,
}

#[derive(Debug, Error)]
#[error("unsafe package name: {0}")]
pub struct PackageNameParseError(String);

impl<'a> Package<'a> {
    pub fn new(
        name: &'a str,
        base: &'a str,
        directory: impl Into<PathBuf>,
    ) -> Result<Self, PackageNameParseError> {
        if !is_package_name(name) || !is_package_name(base) {
            Err(PackageNameParseError(name.to_string()))
        } else {
            Ok(Self {
                name,
                base,
                directory: directory.into(),
            })
        }
    }

    pub fn base(&self) -> &str {
        self.base
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn refresh_repository(&self) -> Result<(), CloneError> {
        remove_checkout(self.directory())?;

        let url = format!("{}/{}.git", config::AUR_URL, self.base);

        let output = Command::new("git")
            .arg("clone")
            .arg(&url)
            .arg(&self.directory)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .map_err(CloneError::spawn)?;

        if !output.status.success() {
            return Err(CloneError::GitCloneFailed {
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }

        Ok(())
    }
}

fn is_package_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"@._+-".contains(&byte))
}

fn remove_checkout(path: &Path) -> Result<(), CloneError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(CloneError::Cleanup {
                path: path.to_path_buf(),
                source,
            });
        }
    };

    let result = if metadata.file_type().is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    };
    result.map_err(|source| CloneError::Cleanup {
        path: path.to_path_buf(),
        source,
    })
}

impl fmt::Display for Package<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.name.fmt(f)
    }
}

#[derive(Debug, Error)]
pub enum CloneError {
    #[error("git is not installed")]
    Missing,

    #[error("failed to execute git")]
    Git(#[source] std::io::Error),

    #[error("git clone failed: {stderr}")]
    GitCloneFailed { stderr: String },

    #[error("failed to remove cached checkout at {path}")]
    Cleanup {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl CloneError {
    fn spawn(error: std::io::Error) -> Self {
        match error.kind() {
            std::io::ErrorKind::NotFound => Self::Missing,
            _ => Self::Git(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_arch_package_names() {
        for name in [
            "firefox-nightly",
            "lib32_foo",
            "foo+bar",
            "foo@bar",
            "foo.rs",
        ] {
            assert!(is_package_name(name), "{name}");
        }
    }

    #[test]
    fn rejects_names_that_can_escape_the_cache() {
        for name in ["", ".", "..", "foo/bar", "foo\\bar", "foo bar"] {
            assert!(!is_package_name(name), "{name}");
        }
    }

    #[test]
    fn removes_cached_symlink_without_following_it() {
        use std::os::unix::fs::symlink;

        let directory = crate::test_support::TempDir::new("cached-symlink");
        let target = directory.path().join("target");
        let checkout = directory.path().join("checkout");
        fs::create_dir(&target).expect("target directory");
        fs::write(target.join("preserved"), b"data").expect("target file");
        symlink(&target, &checkout).expect("checkout symlink");

        remove_checkout(&checkout).expect("remove cached symlink");

        assert!(!checkout.exists());
        assert!(target.join("preserved").exists());
    }
}

use std::{
    fs,
    io::{self, Write},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
};

use thiserror::Error;

use super::package::is_executable_entry;

pub const ORIGINALS_ROOT: &str = "/var/lib/purr/originals";
pub const MANAGER_PATH: &str = "/usr/bin/purr";
const BOOTSTRAP_PACKAGES: &[&str] = &[
    "purr",
    "bash",
    "bubblewrap",
    "coreutils",
    "dash",
    "doas",
    "filesystem",
    "fish",
    "makepkg",
    "pacman",
    "sudo",
    "util-linux",
    "zsh",
];
/// rwxr-xr-x: owner read/write/execute, group and others read/execute.
const WRAPPER_PERMISSION_MODE: u32 = 0o755;

fn wrapper_permissions() -> fs::Permissions {
    fs::Permissions::from_mode(WRAPPER_PERMISSION_MODE)
}

#[derive(Debug, Error)]
pub enum WrapperError {
    #[error("invalid executable path: {0}")]
    InvalidPath(PathBuf),

    #[error("failed to start privileged wrapper operation")]
    Privilege(#[source] io::Error),

    #[error("privileged wrapper operation failed with status {0}")]
    PrivilegeFailed(std::process::ExitStatus),

    #[error("wrapper target was changed outside purr: {0}")]
    Changed(PathBuf),

    #[error("wrapper operation failed")]
    Io(#[from] io::Error),
}

pub struct Wrapper {
    entry: PathBuf,
    stored: PathBuf,
}

impl Wrapper {
    pub fn new(entry: &Path) -> Result<Self, WrapperError> {
        Ok(Self {
            entry: entry.to_path_buf(),
            stored: stored_path(entry)?,
        })
    }

    fn from_stored(stored: &Path) -> Result<Self, WrapperError> {
        stored
            .strip_prefix(ORIGINALS_ROOT)
            .map_err(|_| WrapperError::InvalidPath(stored.to_path_buf()))
            .and_then(|relative| Self::new(&Path::new("/").join(relative)))
            .and_then(|wrapper| validate_stored_path(&wrapper.entry, stored).map(|()| wrapper))
    }

    pub fn stored(&self) -> &Path {
        &self.stored
    }

    pub fn script(&self) -> String {
        format!(
            "#!/bin/sh\nexec {manager} run --entry {entry} -- \"$@\"\n",
            manager = shell_quote(MANAGER_PATH),
            entry = shell_quote(&self.entry.to_string_lossy()),
        )
    }

    pub fn is_installed(&self) -> Result<bool, WrapperError> {
        if !self.stored.exists() {
            Ok(false)
        } else if fs::read(&self.entry).is_ok_and(|current| current == self.script().as_bytes()) {
            Ok(true)
        } else {
            Err(WrapperError::Changed(self.entry.to_path_buf()))
        }
    }

    pub fn install(&self) -> Result<(), WrapperError> {
        if self.is_installed()? {
            Ok(())
        } else {
            let mut child = spawn_helper(&self.entry, &self.stored)?;
            send_script(&mut child, &self.script())?;
            ensure_success(child.wait().map_err(WrapperError::Privilege)?)
        }
    }

    pub fn install_as_root(&self, stored: &Path) -> Result<(), WrapperError> {
        validate_stored_path(&self.entry, stored)?;
        let script = read_script()?;
        validate_script(&self.entry, &script)?;

        if self.is_installed()? {
            Ok(())
        } else {
            preserve_original(&self.entry, &self.stored)?;
            self.write_script(&script)
        }
    }

    fn write_script(&self, script: &str) -> Result<(), WrapperError> {
        install_script(&self.entry, script).inspect_err(|_| {
            fs::rename(&self.stored, &self.entry).ok();
        })
    }

    pub fn restore(&self) -> Result<(), WrapperError> {
        validate_script(&self.entry, &fs::read_to_string(&self.entry)?)?;
        restore_original(&self.entry, &self.stored)
    }
}

pub fn should_wrap(package: &str, path: &Path) -> bool {
    is_executable_entry(path) && !BOOTSTRAP_PACKAGES.contains(&package)
}

pub fn restore_all_as_root() -> Result<(), WrapperError> {
    let root = Path::new(ORIGINALS_ROOT);
    if !root.exists() {
        Ok(())
    } else {
        walk_originals(root, &mut |stored| Wrapper::from_stored(stored)?.restore())
    }
}

fn walk_originals(
    directory: &Path,
    visit: &mut impl FnMut(&Path) -> Result<(), WrapperError>,
) -> Result<(), WrapperError> {
    fs::read_dir(directory)?.flatten().try_for_each(|item| {
        let path = item.path();
        if item.file_type()?.is_dir() {
            walk_originals(&path, visit)
        } else if item.file_type()?.is_file() {
            visit(&path)
        } else {
            Ok(())
        }
    })
}

fn stored_path(original: &Path) -> Result<PathBuf, WrapperError> {
    if !original.is_absolute() || !is_executable_entry(original) || original.file_name().is_none() {
        Err(WrapperError::InvalidPath(original.to_path_buf()))
    } else {
        original
            .strip_prefix("/")
            .map_err(|_| WrapperError::InvalidPath(original.to_path_buf()))
            .map(|relative| Path::new(ORIGINALS_ROOT).join(relative))
    }
}

fn spawn_helper(entry: &Path, stored: &Path) -> Result<Child, WrapperError> {
    let executable = match std::env::current_exe() {
        Ok(executable) => executable,
        Err(error) => return Err(WrapperError::Privilege(error)),
    };

    match Command::new("sudo")
        .arg(executable)
        .arg("__wrapper-install")
        .arg(entry)
        .arg(stored)
        .stdin(Stdio::piped())
        .spawn()
    {
        Ok(child) => Ok(child),
        Err(error) => Err(WrapperError::Privilege(error)),
    }
}

fn take_stdin(child: &mut Child) -> Result<ChildStdin, WrapperError> {
    match child.stdin.take() {
        Some(stdin) => Ok(stdin),
        None => Err(WrapperError::Privilege(io::Error::other(
            "missing helper stdin",
        ))),
    }
}

fn send_script(child: &mut Child, script: &str) -> Result<(), WrapperError> {
    if let Err(error) = take_stdin(child)?.write_all(script.as_bytes()) {
        Err(WrapperError::Privilege(error))
    } else {
        Ok(())
    }
}

fn ensure_success(status: std::process::ExitStatus) -> Result<(), WrapperError> {
    if status.success() {
        Ok(())
    } else {
        Err(WrapperError::PrivilegeFailed(status))
    }
}

fn validate_stored_path(entry: &Path, stored: &Path) -> Result<(), WrapperError> {
    if stored_path(entry)? == stored {
        Ok(())
    } else {
        Err(WrapperError::InvalidPath(stored.to_path_buf()))
    }
}

fn read_script() -> Result<String, WrapperError> {
    Ok(io::read_to_string(io::stdin())?)
}

fn validate_script(entry: &Path, script: &str) -> Result<(), WrapperError> {
    if script == Wrapper::new(entry)?.script() {
        Ok(())
    } else {
        Err(WrapperError::Changed(entry.to_path_buf()))
    }
}

fn preserve_original(entry: &Path, stored: &Path) -> Result<(), WrapperError> {
    validate_regular_file(entry)?;
    fs::create_dir_all(parent_of(stored)?)?;
    fs::rename(entry, stored)?;
    Ok(())
}

fn validate_regular_file(entry: &Path) -> Result<(), WrapperError> {
    if !fs::symlink_metadata(entry)?.file_type().is_file() {
        Err(WrapperError::Changed(entry.to_path_buf()))
    } else {
        Ok(())
    }
}

fn parent_of(stored: &Path) -> Result<&Path, WrapperError> {
    match stored.parent() {
        Some(parent) => Ok(parent),
        None => Err(WrapperError::InvalidPath(stored.to_path_buf())),
    }
}

fn install_script(entry: &Path, script: &str) -> Result<(), WrapperError> {
    let temporary = temporary_path(entry);
    fs::write(&temporary, script)?;
    fs::set_permissions(&temporary, wrapper_permissions())?;
    fs::rename(&temporary, entry)?;
    Ok(())
}

fn temporary_path(entry: &Path) -> PathBuf {
    PathBuf::from(format!("{}.purr-{}", entry.display(), std::process::id()))
}

fn restore_original(entry: &Path, stored: &Path) -> Result<(), WrapperError> {
    fs::remove_file(entry)?;
    fs::rename(stored, entry)?;
    Ok(())
}

fn shell_quote(path: &str) -> String {
    if path
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"_./+-".contains(&byte))
    {
        path.to_owned()
    } else {
        format!("'{}'", path.replace('\'', "'\\''"))
    }
}

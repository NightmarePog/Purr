use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::build::SandboxError;

const SANDBOX_NAME: &str = "purr";
const PACMAN_DB_NAME: &str = "pacman_db";

const COPIED_FILES: &[(&str, &str)] = &[
    ("/etc/pacman.conf", "pacman.conf"),
    ("/etc/pacman.d/mirrorlist", "mirrorlist"),
    ("/etc/makepkg.conf", "makepkg.conf"),
];

const GENERATED_FILES: &[(&str, &str)] = &[
    ("passwd", "builder:x:1000:1000:builder:/build:/bin/bash\n"),
    ("group", "builder:x:1000:\n"),
];

pub struct SandboxFiles {
    root: PathBuf,
    pacman_db: PathBuf,
}

impl SandboxFiles {
    pub fn new() -> Result<Self, SandboxError> {
        let root = dirs::data_dir()
            .map(|dir| dir.join(SANDBOX_NAME))
            .ok_or(SandboxError::MissingDataDir)?;

        fs::create_dir_all(&root)?;

        COPIED_FILES
            .iter()
            .try_for_each(|(src, dst)| copy_if_missing(src, root.join(dst)))?;

        GENERATED_FILES
            .iter()
            .try_for_each(|(name, contents)| write_if_missing(root.join(name), contents))?;

        let pacman_db = root.join(PACMAN_DB_NAME);
        copy_pacman_db(&pacman_db)?;

        Ok(Self { root, pacman_db })
    }

    pub fn path(&self, path: impl AsRef<Path>) -> PathBuf {
        self.root.join(path)
    }

    pub fn pacman_db(&self) -> &Path {
        &self.pacman_db
    }
}

fn copy_pacman_db(dest: &Path) -> Result<(), SandboxError> {
    if dest.exists() {
        return Ok(());
    }

    let src_local = Path::new("/var/lib/pacman/local");
    let src_sync = Path::new("/var/lib/pacman/sync");

    let dest_local = dest.join("local");
    fs::create_dir_all(&dest_local)?;

    if src_local.exists() {
        for entry in fs::read_dir(src_local)? {
            let entry = entry?;
            let target = dest_local.join(entry.file_name());
            let ty = entry.file_type()?;
            if ty.is_dir() {
                copy_dir_all(&entry.path(), &target)?;
            } else {
                fs::copy(entry.path(), target)?;
            }
        }
    }

    if src_sync.exists() {
        let dest_sync = dest.join("sync");
        fs::create_dir_all(&dest_sync)?;
        for entry in fs::read_dir(src_sync)? {
            let entry = entry?;
            let target = dest_sync.join(entry.file_name());
            if entry.path().is_file() {
                fs::copy(entry.path(), target)?;
            }
        }
    }

    Ok(())
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), SandboxError> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dest_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dest_path)?;
        } else {
            fs::copy(entry.path(), dest_path)?;
        }
    }
    Ok(())
}

fn copy_if_missing(from: impl AsRef<Path>, to: impl AsRef<Path>) -> Result<(), SandboxError> {
    let to = to.as_ref();

    if !to.exists() {
        fs::copy(from, to)?;
    }

    Ok(())
}

fn write_if_missing(
    path: impl AsRef<Path>,
    contents: impl AsRef<[u8]>,
) -> Result<(), SandboxError> {
    let path = path.as_ref();

    if !path.exists() {
        fs::write(path, contents)?;
    }

    Ok(())
}

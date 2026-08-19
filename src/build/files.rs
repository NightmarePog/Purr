use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::build::SandboxError;

const SANDBOX_NAME: &str = "aur-pkg-manager";
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

    #[cfg(test)]
    pub(crate) fn for_test(root: PathBuf) -> Self {
        let pacman_db = root.join(PACMAN_DB_NAME);
        Self { root, pacman_db }
    }
}

fn copy_pacman_db(dest: &Path) -> Result<(), SandboxError> {
    copy_pacman_db_from(
        Path::new("/var/lib/pacman/local"),
        Path::new("/var/lib/pacman/sync"),
        dest,
    )
}

fn copy_pacman_db_from(src_local: &Path, src_sync: &Path, dest: &Path) -> Result<(), SandboxError> {
    let staging = dest.with_extension(format!("tmp-{}", std::process::id()));
    remove_if_present(&staging)?;

    let dest_local = staging.join("local");
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
        let dest_sync = staging.join("sync");
        fs::create_dir_all(&dest_sync)?;
        for entry in fs::read_dir(src_sync)? {
            let entry = entry?;
            let target = dest_sync.join(entry.file_name());
            if entry.path().is_file() {
                fs::copy(entry.path(), target)?;
            }
        }
    }

    remove_if_present(dest)?;
    fs::rename(staging, dest)?;
    Ok(())
}

fn remove_if_present(path: &Path) -> Result<(), SandboxError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };

    if metadata.file_type().is_dir() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempDir;

    #[test]
    fn recursively_copies_directory_contents() {
        let directory = TempDir::new("copy-tree");
        let source = directory.path().join("source");
        let destination = directory.path().join("destination");
        fs::create_dir_all(source.join("nested")).unwrap();
        fs::write(source.join("root"), b"root").unwrap();
        fs::write(source.join("nested/leaf"), b"leaf").unwrap();

        copy_dir_all(&source, &destination).unwrap();

        assert_eq!(fs::read(destination.join("root")).unwrap(), b"root");
        assert_eq!(fs::read(destination.join("nested/leaf")).unwrap(), b"leaf");
    }

    #[test]
    fn copy_and_write_if_missing_preserve_existing_content() {
        let directory = TempDir::new("preserve-files");
        let source = directory.path().join("source");
        let copied = directory.path().join("copied");
        let generated = directory.path().join("generated");
        fs::write(&source, b"first").unwrap();
        copy_if_missing(&source, &copied).unwrap();
        fs::write(&source, b"second").unwrap();
        copy_if_missing(&source, &copied).unwrap();
        write_if_missing(&generated, b"first").unwrap();
        write_if_missing(&generated, b"second").unwrap();

        assert_eq!(fs::read(copied).unwrap(), b"first");
        assert_eq!(fs::read(generated).unwrap(), b"first");
    }

    #[test]
    fn removal_does_not_follow_symlinks() {
        use std::os::unix::fs::symlink;

        let directory = TempDir::new("remove-symlink");
        let target = directory.path().join("target");
        let link = directory.path().join("link");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("keep"), b"data").unwrap();
        symlink(&target, &link).unwrap();

        remove_if_present(&link).unwrap();

        assert!(!link.exists());
        assert!(target.join("keep").exists());
    }

    #[test]
    fn package_database_refresh_replaces_stale_snapshot() {
        let directory = TempDir::new("pacman-db");
        let local = directory.path().join("source/local");
        let sync = directory.path().join("source/sync");
        let destination = directory.path().join("destination");
        fs::create_dir_all(local.join("demo-1")).unwrap();
        fs::create_dir_all(&sync).unwrap();
        fs::create_dir_all(&destination).unwrap();
        fs::write(local.join("demo-1/desc"), b"description").unwrap();
        fs::write(sync.join("core.db"), b"database").unwrap();
        fs::write(destination.join("stale"), b"stale").unwrap();

        copy_pacman_db_from(&local, &sync, &destination).unwrap();

        assert_eq!(
            fs::read(destination.join("local/demo-1/desc")).unwrap(),
            b"description"
        );
        assert_eq!(
            fs::read(destination.join("sync/core.db")).unwrap(),
            b"database"
        );
        assert!(!destination.join("stale").exists());
        assert!(
            !destination
                .with_extension(format!("tmp-{}", std::process::id()))
                .exists()
        );
    }
}

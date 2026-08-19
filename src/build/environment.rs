use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

use crate::sandbox;

use crate::build::{SandboxError, SandboxFiles};

const CONTAINER_PKG_DIR: &str = "/build/pkg";
const CONTAINER_PACMAN_DB: &str = "/var/lib/pacman";

const READ_ONLY_BINDS: &[(&str, &str)] = &[
    ("/usr", "/usr"),
    ("/bin", "/bin"),
    ("/lib", "/lib"),
    ("/lib64", "/lib64"),
];

const READ_WRITE_BINDS: &[(&str, &str)] = &[("/var/cache/pacman/pkg", "/var/cache/pacman/pkg")];

const SANDBOX_FILES: &[(&str, &str)] = &[
    ("pacman.conf", "/etc/pacman.conf"),
    ("mirrorlist", "/etc/pacman.d/mirrorlist"),
    ("makepkg.conf", "/etc/makepkg.conf"),
    ("passwd", "/etc/passwd"),
    ("group", "/etc/group"),
];

const NETWORK_BINDS: &[&str] = &[
    "/etc/resolv.conf",
    "/etc/ssl",
    "/etc/ca-certificates",
    "/etc/hosts",
];

pub struct Environment {
    builder: sandbox::Builder,
    pacman_db: PathBuf,
}

impl Environment {
    pub fn new(files: SandboxFiles, build_root: &Path) -> Result<Self, SandboxError> {
        let build_path = fs::canonicalize(build_root)?;

        let mut builder = sandbox::Builder::new();

        builder
            .unshare_all()
            .share_net()
            .die_with_parent()
            .proc("/proc")
            .dev("/dev")
            .tmpfs("/tmp")
            .clearenv()
            .setenv("HOME", "/build")
            .setenv(
                "PATH",
                "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
            )
            .setenv("MAKEFLAGS", "-j4");

        READ_ONLY_BINDS.iter().for_each(|(src, dst)| {
            builder.ro_bind(src, dst);
        });

        READ_WRITE_BINDS.iter().for_each(|(src, dst)| {
            builder.bind(src, dst);
        });

        builder.bind(build_path, CONTAINER_PKG_DIR);
        builder.bind(files.pacman_db(), CONTAINER_PACMAN_DB);

        NETWORK_BINDS
            .iter()
            .filter(|path| Path::new(path).exists())
            .for_each(|path| {
                builder.ro_bind(path, path);
            });

        SANDBOX_FILES.iter().for_each(|(src, dst)| {
            builder.ro_bind(files.path(src), dst);
        });

        Ok(Self {
            builder,
            pacman_db: files.pacman_db().to_path_buf(),
        })
    }

    pub fn makepkg(&self, package_dir: &OsStr) -> Result<(bool, String), sandbox::SpawnError> {
        let name = package_dir.to_string_lossy().into_owned();
        let mut builder = self.builder.clone();

        builder
            .chdir(Path::new(CONTAINER_PKG_DIR).join(package_dir))
            .makepkg();

        builder
            .spawn_quiet()
            .and_then(|runner| runner.wait_with_progress(&name))
            .map(|(status, output)| (status.success(), output))
    }

    pub fn pacman_db(&self) -> &Path {
        &self.pacman_db
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempDir;

    #[test]
    fn configures_isolated_build_mounts_and_environment() {
        let directory = TempDir::new("build-environment");
        let build_root = directory.path().join("build");
        let files_root = directory.path().join("files");
        fs::create_dir(&build_root).unwrap();
        fs::create_dir(&files_root).unwrap();
        let environment = Environment::new(SandboxFiles::for_test(files_root), &build_root)
            .expect("build environment");
        let arguments = environment.builder.arguments();

        for required in [
            "--unshare-all",
            "--share-net",
            "--die-with-parent",
            "--clearenv",
            "HOME",
            "/build",
            "MAKEFLAGS",
            "-j4",
            CONTAINER_PKG_DIR,
            CONTAINER_PACMAN_DB,
        ] {
            assert!(
                arguments.iter().any(|argument| argument == required),
                "{required}"
            );
        }
    }
}

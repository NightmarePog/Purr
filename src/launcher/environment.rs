use std::{
    env,
    ffi::{OsStr, OsString},
    fs, io,
    os::unix::{ffi::OsStrExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
};

use thiserror::Error;

use crate::sandbox;

use super::wrapper::ORIGINALS_ROOT;

#[derive(Debug, Error)]
pub enum EnvironmentError {
    #[error("failed to prepare application data")]
    DataDirectory(#[source] std::io::Error),

    #[error("failed to start the sandbox")]
    Sandbox(#[from] sandbox::SpawnError),

    #[error("failed to execute the preserved application")]
    Exec(#[source] std::io::Error),

    #[error("application exited with status {0}")]
    Application(ExitStatus),
}

const APP_DIR: &str = "/home/app";

const PACMAN_STATE_PATHS: &[&str] = &["/var/lib/pacman", "/var/cache/pacman"];

const USER_RUNTIME_DIR: &str = "/run/user/";

const SYSTEM_BINDINGS: &[&str] = &[
    "/usr",
    "/bin",
    "/sbin",
    "/lib",
    "/lib64",
    "/etc/ld.so.cache",
    "/etc/hosts",
    "/etc/localtime",
    "/etc/nsswitch.conf",
    "/etc/resolv.conf",
    "/etc/passwd",
    "/etc/group",
    "/etc/ssl",
    "/etc/ca-certificates",
];

pub fn launch(
    entry: &Path,
    real_entry: &Path,
    app_name: &str,
    args: &[String],
) -> Result<(), EnvironmentError> {
    let root = app_data_root(app_name)?;

    let mut builder = sandbox::Builder::new();
    configure(&mut builder, &root, entry, real_entry);
    builder.command(application_command(real_entry, args));

    let status = builder.spawn().map_err(EnvironmentError::Sandbox)?.wait()?;
    result_of(status)
}

fn application_command<'a>(
    real_entry: &'a Path,
    args: &'a [String],
) -> impl Iterator<Item = OsString> + 'a {
    std::iter::once(real_entry.as_os_str().to_owned()).chain(args.iter().map(Into::into))
}

fn configure(builder: &mut sandbox::Builder, root: &Path, entry: &Path, real_entry: &Path) {
    sandbox_filesystem(builder, root);
    sandbox_environment(builder);
    bind_system_files(builder);
    bind_pacman_state(builder);
    bind_preserved_originals(builder);
    if let Some(wayland) = Wayland::new() {
        wayland.bind(builder);
    }
    builder.ro_bind(real_entry, entry);
}

fn app_data_root(app_name: &str) -> Result<PathBuf, EnvironmentError> {
    let Some(data_dir) = dirs::data_dir() else {
        return Err(EnvironmentError::DataDirectory(io::Error::other(
            "data directory unavailable",
        )));
    };
    let root = data_dir.join("purr/apps").join(app_name);
    fs::create_dir_all(&root).map_err(EnvironmentError::DataDirectory)?;
    Ok(root)
}

fn sandbox_filesystem(builder: &mut sandbox::Builder, root: &Path) {
    builder
        .unshare_all()
        .new_session()
        .die_with_parent()
        .proc("/proc")
        .dev("/dev")
        .tmpfs("/tmp")
        .dir("/run")
        .bind(root, APP_DIR)
        .chdir(APP_DIR);
}

fn sandbox_environment(builder: &mut sandbox::Builder) {
    builder
        .clearenv()
        .setenv("HOME", APP_DIR)
        .setenv("PATH", "/usr/bin:/bin")
        .setenv("XDG_CONFIG_HOME", "/home/app/.config")
        .setenv("XDG_DATA_HOME", "/home/app/.local/share")
        .setenv("XDG_CACHE_HOME", "/home/app/.cache")
        .setenv("XDG_STATE_HOME", "/home/app/.local/state")
        .setenv("PURR_IN_SANDBOX", "1");
}

fn result_of(status: ExitStatus) -> Result<(), EnvironmentError> {
    if status.success() {
        Ok(())
    } else {
        Err(EnvironmentError::Application(status))
    }
}

pub fn exec_preserved(entry: &Path, args: &[String]) -> Result<(), EnvironmentError> {
    Err(EnvironmentError::Exec(
        Command::new(entry).args(args).exec(),
    ))
}

pub fn in_sandbox() -> bool {
    env::var_os("PURR_IN_SANDBOX").is_some()
}

fn ro_bind_existing(builder: &mut sandbox::Builder, path: impl AsRef<Path>) {
    let path = path.as_ref();
    if path.exists() {
        builder.ro_bind(path, path);
    }
}

fn bind_system_files(builder: &mut sandbox::Builder) {
    SYSTEM_BINDINGS
        .iter()
        .for_each(|path| ro_bind_existing(builder, path));
}

fn bind_pacman_state(builder: &mut sandbox::Builder) {
    PACMAN_STATE_PATHS
        .iter()
        .for_each(|path| ro_bind_existing(builder, path));
}

fn bind_preserved_originals(builder: &mut sandbox::Builder) {
    ro_bind_existing(builder, ORIGINALS_ROOT);
}

struct Wayland {
    runtime: OsString,
    display: OsString,
    socket: PathBuf,
}

impl Wayland {
    fn new() -> Option<Self> {
        let runtime = env::var_os("XDG_RUNTIME_DIR")?;
        let display = env::var_os("WAYLAND_DISPLAY")?;
        let socket = PathBuf::from(&runtime).join(&display);

        let wayland = Self {
            runtime,
            display,
            socket,
        };
        wayland.is_socket().then_some(wayland)
    }

    fn bind(&self, builder: &mut sandbox::Builder) {
        builder
            .dir(&self.runtime)
            .ro_bind(&self.socket, &self.socket)
            .setenv("XDG_RUNTIME_DIR", &self.runtime)
            .setenv("WAYLAND_DISPLAY", &self.display);
    }

    fn is_socket(&self) -> bool {
        self.socket.starts_with(USER_RUNTIME_DIR)
            && is_filename(&self.display)
            && self.socket.exists()
    }
}

fn is_filename(name: &OsStr) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty() && bytes != b"." && bytes != b".." && !bytes.contains(&b'/')
}

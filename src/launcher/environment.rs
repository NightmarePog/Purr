use std::{
    env, ffi::OsString, fs, io,
    os::unix::process::CommandExt,
    path::{Component, Path, PathBuf},
    process::{Command, ExitStatus},
};

use thiserror::Error;

use crate::sandbox;

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

pub fn launch(
    entry: &Path,
    real_entry: &Path,
    app_name: &str,
    args: &[String],
) -> Result<(), EnvironmentError> {
    let root = app_data_root(app_name)?;

    let mut builder = sandbox::Builder::new();
    configure(&mut builder, &root, entry, real_entry, args);

    result_of(builder.spawn()?.wait()?)
}

fn configure(
    builder: &mut sandbox::Builder,
    root: &Path,
    entry: &Path,
    real_entry: &Path,
    args: &[String],
) {
    sandbox_filesystem(builder, root);
    sandbox_environment(builder);
    bind_system_files(builder);
    bind_preserved_originals(builder);
    add_wayland_access(builder);
    builder.ro_bind(real_entry, entry).command(argv(entry, args));
}

fn app_data_root(app_name: &str) -> Result<PathBuf, EnvironmentError> {
    let Some(data_dir) = dirs::data_dir() else {
        return Err(EnvironmentError::DataDirectory(io::Error::other(
            "data directory unavailable",
        )));
    };
    let root = data_dir.join("aur-pkg-manager/apps").join(app_name);
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
        .tmpfs("/home")
        .dir(APP_DIR)
        .bind(root, APP_DIR);
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
        .setenv("AUR_MANAGER_IN_SANDBOX", "1");
}

fn argv(entry: &Path, args: &[String]) -> Vec<OsString> {
    std::iter::once(entry.as_os_str().to_owned())
        .chain(args.iter().map(Into::into))
        .collect()
}

fn result_of(status: ExitStatus) -> Result<(), EnvironmentError> {
    if status.success() {
        Ok(())
    } else {
        Err(EnvironmentError::Application(status))
    }
}

pub fn exec_preserved(entry: &Path, args: &[String]) -> Result<(), EnvironmentError> {
    Err(EnvironmentError::Exec(Command::new(entry).args(args).exec()))
}

pub fn in_sandbox() -> bool {
    env::var_os("AUR_MANAGER_IN_SANDBOX").is_some()
}

fn bind_system_files(builder: &mut sandbox::Builder) {
    for path in ["/usr", "/bin", "/sbin", "/lib", "/lib64"] {
        if Path::new(path).exists() {
            builder.ro_bind(path, path);
        }
    }

    builder.dir("/etc");
    for path in [
        "/etc/ld.so.cache",
        "/etc/hosts",
        "/etc/localtime",
        "/etc/nsswitch.conf",
        "/etc/resolv.conf",
        "/etc/passwd",
        "/etc/group",
        "/etc/ssl",
        "/etc/ca-certificates",
    ] {
        if Path::new(path).exists() {
            builder.ro_bind(path, path);
        }
    }
}

fn bind_preserved_originals(builder: &mut sandbox::Builder) {
    let originals = Path::new("/var/lib/aur-manager/originals");
    if originals.exists() {
        builder
            .dir("/var")
            .dir("/var/lib")
            .dir("/var/lib/aur-manager")
            .ro_bind(originals, originals);
    }
}

fn add_wayland_access(builder: &mut sandbox::Builder) {
    let Some((runtime, display, socket)) = wayland_socket() else {
        return;
    };

    builder
        .dir("/run")
        .dir("/run/user")
        .dir(&runtime)
        .ro_bind(&socket, &socket)
        .setenv("XDG_RUNTIME_DIR", runtime)
        .setenv("WAYLAND_DISPLAY", display);
}

fn wayland_socket() -> Option<(PathBuf, PathBuf, PathBuf)> {
    let runtime = env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from)?;
    let display = env::var_os("WAYLAND_DISPLAY").map(PathBuf::from)?;
    if !runtime.starts_with("/run/user/") || !is_socket_name(&display) {
        return None;
    }

    let socket = runtime.join(&display);
    socket.exists().then_some((runtime, display, socket))
}

fn is_socket_name(path: &Path) -> bool {
    let mut components = path.components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}

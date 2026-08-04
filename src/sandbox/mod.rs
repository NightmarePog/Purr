use std::ffi::OsString;

use thiserror::Error;

mod runner;

#[derive(Debug, Error)]
pub enum SpawnError {
    #[error("failed to spawn bubblewrap")]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Ui(#[from] crate::ui::UiError),
}

#[derive(Default, Clone)]
pub struct Builder {
    args: Vec<OsString>,
}

impl Builder {
    pub fn new() -> Self {
        Self::default()
    }

    fn arg(&mut self, arg: impl Into<OsString>) -> &mut Self {
        self.args.push(arg.into());
        self
    }

    fn args<I, S>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    pub fn unshare_all(&mut self) -> &mut Self {
        self.arg("--unshare-all")
    }

    pub fn share_net(&mut self) -> &mut Self {
        self.arg("--share-net")
    }

    pub fn unshare_net(&mut self) -> &mut Self {
        self.arg("--unshare-net")
    }

    pub fn die_with_parent(&mut self) -> &mut Self {
        self.arg("--die-with-parent")
    }

    pub fn new_session(&mut self) -> &mut Self {
        self.arg("--new-session")
    }

    pub fn ro_bind(&mut self, src: impl Into<OsString>, dst: impl Into<OsString>) -> &mut Self {
        self.args(["--ro-bind"]).arg(src).arg(dst)
    }

    pub fn bind(&mut self, src: impl Into<OsString>, dst: impl Into<OsString>) -> &mut Self {
        self.args(["--bind"]).arg(src).arg(dst)
    }

    pub fn dir(&mut self, path: impl Into<OsString>) -> &mut Self {
        self.arg("--dir").arg(path)
    }

    pub fn tmpfs(&mut self, path: impl Into<OsString>) -> &mut Self {
        self.arg("--tmpfs").arg(path)
    }

    pub fn proc(&mut self, path: impl Into<OsString>) -> &mut Self {
        self.arg("--proc").arg(path)
    }

    pub fn dev(&mut self, path: impl Into<OsString>) -> &mut Self {
        self.arg("--dev").arg(path)
    }

    pub fn setenv(&mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> &mut Self {
        self.arg("--setenv").arg(key).arg(value)
    }

    pub fn hostname(&mut self, name: impl Into<OsString>) -> &mut Self {
        self.arg("--hostname").arg(name)
    }

    pub fn chdir(&mut self, path: impl Into<OsString>) -> &mut Self {
        self.arg("--chdir").arg(path)
    }

    pub fn clearenv(&mut self) -> &mut Self {
        self.arg("--clearenv")
    }

    pub fn command<I, S>(&mut self, command: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.arg("--");
        self.args(command)
    }

    pub fn bash(&mut self) -> &mut Self {
        self.command(["/usr/bin/bash"])
    }

    pub fn makepkg(&mut self) -> &mut Self {
        self.command(["/usr/bin/makepkg", "-f", "--noconfirm", "--skippgpcheck"])
    }

    pub fn spawn(self) -> Result<runner::Runner, SpawnError> {
        runner::Runner::spawn(self.args)
    }

    pub fn spawn_quiet(self) -> Result<runner::Runner, SpawnError> {
        runner::Runner::spawn_quiet(self.args)
    }
}

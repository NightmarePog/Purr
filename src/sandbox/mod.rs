use std::ffi::OsString;

use thiserror::Error;

mod runner;

#[derive(Debug, Error)]
pub enum SpawnError {
    #[error("failed to spawn bubblewrap")]
    Io(#[from] std::io::Error),

    #[error("bubblewrap {0} was not captured")]
    MissingPipe(&'static str),

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
        self.command(["/usr/bin/makepkg", "-f", "--noconfirm"])
    }

    pub fn spawn(self) -> Result<runner::Runner, SpawnError> {
        runner::Runner::spawn(self.args)
    }

    pub fn spawn_quiet(self) -> Result<runner::Runner, SpawnError> {
        runner::Runner::spawn_quiet(self.args)
    }

    #[cfg(test)]
    pub(crate) fn arguments(&self) -> &[OsString] {
        &self.args
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn makepkg_keeps_source_signature_verification_enabled() {
        let mut builder = Builder::new();
        builder.makepkg();

        assert!(
            builder
                .args
                .iter()
                .any(|argument| argument == "--noconfirm")
        );
        assert!(
            !builder
                .args
                .iter()
                .any(|argument| argument == "--skippgpcheck")
        );
    }

    #[test]
    fn command_terminates_bubblewrap_options() {
        let mut builder = Builder::new();
        builder.command(["/usr/bin/demo", "--flag"]);

        assert_eq!(
            builder.args,
            ["--", "/usr/bin/demo", "--flag"]
                .into_iter()
                .map(OsString::from)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn bind_and_environment_arguments_preserve_order() {
        let mut builder = Builder::new();
        builder
            .ro_bind("/host", "/sandbox")
            .setenv("KEY", "value")
            .chdir("/sandbox");

        assert_eq!(
            builder.args,
            [
                "--ro-bind",
                "/host",
                "/sandbox",
                "--setenv",
                "KEY",
                "value",
                "--chdir",
                "/sandbox",
            ]
            .into_iter()
            .map(OsString::from)
            .collect::<Vec<_>>()
        );
    }
}

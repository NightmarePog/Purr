use std::{io::IsTerminal, iter::once};

use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use owo_colors::OwoColorize;

use crate::ui::{INDENT, UiError, step};

pub struct Loading(ProgressBar);

pub fn loading(msg: &str) -> Result<Loading, UiError> {
    Ok(Loading(if is_interactive() {
        ProgressBar::with_draw_target(None, ProgressDrawTarget::stdout())
            .with_style(spinner_style()?)
            .with_message(msg.to_owned())
    } else {
        step(msg);
        ProgressBar::hidden()
    }))
}

impl Loading {
    pub fn set_message(&self, message: String) {
        self.0.set_message(message);
    }
}

impl Drop for Loading {
    fn drop(&mut self) {
        self.0.finish_and_clear();
    }
}

pub struct Progress {
    progress: ProgressBar,
    interactive: bool,
}

impl Progress {
    pub fn new() -> Result<Self, UiError> {
        Ok(Self {
            progress: if is_interactive() {
                ProgressBar::new(100).with_style(progress_style()?)
            } else {
                ProgressBar::hidden()
            },
            interactive: is_interactive(),
        })
    }

    pub fn update(
        &mut self,
        label: &str,
        status: &str,
        activity: impl IntoIterator<Item = impl AsRef<str>>,
    ) {
        let Self {
            progress,
            interactive,
        } = self;

        if *interactive {
            progress.set_prefix(label.bold().to_string());
            progress.set_position(u64::from(stage_completion(status)));
            progress.set_message(
                once(phase(status).dimmed().to_string())
                    .chain(activity.into_iter().map(|line| {
                        format!(
                            "{INDENT}{INDENT}{} {}",
                            "│".dimmed(),
                            line.as_ref().dimmed()
                        )
                    }))
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
        } else {
            println!("{INDENT}{label} {}", phase(status));
        }
    }

    pub fn finish(&mut self) {
        self.progress.finish();
    }
}

fn phase(status: &str) -> &str {
    if status.is_empty() {
        "starting"
    } else {
        status
    }
}

pub fn is_interactive() -> bool {
    std::io::stdout().is_terminal()
}

fn spinner_style() -> Result<ProgressStyle, UiError> {
    Ok(ProgressStyle::default_spinner().template("{spinner:.yellow} {msg}")?)
}

fn progress_style() -> Result<ProgressStyle, UiError> {
    Ok(ProgressStyle::default_bar()
        .template("{spinner:.yellow} {prefix:.bold} [{bar:10.cyan/blue}] {msg}")?)
}

fn stage_completion(status: &str) -> u8 {
    match status {
        "" => 0,
        "validating" => 20,
        "extracting" => 30,
        "resolving version" => 40,
        "preparing" => 50,
        "building" => 65,
        "testing" => 75,
        "packaging" => 85,
        "tidying" => 90,
        "compressing" => 95,
        "generating metadata" => 97,
        "done" => 100,
        _ => 10,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_build_stages_to_monotonic_progress() {
        let stages = [
            "",
            "validating",
            "extracting",
            "resolving version",
            "preparing",
            "building",
            "testing",
            "packaging",
            "tidying",
            "compressing",
            "generating metadata",
            "done",
        ];
        let completion = stages.map(stage_completion);

        assert!(completion.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(stage_completion("unknown activity"), 10);
    }

    #[test]
    fn empty_status_has_a_user_facing_phase() {
        assert_eq!(phase(""), "starting");
        assert_eq!(phase("building"), "building");
    }

    #[test]
    fn progress_templates_are_valid() {
        assert!(spinner_style().is_ok());
        assert!(progress_style().is_ok());
    }
}

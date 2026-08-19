mod input;
mod output;
mod plan;
mod progress;
mod size;

use std::fmt::{self, Display, Write as _};
use thiserror::Error;

pub use input::prompt;
pub use output::{
    command, configure, error, header, info, prompt_marker, question, step, success, warn,
};
pub use plan::{aur_details, install_plan, relative_time};
pub use progress::{Loading, Progress, is_interactive, loading};

#[derive(Clone, Copy)]
struct Indent(usize);

impl Indent {
    const fn new(size: usize) -> Self {
        Self(size)
    }

    const fn size(self) -> usize {
        self.0
    }
}

impl Display for Indent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        (0..self.0).try_for_each(|_| formatter.write_char(' '))
    }
}

#[derive(Clone, Copy)]
struct Separator(usize);

impl Display for Separator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        (0..self.0).try_for_each(|_| formatter.write_char('─'))
    }
}

const INDENT: Indent = Indent::new(4);

#[derive(Debug, Error)]
pub enum UiError {
    #[error(transparent)]
    Template(#[from] indicatif::style::TemplateError),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("stdin closed")]
    StdinClosed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_fixed_width_indent_and_separator() {
        assert_eq!(Indent::new(4).to_string(), "    ");
        assert_eq!(Separator(3).to_string(), "───");
        assert_eq!(INDENT.size(), 4);
    }
}

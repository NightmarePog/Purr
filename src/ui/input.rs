use std::io::{self, BufRead, Write};

use crate::ui::UiError;

pub fn prompt() -> Result<String, UiError> {
    io::stdout().flush()?;

    read_line(io::stdin().lock())
}

fn read_line(input: impl BufRead) -> Result<String, UiError> {
    if let Some(input) = input.lines().next().transpose()? {
        Ok(input)
    } else {
        Err(UiError::StdinClosed)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn reads_one_answer_without_line_ending() {
        assert_eq!(read_line(Cursor::new("yes\r\nignored\n")).unwrap(), "yes");
    }

    #[test]
    fn reports_closed_input() {
        assert!(matches!(
            read_line(Cursor::new(Vec::<u8>::new())),
            Err(UiError::StdinClosed)
        ));
    }
}

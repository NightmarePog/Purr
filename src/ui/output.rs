use std::io::{self, IsTerminal};

use console::Term;
use owo_colors::OwoColorize;

use crate::ui::{INDENT, Separator};

pub fn configure() {
    if !io::stdout().is_terminal() || !io::stderr().is_terminal() {
        owo_colors::set_override(false);
    }
}

pub fn command(name: &str, target: &str) {
    println!();
    println!("{} {}", "aur".bold().cyan(), format!("/ {name}").bold());
    println!("{} {}", format!("{INDENT}target").dimmed(), target);
    println!("{}", format!("{INDENT}{}", rule()).dimmed());
}

pub fn info(msg: impl std::fmt::Display) {
    println!("{} {}", format!("{INDENT}·").blue(), msg);
}

pub fn success(msg: impl std::fmt::Display) {
    println!("{} {}", format!("{INDENT}✓").green(), msg);
}

pub fn warn(msg: impl std::fmt::Display) {
    println!("{} {}", format!("{INDENT}!").yellow(), msg);
}

pub fn error(msg: impl std::fmt::Display) {
    eprintln!("{} {}", format!("{INDENT}×").red(), msg);
}

pub fn step(msg: impl std::fmt::Display) {
    println!("{} {}", format!("{INDENT}›").cyan(), msg);
}

pub fn header(msg: &str) {
    println!("\n{} {}", "◆".cyan(), msg.bold());
}

pub fn question(msg: &str) {
    println!("{} {}", format!("{INDENT}?").yellow(), msg);
}

pub fn prompt_marker() {
    print!("{INDENT}{} ", "›".cyan());
}

fn rule() -> Separator {
    rule_for_width(Term::stdout().size().1 as usize)
}

fn rule_for_width(width: usize) -> Separator {
    Separator(width.saturating_sub(INDENT.size()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_rule_accounts_for_indent_and_tiny_terminals() {
        assert_eq!(rule_for_width(10).to_string(), "──────");
        assert_eq!(rule_for_width(2).to_string(), "");
    }
}

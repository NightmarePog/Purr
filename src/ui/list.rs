use comfy_table::{Cell, Color, ContentArrangement, Table, presets::UTF8_FULL};
use owo_colors::OwoColorize;

use crate::ui::INDENT;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Sandboxed,
    Exposed,
    Unmanaged,
}

#[derive(Clone, Debug)]
pub struct Row {
    pub name: String,
    pub version: String,
    pub explicit: bool,
    pub status: Status,
    pub wrapped: usize,
}

#[derive(Debug)]
pub struct GroupRow {
    pub key: Option<char>,
    pub prefix: String,
    pub count: usize,
    pub contained: usize,
}

pub enum Entry<'a> {
    Package(&'a Row),
    Group(&'a GroupRow),
}

pub struct Renderer {
    table: Table,
}

impl Renderer {
    pub fn new() -> Self {
        let mut table = Table::new();

        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_header([
                Cell::new("Package").fg(Color::Cyan),
                Cell::new("Version").fg(Color::Cyan),
                Cell::new("Reason").fg(Color::Cyan),
                Cell::new("Contained").fg(Color::Cyan),
            ]);

        Self { table }
    }

    pub fn add_row(&mut self, entry: &Entry) -> &mut Self {
        self.table.add_row(match entry {
            Entry::Package(row) => package_row(row),
            Entry::Group(group) => group_row(group),
        });
        self
    }

    pub fn render(&self) -> String {
        self.table.to_string()
    }
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

fn package_row(row: &Row) -> [Cell; 4] {
    [
        Cell::new(&row.name),
        Cell::new(&row.version),
        reason_cell(row.explicit),
        status_cell(row),
    ]
}

fn group_row(group: &GroupRow) -> [Cell; 4] {
    let label = match group.key {
        Some(key) => format!("> ({key}) {}", group.prefix.trim_end_matches('-')),
        None => format!("> {}", group.prefix.trim_end_matches('-')),
    };
    [
        Cell::new(label).fg(Color::Cyan),
        Cell::new(group.count.to_string()).fg(Color::DarkGrey),
        Cell::new("group").fg(Color::DarkGrey),
        group_contained_cell(group.contained),
    ]
}

fn group_contained_cell(contained: usize) -> Cell {
    if contained == 0 {
        Cell::new("-").fg(Color::DarkGrey)
    } else {
        Cell::new(format!("[x] {contained}")).fg(Color::Green)
    }
}

fn reason_cell(explicit: bool) -> Cell {
    if explicit {
        Cell::new("explicit").fg(Color::Green)
    } else {
        Cell::new("depend").fg(Color::DarkGrey)
    }
}

fn status_cell(row: &Row) -> Cell {
    match row.status {
        Status::Sandboxed => Cell::new(format!("[x] {}", row.wrapped)).fg(Color::Green),
        Status::Exposed => Cell::new(format!("! {}", row.wrapped)).fg(Color::Yellow),
        Status::Unmanaged => Cell::new("-").fg(Color::DarkGrey),
    }
}

pub fn summary(shown: usize, contained: usize, hidden: usize) -> String {
    format!(
        "{INDENT}{} shown  {} contained  {} hidden",
        shown.bold(),
        contained.bold(),
        hidden.bold(),
    )
}

pub fn hidden_line(hidden: usize, patterns: &[String]) -> String {
    if hidden == 0 {
        String::new()
    } else {
        format!(
            "{INDENT}{} {} hidden: {}{}",
            "+".yellow(),
            hidden,
            patterns.join(", ").yellow(),
            " (c clears)".dimmed()
        )
    }
}

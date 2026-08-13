use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
};

use alpm::{Alpm, File, Package, PackageReason};
use console::{Key, Term};
use owo_colors::OwoColorize;

use crate::{
    cli::CliError,
    launcher::{Wrapper, is_executable_entry},
    ui::{self, INDENT, Status},
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Reason {
    All,
    Explicit,
    Depend,
}

impl Reason {
    fn label(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Explicit => "explicit",
            Self::Depend => "depend",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::All => Self::Explicit,
            Self::Explicit => Self::Depend,
            Self::Depend => Self::All,
        }
    }
}

struct Listed {
    row: ui::Row,
    wrapped: Vec<PathBuf>,
    changed: Vec<PathBuf>,
    open: Vec<PathBuf>,
}

impl Listed {
    fn is_managed(&self) -> bool {
        !self.wrapped.is_empty() || !self.changed.is_empty()
    }
}

struct View {
    base: Vec<Listed>,
    all: bool,
    reason: Reason,
    hidden: BTreeSet<String>,
    expanded: BTreeSet<String>,
    interactive: bool,
}

pub fn dispatch(packages: Vec<String>, managed: bool, hide: Vec<String>) -> Result<(), CliError> {
    let target = if packages.is_empty() {
        if managed {
            "managed packages"
        } else {
            "all packages"
        }
        .to_owned()
    } else {
        packages.join(", ")
    };
    ui::command("list", &target);
    list(&packages, managed, &hide)
}

pub fn list(packages: &[String], managed: bool, hide: &[String]) -> Result<(), CliError> {
    let base: Vec<Listed> = collect()?.collect();

    if !packages.is_empty() {
        return list_specific(&base, packages);
    }

    let attended = io::stdout().is_terminal() && io::stdin().is_terminal();
    let view = View {
        base,
        all: !managed,
        reason: Reason::All,
        hidden: hide.iter().cloned().collect(),
        expanded: BTreeSet::new(),
        interactive: attended,
    };

    if attended {
        interactive(view)
    } else {
        println!("{}", render(&view));
        Ok(())
    }
}

fn collect() -> Result<impl Iterator<Item = Listed>, CliError> {
    let alpm = Alpm::new("/", "/var/lib/pacman")?;
    let mut listed = alpm
        .localdb()
        .pkgs()
        .iter()
        .map(listed_package)
        .collect::<Vec<_>>();

    listed.sort_by(|a, b| a.row.name.cmp(&b.row.name));
    Ok(listed.into_iter())
}

fn listed_package(package: &Package) -> Listed {
    let name = package.name().to_owned();
    let mut wrapped = Vec::new();
    let mut changed = Vec::new();
    let mut open = Vec::new();

    for path in executable_files(package) {
        match installed_state(&path) {
            State::Sandboxed => wrapped.push(path),
            State::Exposed => changed.push(path),
            State::Unmanaged => open.push(path),
        }
    }

    wrapped.sort();
    changed.sort();
    open.sort();

    Listed {
        row: ui::Row {
            version: package.version().to_string(),
            explicit: package.reason() == PackageReason::Explicit,
            status: status_of(&wrapped, &changed),
            wrapped: wrapped.len(),
            name,
        },
        wrapped,
        changed,
        open,
    }
}

enum State {
    Sandboxed,
    Exposed,
    Unmanaged,
}

fn installed_state(path: &Path) -> State {
    match Wrapper::new(path) {
        Ok(wrapper) => match wrapper.is_installed() {
            Ok(true) => State::Sandboxed,
            Err(_) => State::Exposed,
            Ok(false) => State::Unmanaged,
        },
        Err(_) => State::Unmanaged,
    }
}

fn status_of(wrapped: &[PathBuf], changed: &[PathBuf]) -> Status {
    if !wrapped.is_empty() {
        Status::Sandboxed
    } else if !changed.is_empty() {
        Status::Exposed
    } else {
        Status::Unmanaged
    }
}

fn executable_files(package: &Package) -> impl Iterator<Item = PathBuf> + '_ {
    package
        .files()
        .files()
        .iter()
        .filter_map(absolute_path)
        .filter(|path| is_executable_entry(path) && is_regular_file(path))
}

fn absolute_path(file: &File) -> Option<PathBuf> {
    let name = std::str::from_utf8(file.name()).ok()?;
    let name = name.strip_prefix("./").unwrap_or(name);
    (!name.is_empty() && !name.ends_with('/')).then(|| Path::new("/").join(name))
}

fn is_regular_file(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_file())
}

fn list_specific(base: &[Listed], names: &[String]) -> Result<(), CliError> {
    let focus = names
        .iter()
        .map(|name| {
            base.iter()
                .find(|listed| &listed.row.name == name)
                .ok_or_else(|| CliError::NotInstalled(name.clone()))
        })
        .collect::<Result<Vec<&Listed>, _>>()?;

    let rows = focus.iter().map(|listed| &listed.row).collect::<Vec<_>>();
    let mut renderer = ui::Renderer::new();
    for row in rows {
        renderer.add_row(&ui::Entry::Package(row));
    }
    println!("{}", renderer.render());

    for listed in &focus {
        ui::info(format_args!("{}", listed.row.name.bold()));
        detail("run sandboxed".green(), &listed.wrapped);
        detail("wrapper replaced".yellow(), &listed.changed);
        detail("not wrapped".dimmed(), &listed.open);
    }

    Ok(())
}

fn detail(label: impl std::fmt::Display, paths: &[PathBuf]) {
    if paths.is_empty() {
        return;
    }
    for path in paths {
        println!("{INDENT}{INDENT}· {:<18} {}", label, path.display());
    }
}

fn interactive(mut view: View) -> Result<(), CliError> {
    let term = Term::stdout();
    let mut owned = 0usize;

    loop {
        if owned > 0 {
            term.clear_last_lines(owned).map_err(ui::UiError::from)?;
        }
        let out = render(&view);
        println!("{out}");
        owned = out.lines().count();

        match term.read_key_raw().map_err(ui::UiError::from)? {
            Key::Char(character) => match character {
                'q' => break,
                '1'..='9' => toggle_group(&mut view, character),
                'e' => view.reason = view.reason.next(),
                'a' => view.all = !view.all,
                'c' => view.hidden.clear(),
                'h' => {
                    println!(
                        "{} Enter a name glob to hide (e.g. 'python-*'):",
                        format!("{INDENT}?").yellow()
                    );
                    ui::prompt_marker();
                    io::stdout().flush().map_err(ui::UiError::from)?;
                    let glob = ui::prompt()?.trim().to_owned();
                    if !glob.is_empty() {
                        view.hidden.insert(glob);
                    }
                    owned += 2;
                }
                _ => {}
            },
            Key::Enter | Key::Escape | Key::CtrlC => break,
            _ => {}
        }
    }

    Ok(())
}

fn toggle_group(view: &mut View, key: char) {
    let groups = fold_groups(view);
    let index = key as usize - '1' as usize;
    if let Some(group) = groups.get(index)
        && !view.expanded.remove(&group.prefix)
    {
        view.expanded.insert(group.prefix.clone());
    }
}

#[derive(Debug)]
struct FoldGroup {
    key: char,
    prefix: String,
    count: usize,
    contained: usize,
}

const MAX_GROUPS: usize = 9;
const FOLD_THRESHOLD: usize = 3;

fn fold_prefix(name: &str) -> Option<&str> {
    name.split_once('-').map(|(prefix, _)| prefix)
}

fn fold_groups(view: &View) -> Vec<FoldGroup> {
    let mut counts: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for listed in &view.base {
        if !passes(view, listed) {
            continue;
        }
        let Some(prefix) = fold_prefix(&listed.row.name) else {
            continue;
        };
        let entry = counts.entry(prefix).or_insert((0, 0));
        entry.0 += 1;
        if listed.row.status == Status::Sandboxed {
            entry.1 += 1;
        }
    }

    let mut groups = counts
        .into_iter()
        .filter(|(_, (count, _))| *count >= FOLD_THRESHOLD)
        .map(|(prefix, (count, contained))| FoldGroup {
            key: '\0',
            prefix: prefix.to_owned(),
            count,
            contained,
        })
        .collect::<Vec<_>>();

    groups.sort_by_key(|group| std::cmp::Reverse(group.count));
    groups.truncate(MAX_GROUPS);
    for (index, group) in groups.iter_mut().enumerate() {
        group.key = (b'1' + index as u8) as char;
    }
    groups
}

fn matches_reason(view: &View, listed: &Listed) -> bool {
    view.reason == Reason::All || (view.reason == Reason::Explicit) == listed.row.explicit
}

fn is_hidden(view: &View, listed: &Listed) -> bool {
    view.hidden
        .iter()
        .any(|glob| glob_matches(glob, &listed.row.name))
}

fn passes(view: &View, listed: &Listed) -> bool {
    (view.all || listed.is_managed())
        && matches_reason(view, listed)
        && !is_hidden(view, listed)
}

fn empty_hint(view: &View) -> String {
    let installed = view.base.len();
    if installed == 0 {
        return "no packages are installed".dimmed().to_string();
    }
    let managed = view.base.iter().filter(|listed| listed.is_managed()).count();
    if !view.all && managed == 0 {
        return if view.interactive {
            format!(
                "no purr-managed packages yet — press {} to show all {} installed",
                "a".cyan(),
                installed
            )
            .dimmed()
            .to_string()
        } else {
            format!(
                "no purr-managed packages yet — pass {} to list all {} installed",
                "--managed".cyan(),
                installed
            )
            .dimmed()
            .to_string()
        };
    }
    "no packages match the active filters".dimmed().to_string()
}

fn render(view: &View) -> String {
    let mut renderer = ui::Renderer::new();
    let mut contained = 0usize;
    let mut hidden = 0usize;

    let groups = fold_groups(view);
    let group_rows = groups
        .iter()
        .map(|group| ui::GroupRow {
            key: Some(group.key),
            prefix: group.prefix.clone(),
            count: group.count,
            contained: group.contained,
        })
        .collect::<Vec<_>>();
    let mut emitted_groups: BTreeSet<&str> = BTreeSet::new();

    let mut shown = 0usize;
    for listed in &view.base {
        if !view.all && !listed.is_managed() {
            continue;
        }
        if !matches_reason(view, listed) || is_hidden(view, listed) {
            hidden += 1;
            continue;
        }

        if let Some(prefix) = fold_prefix(&listed.row.name) {
            let group_index = groups
                .iter()
                .position(|group| group.prefix == prefix)
                .filter(|_| !view.expanded.contains(prefix))
                .filter(|_| emitted_groups.insert(prefix));
            if let Some(index) = group_index {
                renderer.add_row(&ui::Entry::Group(&group_rows[index]));
                shown += 1;
                contained += group_rows[index].contained;
                continue;
            }
        }

        if listed.row.status == Status::Sandboxed {
            contained += 1;
        }
        renderer.add_row(&ui::Entry::Package(&listed.row));
        shown += 1;
    }

    let mut out = String::new();
    out.push_str(&ui::summary(shown, contained, hidden));
    out.push('\n');
    out.push('\n');
    if shown == 0 {
        out.push_str(&format!("{INDENT}{}\n", empty_hint(view)));
    } else {
        out.push_str(&renderer.render());
        out.push('\n');
    }
    out.push_str(&ui::hidden_line(
        hidden,
        &view.hidden.iter().cloned().collect::<Vec<_>>(),
    ));
    if view.interactive {
        out.push_str(&footer(view));
    }
    out
}

fn footer(view: &View) -> String {
    let groups = fold_groups(view)
        .iter()
        .map(|group| fold_token(group, view.expanded.contains(&group.prefix)))
        .collect::<Vec<_>>()
        .join("  ");

    format!(
        "\n{INDENT}› {groups}  (e) reason:{}  (a) {}  (h)ide  (c)lear  (q)uit",
        view.reason.label().bold(),
        if view.all { "all" } else { "managed" }
    )
}

fn fold_token(group: &FoldGroup, expanded: bool) -> String {
    let label = format!("({}){}", group.key, group.prefix.trim_end_matches('-'));
    if expanded {
        label.green().to_string()
    } else {
        label
    }
}

pub fn glob_matches(pattern: &str, text: &str) -> bool {
    let pattern = pattern.chars().collect::<Vec<_>>();
    let text = text.chars().collect::<Vec<_>>();

    let mut previous = vec![false; text.len() + 1];
    previous[0] = true;

    for &pattern_char in &pattern {
        let mut current = vec![false; text.len() + 1];
        match pattern_char {
            '*' => {
                let mut matched = false;
                for (index, cell) in current.iter_mut().enumerate() {
                    matched |= previous[index];
                    *cell = matched;
                }
            }
            '?' => {
                current[1..].copy_from_slice(&previous[..text.len()]);
            }
            literal => {
                for index in 1..=text.len() {
                    current[index] = previous[index - 1] && text[index - 1] == literal;
                }
            }
        }
        previous = current;
    }

    previous[text.len()]
}

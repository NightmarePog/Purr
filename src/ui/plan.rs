use std::{
    fmt::Display,
    time::{SystemTime, UNIX_EPOCH},
};

use comfy_table::{Cell, Color, ContentArrangement, Row, Table, presets::UTF8_FULL};
use owo_colors::OwoColorize;

use crate::dependency::{AurMeta, InstallPlan, PackageNode, PackageSource};

use crate::ui::{INDENT, size::format_size};

pub fn install_plan(plan: &InstallPlan) {
    print_package_summary(plan);
    println!("{}", package_table(plan));
}

fn print_package_summary(plan: &InstallPlan) {
    let (aur, repo, installed) =
        plan.packages
            .iter()
            .fold((0, 0, 0), |(aur, repo, installed), package| {
                match package.source {
                    PackageSource::Aur => (aur + 1, repo, installed),
                    PackageSource::Repo => (aur, repo + 1, installed),
                    PackageSource::Installed => (aur, repo, installed + 1),
                }
            });

    println!(
        "{INDENT}{} package(s)  {} {}  {} {}  {} {}",
        plan.packages.len().bold(),
        "aur".cyan(),
        aur,
        "repo".green(),
        repo,
        "installed".dimmed(),
        installed,
    );
}

fn package_table(plan: &InstallPlan) -> Table {
    let mut table = Table::new();

    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header([
            Cell::new("Package").fg(Color::Cyan),
            Cell::new("Version").fg(Color::Cyan),
            Cell::new("Source").fg(Color::Cyan),
            Cell::new("Maintainer / Packager").fg(Color::Cyan),
            Cell::new("Votes").fg(Color::Cyan),
            Cell::new("Download").fg(Color::Cyan),
            Cell::new("Size").fg(Color::Cyan),
        ])
        .add_rows(plan.packages.iter().map(package_row));

    table
}

fn package_row(package: &PackageNode) -> impl Into<Row> {
    let size = package
        .size
        .map(|size| Cell::new(format_size(size)))
        .unwrap_or_else(|| Cell::new("-"));

    let download_size = package
        .download_size
        .map(|size| Cell::new(format_size(size)))
        .unwrap_or_else(|| Cell::new("-"));

    [
        Cell::new(&package.name),
        Cell::new(package.version.as_deref().unwrap_or("?")),
        source_cell(&package.source),
        maintainer_cell(package.aur.as_ref(), package.packager.as_deref()),
        votes_cell(package.aur.as_ref()),
        download_size,
        size,
    ]
}

fn source_cell(source: &PackageSource) -> Cell {
    let (label, color) = match source {
        PackageSource::Repo => ("repo", Color::Green),
        PackageSource::Aur => ("aur", Color::DarkBlue),
        PackageSource::Installed => ("installed", Color::DarkGrey),
    };

    Cell::new(label).fg(color)
}

fn maintainer_cell(aur: Option<&AurMeta>, packager: Option<&str>) -> Cell {
    if let Some(aur) = aur {
        return match &aur.maintainer {
            Some(maintainer) => Cell::new(maintainer),
            None => Cell::new("orphan").fg(Color::Red),
        };
    }

    packager.map(Cell::new).unwrap_or_else(|| Cell::new("-"))
}

fn votes_cell(aur: Option<&AurMeta>) -> Cell {
    match aur {
        Some(aur) => Cell::new(aur.votes),
        None => Cell::new("-"),
    }
}

pub fn aur_details(plan: &InstallPlan) {
    plan.packages
        .iter()
        .filter_map(|package| package.aur.as_ref().map(|aur| (&package.name, aur)))
        .for_each(print_aur_details);
}

fn print_aur_details((name, aur): (&String, &AurMeta)) {
    println!("\n{} {}", format!("{INDENT}").cyan(), name.bold());
    aur.description
        .as_deref()
        .into_iter()
        .for_each(|description| println!("{INDENT}{INDENT}{description}"));
    print_aur_metadata(aur);
    print_optional_aur_metadata(aur);
}

fn print_aur_metadata(aur: &AurMeta) {
    detail("base", &aur.base);
    detail("maintainer", aur.maintainer.as_deref().unwrap_or("orphan"));

    aur.submitter
        .as_deref()
        .into_iter()
        .for_each(|submitter| detail("submitter", submitter));

    detail(
        "votes",
        format_args!("{} (popularity {:.2})", aur.votes, aur.popularity),
    );
    detail("updated", relative_time(aur.last_modified));
}

fn print_optional_aur_metadata(aur: &AurMeta) {
    aur.out_of_date
        .map(|flagged| {
            format!("out of date {}", relative_time(flagged))
                .red()
                .to_string()
        })
        .into_iter()
        .for_each(|flagged| detail("flagged", &flagged));

    aur.url
        .as_deref()
        .into_iter()
        .for_each(|url| detail("url", url));
}

fn detail(label: &str, value: impl Display) {
    println!("{INDENT}{INDENT}{:<12} {}", label.dimmed(), value);
}

pub fn relative_time(timestamp: i64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or_default();

    relative_time_at(timestamp, now)
}

fn relative_time_at(timestamp: i64, now: i64) -> String {
    let seconds = now - timestamp;

    if seconds < 0 {
        return "just now".into();
    }

    const MINUTE: i64 = 60;
    const HOUR: i64 = MINUTE * 60;
    const DAY: i64 = HOUR * 24;
    const MONTH: i64 = DAY * 30;
    const YEAR: i64 = DAY * 365;

    let (value, unit) = match seconds {
        0..MINUTE => return "just now".into(),
        MINUTE..HOUR => (seconds / MINUTE, "minute"),
        HOUR..DAY => (seconds / HOUR, "hour"),
        DAY..MONTH => (seconds / DAY, "day"),
        MONTH..YEAR => (seconds / MONTH, "month"),
        _ => (seconds / YEAR, "year"),
    };

    let plural = if value == 1 { "" } else { "s" };

    format!("{value} {unit}{plural} ago")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(name: &str, source: PackageSource) -> PackageNode {
        PackageNode {
            name: name.to_owned(),
            version: Some("1.0".to_owned()),
            source,
            dependencies: Vec::new(),
            size: Some(1024),
            download_size: None,
            provides: Vec::new(),
            packager: Some("packager".to_owned()),
            aur: None,
        }
    }

    #[test]
    fn formats_relative_time_boundaries_and_pluralization() {
        let now = 10 * 365 * 24 * 60 * 60;
        assert_eq!(relative_time_at(now + 1, now), "just now");
        assert_eq!(relative_time_at(now - 59, now), "just now");
        assert_eq!(relative_time_at(now - 60, now), "1 minute ago");
        assert_eq!(relative_time_at(now - 120, now), "2 minutes ago");
        assert_eq!(relative_time_at(now - 3600, now), "1 hour ago");
        assert_eq!(relative_time_at(now - 86_400, now), "1 day ago");
    }

    #[test]
    fn package_table_contains_plan_values() {
        let plan = InstallPlan {
            packages: vec![
                package("repo-demo", PackageSource::Repo),
                package("aur-demo", PackageSource::Aur),
            ],
        };

        let rendered = package_table(&plan).to_string();
        assert!(rendered.contains("repo-demo"));
        assert!(rendered.contains("aur-demo"));
        assert!(rendered.contains("1.0 KiB"));
    }
}

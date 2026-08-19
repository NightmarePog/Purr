use crate::{
    aur::rpc::RpcPackage,
    dependency::{Dependency, source::PackageSource},
};
use std::{collections::HashMap, process::Command, string::FromUtf8Error};
use thiserror::Error;

fn parse_size(value: &str) -> Option<u64> {
    let mut parts = value.split_whitespace();
    let number: f64 = parts.next()?.parse().ok()?;

    match parts.next()? {
        "KiB" => Some((number * 1024.0) as u64),
        "MiB" => Some((number * 1024.0 * 1024.0) as u64),
        "GiB" => Some((number * 1024.0 * 1024.0 * 1024.0) as u64),
        _ => None,
    }
}

#[derive(Debug, Error)]
pub enum PacmanError {
    #[error("pacman is not installed")]
    Missing,

    #[error("failed to execute pacman")]
    Pacman(#[source] std::io::Error),

    #[error("package '{0}' not found in the official repositories")]
    NotFound(String),

    #[error("pacman failed to list installed packages")]
    Query,

    #[error("pacman returned invalid UTF-8")]
    Encoding(#[from] FromUtf8Error),

    #[error("failed to compare package versions")]
    VersionCompare(#[source] std::io::Error),

    #[error("vercmp returned invalid output")]
    InvalidVersionCompare,

    #[error("provider '{0}' not found")]
    ProviderNotFound(String),
}

impl PacmanError {
    pub fn spawn(error: std::io::Error) -> Self {
        match error.kind() {
            std::io::ErrorKind::NotFound => Self::Missing,
            _ => Self::Pacman(error),
        }
    }

    pub fn version_compare(error: std::io::Error) -> Self {
        Self::VersionCompare(error)
    }
}

pub fn installed_packages() -> Result<HashMap<String, String>, PacmanError> {
    let output = Command::new("pacman")
        .arg("-Q")
        .output()
        .map_err(PacmanError::spawn)?;

    if !output.status.success() {
        Err(PacmanError::Query)
    } else {
        Ok(parse_installed_packages(&String::from_utf8(output.stdout)?))
    }
}

fn parse_installed_packages(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|line| line.split_once(' '))
        .map(|(name, version)| (name.into(), version.trim().into()))
        .collect()
}

fn parse_package_list(value: &str) -> Vec<String> {
    if value == "None" {
        Vec::new()
    } else {
        value.split_whitespace().map(str::to_owned).collect()
    }
}

#[derive(Debug, Clone)]
pub struct PackageNode {
    pub name: String,
    pub version: Option<String>,
    pub source: PackageSource,
    pub dependencies: Vec<Dependency>,
    pub size: Option<u64>,
    pub download_size: Option<u64>,
    pub provides: Vec<String>,
    pub packager: Option<String>,
    pub aur: Option<AurMeta>,
}

#[derive(Debug, Clone)]
pub struct AurMeta {
    pub base: String,
    pub maintainer: Option<String>,
    pub submitter: Option<String>,
    pub description: Option<String>,
    pub url: Option<String>,
    pub votes: u32,
    pub popularity: f64,
    pub out_of_date: Option<i64>,
    pub last_modified: i64,
}

impl PackageNode {
    pub fn from_rpc(info: &RpcPackage) -> Self {
        Self {
            name: info.name.clone(),
            version: Some(info.version.clone()),
            source: PackageSource::Aur,
            dependencies: info.dependencies(),
            size: None,
            download_size: None,
            provides: info.provides.clone(),
            packager: None,
            aur: Some(AurMeta::from_rpc(info)),
        }
    }

    pub fn from_pacman(target: &str) -> Result<Self, PacmanError> {
        let name = crate::dependency::normalize_name(target).to_owned();

        let output = Command::new("pacman")
            .env("LC_ALL", "C")
            .args(["-Si", target])
            .output()
            .map_err(PacmanError::spawn)?;

        if !output.status.success() {
            Err(PacmanError::NotFound(name))
        } else {
            Ok(Self::parse_pacman(
                &name,
                &String::from_utf8(output.stdout)?,
                PackageSource::Repo,
            ))
        }
    }

    pub fn from_installed(name: &str) -> Result<Self, PacmanError> {
        let output = Command::new("pacman")
            .env("LC_ALL", "C")
            .args(["-Qi", name])
            .output()
            .map_err(PacmanError::spawn)?;

        if !output.status.success() {
            Err(PacmanError::NotFound(name.to_owned()))
        } else {
            Ok(Self::parse_pacman(
                name,
                &String::from_utf8(output.stdout)?,
                PackageSource::Installed,
            ))
        }
    }

    fn parse_pacman(name: &str, text: &str, source: PackageSource) -> Self {
        text.lines()
            .filter_map(|line| line.split_once(':'))
            .map(|(k, v)| (k.trim(), v.trim()))
            .fold(
                Self {
                    name: name.into(),
                    version: None,
                    source,
                    dependencies: Vec::new(),
                    size: None,
                    download_size: None,
                    provides: Vec::new(),
                    packager: None,
                    aur: None,
                },
                |r, (key, value)| match key {
                    "Version" => Self {
                        version: Some(value.into()),
                        ..r
                    },
                    "Installed Size" => Self {
                        size: parse_size(value),
                        ..r
                    },
                    "Download Size" => Self {
                        download_size: parse_size(value),
                        ..r
                    },
                    "Provides" => Self {
                        provides: parse_package_list(value),
                        ..r
                    },
                    "Packager" => Self {
                        packager: Some(value.into()),
                        ..r
                    },
                    _ => r,
                },
            )
    }
}

impl AurMeta {
    fn from_rpc(info: &RpcPackage) -> Self {
        Self {
            base: info.package_base.clone(),
            maintainer: info.maintainer.clone(),
            submitter: info.submitter.clone(),
            description: info.description.clone(),
            url: info.url.clone(),
            votes: info.votes,
            popularity: info.popularity,
            out_of_date: info.out_of_date,
            last_modified: info.last_modified,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pacman_sizes() {
        assert_eq!(parse_size("1.5 KiB"), Some(1536));
        assert_eq!(parse_size("2 MiB"), Some(2 * 1024 * 1024));
        assert_eq!(parse_size("1 GiB"), Some(1024 * 1024 * 1024));
        assert_eq!(parse_size("unknown"), None);
    }

    #[test]
    fn parses_installed_package_listing_and_skips_noise() {
        let packages = parse_installed_packages("alpha 1.0-1\ninvalid\nbeta 2.0-3\n");

        assert_eq!(packages.get("alpha").map(String::as_str), Some("1.0-1"));
        assert_eq!(packages.get("beta").map(String::as_str), Some("2.0-3"));
        assert_eq!(packages.len(), 2);
    }

    #[test]
    fn parses_pacman_metadata_and_none_provides() {
        let package = PackageNode::parse_pacman(
            "demo",
            "Version : 1.2-1\nInstalled Size : 1.5 KiB\nDownload Size : 2 MiB\nProvides : None\nPackager : Example\n",
            PackageSource::Repo,
        );

        assert_eq!(package.name, "demo");
        assert_eq!(package.version.as_deref(), Some("1.2-1"));
        assert_eq!(package.size, Some(1536));
        assert_eq!(package.download_size, Some(2 * 1024 * 1024));
        assert!(package.provides.is_empty());
        assert_eq!(package.packager.as_deref(), Some("Example"));
        assert_eq!(package.source, PackageSource::Repo);
    }

    #[test]
    fn classifies_missing_pacman_executable() {
        let error = PacmanError::spawn(std::io::Error::from(std::io::ErrorKind::NotFound));
        assert!(matches!(error, PacmanError::Missing));
    }
}

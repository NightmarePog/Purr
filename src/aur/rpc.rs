use crate::{
    config,
    dependency::{Dependency, DependencyKind},
};
use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RpcError {
    #[error("cannot reach the AUR, check your internet connection")]
    Offline(#[source] reqwest::Error),

    #[error("failed to reach AUR RPC")]
    Unreachable(#[source] reqwest::Error),

    #[error("failed to parse AUR response")]
    Malformed(#[source] reqwest::Error),

    #[error("package '{0}' not found")]
    NotFound(String),
}

impl RpcError {
    fn request(error: reqwest::Error) -> Self {
        if error.is_connect() || error.is_timeout() {
            Self::Offline(error)
        } else {
            Self::Unreachable(error)
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct RpcResponse {
    pub results: Vec<RpcPackage>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RpcPackage {
    #[serde(rename = "Name")]
    pub name: String,

    #[serde(rename = "Version")]
    pub version: String,

    #[serde(rename = "Maintainer")]
    pub maintainer: Option<String>,

    #[serde(rename = "Submitter", default)]
    pub submitter: Option<String>,

    #[serde(rename = "PackageBase")]
    pub package_base: String,

    #[serde(rename = "Description", default)]
    pub description: Option<String>,

    #[serde(rename = "URL", default)]
    pub url: Option<String>,

    #[serde(rename = "NumVotes", default)]
    pub votes: u32,

    #[serde(rename = "Popularity", default)]
    pub popularity: f64,

    #[serde(rename = "OutOfDate", default)]
    pub out_of_date: Option<i64>,

    #[serde(rename = "LastModified", default)]
    pub last_modified: i64,

    #[serde(rename = "Depends", default)]
    pub depends: Vec<String>,

    #[serde(rename = "MakeDepends", default)]
    pub make_depends: Vec<String>,

    #[serde(rename = "CheckDepends", default)]
    pub check_depends: Vec<String>,

    #[serde(rename = "OptDepends", default)]
    pub opt_depends: Vec<String>,

    #[serde(rename = "Provides", default)]
    pub provides: Vec<String>,

    #[serde(rename = "Conflicts", default)]
    pub conflicts: Vec<String>,
}

impl RpcPackage {
    pub fn orphan(&self) -> bool {
        self.maintainer.is_none()
    }

    fn dependencies_of<'a>(
        dependencies: &'a [String],
        kind: DependencyKind,
    ) -> impl Iterator<Item = Dependency> + 'a {
        dependencies
            .iter()
            .map(move |raw| Dependency::new(raw, kind))
    }

    pub fn dependencies<B: FromIterator<Dependency>>(&self) -> B {
        Self::dependencies_of(&self.depends, DependencyKind::Runtime)
            .chain(Self::dependencies_of(
                &self.make_depends,
                DependencyKind::Build,
            ))
            .chain(Self::dependencies_of(
                &self.check_depends,
                DependencyKind::Check,
            ))
            .chain(Self::dependencies_of(
                &self.opt_depends,
                DependencyKind::Optional,
            ))
            .chain(Self::dependencies_of(
                &self.provides,
                DependencyKind::Provides,
            ))
            .chain(Self::dependencies_of(
                &self.conflicts,
                DependencyKind::Conflicts,
            ))
            .collect()
    }

    pub fn is_outdated(&self) -> bool {
        self.out_of_date.is_some()
    }
}

pub fn fetch_package_info(package: &str) -> Result<RpcPackage, RpcError> {
    let url = format!("{}/rpc/v5/info/{package}", config::AUR_URL);

    let response: RpcResponse = reqwest::blocking::get(url)
        .map_err(RpcError::request)?
        .json()
        .map_err(RpcError::Malformed)?;

    response
        .results
        .into_iter()
        .next()
        .ok_or_else(|| RpcError::NotFound(package.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package() -> RpcPackage {
        RpcPackage {
            name: "demo".to_owned(),
            version: "1.0-1".to_owned(),
            maintainer: Some("maintainer".to_owned()),
            submitter: None,
            package_base: "demo-base".to_owned(),
            description: None,
            url: None,
            votes: 0,
            popularity: 0.0,
            out_of_date: None,
            last_modified: 0,
            depends: vec!["runtime".to_owned()],
            make_depends: vec!["builder".to_owned()],
            check_depends: vec!["checker".to_owned()],
            opt_depends: vec!["optional: description".to_owned()],
            provides: vec!["virtual-demo=1".to_owned()],
            conflicts: vec!["old-demo".to_owned()],
        }
    }

    #[test]
    fn maps_all_dependency_categories() {
        let dependencies: Vec<Dependency> = package().dependencies();

        for (name, kind) in [
            ("runtime", DependencyKind::Runtime),
            ("builder", DependencyKind::Build),
            ("checker", DependencyKind::Check),
            ("optional: description", DependencyKind::Optional),
            ("virtual-demo", DependencyKind::Provides),
            ("old-demo", DependencyKind::Conflicts),
        ] {
            assert!(
                dependencies
                    .iter()
                    .any(|dependency| dependency.name == name && dependency.kind == kind),
                "missing {name:?}"
            );
        }
    }

    #[test]
    fn identifies_orphaned_and_outdated_packages() {
        let mut package = package();
        assert!(!package.orphan());
        assert!(!package.is_outdated());

        package.maintainer = None;
        package.out_of_date = Some(1);
        assert!(package.orphan());
        assert!(package.is_outdated());
    }
}

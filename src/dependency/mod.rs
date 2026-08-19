mod graph;
mod package;
mod plan;
mod resolver;
mod source;

use std::process::Command;

pub use graph::DependencyGraph;
pub use package::{AurMeta, PackageNode, PacmanError, installed_packages};
pub use plan::InstallPlan;
pub use resolver::{ResolveError, Resolver};
pub use source::PackageSource;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DependencyKind {
    Runtime,
    Build,
    Check,
    Optional,
    Provides,
    Conflicts,
}

impl DependencyKind {
    pub fn is_resolvable(self) -> bool {
        matches!(self, Self::Runtime | Self::Build | Self::Check)
    }
}

#[derive(Debug, Clone)]
pub struct Dependency {
    pub name: String,
    pub kind: DependencyKind,
    spec: String,
    requirement: Option<VersionRequirement>,
    requirement_name: String,
}

#[derive(Debug, Clone)]
struct VersionRequirement {
    operator: VersionOperator,
    version: String,
}

#[derive(Debug, Clone, Copy)]
enum VersionOperator {
    Equal,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

impl VersionOperator {
    const fn symbol(self) -> &'static str {
        match self {
            Self::Equal => "=",
            Self::Less => "<",
            Self::LessEqual => "<=",
            Self::Greater => ">",
            Self::GreaterEqual => ">=",
        }
    }
}

impl Dependency {
    pub fn new(raw: &str, kind: DependencyKind) -> Self {
        let spec = raw.trim().to_owned();
        let (name, requirement) = parse_requirement(&spec);
        let name = name.to_owned();

        Self {
            name: name.clone(),
            kind,
            spec,
            requirement,
            requirement_name: name,
        }
    }

    fn for_provider(&self, name: &str) -> Self {
        let spec = self
            .requirement
            .as_ref()
            .map(|requirement| {
                format!(
                    "{}{}{}",
                    name,
                    requirement.operator.symbol(),
                    requirement.version
                )
            })
            .unwrap_or_else(|| name.to_owned());

        Self {
            name: name.to_owned(),
            kind: self.kind,
            spec,
            requirement: self.requirement.clone(),
            requirement_name: self.requirement_name.clone(),
        }
    }
}

pub fn normalize_name(dependency: &str) -> &str {
    parse_requirement(dependency).0
}

fn parse_requirement(dependency: &str) -> (&str, Option<VersionRequirement>) {
    let dependency = dependency.trim();
    let (name, expression) = match dependency.find(['<', '>', '=']) {
        Some(index) => dependency.split_at(index),
        None => (dependency, ""),
    };

    (name.trim(), VersionRequirement::parse(expression))
}

impl VersionRequirement {
    fn parse(expression: &str) -> Option<Self> {
        let (operator, version) = match expression.as_bytes() {
            [b'>', b'=', ..] => (VersionOperator::GreaterEqual, &expression[2..]),
            [b'<', b'=', ..] => (VersionOperator::LessEqual, &expression[2..]),
            [b'>', ..] => (VersionOperator::Greater, &expression[1..]),
            [b'<', ..] => (VersionOperator::Less, &expression[1..]),
            [b'=', ..] => (VersionOperator::Equal, &expression[1..]),
            _ => return None,
        };

        let version = version.trim();
        (!version.is_empty()).then(|| Self {
            operator,
            version: version.to_owned(),
        })
    }

    fn matches(&self, actual: &str) -> Result<bool, PacmanError> {
        let output = Command::new("vercmp")
            .args([actual, &self.version])
            .output()
            .map_err(PacmanError::version_compare)?;
        let comparison = String::from_utf8(output.stdout)?
            .trim()
            .parse::<i8>()
            .map_err(|_| PacmanError::InvalidVersionCompare)?;

        Ok(match self.operator {
            VersionOperator::Equal => comparison == 0,
            VersionOperator::Less => comparison < 0,
            VersionOperator::LessEqual => comparison <= 0,
            VersionOperator::Greater => comparison > 0,
            VersionOperator::GreaterEqual => comparison >= 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dependency_names_and_version_operators() {
        for (raw, name, symbol, version) in [
            ("demo=1.0", "demo", "=", "1.0"),
            ("demo<2", "demo", "<", "2"),
            ("demo<=2", "demo", "<=", "2"),
            ("demo>1", "demo", ">", "1"),
            (" demo >= 1.2 ", "demo", ">=", "1.2"),
        ] {
            let dependency = Dependency::new(raw, DependencyKind::Runtime);
            let requirement = dependency.requirement.expect("version requirement");
            assert_eq!(dependency.name, name);
            assert_eq!(requirement.operator.symbol(), symbol);
            assert_eq!(requirement.version, version);
        }
    }

    #[test]
    fn provider_keeps_the_original_requirement_target() {
        let dependency = Dependency::new("virtual-api>=3", DependencyKind::Build);
        let provider = dependency.for_provider("real-package");

        assert_eq!(provider.name, "real-package");
        assert_eq!(provider.spec, "real-package>=3");
        assert_eq!(provider.requirement_name, "virtual-api");
        assert_eq!(provider.kind, DependencyKind::Build);
    }

    #[test]
    fn only_build_relevant_dependencies_are_resolved() {
        for kind in [
            DependencyKind::Runtime,
            DependencyKind::Build,
            DependencyKind::Check,
        ] {
            assert!(kind.is_resolvable());
        }
        for kind in [
            DependencyKind::Optional,
            DependencyKind::Provides,
            DependencyKind::Conflicts,
        ] {
            assert!(!kind.is_resolvable());
        }
    }
}

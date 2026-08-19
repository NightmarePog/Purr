use crate::dependency::PackageNode;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Default, Clone)]
pub struct DependencyGraph {
    pub packages: HashMap<String, PackageNode>,
}

impl DependencyGraph {
    pub fn insert(&mut self, package: PackageNode) {
        self.packages.insert(package.name.clone(), package);
    }

    pub fn install_order<B: FromIterator<PackageNode>>(&self) -> B {
        let mut result = Vec::new();
        let mut visited = HashSet::new();

        self.packages.keys().for_each(|name| {
            self.visit(name, &mut visited, &mut result);
        });

        result.into_iter().collect()
    }

    fn visit(&self, name: &str, visited: &mut HashSet<String>, result: &mut Vec<PackageNode>) {
        if !visited.insert(name.into()) {
            return;
        }

        if let Some(node) = self.packages.get(name) {
            self.visit_dependencies(node, visited, result);
            result.push(node.clone());
        }
    }

    fn visit_dependencies(
        &self,
        node: &PackageNode,
        visited: &mut HashSet<String>,
        result: &mut Vec<PackageNode>,
    ) {
        node.dependencies
            .iter()
            .filter(|dep| dep.kind.is_resolvable())
            .for_each(|dep| self.visit(&dep.name, visited, result));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dependency::{Dependency, DependencyKind, PackageSource};

    fn package(name: &str, dependencies: Vec<Dependency>) -> PackageNode {
        PackageNode {
            name: name.to_owned(),
            version: Some("1".to_owned()),
            source: PackageSource::Aur,
            dependencies,
            size: None,
            download_size: None,
            provides: Vec::new(),
            packager: None,
            aur: None,
        }
    }

    #[test]
    fn orders_dependencies_before_consumers() {
        let mut graph = DependencyGraph::default();
        graph.insert(package("base", Vec::new()));
        graph.insert(package(
            "app",
            vec![Dependency::new("base", DependencyKind::Runtime)],
        ));

        let order: Vec<PackageNode> = graph.install_order();
        let names = order
            .iter()
            .map(|node| node.name.as_str())
            .collect::<Vec<_>>();
        let base = names.iter().position(|name| *name == "base").unwrap();
        let app = names.iter().position(|name| *name == "app").unwrap();
        assert!(base < app);
    }

    #[test]
    fn ignores_optional_edges_and_emits_each_cycle_member_once() {
        let mut graph = DependencyGraph::default();
        graph.insert(package(
            "a",
            vec![
                Dependency::new("b", DependencyKind::Runtime),
                Dependency::new("optional", DependencyKind::Optional),
            ],
        ));
        graph.insert(package(
            "b",
            vec![Dependency::new("a", DependencyKind::Runtime)],
        ));
        graph.insert(package("optional", Vec::new()));

        let mut visited = HashSet::new();
        let mut order = Vec::new();
        graph.visit("a", &mut visited, &mut order);
        let names = order.into_iter().map(|node| node.name).collect::<Vec<_>>();
        assert_eq!(names.iter().filter(|name| *name == "a").count(), 1);
        assert_eq!(names.iter().filter(|name| *name == "b").count(), 1);
        assert!(!names.iter().any(|name| name == "optional"));
    }
}

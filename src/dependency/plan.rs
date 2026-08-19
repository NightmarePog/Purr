use crate::dependency::{DependencyGraph, PackageNode};

pub struct InstallPlan {
    pub packages: Vec<PackageNode>,
}

impl InstallPlan {
    pub fn from_graph(graph: &DependencyGraph) -> Self {
        Self {
            packages: graph.install_order(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dependency::{Dependency, DependencyKind, PackageSource};

    fn node(name: &str, dependencies: Vec<Dependency>) -> PackageNode {
        PackageNode {
            name: name.to_owned(),
            version: None,
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
    fn preserves_dependency_first_graph_order() {
        let mut graph = DependencyGraph::default();
        graph.insert(node("dependency", Vec::new()));
        graph.insert(node(
            "target",
            vec![Dependency::new("dependency", DependencyKind::Build)],
        ));

        let plan = InstallPlan::from_graph(&graph);
        let dependency = plan
            .packages
            .iter()
            .position(|package| package.name == "dependency")
            .expect("dependency in plan");
        let target = plan
            .packages
            .iter()
            .position(|package| package.name == "target")
            .expect("target in plan");
        assert!(dependency < target);
    }
}

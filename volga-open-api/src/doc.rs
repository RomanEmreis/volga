//! Types and utils for OpenAPI documents.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use serde::{Deserialize, Serialize};
use super::{op::OpenApiOperation, schema::OpenApiSchema};

/// Represents OpenAPI document.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OpenApiDocument {
    pub(super) openapi: String,
    pub(super) info: OpenApiInfo,
    pub(super) components: OpenApiComponents,
    pub(super) paths: BTreeMap<String, BTreeMap<String, OpenApiOperation>>,
}

/// Represents OpenAPI info.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct OpenApiInfo {
    pub(super) title: String,
    pub(super) version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) description: Option<String>,
}

/// Represents OpenAPI components.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct OpenApiComponents {
    pub(super) schemas: BTreeMap<String, OpenApiSchema>,
}

impl OpenApiDocument {
    pub(super) fn prune_unreferenced_components(&mut self) {
        let mut reachable = BTreeSet::new();

        for methods in self.paths.values() {
            for op in methods.values() {
                op.collect_component_refs(&mut reachable);
            }
        }

        let mut queue: VecDeque<String> = reachable.iter().cloned().collect();
        while let Some(name) = queue.pop_front() {
            let Some(schema) = self.components.schemas.get(&name) else {
                continue;
            };

            let mut nested = BTreeSet::new();
            schema.collect_component_refs(&mut nested);

            for nested_name in nested {
                if reachable.insert(nested_name.clone()) {
                    queue.push_back(nested_name);
                }
            }
        }

        self.components
            .schemas
            .retain(|name, _| reachable.contains(name));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::op::OpenApiOperation;

    #[test]
    fn prune_unreferenced_components_keeps_only_reachable_transitive_refs() {
        let mut op = OpenApiOperation::default();
        op.set_response_body(
            OpenApiSchema::reference("User"),
            None,
            "application/json",
        );

        let mut paths = BTreeMap::new();
        paths.insert(
            "/users".to_string(),
            BTreeMap::from([("get".to_string(), op)]),
        );

        let mut schemas = BTreeMap::new();
        schemas.insert(
            "User".to_string(),
            OpenApiSchema::object().with_property("profile", OpenApiSchema::reference("Profile")),
        );
        schemas.insert(
            "Profile".to_string(),
            OpenApiSchema::object().with_property("name", OpenApiSchema::string()),
        );
        schemas.insert(
            "Unused".to_string(),
            OpenApiSchema::object().with_property("x", OpenApiSchema::integer()),
        );

        let mut doc = OpenApiDocument {
            openapi: "3.0.0".to_string(),
            info: OpenApiInfo {
                title: "Test".to_string(),
                version: "1.0.0".to_string(),
                description: None,
            },
            components: OpenApiComponents { schemas },
            paths,
        };

        doc.prune_unreferenced_components();

        assert!(doc.components.schemas.contains_key("User"));
        assert!(doc.components.schemas.contains_key("Profile"));
        assert!(!doc.components.schemas.contains_key("Unused"));
        assert_eq!(doc.components.schemas.len(), 2);
    }
}

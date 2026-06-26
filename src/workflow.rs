use std::collections::BTreeMap;

use serde_json::{json, Value};

use crate::callback::WorkflowPayload;
use crate::parser::Frontmatter;

#[derive(Debug, Clone)]
pub struct WorkflowContext {
    workflow: Value,
    nodes: BTreeMap<String, Value>,
}

impl WorkflowContext {
    pub fn from_frontmatter(frontmatter: &Frontmatter) -> Option<Self> {
        let raw = &frontmatter.workflow.as_ref()?.0;
        let workflow = workflow_summary(raw);
        let nodes = workflow_nodes(raw);
        Some(Self { workflow, nodes })
    }

    pub fn payload_for_step(&self, step_id: Option<&str>) -> Option<WorkflowPayload> {
        let id = step_id?;
        let node = self.nodes.get(id)?;
        Some(WorkflowPayload {
            workflow: self.workflow.clone(),
            workflow_node: compact_node(id, node),
        })
    }
}

#[cfg(test)]
pub fn declared_node_ids(frontmatter: &Frontmatter) -> std::collections::BTreeSet<String> {
    frontmatter
        .workflow
        .as_ref()
        .map(|w| workflow_nodes(&w.0))
        .unwrap_or_default()
        .into_keys()
        .collect()
}

fn workflow_summary(raw: &Value) -> Value {
    match raw {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                if k != "nodes" {
                    out.insert(k.clone(), v.clone());
                }
            }
            Value::Object(out)
        }
        other => other.clone(),
    }
}

fn workflow_nodes(raw: &Value) -> BTreeMap<String, Value> {
    raw.get("nodes")
        .and_then(|v| v.as_object())
        .map(|nodes| {
            nodes
                .iter()
                .map(|(id, node)| (id.clone(), node.clone()))
                .collect()
        })
        .unwrap_or_default()
}

fn compact_node(id: &str, node: &Value) -> Value {
    let primitive = node.get("primitive").cloned().unwrap_or(Value::Null);
    let title = node.get("title").cloned().unwrap_or(Value::Null);
    json!({
        "id": id,
        "primitive": primitive,
        "title": title,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_payload_for_declared_step() {
        let fm = Frontmatter {
            workflow: Some(crate::parser::WorkflowManifest(json!({
                "slug": "stripe/balance",
                "version": "2026-05-09T00:00:00Z",
                "nodes": {
                    "balance": {
                        "primitive": "stripe/balance-snapshot",
                        "title": "Stripe balance snapshot",
                        "outputs": [{"name": "snapshot_path"}]
                    }
                }
            }))),
            ..Default::default()
        };
        let ctx = WorkflowContext::from_frontmatter(&fm).unwrap();
        let payload = ctx.payload_for_step(Some("balance")).unwrap();
        assert_eq!(payload.workflow["slug"], "stripe/balance");
        assert!(payload.workflow.get("nodes").is_none());
        assert_eq!(payload.workflow_node["id"], "balance");
        assert_eq!(
            payload.workflow_node["primitive"],
            "stripe/balance-snapshot"
        );
    }

    #[test]
    fn declared_node_ids_extracts_nodes() {
        let fm = Frontmatter {
            workflow: Some(crate::parser::WorkflowManifest(
                json!({"nodes": {"a": {}, "b": {}}}),
            )),
            ..Default::default()
        };
        let ids: Vec<_> = declared_node_ids(&fm).into_iter().collect();
        assert_eq!(ids, vec!["a".to_string(), "b".to_string()]);
    }
}

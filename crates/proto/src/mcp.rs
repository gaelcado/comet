//! Contracts for inspecting and configuring the built-in Zeron MCP tools.
//!
//! The catalog itself lives in `zeron-mcp`; these are the owned forms that
//! cross RPC. Two counters stay distinct: `catalogVersion` changes with the
//! built-in names, descriptions and schemas, `policyRevision` with each saved
//! per-tool override on one engine.

use serde::{Deserialize, Serialize};

/// Where a tool may be offered, independent of any policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum McpToolAvailability {
    #[default]
    Always,
    /// Only to an agent Zeron launched in a chat (it acts on that chat).
    ChatOnly,
    /// A constraint added by a newer engine. Never offered.
    #[serde(other)]
    Unknown,
}

impl McpToolAvailability {
    /// Whether a bridge may offer the tool, given whether it speaks for a chat.
    pub fn allows(self, in_chat: bool) -> bool {
        match self {
            Self::Always => true,
            Self::ChatOnly => in_chat,
            Self::Unknown => false,
        }
    }
}

/// One built-in tool as the catalog defines it, before any override.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolDescriptor {
    pub name: String,
    /// The original description.
    pub description: String,
    pub input_schema: serde_json::Value,
    #[serde(default)]
    pub availability: McpToolAvailability,
}

/// A user's change to one tool. An omitted field follows the default (enabled,
/// original description) rather than storing a copy of it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolOverride {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description_override: Option<String>,
}

/// One tool's descriptor and its effective policy. It is unavailable when
/// `enabled` is false, and in contexts its `availability` excludes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolState {
    pub descriptor: McpToolDescriptor,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description_override: Option<String>,
}

impl McpToolState {
    /// The description agents are offered.
    pub fn effective_description(&self) -> &str {
        self.description_override
            .as_deref()
            .unwrap_or(&self.descriptor.description)
    }
}

/// Every built-in tool with its effective policy, disabled ones included.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpCatalogSnapshot {
    pub catalog_version: u32,
    pub policy_revision: u64,
    pub tools: Vec<McpToolState>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn snapshot_round_trips_and_omitted_overrides_follow_defaults() {
        let snapshot = McpCatalogSnapshot {
            catalog_version: 1,
            policy_revision: 7,
            tools: vec![McpToolState {
                descriptor: McpToolDescriptor {
                    name: "show_visual".into(),
                    description: "original".into(),
                    input_schema: json!({ "type": "object", "properties": {} }),
                    availability: McpToolAvailability::ChatOnly,
                },
                enabled: false,
                description_override: Some("edited".into()),
            }],
        };
        let wire = serde_json::to_value(&snapshot).unwrap();
        assert_eq!(wire["policyRevision"], 7);
        assert_eq!(wire["tools"][0]["descriptor"]["availability"], "chatOnly");
        assert_eq!(
            wire["tools"][0]["descriptor"]["inputSchema"]["type"],
            "object"
        );
        assert_eq!(
            serde_json::from_value::<McpCatalogSnapshot>(wire).unwrap(),
            snapshot
        );
        assert_eq!(snapshot.tools[0].effective_description(), "edited");

        let reset: McpToolOverride = serde_json::from_value(json!({ "name": "whoami" })).unwrap();
        assert_eq!(
            (reset.enabled, reset.description_override.as_deref()),
            (None, None)
        );
        assert_eq!(
            serde_json::to_value(&reset).unwrap(),
            json!({ "name": "whoami" })
        );

        let newer: McpToolDescriptor = serde_json::from_value(json!({
            "name": "future", "description": "d", "inputSchema": {}, "availability": "projectOnly"
        }))
        .unwrap();
        assert!(!newer.availability.allows(true));
    }
}

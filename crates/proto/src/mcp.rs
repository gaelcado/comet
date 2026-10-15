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
    /// The stored policy could not be read (corrupt file or unknown version).
    /// Every tool is then reported disabled and refused until a
    /// `SetZeronMcpToolOverride` against this `policyRevision` rebuilds the
    /// policy from defaults; the unreadable file is kept aside for diagnosis.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub policy_unreadable: bool,
}

/// Longest description override accepted, in UTF-8 bytes. Built-in
/// descriptions are not bound by it.
pub const MCP_DESCRIPTION_OVERRIDE_MAX_BYTES: usize = 16 * 1024;

/// Why a description override is rejected. Shared so Settings can validate
/// inline with exactly the engine's rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpDescriptionError {
    Blank,
    TooLong,
    ControlCharacter,
}

impl std::fmt::Display for McpDescriptionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Blank => f.write_str("the description is empty"),
            Self::TooLong => write!(
                f,
                "the description is over {} KiB",
                MCP_DESCRIPTION_OVERRIDE_MAX_BYTES / 1024
            ),
            Self::ControlCharacter => f.write_str("the description contains a control character"),
        }
    }
}

/// Validate a description override: non-blank, at most
/// [`MCP_DESCRIPTION_OVERRIDE_MAX_BYTES`], and no control characters other
/// than tab and line breaks.
pub fn validate_description_override(text: &str) -> Result<(), McpDescriptionError> {
    if text.trim().is_empty() {
        return Err(McpDescriptionError::Blank);
    }
    if text.len() > MCP_DESCRIPTION_OVERRIDE_MAX_BYTES {
        return Err(McpDescriptionError::TooLong);
    }
    if text
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\t' | '\n' | '\r'))
    {
        return Err(McpDescriptionError::ControlCharacter);
    }
    Ok(())
}

/// `WatchZeronMcpCatalog` params. Relay-forwardable.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchZeronMcpCatalogParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_device_id: Option<String>,
}

/// How a `SetZeronMcpToolOverride` changes the description.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "kind", content = "text", rename_all = "camelCase")]
pub enum McpDescriptionPatch {
    /// Keep the current override, or the original.
    #[default]
    Unchanged,
    /// Replace the description. Text equal to the original clears the
    /// override, so the tool follows future versions of its description.
    Set(String),
    /// Drop the override and follow the original description.
    Reset,
}

/// `SetZeronMcpToolOverride` params. Relay-forwardable; replies with the
/// persisted [`McpCatalogSnapshot`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetZeronMcpToolOverrideParams {
    pub name: String,
    /// The `policyRevision` the change was made against. A different current
    /// revision fails with `mcp.policy_conflict` and changes nothing.
    pub expected_policy_revision: u64,
    /// `None` leaves the switch as it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub description_patch: McpDescriptionPatch,
    /// Retrying the same operation after a lost reply returns the current
    /// policy instead of a conflict.
    pub operation_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_device_id: Option<String>,
}

/// `GetZeronMcpDiscovery` params. Asked by a bridge of its own engine; never
/// relay-forwarded.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetZeronMcpDiscoveryParams {
    /// The chat the bridge speaks for; chat-only tools need one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_chat_id: Option<String>,
    /// The bridge's own version, for diagnosing lagging sessions.
    #[serde(default)]
    pub bridge_version: String,
}

/// One tool a bridge may offer now. The bridge serves its own schema for the
/// name; only the description can differ from its built-in one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpDiscoveredTool {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description_override: Option<String>,
}

/// `GetZeronMcpDiscovery` reply: enabled tools available in the caller's
/// context, in catalog order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpDiscovery {
    pub catalog_version: u32,
    pub policy_revision: u64,
    pub tools: Vec<McpDiscoveredTool>,
}

/// `AdmitZeronMcpToolCall` params. Never relay-forwarded.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdmitZeronMcpToolCallParams {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_chat_id: Option<String>,
    #[serde(default)]
    pub bridge_version: String,
}

/// Why a call was refused. Stable wire names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpAdmissionRefusal {
    /// Switched off in Settings.
    Disabled,
    /// The tool exists but not for this caller (a chat-only tool outside a chat).
    UnavailableInContext,
    /// The engine's catalog has no such tool.
    Unknown,
    /// The stored policy is unreadable; nothing is admitted until it is repaired.
    PolicyUnreadable,
    /// A reason added by a newer engine. Still a refusal.
    #[serde(other)]
    Other,
}

/// `AdmitZeronMcpToolCall` reply. The engine's answer at this instant; a
/// bridge never caches it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpAdmission {
    pub admitted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refusal: Option<McpAdmissionRefusal>,
    pub catalog_version: u32,
    pub policy_revision: u64,
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
            policy_unreadable: false,
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

        // Policy RPC wire shapes the Settings UI and older bridges rely on.
        let set: SetZeronMcpToolOverrideParams = serde_json::from_value(json!({
            "name": "send_message", "expectedPolicyRevision": 3, "operationId": "op",
            "descriptionPatch": { "kind": "set", "text": "Send." }
        }))
        .unwrap();
        assert_eq!(
            (set.enabled, set.description_patch),
            (None, McpDescriptionPatch::Set("Send.".into()))
        );
        assert_eq!(
            serde_json::to_value(McpDescriptionPatch::Reset).unwrap(),
            json!({ "kind": "reset" })
        );
        assert_eq!(
            serde_json::to_value(McpAdmissionRefusal::UnavailableInContext).unwrap(),
            "unavailable_in_context"
        );
        assert_eq!(
            serde_json::from_value::<McpAdmissionRefusal>(json!("quota")).unwrap(),
            McpAdmissionRefusal::Other
        );
        assert_eq!(
            validate_description_override(" \n"),
            Err(McpDescriptionError::Blank)
        );
        assert_eq!(
            validate_description_override("a\u{7}b"),
            Err(McpDescriptionError::ControlCharacter)
        );
        assert!(validate_description_override("Line one.\n\tLine two.").is_ok());
        assert_eq!(
            validate_description_override(&"x".repeat(MCP_DESCRIPTION_OVERRIDE_MAX_BYTES + 1)),
            Err(McpDescriptionError::TooLong)
        );
    }
}

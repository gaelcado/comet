//! The built-in Zeron MCP tool policy of one engine profile: per-tool
//! switches and description overrides, persisted in
//! `mcp-tool-settings.json` under the profile's store root, and the
//! admission every bridge asks for before running a tool.
//!
//! [`McpToolPolicy`] owns the file, its revision and admission. Writes are
//! compare-and-set on `policyRevision`, written to an adjacent temporary file
//! and renamed into place; the new revision is published only once that
//! succeeds, so an acknowledged change binds every later admission.
//!
//! The policy fails closed. A missing file means the historical defaults
//! (every tool enabled, original descriptions). A file that cannot be read
//! or has an unknown version is never replaced by defaults: every tool is
//! refused until Settings sends a change against the published revision,
//! which keeps a copy of the unreadable file and rebuilds the policy from
//! defaults. Override text never reaches the logs.

use std::collections::{BTreeMap, VecDeque};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use zeron_mcp::catalog::{CATALOG_VERSION, ToolName, descriptors};
use zeron_proto::{
    McpAdmission, McpAdmissionRefusal, McpCatalogSnapshot, McpDescriptionError,
    McpDescriptionPatch, McpDiscoveredTool, McpDiscovery, McpToolState,
    SetZeronMcpToolOverrideParams, validate_description_override,
};

pub const MCP_TOOL_SETTINGS_FILE: &str = "mcp-tool-settings.json";
const FILE_VERSION: u32 = 1;
/// Applied operation ids remembered for replay after a lost reply.
const RECENT_OPERATIONS: usize = 64;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PolicyFile {
    version: u32,
    revision: u64,
    /// Kept as stored, including names this build's catalog lacks (written by
    /// a newer build); those never create or enable a tool.
    #[serde(default)]
    tools: BTreeMap<String, ToolEntry>,
}

impl PolicyFile {
    fn defaults(revision: u64) -> Self {
        Self {
            version: FILE_VERSION,
            revision,
            tools: BTreeMap::new(),
        }
    }
}

/// An omitted field follows the default; the original is never copied in.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToolEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description_override: Option<String>,
}

enum Stored {
    Valid(PolicyFile),
    /// The file could not be used. Nothing is admitted until a change made
    /// against `revision` rebuilds the policy.
    Unreadable {
        revision: u64,
    },
}

impl Stored {
    fn revision(&self) -> u64 {
        match self {
            Self::Valid(file) => file.revision,
            Self::Unreadable { revision } => *revision,
        }
    }
}

struct Writer {
    stored: Stored,
    /// `(operationId, expectedPolicyRevision)` of recently applied changes.
    recent: VecDeque<(String, u64)>,
}

struct Inner {
    path: PathBuf,
    /// Serializes changes and holds the durable state. Readers use
    /// `published` and never wait on disk I/O.
    writer: Mutex<Writer>,
    published: watch::Sender<McpCatalogSnapshot>,
}

#[derive(Debug, thiserror::Error)]
pub enum McpPolicyError {
    #[error("the Zeron MCP policy changed (now revision {current})")]
    Conflict { current: u64 },
    #[error("the Zeron MCP policy file is unreadable")]
    Unreadable,
    #[error("unknown Zeron MCP tool: {0}")]
    UnknownTool(String),
    #[error("invalid description: {0}")]
    InvalidDescription(McpDescriptionError),
    #[error("could not save the Zeron MCP policy: {0}")]
    Persist(#[source] std::io::Error),
}

/// Policy owner for one engine profile. Cheap to clone.
#[derive(Clone)]
pub struct McpToolPolicy {
    inner: Arc<Inner>,
}

impl McpToolPolicy {
    /// Load the profile's policy. Never fails: an unusable file is reported
    /// through [`McpCatalogSnapshot::policy_unreadable`] and refuses tools.
    pub fn open(profile_store_root: &Path) -> Self {
        let path = profile_store_root.join(MCP_TOOL_SETTINGS_FILE);
        let stored = load(&path);
        let published = watch::Sender::new(snapshot_of(&stored));
        Self {
            inner: Arc::new(Inner {
                path,
                writer: Mutex::new(Writer {
                    stored,
                    recent: VecDeque::new(),
                }),
                published,
            }),
        }
    }

    /// Every tool with its effective state, disabled ones included.
    pub fn snapshot(&self) -> McpCatalogSnapshot {
        self.inner.published.borrow().clone()
    }

    /// The current snapshot, then one per published change.
    pub fn watch(&self) -> watch::Receiver<McpCatalogSnapshot> {
        self.inner.published.subscribe()
    }

    /// The tools a bridge may offer: enabled and allowed in its context.
    pub fn discovery(&self, in_chat: bool) -> Result<McpDiscovery, McpPolicyError> {
        let snapshot = self.inner.published.borrow();
        if snapshot.policy_unreadable {
            return Err(McpPolicyError::Unreadable);
        }
        Ok(McpDiscovery {
            catalog_version: snapshot.catalog_version,
            policy_revision: snapshot.policy_revision,
            tools: snapshot
                .tools
                .iter()
                .filter(|tool| tool.enabled && tool.descriptor.availability.allows(in_chat))
                .map(|tool| McpDiscoveredTool {
                    name: tool.descriptor.name.clone(),
                    description_override: tool.description_override.clone(),
                })
                .collect(),
        })
    }

    /// Admit one call under the published policy. This is the linearization
    /// point: once a disabling change is acknowledged, no later call is admitted.
    pub fn admit(&self, name: &str, in_chat: bool) -> McpAdmission {
        let snapshot = self.inner.published.borrow();
        let refusal = match snapshot.tools.iter().find(|t| t.descriptor.name == name) {
            _ if snapshot.policy_unreadable => Some(McpAdmissionRefusal::PolicyUnreadable),
            None => Some(McpAdmissionRefusal::Unknown),
            Some(tool) if !tool.descriptor.availability.allows(in_chat) => {
                Some(McpAdmissionRefusal::UnavailableInContext)
            }
            Some(tool) if !tool.enabled => Some(McpAdmissionRefusal::Disabled),
            Some(_) => None,
        };
        McpAdmission {
            admitted: refusal.is_none(),
            refusal,
            catalog_version: snapshot.catalog_version,
            policy_revision: snapshot.policy_revision,
        }
    }

    /// Apply one tool's change if `expected_policy_revision` is current, and
    /// return the persisted policy. Blocking file I/O: call off async workers.
    ///
    /// Against an unreadable policy the change rebuilds it from defaults
    /// (a copy of the unreadable file is kept first). Replaying an applied
    /// `operation_id` returns the current policy rather than a conflict.
    pub fn set_override(
        &self,
        change: &SetZeronMcpToolOverrideParams,
    ) -> Result<McpCatalogSnapshot, McpPolicyError> {
        let tool = ToolName::parse(&change.name)
            .ok_or_else(|| McpPolicyError::UnknownTool(change.name.clone()))?;
        if let McpDescriptionPatch::Set(text) = &change.description_patch {
            validate_description_override(text).map_err(McpPolicyError::InvalidDescription)?;
        }
        let mut writer = lock(&self.inner.writer);
        let current = writer.stored.revision();
        if current != change.expected_policy_revision {
            let replay = writer.recent.iter().any(|(operation, expected)| {
                *operation == change.operation_id && *expected == change.expected_policy_revision
            });
            if replay {
                return Ok(self.snapshot());
            }
            return Err(McpPolicyError::Conflict { current });
        }
        let (mut file, repairing) = match &writer.stored {
            Stored::Valid(file) => (file.clone(), false),
            Stored::Unreadable { revision } => (PolicyFile::defaults(*revision), true),
        };
        let original = self
            .inner
            .published
            .borrow()
            .tools
            .iter()
            .find(|t| t.descriptor.name == tool.as_str())
            .map(|t| t.descriptor.description.clone())
            .unwrap_or_default();
        let entry = file.tools.entry(tool.as_str().to_owned()).or_default();
        let before = entry.clone();
        match change.enabled {
            Some(true) => entry.enabled = None,
            Some(false) => entry.enabled = Some(false),
            None => {}
        }
        match &change.description_patch {
            McpDescriptionPatch::Unchanged => {}
            McpDescriptionPatch::Reset => entry.description_override = None,
            McpDescriptionPatch::Set(text) => {
                entry.description_override = (*text != original).then(|| text.clone());
            }
        }
        let changed = *entry != before;
        if *entry == ToolEntry::default() {
            file.tools.remove(tool.as_str());
        }
        if changed || repairing {
            file.revision = current.saturating_add(1);
            if repairing {
                set_aside(&self.inner.path).map_err(McpPolicyError::Persist)?;
            }
            persist(&self.inner.path, &file).map_err(McpPolicyError::Persist)?;
            tracing::info!(
                tool = tool.as_str(),
                revision = file.revision,
                repaired = repairing,
                disabled = file.tools.get(tool.as_str()).and_then(|e| e.enabled) == Some(false),
                described = file
                    .tools
                    .get(tool.as_str())
                    .is_some_and(|e| e.description_override.is_some()),
                "Zeron MCP tool policy saved"
            );
            writer.stored = Stored::Valid(file);
            self.inner
                .published
                .send_replace(snapshot_of(&writer.stored));
        }
        if writer.recent.len() == RECENT_OPERATIONS {
            writer.recent.pop_front();
        }
        writer
            .recent
            .push_back((change.operation_id.clone(), change.expected_policy_revision));
        Ok(self.snapshot())
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn load(path: &Path) -> Stored {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Stored::Valid(PolicyFile::defaults(0));
        }
        Err(err) => {
            tracing::error!(
                path = %path.display(),
                error = %err,
                "Zeron MCP tool policy unreadable; refusing its tools until it is rebuilt in Settings"
            );
            return Stored::Unreadable { revision: 1 };
        }
    };
    match serde_json::from_slice::<PolicyFile>(&bytes) {
        Ok(file) if file.version == FILE_VERSION => Stored::Valid(file),
        Ok(file) => {
            tracing::error!(
                path = %path.display(),
                version = file.version,
                "Zeron MCP tool policy has an unknown version; refusing its tools until it is rebuilt in Settings"
            );
            Stored::Unreadable {
                revision: file.revision.saturating_add(1),
            }
        }
        Err(err) => {
            // Only the error class and position: serde messages can quote values.
            tracing::error!(
                path = %path.display(),
                class = ?err.classify(),
                line = err.line(),
                column = err.column(),
                "Zeron MCP tool policy is corrupt; refusing its tools until it is rebuilt in Settings"
            );
            // Stay ahead of any revision a client saw before the damage.
            let hint = serde_json::from_slice::<serde_json::Value>(&bytes)
                .ok()
                .and_then(|value| value.get("revision")?.as_u64());
            Stored::Unreadable {
                revision: hint.map_or(1, |revision| revision.saturating_add(1)),
            }
        }
    }
}

fn snapshot_of(stored: &Stored) -> McpCatalogSnapshot {
    let file = match stored {
        Stored::Valid(file) => Some(file),
        Stored::Unreadable { .. } => None,
    };
    let tools = descriptors()
        .into_iter()
        .map(|descriptor| {
            let entry = file.and_then(|file| file.tools.get(&descriptor.name));
            McpToolState {
                // Unreadable: report what admission does, refuse everything.
                enabled: file.is_some() && entry.and_then(|e| e.enabled).unwrap_or(true),
                description_override: entry.and_then(|e| e.description_override.clone()),
                descriptor,
            }
        })
        .collect();
    McpCatalogSnapshot {
        catalog_version: CATALOG_VERSION,
        policy_revision: stored.revision(),
        tools,
        policy_unreadable: file.is_none(),
    }
}

/// Keep a copy of an unreadable policy file for diagnosis before it is
/// rebuilt. A copy, so a failed rebuild still finds it in place next boot
/// instead of falling back to defaults.
fn set_aside(path: &Path) -> std::io::Result<()> {
    let aside = path.with_file_name(format!(
        "{MCP_TOOL_SETTINGS_FILE}.unreadable-{}",
        crate::now_ms()
    ));
    match std::fs::copy(path, &aside) {
        Ok(_) => {
            tracing::warn!(kept = %aside.display(), "unreadable Zeron MCP tool policy set aside");
            Ok(())
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

fn persist(path: &Path, file: &PolicyFile) -> std::io::Result<()> {
    let mut bytes = serde_json::to_vec_pretty(file).map_err(std::io::Error::other)?;
    bytes.push(b'\n');
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("MCP tool policy path has no parent"))?;
    std::fs::create_dir_all(parent)?;
    let temp_path = parent.join(format!(
        ".{MCP_TOOL_SETTINGS_FILE}.tmp-{}",
        uuid::Uuid::new_v4()
    ));
    let result = (|| {
        let mut temp = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp_path)?;
        temp.write_all(&bytes)?;
        temp.sync_all()?;
        std::fs::rename(&temp_path, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp_path);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(
        name: &str,
        expected: u64,
        enabled: Option<bool>,
        patch: McpDescriptionPatch,
        operation: &str,
    ) -> SetZeronMcpToolOverrideParams {
        SetZeronMcpToolOverrideParams {
            name: name.into(),
            expected_policy_revision: expected,
            enabled,
            description_patch: patch,
            operation_id: operation.into(),
            target_device_id: None,
        }
    }

    fn state<'a>(snapshot: &'a McpCatalogSnapshot, name: &str) -> &'a McpToolState {
        snapshot
            .tools
            .iter()
            .find(|t| t.descriptor.name == name)
            .unwrap()
    }

    #[test]
    fn changes_persist_across_reopen_with_compare_and_set() {
        let root = tempfile::tempdir().unwrap();
        let policy = McpToolPolicy::open(root.path());
        assert_eq!(policy.snapshot().policy_revision, 0);
        assert!(policy.snapshot().tools.iter().all(|t| t.enabled));

        let disabled = policy
            .set_override(&change(
                "send_message",
                0,
                Some(false),
                McpDescriptionPatch::Set("Send one message.".into()),
                "op-1",
            ))
            .unwrap();
        assert_eq!(disabled.policy_revision, 1);
        assert!(!state(&disabled, "send_message").enabled);
        assert!(!policy.admit("send_message", false).admitted);
        // A lost reply retried with the same operation is not a conflict.
        assert_eq!(
            policy
                .set_override(&change(
                    "send_message",
                    0,
                    Some(false),
                    McpDescriptionPatch::Unchanged,
                    "op-1"
                ))
                .unwrap()
                .policy_revision,
            1
        );
        // A stale writer changes nothing.
        assert!(matches!(
            policy.set_override(&change(
                "whoami",
                0,
                Some(false),
                McpDescriptionPatch::Unchanged,
                "op-2"
            )),
            Err(McpPolicyError::Conflict { current: 1 })
        ));
        // Overrides a newer build wrote for tools this one lacks survive.
        let path = root.path().join(MCP_TOOL_SETTINGS_FILE);
        let mut stored: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        stored["tools"]["future_tool"] = serde_json::json!({ "enabled": false });
        std::fs::write(&path, serde_json::to_vec(&stored).unwrap()).unwrap();

        let reopened = McpToolPolicy::open(root.path());
        assert_eq!(reopened.snapshot(), policy.snapshot());
        assert_eq!(
            state(&reopened.snapshot(), "send_message").effective_description(),
            "Send one message."
        );
        assert_eq!(
            reopened.admit("future_tool", true).refusal,
            Some(McpAdmissionRefusal::Unknown)
        );
        let reset = reopened
            .set_override(&change(
                "send_message",
                1,
                Some(true),
                McpDescriptionPatch::Reset,
                "op-3",
            ))
            .unwrap();
        assert_eq!(reset.policy_revision, 2);
        assert!(reopened.admit("send_message", false).admitted);
        let on_disk: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            on_disk,
            serde_json::json!({
                "version": 1, "revision": 2,
                "tools": { "future_tool": { "enabled": false } }
            }),
            "defaults are omitted, not copied"
        );
    }

    #[test]
    fn a_corrupt_file_refuses_every_tool_until_rebuilt() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(MCP_TOOL_SETTINGS_FILE);
        std::fs::write(&path, br#"{"version":1,"revision":4,"tools":{"whoami":"#).unwrap();

        let policy = McpToolPolicy::open(root.path());
        let snapshot = policy.snapshot();
        assert!(snapshot.policy_unreadable);
        assert!(snapshot.tools.iter().all(|t| !t.enabled));
        assert_eq!(
            policy.admit("whoami", true).refusal,
            Some(McpAdmissionRefusal::PolicyUnreadable)
        );
        assert!(matches!(
            policy.discovery(true),
            Err(McpPolicyError::Unreadable)
        ));
        // A writer that never saw the unreadable state cannot rebuild it.
        assert!(matches!(
            policy.set_override(&change(
                "whoami",
                4,
                None,
                McpDescriptionPatch::Reset,
                "op-1"
            )),
            Err(McpPolicyError::Conflict { .. })
        ));

        let rebuilt = policy
            .set_override(&change(
                "show_visual",
                snapshot.policy_revision,
                Some(false),
                McpDescriptionPatch::Unchanged,
                "op-2",
            ))
            .unwrap();
        assert!(!rebuilt.policy_unreadable);
        assert!(rebuilt.policy_revision > snapshot.policy_revision);
        assert!(policy.admit("whoami", true).admitted);
        assert!(!policy.admit("show_visual", true).admitted);
        let kept: Vec<_> = std::fs::read_dir(root.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .filter(|name| name.starts_with("mcp-tool-settings.json.unreadable-"))
            .collect();
        assert_eq!(kept.len(), 1, "the damaged file is kept for diagnosis");
        assert!(
            !McpToolPolicy::open(root.path())
                .snapshot()
                .policy_unreadable
        );
    }
}

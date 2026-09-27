//! Typed device-update facade. UIKit never decodes relay payloads.
use super::{CoreClient, CoreResult, on_runtime};
use std::sync::Arc;
use zeron_client::updates as u;
use zeron_proto::{HarnessUpdatePhase as P, HarnessUpdatePolicy as Policy};

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum AgentUpdatePolicy {
    Notify,
    AutoWhenIdle,
    Off,
}
impl From<AgentUpdatePolicy> for Policy {
    fn from(p: AgentUpdatePolicy) -> Self {
        match p {
            AgentUpdatePolicy::Notify => Self::Notify,
            AgentUpdatePolicy::AutoWhenIdle => Self::AutoWhenIdle,
            AgentUpdatePolicy::Off => Self::Off,
        }
    }
}
impl From<Policy> for AgentUpdatePolicy {
    fn from(p: Policy) -> Self {
        match p {
            Policy::Notify => Self::Notify,
            Policy::AutoWhenIdle => Self::AutoWhenIdle,
            Policy::Off => Self::Off,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum AgentUpdateConnection {
    Loading,
    Ready,
    Offline,
    Unsupported,
    Removed,
    Reconnecting,
    Closed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum AgentUpdatePhase {
    Dormant,
    Checking,
    Current,
    Available,
    WaitingForIdle,
    Preparing,
    Downloading,
    Installing,
    Verifying,
    Updated,
    ManualActionRequired,
    Failed,
}
impl From<P> for AgentUpdatePhase {
    fn from(p: P) -> Self {
        match p {
            P::Dormant => Self::Dormant,
            P::Checking => Self::Checking,
            P::Current => Self::Current,
            P::Available => Self::Available,
            P::WaitingForIdle => Self::WaitingForIdle,
            P::Preparing => Self::Preparing,
            P::Downloading => Self::Downloading,
            P::Installing => Self::Installing,
            P::Verifying => Self::Verifying,
            P::Updated => Self::Updated,
            P::ManualActionRequired => Self::ManualActionRequired,
            P::Failed => Self::Failed,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AgentUpdateRow {
    pub harness: String,
    pub name: String,
    pub installed_version: Option<String>,
    pub latest_version: Option<String>,
    pub policy: AgentUpdatePolicy,
    pub phase: AgentUpdatePhase,
    pub progress_message: Option<String>,
    pub error: Option<String>,
    pub manual_command: Option<String>,
    pub can_apply: bool,
    pub can_cancel: bool,
    pub can_set_policy: bool,
    pub can_dismiss: bool,
    pub pending: bool,
}
impl From<u::UpdateRow> for AgentUpdateRow {
    fn from(r: u::UpdateRow) -> Self {
        let s = r.status;
        let harness = serde_json::to_value(s.harness)
            .expect("harness serializes")
            .as_str()
            .unwrap()
            .to_owned();
        Self {
            name: zeron_client::catalog::harness_label(&harness),
            harness,
            installed_version: s.installed_version,
            latest_version: s.latest_version,
            policy: s.policy.into(),
            phase: s.phase.into(),
            progress_message: s.progress.and_then(|p| p.message),
            error: r.action_error.or(s.error.map(|e| e.message)),
            manual_command: s.manual_command,
            can_apply: r.can_apply,
            can_cancel: r.can_cancel,
            can_set_policy: r.can_set_policy,
            can_dismiss: r.can_dismiss,
            pending: r.pending,
        }
    }
}
#[derive(Debug, Clone, uniffi::Record)]
pub struct AgentUpdateSnapshot {
    pub device_id: String,
    pub device_name: String,
    pub revision: u64,
    pub connection: AgentUpdateConnection,
    pub rows: Vec<AgentUpdateRow>,
    pub watch_error: Option<String>,
    pub check_pending: bool,
    pub check_error: Option<String>,
}
#[derive(uniffi::Object)]
pub struct AgentUpdatesHandle {
    handle: Arc<u::UpdateHandle>,
}
#[uniffi::export]
impl CoreClient {
    pub fn open_agent_updates(&self, device_id: String) -> CoreResult<Arc<AgentUpdatesHandle>> {
        Ok(Arc::new(AgentUpdatesHandle {
            handle: self.client.open_agent_updates(&device_id)?,
        }))
    }
}
#[uniffi::export]
impl AgentUpdatesHandle {
    pub fn snapshot(&self) -> AgentUpdateSnapshot {
        let s = self.handle.snapshot();
        AgentUpdateSnapshot {
            device_id: s.device_id,
            device_name: s.device_name,
            revision: s.revision,
            connection: match s.connection {
                u::UpdateConnection::Loading => AgentUpdateConnection::Loading,
                u::UpdateConnection::Ready => AgentUpdateConnection::Ready,
                u::UpdateConnection::Offline => AgentUpdateConnection::Offline,
                u::UpdateConnection::Unsupported => AgentUpdateConnection::Unsupported,
                u::UpdateConnection::Removed => AgentUpdateConnection::Removed,
                u::UpdateConnection::Reconnecting => AgentUpdateConnection::Reconnecting,
                u::UpdateConnection::Closed => AgentUpdateConnection::Closed,
            },
            rows: s.rows.into_iter().map(Into::into).collect(),
            watch_error: s.watch_error,
            check_pending: s.check_pending,
            check_error: s.check_error,
        }
    }
    pub fn close(&self) {
        self.handle.close();
    }
    pub fn retry(&self) {
        self.handle.retry();
    }
    pub async fn check(&self) -> CoreResult<()> {
        self.action(None, u::UpdateAction::Check).await
    }
    pub async fn apply(&self, harness: String) -> CoreResult<()> {
        self.action(Some(harness), u::UpdateAction::Apply).await
    }
    pub async fn cancel(&self, harness: String) -> CoreResult<()> {
        self.action(Some(harness), u::UpdateAction::Cancel).await
    }
    pub async fn dismiss(&self, harness: String) -> CoreResult<()> {
        self.action(Some(harness), u::UpdateAction::Dismiss).await
    }
    pub async fn set_policy(&self, harness: String, policy: AgentUpdatePolicy) -> CoreResult<()> {
        self.action(Some(harness), u::UpdateAction::SetPolicy(policy.into()))
            .await
    }
}
impl AgentUpdatesHandle {
    async fn action(&self, harness: Option<String>, action: u::UpdateAction) -> CoreResult<()> {
        let id = harness
            .map(|h| serde_json::from_value(serde_json::Value::String(h)))
            .transpose()
            .map_err(|_| zeron_client::ClientError::InvalidArgument("Unknown agent".into()))?;
        let handle = self.handle.clone();
        on_runtime(async move { handle.act(id, action).await }).await
    }
}

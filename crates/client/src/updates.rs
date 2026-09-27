//! Device-scoped update observation and actions. Installations belong to the host;
//! view lifetime only owns observation. Mutations are never replayed on reconnect.
use crate::error::Result;
use crate::{Client, ClientError, client::ClientInner, lock};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use zeron_proto::{
    HarnessId, HarnessUpdatePhase as Phase, HarnessUpdatePolicy as Policy, HarnessUpdateStatus,
};
use zeron_rpc::methods;

pub const CAPABILITY: &str = "harness-updates-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateConnection {
    Loading,
    Ready,
    Offline,
    Unsupported,
    Removed,
    Reconnecting,
    Closed,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateRow {
    pub status: HarnessUpdateStatus,
    pub can_apply: bool,
    pub can_cancel: bool,
    pub can_set_policy: bool,
    pub can_dismiss: bool,
    pub pending: bool,
    pub action_error: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateSnapshot {
    pub device_id: String,
    pub device_name: String,
    pub revision: u64,
    pub connection: UpdateConnection,
    pub rows: Vec<UpdateRow>,
    pub watch_error: Option<String>,
    pub check_pending: bool,
    pub check_error: Option<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateAction {
    Check,
    Apply,
    Cancel,
    Dismiss,
    SetPolicy(Policy),
}
// Policy is intentionally not a cycle: one selection means one mutation.

#[derive(Default)]
pub(crate) struct UpdateStore(Mutex<HashMap<String, Arc<UpdateCore>>>);
struct State {
    statuses: Vec<HarnessUpdateStatus>,
    revision: u64,
    fresh: bool,
    error: Option<String>,
    pending: HashSet<String>,
    errors: HashMap<String, String>,
    viewers: usize,
    gate: UpdateConnection,
    unsupported: bool,
    generation: u64,
    watch: CancellationToken,
}
pub(crate) struct UpdateCore {
    client: Weak<ClientInner>,
    device: String,
    state: Mutex<State>,
    demo: Option<Arc<demo::DemoUpdates>>,
}
pub struct UpdateHandle {
    core: Arc<UpdateCore>,
    attached: std::sync::atomic::AtomicBool,
}
impl Drop for UpdateHandle {
    fn drop(&mut self) {
        self.close();
    }
}
impl UpdateStore {
    pub(crate) fn refresh(&self, force: bool) {
        let cores: Vec<_> = lock(&self.0).values().cloned().collect();
        for core in cores {
            let restart = {
                let s = lock(&core.state);
                s.viewers > 0 && (force || s.gate != core.gate().1)
            };
            if restart {
                core.restart();
            }
        }
    }
}
impl Client {
    pub fn open_agent_updates(&self, device_id: &str) -> Result<Arc<UpdateHandle>> {
        if self.inner.cancel.is_cancelled() {
            return Err(ClientError::Closed);
        }
        if !self
            .workspace()
            .devices
            .iter()
            .any(|d| d.id == device_id && d.is_execution_host)
        {
            return Err(ClientError::NotFound(device_id.into()));
        }
        let core = lock(&self.inner.updates.0)
            .entry(device_id.into())
            .or_insert_with(|| {
                Arc::new(UpdateCore {
                    client: Arc::downgrade(&self.inner),
                    device: device_id.into(),
                    state: Mutex::new(State {
                        statuses: Vec::new(),
                        revision: 0,
                        fresh: false,
                        error: None,
                        pending: HashSet::new(),
                        errors: HashMap::new(),
                        viewers: 0,
                        generation: 0,
                        gate: UpdateConnection::Loading,
                        unsupported: false,
                        watch: CancellationToken::new(),
                    }),
                    demo: self
                        .inner
                        .is_demo()
                        .then(|| Arc::new(demo::DemoUpdates::new())),
                })
            })
            .clone();
        let start = {
            let mut s = lock(&core.state);
            s.viewers += 1;
            s.viewers == 1
        };
        if start {
            core.restart();
        }
        Ok(Arc::new(UpdateHandle {
            core,
            attached: std::sync::atomic::AtomicBool::new(true),
        }))
    }
}
fn cancellable(p: Phase) -> bool {
    matches!(
        p,
        Phase::WaitingForIdle | Phase::Preparing | Phase::Downloading
    )
}
fn key(h: Option<HarnessId>, action: UpdateAction) -> String {
    if matches!(action, UpdateAction::Check) {
        return "check".into();
    }
    format!(
        "{}:{}",
        h.map(|h| serde_json::to_value(h)
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned())
            .unwrap_or_default(),
        if matches!(action, UpdateAction::Cancel) {
            "cancel"
        } else {
            "action"
        }
    )
}
impl UpdateCore {
    fn gate(&self) -> (String, UpdateConnection) {
        let Some(client) = self.client.upgrade() else {
            return (self.device.clone(), UpdateConnection::Closed);
        };
        if client.cancel.is_cancelled() {
            return (self.device.clone(), UpdateConnection::Closed);
        }
        let ws = client.workspace.snapshot();
        let Some(device) = ws
            .devices
            .iter()
            .find(|d| d.id == self.device && d.is_execution_host)
        else {
            return (self.device.clone(), UpdateConnection::Removed);
        };
        let state = if !device.capabilities.iter().any(|c| c == CAPABILITY) {
            UpdateConnection::Unsupported
        } else if !device.online || !client.network_online() || !client.updates_foreground() {
            UpdateConnection::Offline
        } else {
            UpdateConnection::Ready
        };
        (device.name.clone(), state)
    }
    fn notify(&self, s: &mut State) {
        s.revision += 1;
        if let Some(c) = self.client.upgrade() {
            c.events.updates(&self.device, s.revision);
        }
    }
    fn publish(&self, generation: u64, result: Result<Vec<HarnessUpdateStatus>>) {
        let result = result.and_then(|rows| {
            let mut ids = HashSet::new();
            if rows.iter().any(|r| !ids.insert(r.harness)) {
                Err(ClientError::HostError(
                    "Invalid update status: duplicate agent".into(),
                ))
            } else {
                Ok(rows)
            }
        });
        let result = if result.is_ok() && self.gate().1 != UpdateConnection::Ready {
            Err(ClientError::HostUnavailable("Device disconnected".into()))
        } else {
            result
        };
        let mut s = lock(&self.state);
        if s.generation != generation || s.watch.is_cancelled() {
            return;
        }
        match result {
            Ok(rows) => {
                s.statuses = rows;
                s.fresh = true;
                s.error = None;
            }
            Err(error) => {
                s.unsupported = matches!(error, ClientError::Unsupported(_));
                s.fresh = false;
                s.error = Some(error.to_string());
            }
        }
        self.notify(&mut s);
    }
    fn restart(self: &Arc<Self>) {
        let (generation, cancel) = {
            let mut s = lock(&self.state);
            if s.viewers == 0 {
                return;
            }
            s.watch.cancel();
            s.generation += 1;
            s.fresh = false;
            s.unsupported = false;
            s.gate = self.gate().1;
            s.watch = self
                .client
                .upgrade()
                .map(|c| c.cancel.child_token())
                .unwrap_or_default();
            self.notify(&mut s);
            (s.generation, s.watch.clone())
        };
        let core = self.clone();
        crate::runtime::shared().spawn(async move {
            let mut backoff = Duration::from_millis(500);
            loop {
                if cancel.is_cancelled() { return; }
                let eligible = core.gate().1 == UpdateConnection::Ready;
                if eligible {
                    let result = tokio::select! {
                        _ = cancel.cancelled() => return,
                        result = core.observe(generation, &cancel) => result,
                    };
                    let unsupported = matches!(result, ClientError::Unsupported(_));
                    core.publish(generation, Err(result));
                    if unsupported { return; }
                } else {
                    core.publish(generation, Err(ClientError::HostUnavailable("Device is unavailable".into())));
                }
                tokio::select! { _ = cancel.cancelled() => return, _ = tokio::time::sleep(backoff) => {} }
                backoff = (backoff * 2).min(Duration::from_secs(5));
            }
        });
    }
    async fn observe(&self, generation: u64, cancel: &CancellationToken) -> ClientError {
        let Some(client) = self.client.upgrade() else {
            return ClientError::Closed;
        };
        let mut demo_rx = self.demo.as_ref().map(|d| d.rows.subscribe());
        let mut live_rx = if let Some(live) = client.live() {
            match tokio::time::timeout(
                Duration::from_secs(30),
                live.relay.watch_updates(&self.device),
            )
            .await
            {
                Ok(Ok(rx)) => Some(rx),
                Ok(Err(e)) => return e,
                Err(_) => return ClientError::HostUnavailable("Update watch timed out".into()),
            }
        } else {
            None
        };
        drop(client);
        if let Some(rx) = &demo_rx {
            self.publish(generation, Ok(rx.borrow().clone()));
        }
        loop {
            tokio::select! {
                _ = cancel.cancelled() => return ClientError::Closed,
                _ = tokio::time::sleep(Duration::from_millis(500)) => {
                    if self.gate().1 != UpdateConnection::Ready { return ClientError::HostUnavailable("Device disconnected".into()); }
                }
                result = async {
                    if let Some(rx) = &mut live_rx {
                        let value = rx.recv().await.ok_or_else(|| ClientError::HostUnavailable("Update watch disconnected".into()))?;
                        serde_json::from_value(value).map_err(|e| ClientError::HostError(format!("Invalid update status: {e}")))
                    } else if let Some(rx) = &mut demo_rx {
                        rx.changed().await.map_err(|_| ClientError::Closed)?;
                        Ok(rx.borrow_and_update().clone())
                    } else { Err(ClientError::Closed) }
                } => match result { Ok(rows) => self.publish(generation, Ok(rows)), Err(e) => return e }
            }
        }
    }
}
impl UpdateHandle {
    pub fn close(&self) {
        if !self
            .attached
            .swap(false, std::sync::atomic::Ordering::AcqRel)
        {
            return;
        }
        let mut s = lock(&self.core.state);
        s.viewers -= 1;
        if s.viewers == 0 {
            s.watch.cancel();
            s.fresh = false;
        }
    }
    pub fn snapshot(&self) -> UpdateSnapshot {
        let (device_name, gate) = self.core.gate();
        let s = lock(&self.core.state);
        let ready = gate == UpdateConnection::Ready && s.fresh;
        let connection = if s.unsupported {
            UpdateConnection::Unsupported
        } else if gate != UpdateConnection::Ready {
            gate
        } else if s.fresh {
            UpdateConnection::Ready
        } else if s.error.is_some() || !s.statuses.is_empty() {
            UpdateConnection::Reconnecting
        } else {
            UpdateConnection::Loading
        };
        let rows = s
            .statuses
            .iter()
            .map(|r| {
                let k = key(Some(r.harness), UpdateAction::Apply);
                let pending = s.pending.contains(&k);
                UpdateRow {
                    status: r.clone(),
                    pending,
                    can_apply: ready
                        && !pending
                        && r.can_apply
                        && (r.phase == Phase::Available
                            || (r.phase == Phase::Failed
                                && r.error.as_ref().is_some_and(|e| e.retryable))),
                    can_cancel: ready
                        && cancellable(r.phase)
                        && !s
                            .pending
                            .contains(&key(Some(r.harness), UpdateAction::Cancel)),
                    can_set_policy: ready && !s.pending.contains(&format!("{k}:policy")),
                    can_dismiss: ready
                        && !pending
                        && r.phase == Phase::Available
                        && r.latest_version.is_some(),
                    action_error: s.errors.get(&k).cloned(),
                }
            })
            .collect();
        UpdateSnapshot {
            device_id: self.core.device.clone(),
            device_name,
            revision: s.revision,
            connection,
            rows,
            watch_error: s.error.clone(),
            check_pending: s.pending.contains("check"),
            check_error: s.errors.get("check").cloned(),
        }
    }
    pub fn retry(&self) {
        if self.attached.load(std::sync::atomic::Ordering::Acquire) {
            self.core.restart();
        }
    }
    pub async fn act(&self, harness: Option<HarnessId>, action: UpdateAction) -> Result<()> {
        let snapshot = self.snapshot();
        if !self.attached.load(std::sync::atomic::Ordering::Acquire) {
            return Err(ClientError::Closed);
        }
        if snapshot.connection != UpdateConnection::Ready {
            return Err(ClientError::HostUnavailable(
                "Wait for fresh device status".into(),
            ));
        }
        let row = snapshot
            .rows
            .iter()
            .find(|r| Some(r.status.harness) == harness);
        let allowed = match action {
            UpdateAction::Check => !snapshot.check_pending,
            UpdateAction::Apply => row.is_some_and(|r| r.can_apply),
            UpdateAction::Cancel => row.is_some_and(|r| r.can_cancel),
            UpdateAction::Dismiss => row.is_some_and(|r| r.can_dismiss),
            UpdateAction::SetPolicy(_) => row.is_some_and(|r| r.can_set_policy),
        };
        if !allowed {
            return Err(ClientError::InvalidArgument(
                "Action is not available for this agent".into(),
            ));
        }
        let mut k = key(harness, action);
        if matches!(action, UpdateAction::SetPolicy(_)) {
            k.push_str(":policy");
        }
        {
            let mut s = lock(&self.core.state);
            if !s.fresh
                || self.core.gate().1 != UpdateConnection::Ready
                || row.is_some_and(|r| !s.statuses.contains(&r.status))
            {
                return Err(ClientError::HostUnavailable(
                    "Agent status changed; refresh before retrying".into(),
                ));
            }
            if !s.pending.insert(k.clone()) {
                return Err(ClientError::InvalidArgument(
                    "Action already pending".into(),
                ));
            }
            s.errors.remove(&key(harness, UpdateAction::Apply));
            if action == UpdateAction::Check {
                s.errors.remove("check");
            }
            self.core.notify(&mut s);
        }
        let core = self.core.clone();
        let version = row.and_then(|r| r.status.latest_version.clone());
        crate::runtime::shared().spawn(async move {
            let Some(client) = core.client.upgrade() else { return Err(ClientError::Closed) };
            let request = async {
                if core.gate().1 != UpdateConnection::Ready { return Err(ClientError::HostUnavailable("Device disconnected".into())); }
                if let Some(demo) = &core.demo { return demo.act(harness, action).await; }
                let (method, params, timeout) = match action {
                    UpdateAction::Check => (methods::CHECK_HARNESS_UPDATES, json!({"harness":harness}), 120),
                    UpdateAction::Apply => (methods::APPLY_HARNESS_UPDATE, json!({"harness":harness}), 1200),
                    UpdateAction::Cancel => (methods::CANCEL_HARNESS_UPDATE, json!({"harness":harness}), 30),
                    UpdateAction::Dismiss => (methods::DISMISS_HARNESS_UPDATE, json!({"harness":harness,"version":version}), 30),
                    UpdateAction::SetPolicy(policy) => (methods::SET_HARNESS_UPDATE_POLICY, json!({"harness":harness,"policy":policy}), 30),
                };
                let live = client.live().ok_or(ClientError::Closed)?;
                match live.relay.call_once(&core.device, method, params, Duration::from_secs(timeout)).await {
                    Ok(v) if action == UpdateAction::Cancel && v["cancelled"] != true => Err(ClientError::HostError("Installation has already started and cannot be cancelled".into())),
                    Ok(_) => Ok(()),
                    Err(ClientError::HostError(e)) if action == UpdateAction::Apply && e == "ApplyHarnessUpdate: update cancelled" => Ok(()),
                    Err(e) => Err(e),
                }
            };
            let result = tokio::select! { _ = client.cancel.cancelled() => Err(ClientError::Closed), r = request => r };
            let ambiguous = matches!(result, Err(ClientError::HostUnavailable(_)));
            {
                let mut s = lock(&core.state);
                s.pending.remove(&k);
                if let Err(e) = &result { s.errors.insert(if action == UpdateAction::Check { "check".into() } else { key(harness, UpdateAction::Apply) }, e.to_string()); }
                if ambiguous { s.fresh = false; }
                core.notify(&mut s);
            }
            if ambiguous && lock(&core.state).viewers > 0 { core.restart(); }
            result
        }).await.map_err(|e| ClientError::Internal(e.to_string()))?
    }
}

mod demo;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_watch_generation_cannot_reenable_actions() {
        let dir = tempfile::tempdir().unwrap();
        let client = Client::new(
            crate::ClientConfig::new("https://edge.invalid", dir.path()),
            crate::Credentials::Demo(Default::default()),
            Arc::new(crate::events::NullListener),
        )
        .unwrap();
        let h = client.open_agent_updates("dev-mac").unwrap();
        let generation = lock(&h.core.state).generation;
        h.close();
        h.core.restart();
        assert!(
            lock(&h.core.state).watch.is_cancelled(),
            "a racing reconnect must not revive a detached watch"
        );
        h.core.publish(generation, Ok(Vec::new()));
        assert!(!lock(&h.core.state).fresh);
        let next = client.open_agent_updates("dev-mac").unwrap();
        next.core
            .publish(generation, Err(ClientError::Internal("obsolete".into())));
        assert_ne!(lock(&next.core.state).error.as_deref(), Some("obsolete"));
        client.shutdown();
    }
}

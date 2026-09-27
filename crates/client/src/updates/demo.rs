//! Deterministic host fixture. Its state outlives view subscriptions.
use super::*;
use zeron_proto::{HarnessInstallSource, HarnessUpdateFailure};
pub(super) struct DemoUpdates {
    active: Mutex<HashMap<HarnessId, (u64, bool, Phase)>>,
    sequence: std::sync::atomic::AtomicU64,
    pub rows: tokio::sync::watch::Sender<Vec<HarnessUpdateStatus>>,
}
impl DemoUpdates {
    pub fn new() -> Self {
        let row = |harness, installed: &str, latest: Option<&str>, phase, can_apply| {
            HarnessUpdateStatus {
                harness,
                installed_version: Some(installed.into()),
                latest_version: latest.map(str::to_owned),
                channel: None,
                source: HarnessInstallSource::Vendor,
                policy: Policy::Notify,
                phase,
                progress: None,
                checked_at: Some(crate::now_ms()),
                error: None,
                can_apply,
                manual_command: None,
            }
        };
        let mut rows = vec![
            row(
                HarnessId::ClaudeCode,
                "2.1.4",
                Some("2.1.5"),
                Phase::Available,
                true,
            ),
            row(
                HarnessId::Codex,
                "0.155.0",
                Some("0.155.1"),
                Phase::Available,
                true,
            ),
            row(HarnessId::Hermes, "0.3.0", None, Phase::Available, true),
            row(
                HarnessId::Opencode,
                "1.2.3",
                Some("1.2.4"),
                Phase::Failed,
                true,
            ),
            row(
                HarnessId::Antigravity,
                "0.1.0",
                None,
                Phase::ManualActionRequired,
                false,
            ),
            row(HarnessId::Pi, "0.60.0", None, Phase::Current, false),
        ];
        rows[3].error = Some(HarnessUpdateFailure { message: "The release server could not be reached. Try again when the connection is restored.".into(), retryable: true });
        rows[4].manual_command =
            Some("Update the pinned Antigravity ACP server on this host.".into());
        Self {
            active: Mutex::new(HashMap::new()),
            sequence: std::sync::atomic::AtomicU64::new(0),
            rows: tokio::sync::watch::channel(rows).0,
        }
    }
    pub async fn act(
        self: &Arc<Self>,
        harness: Option<HarnessId>,
        action: UpdateAction,
    ) -> Result<()> {
        if action == UpdateAction::Check {
            tokio::time::sleep(Duration::from_millis(200)).await;
            self.rows.send_modify(|rows| {
                for r in rows {
                    r.checked_at = Some(crate::now_ms());
                }
            });
            return Ok(());
        }
        let harness =
            harness.ok_or_else(|| ClientError::InvalidArgument("Agent required".into()))?;
        if action == UpdateAction::Apply {
            return self.install(harness, false).await;
        }
        let mut accepted = false;
        let mut schedule = false;
        {
            let mut active = lock(&self.active);
            self.rows.send_modify(|rows| {
                let Some(r) = rows.iter_mut().find(|r| r.harness == harness) else {
                    return;
                };
                match action {
                    UpdateAction::Cancel if cancellable(r.phase) => {
                        if let Some((_, _, phase)) = active.remove(&harness) {
                            r.phase = phase;
                            accepted = true;
                        }
                    }
                    UpdateAction::SetPolicy(p) => {
                        r.policy = p;
                        accepted = true;
                        if let Some((_, automatic, phase)) = active.get(&harness).copied()
                            && cancellable(r.phase)
                            && (p == Policy::Off || (automatic && p != Policy::AutoWhenIdle))
                        {
                            active.remove(&harness);
                            r.phase = phase;
                        }
                        if p == Policy::Off && !active.contains_key(&harness) {
                            r.phase = Phase::Dormant;
                        }
                        if p != Policy::Off && r.phase == Phase::Dormant {
                            r.phase = Phase::Available;
                        }
                        schedule =
                            p == Policy::AutoWhenIdle && r.phase == Phase::Available && r.can_apply;
                    }
                    UpdateAction::Dismiss if r.latest_version.is_some() => {
                        r.phase = Phase::Current;
                        accepted = true;
                    }
                    _ => {}
                }
            });
        }
        if !accepted {
            return Err(ClientError::HostError(
                "Action is no longer available".into(),
            ));
        }
        if schedule {
            let host = self.clone();
            crate::runtime::shared().spawn(async move {
                let _ = host.install(harness, true).await;
            });
        }
        Ok(())
    }
    async fn install(&self, harness: HarnessId, automatic: bool) -> Result<()> {
        let id = self
            .sequence
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut accepted = false;
        {
            let mut active = lock(&self.active);
            self.rows.send_modify(|rows| {
                let Some(r) = rows.iter_mut().find(|r| r.harness == harness) else {
                    return;
                };
                if active.contains_key(&harness) || (automatic && r.policy != Policy::AutoWhenIdle)
                {
                    return;
                }
                active.insert(harness, (id, automatic, r.phase));
                r.phase = Phase::WaitingForIdle;
                r.error = None;
                accepted = true;
            });
        }
        if !accepted {
            return Err(ClientError::HostError(
                "Agent already updating or policy changed".into(),
            ));
        }
        for (delay, phase) in [
            (2500, Phase::Preparing),
            (1000, Phase::Installing),
            (1000, Phase::Verifying),
            (600, Phase::Updated),
        ] {
            tokio::time::sleep(Duration::from_millis(delay)).await;
            let mut active = lock(&self.active);
            if active
                .get(&harness)
                .is_none_or(|(current, _, _)| *current != id)
            {
                return Ok(());
            }
            self.rows.send_modify(|rows| {
                let r = rows.iter_mut().find(|r| r.harness == harness).unwrap();
                r.phase = phase;
                if phase == Phase::Updated {
                    r.installed_version = r.latest_version.clone().or(r.installed_version.clone());
                }
            });
            if phase == Phase::Updated {
                active.remove(&harness);
            }
        }
        Ok(())
    }
}

use std::{sync::Arc, time::Duration};
use zeron_client::updates::{UpdateAction as A, UpdateConnection as C, UpdateHandle};
use zeron_client::{Client, ClientConfig, Credentials, DemoOptions, events::NullListener};
use zeron_proto::{HarnessId as H, HarnessUpdatePhase as P, HarnessUpdatePolicy as Policy};

fn demo() -> (Client, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let c = Client::new(
        ClientConfig::new("https://edge.invalid", dir.path()),
        Credentials::Demo(DemoOptions::default()),
        Arc::new(NullListener),
    )
    .unwrap();
    (c, dir)
}
async fn until(mut f: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(8), async {
        while !f() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}
async fn ready(c: &Client, id: &str) -> Arc<UpdateHandle> {
    let h = c.open_agent_updates(id).unwrap();
    until(|| h.snapshot().connection == C::Ready).await;
    h
}
#[tokio::test]
async fn cancel_versionless_update_preserves_availability_and_other_hosts() {
    let (c, _dir) = demo();
    let h = ready(&c, "dev-mac").await;
    let other = ready(&c, "dev-vps").await;
    let apply = tokio::spawn({
        let h = h.clone();
        async move { h.act(Some(H::Hermes), A::Apply).await }
    });
    until(|| {
        h.snapshot()
            .rows
            .iter()
            .any(|r| r.status.harness == H::Hermes && r.can_cancel)
    })
    .await;
    assert!(h.act(Some(H::Hermes), A::Apply).await.is_err());
    h.act(Some(H::Hermes), A::Cancel).await.unwrap();
    apply.await.unwrap().unwrap();
    until(|| {
        h.snapshot()
            .rows
            .iter()
            .any(|r| r.status.harness == H::Hermes && r.status.phase == P::Available && r.can_apply)
    })
    .await;
    let s = h.snapshot();
    let r = s
        .rows
        .iter()
        .find(|r| r.status.harness == H::Hermes)
        .unwrap();
    assert!(r.status.latest_version.is_none());
    assert!(!r.can_dismiss);
    assert_eq!(
        other
            .snapshot()
            .rows
            .iter()
            .find(|r| r.status.harness == H::Hermes)
            .unwrap()
            .status
            .phase,
        P::Available
    );
    c.shutdown();
}
#[tokio::test]
async fn explicit_policy_selection_and_notify_stop_only_automatic_work() {
    let (c, _dir) = demo();
    let h = ready(&c, "dev-mac").await;
    h.act(Some(H::ClaudeCode), A::SetPolicy(Policy::Off))
        .await
        .unwrap();
    until(|| h.snapshot().rows[0].status.phase == P::Dormant).await;
    h.act(Some(H::ClaudeCode), A::SetPolicy(Policy::AutoWhenIdle))
        .await
        .unwrap();
    until(|| h.snapshot().rows[0].can_cancel).await;
    h.act(Some(H::ClaudeCode), A::SetPolicy(Policy::Notify))
        .await
        .unwrap();
    until(|| h.snapshot().rows[0].status.phase == P::Available).await;
    let apply = tokio::spawn({
        let h = h.clone();
        async move { h.act(Some(H::Codex), A::Apply).await }
    });
    until(|| h.snapshot().rows[1].can_cancel).await;
    h.act(Some(H::Codex), A::SetPolicy(Policy::Notify))
        .await
        .unwrap();
    assert_eq!(h.snapshot().rows[1].status.phase, P::WaitingForIdle);
    apply.await.unwrap().unwrap();
    until(|| h.snapshot().rows[1].status.phase == P::Updated).await;
    assert_eq!(h.snapshot().rows[0].status.phase, P::Available);
    c.shutdown();
}
#[tokio::test]
async fn closing_views_and_cancelling_callers_does_not_cancel_installation() {
    let (c, _dir) = demo();
    let h = ready(&c, "dev-mac").await;
    let apply = tokio::spawn({
        let h = h.clone();
        async move { h.act(Some(H::Codex), A::Apply).await }
    });
    until(|| h.snapshot().rows[1].can_cancel).await;
    h.close();
    apply.abort();
    drop(h);
    tokio::time::sleep(Duration::from_secs(6)).await;
    let h = ready(&c, "dev-mac").await;
    assert_eq!(h.snapshot().rows[1].status.phase, P::Updated);
    c.shutdown();
    assert_eq!(h.snapshot().connection, C::Closed);
}
#[tokio::test]
async fn reconnect_requires_fresh_status_and_unsupported_hosts_are_read_only() {
    let (c, _dir) = demo();
    let h = ready(&c, "dev-mac").await;
    c.set_network_online(false);
    assert_eq!(h.snapshot().connection, C::Offline);
    assert!(!h.snapshot().rows[0].can_apply);
    assert!(h.act(Some(H::ClaudeCode), A::Apply).await.is_err());
    c.set_network_online(true);
    until(|| h.snapshot().connection == C::Ready).await;
    assert!(h.snapshot().rows[0].can_apply);
    let offline = c.open_agent_updates("dev-offline").unwrap();
    assert_eq!(offline.snapshot().connection, C::Offline);
    let unsupported = c.open_agent_updates("dev-studio").unwrap();
    assert_eq!(unsupported.snapshot().connection, C::Unsupported);
    assert!(unsupported.act(None, A::Check).await.is_err());
    assert!(c.open_agent_updates("missing").is_err());
    c.shutdown();
}

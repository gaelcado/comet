use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use zeron_client::updates::{UpdateAction, UpdateConnection};
use zeron_rpc::{RpcError, RpcReply, methods};

struct UpdateService {
    rows: tokio::sync::watch::Sender<serde_json::Value>,
    applies: AtomicUsize,
    watches: Arc<AtomicUsize>,
    cancel: tokio::sync::Notify,
}
fn status(phase: &str) -> serde_json::Value {
    serde_json::json!([{"harness":"hermes", "installedVersion":"0.3.0", "policy":"notify", "phase":phase, "canApply":true}])
}
struct WatchGuard(Arc<AtomicUsize>);
impl Drop for WatchGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}
#[async_trait::async_trait]
impl zeron_rpc::RpcService for UpdateService {
    async fn handle(&self, method: &str, params: serde_json::Value) -> Result<RpcReply, RpcError> {
        match method {
            methods::LIST_HARNESSES => Ok(RpcReply::Value(serde_json::json!([]))),
            methods::WATCH_HARNESS_UPDATES => {
                self.watches.fetch_add(1, Ordering::SeqCst);
                let guard = WatchGuard(self.watches.clone());
                let rx = self.rows.subscribe();
                Ok(RpcReply::Stream(Box::pin(futures::stream::unfold(
                    (rx, true, guard),
                    |(mut rx, first, guard)| async move {
                        if !first && rx.changed().await.is_err() {
                            return None;
                        }
                        let value = rx.borrow_and_update().clone();
                        Some((value, (rx, false, guard)))
                    },
                ))))
            }
            methods::APPLY_HARNESS_UPDATE => {
                assert_eq!(params["harness"], "hermes");
                self.applies.fetch_add(1, Ordering::SeqCst);
                self.rows.send_replace(status("waiting-for-idle"));
                self.cancel.notified().await;
                Err(RpcError::Failed("update cancelled".into()))
            }
            methods::CANCEL_HARNESS_UPDATE => {
                self.rows.send_replace(status("available"));
                self.cancel.notify_one();
                Ok(RpcReply::Value(serde_json::json!({"cancelled":true})))
            }
            _ => Err(RpcError::UnknownMethod(method.into())),
        }
    }
}
async fn eventually(mut f: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while !f() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn updates_live_watch_cancel_and_malformed_frames() {
    let edge = MockEdge::start().await;
    let host = HostRegistry::start(&edge).await;
    {
        let (mut device, _, _) = host_rows(Utc::now());
        device
            .capabilities
            .push(zeron_client::updates::CAPABILITY.into());
        host.doc.lock().unwrap().upsert_device(&device).unwrap();
        host.client.nudge();
    }
    let service = Arc::new(UpdateService {
        rows: tokio::sync::watch::channel(status("available")).0,
        applies: AtomicUsize::new(0),
        watches: Arc::new(AtomicUsize::new(0)),
        cancel: tokio::sync::Notify::new(),
    });
    let relay = zeron_rpc::HostRelay::spawn(
        zeron_rpc::HostRelayConfig::new(
            edge.edge_url(),
            HOST,
            Arc::new(zeron_rpc::StaticToken("t".into())),
        ),
        service.clone(),
        Arc::new(|_| true),
    );
    let dir = tempfile::tempdir().unwrap();
    let client = phone(&edge, dir.path());
    eventually(|| {
        edge.relay_host_connected(HOST)
            && client.workspace().device(HOST).is_some_and(|d| {
                d.online
                    && d.capabilities
                        .iter()
                        .any(|c| c == zeron_client::updates::CAPABILITY)
            })
    })
    .await;
    let h = client.open_agent_updates(HOST).unwrap();
    let second = client.open_agent_updates(HOST).unwrap();
    eventually(|| h.snapshot().connection == UpdateConnection::Ready).await;
    assert_eq!(
        service.watches.load(Ordering::SeqCst),
        1,
        "shared device subscription"
    );
    assert!(h.snapshot().rows[0].status.latest_version.is_none());
    let task = tokio::spawn({
        let h = h.clone();
        async move { h.act(Some(HarnessId::Hermes), UpdateAction::Apply).await }
    });
    eventually(|| h.snapshot().rows[0].can_cancel).await;
    h.act(Some(HarnessId::Hermes), UpdateAction::Cancel)
        .await
        .unwrap();
    task.await.unwrap().unwrap();
    eventually(|| h.snapshot().rows[0].can_apply).await;
    assert!(h.snapshot().rows[0].action_error.is_none());
    assert_eq!(service.applies.load(Ordering::SeqCst), 1);
    service
        .rows
        .send_replace(serde_json::json!({"unexpected":"payload"}));
    eventually(|| h.snapshot().watch_error.is_some()).await;
    assert!(!h.snapshot().rows[0].can_apply);
    service.rows.send_replace(status("available"));
    h.retry();
    eventually(|| h.snapshot().connection == UpdateConnection::Ready).await;
    let lost_reply = tokio::spawn({
        let h = h.clone();
        async move { h.act(Some(HarnessId::Hermes), UpdateAction::Apply).await }
    });
    eventually(|| service.applies.load(Ordering::SeqCst) == 2).await;
    edge.disconnect_device_clients(HOST);
    let result = tokio::time::timeout(Duration::from_secs(5), lost_reply)
        .await
        .unwrap()
        .unwrap();
    assert!(result.is_err());
    eventually(|| h.snapshot().connection == UpdateConnection::Ready).await;
    assert_eq!(
        service.applies.load(Ordering::SeqCst),
        2,
        "ambiguous apply must not be replayed"
    );
    h.close();
    assert_eq!(second.snapshot().connection, UpdateConnection::Ready);
    second.close();
    eventually(|| service.watches.load(Ordering::SeqCst) == 0).await;
    client.shutdown();
    drop(relay);
}

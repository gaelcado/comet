//! Bounded platform callbacks; no PCM or provider credentials cross UniFFI.
use super::*;
use std::collections::HashMap;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use tokio::sync::{mpsc, oneshot, watch};
use tokio_util::sync::CancellationToken;
use zeron_proto::voice::{VoiceRejection, remote::Sdp};
use zeron_voice_session::VoiceMediaEndpoint;

#[derive(Clone, Copy, uniffi::Enum)]
pub enum VoiceMediaOperation {
    Prepare,
    Offer,
    ApplyAnswer,
    SetMuted,
    Levels,
    Close,
}
#[derive(Clone, uniffi::Record)]
pub struct VoiceMediaRequest {
    pub request_id: u64,
    pub operation: VoiceMediaOperation,
    pub sdp: Option<String>,
    pub muted: bool,
}
#[uniffi::export(with_foreign)]
pub trait VoiceMediaListener: Send + Sync {
    fn on_request(&self, request: VoiceMediaRequest);
}
#[uniffi::export(with_foreign)]
pub trait VoiceSessionListener: Send + Sync {
    /// Ephemeral UI event. Do not log or persist transcript payloads here.
    fn on_voice_event(&self, event_json: String);
    fn on_voice_closed(&self, reason: Option<String>);
}
struct Reply {
    sdp: Option<String>,
    microphone: u16,
    speaker: u16,
}
struct PlatformMedia {
    listener: Arc<dyn VoiceMediaListener>,
    pending: Mutex<HashMap<u64, oneshot::Sender<Result<Reply, VoiceRejection>>>>,
    next: AtomicU64,
    closed: AtomicBool,
}
impl PlatformMedia {
    async fn request(
        &self,
        operation: VoiceMediaOperation,
        sdp: Option<String>,
        muted: bool,
    ) -> Result<Reply, VoiceRejection> {
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().unwrap();
            if self.closed.load(Ordering::Acquire) {
                return Err(VoiceRejection::InvalidLease);
            }
            if pending.len() >= 4 {
                return Err(VoiceRejection::Overflow);
            }
            pending.insert(id, tx);
        }
        struct Pending<'a>(&'a PlatformMedia, u64);
        impl Drop for Pending<'_> {
            fn drop(&mut self) {
                self.0.pending.lock().unwrap().remove(&self.1);
            }
        }
        let _pending = Pending(self, id);
        self.listener.on_request(VoiceMediaRequest {
            request_id: id,
            operation,
            sdp,
            muted,
        });
        tokio::time::timeout(std::time::Duration::from_secs(30), rx)
            .await
            .map_err(|_| VoiceRejection::DeviceUnavailable)?
            .map_err(|_| VoiceRejection::DeviceUnavailable)?
    }
}
#[async_trait::async_trait]
impl VoiceMediaEndpoint for PlatformMedia {
    async fn prepare(&self) -> Result<(), VoiceRejection> {
        self.request(VoiceMediaOperation::Prepare, None, true)
            .await
            .map(|_| ())
    }
    async fn offer(&self) -> Result<Sdp, VoiceRejection> {
        Sdp::new(
            self.request(VoiceMediaOperation::Offer, None, true)
                .await?
                .sdp
                .ok_or(VoiceRejection::Protocol)?,
        )
    }
    async fn apply_answer(&self, answer: Sdp) -> Result<(), VoiceRejection> {
        self.request(
            VoiceMediaOperation::ApplyAnswer,
            Some(answer.expose().into()),
            true,
        )
        .await
        .map(|_| ())
    }
    async fn set_muted(&self, muted: bool) -> Result<(), VoiceRejection> {
        self.request(VoiceMediaOperation::SetMuted, None, muted)
            .await
            .map(|_| ())
    }
    async fn levels(&self) -> Result<(u16, u16), VoiceRejection> {
        let r = self
            .request(VoiceMediaOperation::Levels, None, true)
            .await?;
        Ok((r.microphone, r.speaker))
    }
    fn close(&self) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        let pending = std::mem::take(&mut *self.pending.lock().unwrap());
        for (_, tx) in pending {
            let _ = tx.send(Err(VoiceRejection::InvalidLease));
        }
        self.listener.on_request(VoiceMediaRequest {
            request_id: 0,
            operation: VoiceMediaOperation::Close,
            sdp: None,
            muted: true,
        });
    }
}

#[derive(uniffi::Object)]
pub struct VoiceCall {
    media: Arc<PlatformMedia>,
    cancel: CancellationToken,
    muted: watch::Sender<bool>,
}
impl Drop for VoiceCall {
    fn drop(&mut self) {
        self.media.close();
        self.cancel.cancel();
    }
}
#[uniffi::export]
impl VoiceCall {
    pub fn set_muted(&self, muted: bool) {
        if !self.cancel.is_cancelled() {
            self.muted.send_replace(muted);
        }
    }
    pub fn stop(&self) {
        self.media.close();
        self.cancel.cancel();
    }
    /// Late native callbacks simply miss the retired request. No resume is possible.
    pub fn complete_media(
        &self,
        request_id: u64,
        success: bool,
        sdp: Option<String>,
        microphone: u16,
        speaker: u16,
    ) {
        if self.media.closed.load(Ordering::Acquire) {
            return;
        }
        if let Some(tx) = self.media.pending.lock().unwrap().remove(&request_id) {
            let reply = if success && sdp.as_ref().is_none_or(|s| s.len() <= 65_536) {
                Ok(Reply {
                    sdp,
                    microphone,
                    speaker,
                })
            } else {
                Err(VoiceRejection::DeviceUnavailable)
            };
            let _ = tx.send(reply);
        }
    }
}
#[uniffi::export]
impl CoreClient {
    /// Returns immediately. Callbacks are dispatched on the core runtime; the
    /// platform must hop to its media/UI executor and call complete_media.
    pub fn start_voice(
        &self,
        host_device_id: String,
        voice: Option<String>,
        media: Arc<dyn VoiceMediaListener>,
        listener: Arc<dyn VoiceSessionListener>,
    ) -> Arc<VoiceCall> {
        let platform = Arc::new(PlatformMedia {
            listener: media,
            pending: Default::default(),
            next: AtomicU64::new(0),
            closed: AtomicBool::new(false),
        });
        let cancel = self.client.voice_cancellation();
        let (muted, rx) = watch::channel(false);
        let handle = Arc::new(VoiceCall {
            media: platform.clone(),
            cancel: cancel.clone(),
            muted,
        });
        let client = self.client.clone();
        zc::runtime::handle().spawn(async move {
            let (events, mut receiver) = mpsc::channel(32);
            let forward_listener = listener.clone();
            let forward = tokio::spawn(async move {
                while let Some(event) = receiver.recv().await {
                    if let Ok(json) = serde_json::to_string(&event) {
                        forward_listener.on_voice_event(json);
                    }
                }
            });
            let operation = async {
                let control = client
                    .voice_transport(&host_device_id)
                    .await
                    .map_err(|_| VoiceRejection::RemoteHost)?;
                let config = zeron_proto::ChatConfig {
                    harness: zeron_proto::HarnessId::Codex,
                    model: None,
                    reasoning: None,
                    model_options: Default::default(),
                    sandbox: zeron_proto::SandboxLevel::WorkspaceWrite,
                };
                zeron_voice_session::run(
                    control,
                    platform.clone(),
                    config,
                    voice,
                    cancel.clone(),
                    events,
                    rx,
                )
                .await
            };
            let result = tokio::select! {biased;_=cancel.cancelled()=>Ok(()),r=operation=>r};
            platform.close();
            forward.abort();
            let _ = forward.await;
            listener.on_voice_closed(result.err().map(|reason| format!("{reason:?}")));
        });
        handle
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Listener(Mutex<Vec<u64>>);
    impl VoiceMediaListener for Listener {fn on_request(&self,r:VoiceMediaRequest){self.0.lock().unwrap().push(r.request_id);}}
    #[tokio::test]
    async fn close_releases_pending_callbacks_and_rejects_late_replies() {
        let listener=Arc::new(Listener(Mutex::new(vec![])));
        let media=Arc::new(PlatformMedia{listener:listener.clone(),pending:Default::default(),next:AtomicU64::new(0),closed:AtomicBool::new(false)});
        let clone=media.clone();let task=tokio::spawn(async move{clone.offer().await});
        tokio::task::yield_now().await;
        media.close();assert!(task.await.unwrap().is_err());
        assert!(media.pending.lock().unwrap().is_empty());
        let (muted,_)=watch::channel(false);
        let call=VoiceCall{media:media.clone(),cancel:CancellationToken::new(),muted};
        call.complete_media(1,true,Some("late SDP".into()),0,0);
        assert!(media.offer().await.is_err());
        assert_eq!(listener.0.lock().unwrap().iter().filter(|id|**id==0).count(),1);
    }
}

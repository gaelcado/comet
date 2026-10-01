//! Viewport-owned voice controller. No automatic reconnect or microphone resume.
use gpui::{Context, Task};
use gpui_tokio::Tokio;
use zeron_proto::voice::*;
use zeron_rpc::methods;
use crate::state::EngineHandle;
#[cfg(feature = "voice-experimental")]
mod session;

pub struct VoiceController {
    pub phase: VoicePhase,
    pub chat_id: Option<String>,
    pub reason: Option<VoiceRejection>,
    pub snapshot: Option<VoiceSnapshot>,
    pub partial: String,
    epoch: u64,
    task: Option<Task<()>>,
    cancellation: tokio_util::sync::CancellationToken,
}
impl Default for VoiceController {
    fn default() -> Self { Self { phase:VoicePhase::Closed,chat_id:None,reason:None,snapshot:None,partial:String::new(),epoch:0,task:None,cancellation:tokio_util::sync::CancellationToken::new() } }
}
impl VoiceController {
    pub fn begin(&mut self, engine:EngineHandle, chat_id:String, host:String, cx:&mut Context<Self>) {
        self.cancel(cx);
        self.phase=VoicePhase::Checking; self.chat_id=Some(chat_id.clone()); self.reason=None;
        let epoch=self.epoch;
        let request=StartVoice { chat_id,host_device_id:host,voice:None };
        let cancellation=self.cancellation.clone();
        let (events,mut event_rx)=tokio::sync::mpsc::channel(32);
        let query=Tokio::spawn(cx,async move {
            let eligibility:VoiceEligibility=tokio::time::timeout(std::time::Duration::from_secs(5),
                engine.client().call_as(methods::VOICE_ELIGIBILITY,serde_json::to_value(&request).unwrap()))
                .await.map_err(|_|VoiceRejection::Protocol)?.map_err(|_|VoiceRejection::Unsupported)?;
            if !eligibility.available { return Err(eligibility.reason.unwrap_or(VoiceRejection::Unsupported)); }
            if eligibility.ordinary_usage_allowed!=Some(true) { return Err(VoiceRejection::IncludedUsageUnavailable); }
            if !eligibility.credits_excluded { return Err(VoiceRejection::CreditExclusionUnverified); }
            if eligibility.format.is_none() { return Err(VoiceRejection::AudioFormatUnverified); }
            if !eligibility.duplex_verified { return Err(VoiceRejection::DuplexUnverified); }
            #[cfg(feature = "voice-experimental")]
            { session::run(engine,request,cancellation,events).await }
            #[cfg(not(feature = "voice-experimental"))]
            { let _=(cancellation,events); Err::<(),_>(VoiceRejection::Disabled) }
        });
        self.task=Some(cx.spawn(async move |this,cx| {
            let receive=async {
                while let Some(event)=event_rx.recv().await {
                    if this.update(cx,|controller,cx| { if controller.epoch==epoch { controller.reduce(event,cx); } }).is_err() { break; }
                }
            };
            receive.await;
            let reason=query.await.unwrap_or(Err(VoiceRejection::Protocol)).err().unwrap_or(VoiceRejection::Protocol);
            let _=this.update(cx,|controller,cx| {
                if controller.epoch!=epoch { return; }
                controller.phase=VoicePhase::Failed; controller.reason=Some(reason); controller.task=None; cx.notify();
            });
        }));
        cx.notify();
    }
    pub fn cancel(&mut self,cx:&mut Context<Self>) {
        self.epoch=self.epoch.wrapping_add(1); self.cancellation.cancel();
        self.cancellation=tokio_util::sync::CancellationToken::new(); self.task=None;
        self.phase=VoicePhase::Closed; self.snapshot=None; self.partial.clear(); self.chat_id=None;
        cx.notify();
    }
    pub fn reduce(&mut self,event:VoiceEvent,cx:&mut Context<Self>) {
        match event {
            VoiceEvent::Snapshot { snapshot } => {
                if self.chat_id.as_deref()!=Some(snapshot.chat_id.as_str()) { return; }
                if self.snapshot.as_ref().is_some_and(|old|old.generation>snapshot.generation) { return; }
                self.phase=snapshot.phase; self.snapshot=Some(snapshot);
            }
            VoiceEvent::Partial { generation,text } if self.snapshot.as_ref().is_some_and(|s|s.generation==generation) => {
                if self.partial.len()+text.len()<=MAX_TRANSCRIPT_BYTES { self.partial.push_str(&text); }
            }
            VoiceEvent::Closed { generation,reason } if self.snapshot.as_ref().is_some_and(|s|s.generation==generation) => {
                self.phase=VoicePhase::Closed; self.reason=reason; self.partial.clear(); self.snapshot=None;
            }
            _=>return,
        }
        cx.notify();
    }
    pub fn reason_text(&self) -> &'static str {
        match self.reason {
            Some(VoiceRejection::CreditExclusionUnverified)=>"Voice is unavailable until Codex can guarantee use of included quota only.",
            Some(VoiceRejection::WrongHarness)=>"Voice requires a Codex chat.",
            Some(VoiceRejection::RemoteHost)=>"Voice requires a chat hosted on this device.",
            Some(VoiceRejection::IncludedUsageUnavailable)=>"Included Codex usage is unavailable.",
            Some(VoiceRejection::AudioFormatUnverified)=>"This Codex voice format has not been verified.",
            Some(_)=>"Voice is unavailable on this engine.",
            None=>"",
        }
    }
}

impl Drop for VoiceController { fn drop(&mut self) { self.cancellation.cancel(); } }

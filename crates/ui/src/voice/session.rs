//! Native audio runs on its own worker. Dedicated media RPC preserves control
//! progress on the normal engine connection. No audio reaches the UI renderer.
use std::sync::Arc;
use std::sync::atomic::{AtomicBool,Ordering};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use zeron_proto::voice::*;
use zeron_rpc::methods;
use crate::state::EngineHandle;

struct ProviderGuard { engine:EngineHandle,lease:VoiceLease }
impl Drop for ProviderGuard {
    fn drop(&mut self) {
        let engine=self.engine.clone(); let lease=self.lease.clone();
        if let Ok(runtime)=tokio::runtime::Handle::try_current() {
            runtime.spawn(async move { let _=tokio::time::timeout(Duration::from_secs(2),engine.client().call(methods::STOP_VOICE,serde_json::to_value(lease).unwrap())).await; });
        }
    }
}
enum AudioCommand { Start, Output(VoiceFrame), Mute(bool), Stop }
struct AudioGuard { commands:std::sync::mpsc::SyncSender<AudioCommand>, stopped:Arc<AtomicBool> }
impl Drop for AudioGuard {
    fn drop(&mut self) { self.stopped.store(true,Ordering::Release); let _=self.commands.try_send(AudioCommand::Stop); }
}

pub(super) async fn run(engine:EngineHandle, request:StartVoice, cancellation:CancellationToken, events:mpsc::Sender<VoiceEvent>) -> Result<(),VoiceRejection> {
    // Independent connections for media and owner stream, while Stop stays on
    // the normal control client. Both modes use identical JSON serialization.
    let media=engine.media_client().await.map_err(|_|VoiceRejection::Protocol)?;
    let owner_client=engine.media_client().await.map_err(|_|VoiceRejection::Protocol)?;
    let (commands,command_rx)=std::sync::mpsc::sync_channel(8);
    let stopped=Arc::new(AtomicBool::new(false));
    let _audio=AudioGuard { commands:commands.clone(),stopped:stopped.clone() };
    let (ready,ready_rx)=tokio::sync::oneshot::channel();
    let (capture,mut capture_rx)=mpsc::channel::<VoiceFrame>(MEDIA_QUEUE_FRAMES);
    // The lease generation is assigned by engine after preparation. Open
    // paused streams now; the thread starts capture only after Start succeeds.
    let (generation_tx,generation_rx)=std::sync::mpsc::sync_channel(1);
    std::thread::Builder::new().name("zeron-voice-audio".into()).spawn(move || {
        let mut worker=match zeron_audio::worker::AudioWorker::open(0) {
            Ok(worker)=> { let _=ready.send(Ok(())); worker },
            Err(_)=> { let _=ready.send(Err(VoiceRejection::DeviceUnavailable)); return; }
        };
        let generation=loop {
            if stopped.load(Ordering::Acquire) { return; }
            match generation_rx.recv_timeout(Duration::from_millis(20)) {
                Ok(generation)=>break generation,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected)=>return,
                Err(_)=>{},
            }
        };
        if worker.invalidate_playout(generation).is_err() { return; }
        let mut started=false;
        while !stopped.load(Ordering::Acquire) {
            match command_rx.recv_timeout(Duration::from_millis(5)) {
                Ok(AudioCommand::Start)=> { if worker.io.start().is_err() { break; } started=true; },
                Ok(AudioCommand::Output(frame))=> { if worker.io.enqueue(&frame).is_err() { break; } },
                Ok(AudioCommand::Mute(muted))=>worker.io.mute(muted),
                Ok(AudioCommand::Stop)=>break,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected)=>break,
                Err(_)=>{},
            }
            if started {
                match worker.poll() {
                    Ok(frames)=> for frame in frames { if capture.try_send(frame).is_err() { return; } },
                    Err(_)=>break,
                }
            }
        }
    }).map_err(|_|VoiceRejection::DeviceUnavailable)?;
    tokio::select! { biased; _=cancellation.cancelled()=>return Ok(()), result=ready_rx=>result.map_err(|_|VoiceRejection::DeviceUnavailable)?? }
    let lease:VoiceLease=tokio::select! {
        biased;
        _=cancellation.cancelled()=>return Ok(()),
        result=tokio::time::timeout(Duration::from_secs(8),engine.client().call_as(methods::START_VOICE,serde_json::to_value(&request).unwrap()))=>result.map_err(|_|VoiceRejection::Protocol)?.map_err(|_|VoiceRejection::Protocol)?,
    };
    let _provider=ProviderGuard { engine:engine.clone(),lease:lease.clone() };
    let mut owner=owner_client.subscribe_scoped(methods::OWN_VOICE,serde_json::to_value(&lease).unwrap()).await.map_err(|_|VoiceRejection::Protocol)?;
    generation_tx.try_send(lease.generation).map_err(|_|VoiceRejection::DeviceUnavailable)?;
    commands.try_send(AudioCommand::Start).map_err(|_|VoiceRejection::Overflow)?;
    loop {
        tokio::select! {
            biased;
            _=cancellation.cancelled()=>break,
            _=events.closed()=>break,
            event=owner.recv()=> {
                let event:VoiceEvent=serde_json::from_value(event.ok_or(VoiceRejection::Protocol)?).map_err(|_|VoiceRejection::Protocol)?;
                match event {
                    VoiceEvent::Audio { frame } if frame.generation==lease.generation=> { commands.try_send(AudioCommand::Output(frame)).map_err(|_|VoiceRejection::Overflow)?; },
                    VoiceEvent::Snapshot { ref snapshot }=> {
                        if snapshot.generation!=lease.generation { continue; }
                        commands.try_send(AudioCommand::Mute(snapshot.muted)).map_err(|_|VoiceRejection::Overflow)?;
                        events.try_send(event).map_err(|_|VoiceRejection::Overflow)?;
                    },
                    VoiceEvent::Closed { generation,.. } if generation==lease.generation=> { let _=events.try_send(event); break; },
                    VoiceEvent::Audio { .. }=>{},
                    VoiceEvent::InvalidatePlayout { .. }=>return Err(VoiceRejection::DuplexUnverified),
                    other=>events.try_send(other).map_err(|_|VoiceRejection::Overflow)?,
                }
            }
            frame=capture_rx.recv()=> {
                let frame=frame.ok_or(VoiceRejection::DeviceUnavailable)?;
                // Single in-flight append; buffers cannot grow while waiting.
                tokio::select! { biased; _=cancellation.cancelled()=>break,
                    reply=tokio::time::timeout(Duration::from_secs(2),media.call(methods::APPEND_VOICE,serde_json::to_value(AppendVoice { lease:lease.clone(),frame }).unwrap()))=> { reply.map_err(|_|VoiceRejection::Protocol)?.map_err(|_|VoiceRejection::Protocol)?; }
                }
            }
        }
    }
    Ok(())
}

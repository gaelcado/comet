//! One local voice owner per engine. Media and leases are never journaled.
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::Instant;
use zeron_proto::voice::*;

#[derive(Clone, Default)]
pub struct VoiceManager {
    inner: Arc<Inner>,
}
#[derive(Default)]
struct Inner {
    slot: Mutex<Option<Slot>>,
    generation: AtomicU64,
}
struct Slot {
    lease: VoiceLease,
    snapshot: VoiceSnapshot,
    attach_deadline: Instant,
    owner_attached: bool,
    events: Option<mpsc::Receiver<VoiceEvent>>,
    sender: mpsc::Sender<VoiceEvent>,
}

/// An owner stream owns this guard. Dropping the stream releases only this
/// generation; an old window cannot close its successor's microphone.
pub struct VoiceOwner {
    manager: VoiceManager,
    lease: VoiceLease,
    events: mpsc::Receiver<VoiceEvent>,
}
impl VoiceOwner {
    pub async fn next(&mut self) -> Option<VoiceEvent> {
        self.events.recv().await
    }
}
impl Drop for VoiceOwner {
    fn drop(&mut self) {
        let _ = self.manager.stop(&self.lease);
    }
}

impl VoiceManager {
    pub fn eligibility(&self) -> VoiceEligibility {
        VoiceEligibility {
            available: false,
            reason: Some(VoiceRejection::CreditExclusionUnverified),
            ordinary_usage_allowed: None,
            credits_excluded: false,
            format: None,
            duplex_verified: false,
        }
    }
    pub fn start(&self, _chat: &str) -> Result<VoiceLease, VoiceRejection> {
        // Intentionally no configuration override. This gate must be replaced
        // with a verified provider capability, covering delegated tasks too.
        Err(VoiceRejection::CreditExclusionUnverified)
    }
    #[cfg(test)]
    pub(crate) fn reserve(&self, chat: &str) -> Result<VoiceLease, VoiceRejection> {
        let mut slot = self.inner.slot.lock().unwrap();
        if slot.is_some() {
            return Err(VoiceRejection::Busy);
        }
        let generation = self.inner.generation.fetch_add(1, Ordering::AcqRel) + 1;
        let lease = VoiceLease {
            session_id: uuid::Uuid::new_v4().to_string(),
            generation,
            token: uuid::Uuid::new_v4().to_string(),
        };
        let snapshot = VoiceSnapshot {
            session_id: lease.session_id.clone(),
            chat_id: chat.into(),
            generation,
            phase: VoicePhase::Starting,
            muted: false,
            playing: false,
            work: VoiceWork::Idle,
            reason: None,
        };
        let (sender, events) = mpsc::channel(32);
        let _ = sender.try_send(VoiceEvent::Snapshot {
            snapshot: snapshot.clone(),
        });
        *slot = Some(Slot {
            lease: lease.clone(),
            snapshot,
            attach_deadline: Instant::now() + Duration::from_secs(5),
            owner_attached: false,
            events: Some(events),
            sender,
        });
        drop(slot);
        let manager = self.clone();
        let pending = lease.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(5)).await;
            manager.expire_unattached(&pending);
        });
        Ok(lease)
    }
    #[cfg(test)]
    fn expire_unattached(&self, lease: &VoiceLease) {
        let mut slot = self.inner.slot.lock().unwrap();
        if slot.as_ref().is_some_and(|s| {
            Self::matches(s, lease) && !s.owner_attached && Instant::now() >= s.attach_deadline
        }) {
            slot.take();
        }
    }
    fn matches(slot: &Slot, lease: &VoiceLease) -> bool {
        slot.lease.session_id == lease.session_id
            && slot.lease.generation == lease.generation
            && slot.lease.token == lease.token
    }
    pub fn own(&self, lease: VoiceLease) -> Result<VoiceOwner, VoiceRejection> {
        let mut state = self.inner.slot.lock().unwrap();
        let slot = state
            .as_mut()
            .filter(|s| Self::matches(s, &lease))
            .ok_or(VoiceRejection::InvalidLease)?;
        if slot.owner_attached || Instant::now() >= slot.attach_deadline {
            return Err(VoiceRejection::InvalidLease);
        }
        let events = slot.events.take().ok_or(VoiceRejection::InvalidLease)?;
        slot.owner_attached = true;
        Ok(VoiceOwner {
            manager: self.clone(),
            lease,
            events,
        })
    }
    pub fn mute(&self, lease: &VoiceLease, muted: bool) -> Result<(), VoiceRejection> {
        let mut state = self.inner.slot.lock().unwrap();
        let slot = state
            .as_mut()
            .filter(|s| Self::matches(s, lease))
            .ok_or(VoiceRejection::InvalidLease)?;
        slot.snapshot.muted = muted;
        if slot
            .sender
            .try_send(VoiceEvent::Snapshot {
                snapshot: slot.snapshot.clone(),
            })
            .is_err()
        {
            state.take();
            return Err(VoiceRejection::Overflow);
        }
        Ok(())
    }
    pub fn stop(&self, lease: &VoiceLease) -> Result<(), VoiceRejection> {
        let mut state = self.inner.slot.lock().unwrap();
        let Some(slot) = state.as_ref() else {
            return Ok(());
        };
        if !Self::matches(slot, lease) {
            return Err(VoiceRejection::InvalidLease);
        }
        let slot = state.take().unwrap();
        let _ = slot.sender.try_send(VoiceEvent::Closed {
            generation: lease.generation,
            reason: None,
        });
        Ok(())
    }
    pub fn restricts_origin(&self, chat_id: &str) -> bool {
        self.inner
            .slot
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|slot| slot.snapshot.chat_id == chat_id)
    }
    pub fn retire(&self) {
        self.inner.generation.fetch_add(1, Ordering::AcqRel);
        self.inner.slot.lock().unwrap().take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn exclusive_owner_drop_and_stale_guards() {
        let manager = VoiceManager::default();
        assert_eq!(
            manager.start("chat").unwrap_err(),
            VoiceRejection::CreditExclusionUnverified
        );
        let lease = manager.reserve("chat").unwrap();
        assert_eq!(manager.reserve("chat").unwrap_err(), VoiceRejection::Busy);
        let mut bad = lease.clone();
        bad.token = "bad".into();
        assert!(manager.own(bad).is_err());
        let owner = manager.own(lease.clone()).unwrap();
        assert!(manager.own(lease.clone()).is_err());
        drop(owner);
        let next = manager.reserve("chat").unwrap();
        assert_eq!(manager.stop(&lease), Err(VoiceRejection::InvalidLease));
        assert!(manager.own(next).is_ok());
    }
    #[tokio::test(start_paused = true)]
    async fn unattached_owner_watchdog_releases_reservation() {
        let manager = VoiceManager::default();
        let old = manager.reserve("chat").unwrap();
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(6)).await;
        tokio::task::yield_now().await;
        assert!(manager.own(old.clone()).is_err());
        let next = manager.reserve("next").unwrap();
        assert!(next.generation > old.generation);
        let owner = manager.own(next).unwrap();
        tokio::time::advance(Duration::from_secs(6)).await;
        tokio::task::yield_now().await;
        assert!(manager.restricts_origin("next"));
        drop(owner);
        assert!(!manager.restricts_origin("next"));
    }

    #[tokio::test]
    async fn account_retirement_invalidates_lease() {
        let manager = VoiceManager::default();
        let lease = manager.reserve("chat").unwrap();
        manager.retire();
        assert!(manager.own(lease).is_err());
    }
}

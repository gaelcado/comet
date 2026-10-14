//! What a call sounds like, for its orb: each of its two voices as how loud
//! a few bands of it are, lowest first.
//!
//! The user's voice is measured from the microphone itself
//! ([`zeron_voice::spectrum`]), beside the call's own capture: levels only,
//! and only while the call is live and the microphone is not muted. Codex's
//! voice never leaves the call's audio process, which reports only how loud
//! it is, so its bands are the rhythms of that loudness instead
//! ([`Rhythm`]): the phrase, the syllable, the attack.
use std::time::Instant;

pub(crate) use zeron_voice::spectrum::BANDS;
use zeron_voice::spectrum::Listener;

/// How loud each band is, 0–1, lowest first.
pub(crate) type Bands = [f32; BANDS];

/// The peak, in decibels, that reads as silence and the one that fills the
/// scale.
const SCALE: (f32, f32) = (-46.0, -8.0);
/// Seconds over which a phrase, a syllable and an attack are told apart.
const PACE: (f32, f32, f32) = (0.45, 0.14, 0.05);

/// A voice of which only the loudness is known, split by how fast that
/// loudness moves. The slow bands carry the phrase, the middle one the
/// level itself, the fast ones each syllable's attack and its flutter:
/// heavy to fine, as a spectrum's bands run from low to high.
#[derive(Default)]
struct Rhythm {
    phrase: f32,
    syllable: f32,
    quick: f32,
    at: Option<Instant>,
    bands: Bands,
}

impl Rhythm {
    fn hear(&mut self, peak: f32, now: Instant) {
        let loud = 20.0 * peak.max(1e-6).log10();
        let level = ((loud - SCALE.0) / (SCALE.1 - SCALE.0)).clamp(0.0, 1.0);
        let dt = self
            .at
            .replace(now)
            .map_or(0.0, |at| now.saturating_duration_since(at).as_secs_f32())
            .min(0.25);
        for (follower, pace) in [
            (&mut self.phrase, PACE.0),
            (&mut self.syllable, PACE.1),
            (&mut self.quick, PACE.2),
        ] {
            *follower += (level - *follower) * (1.0 - (-dt / pace).exp());
        }
        self.bands = [
            self.phrase,
            self.syllable,
            level,
            ((level - self.syllable) * 2.5).clamp(0.0, 1.0),
            ((level - self.quick).abs() * 4.0).clamp(0.0, 1.0),
        ];
    }
}

/// The two voices of a call.
#[derive(Default)]
pub(crate) struct Hearing {
    ears: Option<Listener>,
    user: Rhythm,
    agent: Rhythm,
}

impl Hearing {
    /// Listens to the microphone while the call is `open`, measuring nothing
    /// while it is `muted`, and lets go of it otherwise.
    pub(crate) fn follow(&mut self, open: bool, muted: bool) {
        if !open {
            *self = Self::default();
            return;
        }
        // Never from a test, and not when asked not to: without it the
        // user's voice is followed by its loudness, as Codex's is.
        if self.ears.is_none()
            && !cfg!(test)
            && std::env::var("ZERON_VOICE_SPECTRUM").as_deref() != Ok("0")
        {
            self.ears = Listener::start().ok();
        }
        if let Some(ears) = &self.ears {
            ears.pause(muted);
        }
    }

    /// The call's own meters: the peak of each voice, 0–1.
    pub(crate) fn levels(&mut self, microphone: f32, speaker: f32, now: Instant) {
        self.user.hear(microphone, now);
        self.agent.hear(speaker, now);
    }

    pub(crate) fn voice(&self) -> super::orb::Voice {
        super::orb::Voice {
            user: self
                .ears
                .as_ref()
                .and_then(Listener::levels)
                .unwrap_or(self.user.bands),
            agent: self.agent.bands,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn a_voice_known_only_by_its_loudness_is_split_by_its_rhythm() {
        let start = Instant::now();
        let mut rhythm = Rhythm::default();
        // Syllables, five a second, read every 40 ms as the call reports them.
        let mut attack = 0.0f32;
        for tick in 0..100u32 {
            let t = tick as f32 * 0.04;
            let peak = 0.004 + 0.3 * (t * 5.0 * std::f32::consts::TAU).sin().max(0.0);
            rhythm.hear(peak, start + Duration::from_millis(u64::from(tick) * 40));
            attack = attack.max(rhythm.bands[3]);
            // The phrase holds through the gaps between syllables…
            assert!(tick < 30 || rhythm.bands[0] > 0.2, "{:?}", rhythm.bands);
        }
        // …while each syllable has an attack of its own.
        assert!(attack > 0.5, "{attack}");
        // Silence empties every band, the slow ones last.
        for tick in 100..150u32 {
            rhythm.hear(0.0, start + Duration::from_millis(u64::from(tick) * 40));
        }
        assert!(
            rhythm.bands.iter().all(|band| *band < 0.02),
            "{:?}",
            rhythm.bands
        );
    }
}

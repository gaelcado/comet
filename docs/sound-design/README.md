# Rounded notification sounds

This contribution selects a compact four-cue product signature. It replaces the
existing completion and agent-question sounds, adds one shared attention cue for
run failures and durable connection outages, and preserves the Appshot capture cue
from the separate Appshots contribution.

- [`done.wav`](../../crates/ui/assets/sounds/done.wav): completion, settling
  rounded pair.
- [`request.wav`](../../crates/ui/assets/sounds/request.wav): agent question,
  rising rounded pair.
- [`attention.wav`](../../crates/ui/assets/sounds/attention.wav): failure or
  durable disconnection, restrained downward rounded pair.
- [`generate-notification-sounds.py`](../../scripts/generate-notification-sounds.py):
  reproduces the completion and question assets;
  `--variants-dir <directory>` also exports the two click-only alternatives.

Settings → Notifications has a master session-sounds switch and independent
choices for completion, input required, and errors/disconnections. Appshot capture
keeps its independent **Capture sound** setting on the Appshots page.

## Realtime voice calls

A voice call has two cues of its own. They keep the rounded clicks of the
microphone auditions (07, 08) and replace their notes with a chime rendered for
the call: each note is two voices a cent apart, spread across the stereo field,
with a struck onset over a longer body (or a slow swell with none), in a
short damped room.

- [`voice-start.wav`](../../crates/ui/assets/sounds/voice-start.wav): the call
  connected and Codex is listening; the light opening pair, then a single
  soft note (E) with its fifth glowing in above it instead of being struck:
  one note, not a melody. 0.9 s. It answers the wait between pressing the
  voice button and being heard.
- [`voice-end.wav`](../../crates/ui/assets/sounds/voice-end.wav): a connected
  call ended; the closing pair, then a plain falling fifth (D to G) in a sparer
  voice. 0.86 s.

Both sit at the family's level (about -20 dBFS peak) and stay under 500 Hz in
their fundamentals.

A start that fails, or a call that drops, plays `attention.wav` under the
errors/disconnections choice. A start abandoned before it connects is silent,
and so are mute and the call screen: they are frequent and already visible.
Settings → Voice has the independent **Call sounds** switch for the two cues.

## Additional auditions

[Ten additional variants](auditions/README.md) cover send, queue, upload ready,
completion, questions, attention, microphone on/off, reconnection and undo. Only
the attention audition is promoted to an application asset; the rest remain
references without triggers.

Earlier click-only alternatives: [completion](auditions/done-minimal.wav) and
[agent question](auditions/request-minimal.wav).

Run `python3 scripts/generate-sound-auditions.py --install-attention --install-voice`
to regenerate the ten numbered WAVs, their manifest, the attention asset and
the two voice call cues. All
synthesis uses Python's standard library and original rounded pressure pulses,
with no external samples.

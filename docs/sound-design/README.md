# Rounded notification sounds

This contribution is independent of Appshots. It replaces the existing completion
and agent-question sounds while preserving their triggers and settings.

- `crates/ui/assets/sounds/done.wav`: completion, settling rounded pair.
- `crates/ui/assets/sounds/request.wav`: agent question, rising rounded pair.
- `scripts/generate-notification-sounds.py`: reproduces those selected assets;
  `--variants-dir <directory>` also exports the two click-only alternatives.

## Additional auditions

[Ten additional variants](auditions/README.md) cover send, queue, upload ready,
completion, questions, attention, microphone on/off, reconnection and undo.
These are design candidates, not newly enabled sounds or triggers.

Earlier click-only alternatives: [completion](auditions/done-minimal.wav) and
[agent question](auditions/request-minimal.wav).

Run `python3 scripts/generate-sound-auditions.py` to regenerate the ten numbered
WAVs and their manifest. All synthesis uses Python's standard library and original
rounded pressure pulses, with no external samples.

The Appshot capture cue and its generator belong exclusively to
`publish/appshots`. This branch neither adds nor modifies that cue or the Appshots
feature. Both contributions are independently based on upstream.

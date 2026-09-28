# Synthetic speech fixtures

These PCM16 mono WAV files were generated with macOS text-to-speech for Zeron's
Parakeet verification on 2026-09-27, then converted to common microphone rates.
They contain no microphone recordings or personal speech.

- `english-48000.wav` (48 kHz): “Please update the Rust function, run cargo test,
  and open the GitHub pull request.”
- `french-44100.wav` (44.1 kHz): “Peux-tu modifier la fonction Rust, lancer cargo
  test, puis ouvrir la pull request sur GitHub ?”

The runtime test checks a stable phrase in each language instead of requiring
exact spelling of technical names. It exercises the production resampler and
pinned INT8 model on each desktop release architecture. It does not test a
physical microphone, OS permission prompts, or perceived UI behavior.

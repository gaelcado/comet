# Desktop dictation

Settings → Voice downloads NVIDIA Parakeet TDT 0.6B v3 for this device and enables the composer microphone. The optional INT8 ONNX download is **670,479,942 bytes**, derived from the pinned manifest, and is independent of the chat's engine host. Turn off hides dictation without deleting the model; Remove model disables it and removes the local cache.

Start/stop with the microphone or the rebindable **Start / stop dictation** shortcut in Settings → Shortcuts (default Cmd/Ctrl+Shift+D). Recording is limited to one minute. Stop transcribes locally into the selected draft range. It never invokes an agent or submits on its own. Explicit Send waits for finalization, then follows the normal Send/Queue path once. If no speech is recognized, the composer explains this and preserves the draft without sending. Esc cancels; cancelling or a failed/timed-out finalization never sends. Microphone permission is requested on first use; if the permission dialog takes focus, retry after granting permission.

The model detects the spoken language automatically and does not translate. Supported languages: Bulgarian, Croatian, Czech, Danish, Dutch, English, Estonian, Finnish, French, German, Greek, Hungarian, Italian, Latvian, Lithuanian, Maltese, Polish, Portuguese, Romanian, Russian, Slovak, Slovenian, Spanish, Swedish, Ukrainian. Technical names may need correction, especially in mixed-language speech.

No audio files are created by production dictation. Audio stays in a bounded, transient buffer on the desktop running the UI; it is not attached, logged, synchronized or sent to the coding-agent host. Dictated text is an ordinary editable draft and is handled like typed text. Model downloads contact Hugging Face; transcription does not use the network.

## Implementation and limits

`zeron-voice` owns downloads, model integrity, CPAL capture, anti-aliased conversion of device-rate audio to 16 kHz, and a single background Parakeet worker. `zeron-ui::dictation` owns permissions, local settings and the small transcription event interface. Composer inputs own draft-range/generation protection and grouped undo. A manual edit, selection move, IME update, focus leaving the composer, chat switch or queue-draft replacement invalidates the session. Attachments retain their existing behavior.

The production adapter performs offline final transcription on Stop. No live partials or native streaming are advertised. Editor partial-result behavior is tested through a deterministic adapter. Audio is capped at 60 seconds; UI finalization has a 30-second deadline. An in-flight native ONNX call cannot be forcibly interrupted safely: cancellation immediately stops capture and invalidates results; one worker keeps native work bounded and rejects overlapping sessions until it returns. Cached weights unload after 30 idle seconds or model removal.

Pinned runtime: `parakeet-rs = 0.3.8`, `ort/ort-sys = 2.0.0-rc.13`, ONNX Runtime **1.28.0**, CPU execution; `cpal = 0.17.3`, `rubato = 0.16.2`. The download manifest and conversion attribution are in `crates/voice/model.json` and `crates/voice/NOTICE.md`. The conversion pins immutable bytes but its publisher does not provide the source-weight revision or exact INT8 conversion tool version; this provenance limitation is explicit.

Release targets are macOS arm64, Linux x86_64/aarch64 and Windows x86_64. Linux CI installs ALSA headers. macOS uses AVFoundation only for permission, and its bundle includes microphone usage text and the hardened-runtime audio-input entitlement. The selected ONNX binary distribution has no Intel macOS build; Intel development requires an independently built matching ONNX Runtime. Windows' prebuilt ONNX archive links the system DirectML/DX12 libraries even though the chosen execution provider is CPU. The voice runtime workflow exercises real-model inference on all four release targets; desktop UI, package and physical-microphone validation are separate checks. iOS does not depend on this crate.

On Windows, microphone access for desktop apps must be enabled in the system privacy settings. On Linux, the selected ALSA input must be available to the user and the audio server. Settings → Voice lists available inputs and refreshes while visible; a disconnected selection falls back to the system default. Test built-in, USB and Bluetooth microphones on the target OS: a synthetic WAV test cannot establish permission, device routing or capture behavior.

## Verification

Deterministic editor fixtures cover partial replacement, Unicode, undo/redo, failure, stale events, cancellation, Send finalization, queue editing, navigation, focus and input-request takeover. `cargo test -p zeron-voice` covers pinned manifest shape/size, cancellation and corrupt cache rejection. The `verify` example runs explicitly supplied synthetic/public PCM16 WAV files and prints transcripts/timings for development only; production never prints transcripts.

`.github/workflows/voice.yml` runs the voice tests and the opt-in `tests/runtime.rs` smoke test on macOS arm64, Windows x86_64, and Linux x86_64/arm64. To reproduce the real-model test, set `ZERON_VOICE_TEST_MODEL_DIR` to an isolated cache directory and run `cargo test --release --locked -p zeron-voice --test runtime -- --ignored --nocapture`. It downloads the pinned model if necessary and checks English at 48 kHz, French at 44.1 kHz and silence. Fixtures are synthetic speech; no microphone recording is uploaded to CI.

Future mobile implementation is scoped in the [PR #591 handoff](https://github.com/zeronsh/zeron/pull/591#issuecomment-5869764738), including native capture, lifecycle cancellation, mobile inference benchmarks, UTF-16/UTF-8 selection handling and preservation of iOS delivery modes.

See the task evidence under `/Users/gaelcado/zeron/evidence/voice/` for actual runtime transcripts, hardware measurements and native build captures. Standalone synthetic model accuracy is distinct from a live microphone test.

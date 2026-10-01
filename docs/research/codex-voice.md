# Native Codex voice — experimental, no rollout

Target: codex-cli 0.159.0, experimental app-server protocol, local stdio
WebSocket media bridge. No public OpenAI API key or API fallback.

Sources checked 2026-10-01:
- https://learn.chatgpt.com/docs/app-server
- https://learn.chatgpt.com/docs/features/voice
- https://learn.chatgpt.com/docs/pricing

The desktop product shares Codex usage. That statement does **not** establish
an atomic included-subscription-only control for a third-party app-server client.
`ordinaryUsageAllowed` is nullable permission data, not a credit spending cap.
The 0.159.0 start schema has no included-only option. Therefore production start
must fail closed with `creditExclusionUnverified`; no local flag or environment
variable can waive this requirement. Account polling is not a replacement.

Acceptance gates:

| Gate | Status | Required evidence |
| --- | --- | --- |
| G0 protocol | schema inspected | native service format and version smoke |
| G1 cost | blocked | provider-enforced exclusion of purchased credits for voice and delegated tasks |
| G2 transcript | simulated | canonical identities and promotion observed live |
| G3 duplex | simulated | native interruption/context semantics and speakers/Bluetooth trials |
| G4 packaging | local Linux build | signed macOS, Windows and device/permission trials |

No live session is started by a build, test or the default probe. Until all gates
pass, this is an experimental implementation, not a subscription-only product.
Fixture peers never contact the provider. PCM16 little-endian mono at 24 kHz is
an internal test format; do not mistake it for a negotiated provider format.

Run `python3 scripts/codex-voice-probe.py` for a sanitized read-only inspection.
It sends only initialize, account/read, account/rateLimits/read and listVoices.
Unknown values stay unknown. No thread, inference, purchase or microphone starts.

## Delivered scope and remaining implementation

This patch supplies experimental foundations. The release build does not enable
`zeron-ui/voice-experimental`. Enabling that build feature exposes an eligibility
button; it does **not** bypass the engine's rejection. The current code cannot
make a live voice call, even for an account with remaining included usage.

Implemented and exercised offline:

- Local RPC contracts, generations, redacted leases, bounded media queues and
  isolated realtime notification routing on the existing app-server transport.
- Codex idle thread startup without an initial prompt, with the same MCP setup
  and process as text; text steering still works on that idle peer.
- Voice owner reservation/drop tests and fail-closed billing rejection through
  memory RPC and loopback WebSocket. No rejected start launches a task.
- Canonical transcript reducer as a standalone document utility; duplicate
  identities are ignored and equal phrases with distinct IDs remain distinct.
- Desktop CPAL streams, continuous resampling and bounded rings; WebRTC AEC3
  with the actually played output as reverse reference. Synthetic delayed echo
  and double-talk checks pass without opening hardware.
- Bezel's orb component copied into Zeron at the pinned source revision, with
  MIT notices, the existing GPUI revision, a 30 fps ceiling and visibility /
  reduced-motion animation gates. No Bezel dependency or checkout is needed.
- Composer suppression and draft retention driven by voice snapshots, with a
  central orb and separate mute, end-voice and stop-task controls.
- A prototype MCP origin policy and macOS microphone permission declarations.

Remaining work before this can provide voice to users:

1. Resolve G1 with an actual provider-enforced spending boundary, including
   delegated tasks. Account percentages and an available credit balance cannot
   serve as that boundary. Also verify G0's actual negotiated audio format.
2. Connect the engine manager to `SessionsEngine::take_voice_bridge`, prepare
   new chats without a submitted prompt, and implement native realtime/start
   behind those verified capabilities. Today both StartVoice and the harness
   Start command reject; there is no native realtime/start implementation.
3. Wire final events into the serialized document owner. Observe canonical
   item identities, BEM promotion and task correlation; the standalone reducer
   is not currently called from the running engine.
4. Implement actual provider lifecycle, account quota changes, delegated task
   status, update leases, interruption/item invalidation and played-context
   handling. Native barge-in is not supported by the current bridge; an
   invalidation event closes the prototype rather than faking success.
5. Complete trusted policy inheritance for nested/remote delegations and tasks
   continuing after voice ends. The present policy blocks mutations while a
   prototype owner exists; it does not yet enable voice orchestration.
6. Finish UI preparation/handshake, independent append and playout progress,
   device/voice selection, input questions, profile/route lifecycle and layout
   trials. Snapshot and draft unit tests are not a complete window/audio test.
7. Validate AEC timing, alias rejection, latency, device loss and permissions
   on real speakers/headphones/Bluetooth. Build and install signed packages on
   macOS and Windows. Only Linux compilation and offline DSP are verified.

The provider block must not be interpreted as the only remaining code change.
Opening that block alone would expose unfinished paths. All gates and remaining
integration must be completed together before changing availability or releases.

## Offline validation and build

On Linux, CPAL requires ALSA development headers; bundled WebRTC audio processing
requires a C++ compiler, Meson, Ninja, Clang/libclang and pkg-config. GPUI uses the
existing desktop prerequisites. The `Experimental native voice` CI workflow
installs those tools and runs fixture tests without credentials or inference.
The AEC is optional and included only with the experimental desktop UI feature.
The mobile dependency graph does not include CPAL or the audio crate.

Local checks on Linux (2026-10-01): harness unit suite, Codex/ACP fixture
integration, protocol/document/engine voice checks, existing MCP tests, codec /
resampling / synthetic AEC checks, UI reducer / draft / visual mapping tests,
orb geometry suite, and compilation with the experimental audio feature.
macOS plists parse and shell packaging scripts pass syntax checks. These are
build and simulation results, not live billing, provider or hardware evidence.

No tests start inference or use a public API key. No push or release is part of
this work. Manual provider validation must remain blocked until its spending
behavior can satisfy the user's included-subscription-only requirement.

## Audit corrections

The Astra high review identified and the patch corrected these offline runtime
regressions before approval:

- Preserve backpressure for text/ACP notifications instead of closing their
  shared peer on a burst. A subprocess fixture sends 600 events to a 256-slot
  channel, delays consumption and still receives the pending RPC response.
- Close child stdin after both writer queues are dropped; the health timer
  must not retain a peer indefinitely. The same fixture exits only on stdin EOF.
- Never retry a failed idle startup through the normal prompt dispatch path.
  Existing text startup retries retain their original behavior.
- Process and publish the first idle SessionStarted without starting a turn,
  creating a message or losing the native thread identity.
- Apply voice-origin delegation rejection before remote forwarding. Offline
  owner fixtures reject command/message/mutation before consulting a relay.

Engine fixtures cover the successful idle bootstrap and startup failure with an
engine-injected resume, asserting zero inference calls and zero user messages.
The successful bootstrap also stays Idle, stores the new thread identity and
publishes its SessionStarted. These tests use a synthetic Codex harness.

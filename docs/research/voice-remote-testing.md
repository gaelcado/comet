# Remote voice development testing

The implementation is opt-in. Real Mac/Fedora and physical iPhone acceptance is
pending and will be performed by the user. Compilation and fake-provider tests
are not evidence of subscription/media compatibility. No production rollout or
server deployment is part of this change.

## Desktop

Start the execution host's new Zeron engine/app with `ZERON_REMOTE_VOICE=1` in
its environment. Codex must already be installed and authenticated through
ChatGPT on that host. It needs no audio helper, microphone or speaker. Continue
using the existing registered device and account; do not register another host.

On macOS ARM64, build the client bundle with an explicit pinned runtime package:

```sh
ZERON_REMOTE_VOICE=1 \
ZERON_VOICE_RUNTIME_PACKAGE=/absolute/path/to/codex-0.160.0-package \
ZERON_DEV_BUILD_ONLY=1 ./scripts/run-macos-dev.sh
```

The package directory contains `codex-package.json` and `codex-resources/voice`.
The packaging script verifies the pinned artifact, copies the complete media
subtree and signs it with the app's identity. It does not put the Codex executable
or credentials in the client bundle. For direct developer runs,
`ZERON_VOICE_MEDIA_DIR` may explicitly point at a projected runtime containing
`zeron-runtime.json`. There is no automatic PATH fallback.

Open `target/macos-dev/Zeron Dev.app`, select **Settings → Voice → Codex voice
device**, choose the registered Fedora host and start voice from the sidebar orb.
The microphone permission belongs to this Mac. Host choice is local to these UI
settings and cannot change an active call. A host with an older build or disabled
remote gate is not eligible. Style is validated by the chosen host at Prepare.

With the flag unset, desktop retains its existing local Codex voice path. Disable
the flag and restart to roll back; no transcript migration or credential transfer
is needed. Do not remove the user's CLI to test the new path: use an isolated
client environment or copy the completed bundle to a test Mac without a CLI.

## Acceptance checklist

- Mac audio and Fedora Codex, including a host without an audio device/helper.
- Bidirectional conversation, interruption, mute and immediate local hang-up.
- Two successive calls and MCP delegation to a different provider, with return
  messages addressed to the complete orchestrator chat ID.
- One final transcript copy after sync; no active call or audio on other devices.
- Host shutdown, owner disconnect, half-open connection and late callbacks.
- Cancel while preparing/negotiating, then immediately start another call.
- 20 start/stop cycles, one 15-minute call and devices on separate networks.
- Open/reopen from Settings and drag the topbar; retain existing orb smoothing.

Record versions, anonymized platform/network labels, observed outcomes and
monotonic durations. Do not record SDP, tokens, audio or private transcript text.
A failed negotiation should be investigated using the opt-in split-host smoke
and its sanitized stages; it does not imply trying an API-key fallback.

## Automated coverage

`voice-tests.yml` includes the new media/coordinator crates and explicit opt-in
fake-provider remote-engine tests. These require no live provider or microphone.
The new V2 controls travel over the existing relay and remain ephemeral; no audio
frames or volume-meter stream are added to sync or the command ledger.

## iOS

Build the Rust core with `scripts/ios/build-core.sh iphonesimulator` (or `iphoneos`
for a physical device). Open `apps/ios/Zeron.xcodeproj`, use the **Zeron** scheme
and add the launch argument `-remote-voice`. Sign a physical-device build using
your existing development team. No Codex executable or OpenAI credentials are
installed in the iOS app.

In **More → Settings → Voice**, select an online registered execution host with
`voice-client-media-v1`, optionally choose a style and tap Start call. Microphone
permission is requested after checking the host capability. Mute and hang-up act
locally first. Navigating away retains the controls when you return; backgrounding,
audio interruption or losing a route ends the call without automatic restart.
The remote host also needs `ZERON_REMOTE_VOICE=1` and authenticated Codex.

Offline iOS lifecycle tests (no microphone permission or provider):

```sh
scripts/ios/build-core.sh iphonesimulator
ZERON_SKIP_CORE=1 xcodebuild -project apps/ios/Zeron.xcodeproj -scheme Zeron \
  -destination 'platform=iOS Simulator,name=Zeron iPhone 17 Pro' \
  ARCHS=arm64 ONLY_ACTIVE_ARCH=YES CODE_SIGN_IDENTITY=- \
  -only-testing:ZeronTests/RemoteVoiceLifecycleTests test
```

Use an available ARM64 simulator name on your machine. The Rust build script
currently creates an ARM64 simulator archive; a generic x86_64 simulator build
cannot link that archive. Keep simulator ad-hoc signing enabled so it can launch.

## Local validation record

On macOS ARM64, the iOS simulator build and all three native lifecycle tests
passed. Rust tests cover incompatible hosts before permission, cancellation
while an offer is stalled, a half-open heartbeat, local meters, owner drop,
duplicate prepare/negotiation, late controls and bounded UniFFI callbacks.
The full remote flow also passed with two engines over the WebSocket test relay
and a fake Codex package whose audio helper was removed. The test checks that
closing the owner releases the call without breaking the shared connection.

Live sound, Wi-Fi/cellular behavior, Bluetooth, battery/CPU, latency, cross-network
provider acceptance and real MCP delegation are still the user's acceptance tests.
The development flag stays opt-in until those pass.

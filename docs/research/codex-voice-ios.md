# Native iOS voice endpoint (development)

Physical-device acceptance is delegated to the user; G3/G4 remain pending.
The initial endpoint is foreground-only and does not contain Codex or credentials.

The dependency is pinned to [stasel/WebRTC 150.0.0](https://github.com/stasel/WebRTC/blob/150.0.0/Package.swift),
whose Swift package declares binary SHA-256
`f9890492b0016e4c88ab20f07867b8b420054caedc8a692b2ec6ac041f3cf6b2`.
Xcode verifies this artifact checksum and records the resolved revision. Retain
WebRTC's bundled notices when distributing (`Voice/WebRTC-LICENSE.txt` is copied
from the pinned XCFramework and included as an app resource). The package wraps the upstream native
SDK rather than reimplementing its codecs, echo cancellation or audio device.

Contract reference: the [pinned Codex helper transport](https://github.com/openai/codex/blob/a956835d020762cb2b570053af06f643a11c0ecc/codex-rs/voice-host/src/transport.rs)
adds an audio track, creates an ordered `oai-events` data channel, gathers ICE into
the offer, and waits for the channel to open after applying the answer. It discards
incoming channel message bodies and rejects remotely created channels. The iOS
endpoint follows those behaviors. This source inspection does not establish that
subscription negotiation, interruption or MCP work on an actual iPhone.

The endpoint keeps its audio track and manual audio device disabled until the
host confirms the lease. Mute disables the local track without a network roundtrip.
Close disables audio and releases the peer before remote cleanup. Backgrounding,
interruption, a lost audio route or failed connection closes the call; it never
resumes automatically. Cancellation invalidates pending continuations so late SDK
callbacks cannot return a second result or reactivate media.

Required live checks: iPhone on Wi-Fi and cellular, Fedora and Mac hosts, actual
bidirectional sound, barge-in/AEC, Bluetooth and route changes, background during
startup and active speech, denied permission, full-ID MCP delegation and unique
host-side transcripts. Measure package size, CPU and latency on the target device;
none are claimed by an offline build.

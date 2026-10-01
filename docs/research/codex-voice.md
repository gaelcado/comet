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

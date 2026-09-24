# Agent updates visual evidence

Source: fix/harness-update-lifecycle at 319a7210b5706614a9421a8cfac6b53ba6f7b713.
Captured 2026-09-25. Synthetic data only.

Desktop: production GPUI Shell rendered to PNG with appshots-fixture, debug profile, 960 × 720 point window. Frosted surface, default motion. Temporary fixture changes are recorded in fixture.patch and fixture.rs; these were removed from the contribution checkout after capture. These are GPUI framebuffer captures, without macOS window chrome. No live updater or animation smoothness claims.

Build: DEVELOPER_DIR=/Library/Developer/CommandLineTools CARGO_INCREMENTAL=0 cargo build -p zeron-ui --example agent-updates-fixture --features appshots-fixture
Executable: /tmp/pr389-AgentUpdates.app/Contents/MacOS/agent-updates-fixture
Runtime bundle identity verified: sh.zeron.agent-updates-fixture. Dark capture PID 67555; light capture PID 67714. Isolated temporary data directories, no engine connection or IPC listener. Protected pre-existing Zeron PIDs 21053 and 59711 were not targeted.
Executable SHA256: 6d032cbdba503006f5cc72381843f11134d539d8b9faf5389dfd37d3db2c6e6b

Mobile: unmodified iOS source at the same SHA; Xcode Debug build, iPhone 17 Pro / iOS 26.3 Simulator. App sh.zeron.ios. Launch arguments: -demo -route updates:dev-mac; -demo -sheet devices. PIDs 65108 and 65448. The app forces dark appearance. Native simctl screenshots, full screen.
Build: xcodebuild -project apps/ios/Zeron.xcodeproj -scheme Zeron -destination 'platform=iOS Simulator,id=21D6E476-0681-4087-845F-0E860E50CCEE' -derivedDataPath /tmp/pr389-ios-evidence build
Executable: /tmp/pr389-ios-evidence/Build/Products/Debug-iphonesimulator/Zeron.app/Zeron
Executable SHA256: a27743f1eaf4258421c9a0abada317b5716c3ad99523c702bf7de82798617874
Debug dylib SHA256: f5e65e5317407be208b4c341c9685fcb18ead985b392f23245659b90f23229ef

Reviewed states: compact summary; active glyph with no loading bar; expanded first 3.5 rows and bottom fade; scrolled list with top fade and thin scrollbar; mobile device picker and update sheet with available, waiting, failed, and manual states. All included images inspected. Failed initial fixture capture (reentrant render borrow) was corrected before capture. Old iOS cache lacked LoroFFI; fresh derived data build succeeded. iOS light capture excluded because application forces dark mode.

## Artifacts

- [dark/desktop-compact.png](dark/desktop-compact.png) — SHA256 `51f82146876ebe54e7b9e5403da6833838d7ac3db29b174241cf817c72a29699`
- [dark/desktop-active.png](dark/desktop-active.png) — SHA256 `f6b9880b9363ed030581a47fdc5fb7b9cbab2797088c450b6d55086fdf9a1d0e`
- [dark/desktop-expanded.png](dark/desktop-expanded.png) — SHA256 `1f3a452562f95bfded62a8235ef3a5c9a79de675032d125bd731a67a76960dd6`
- [dark/desktop-scrolled.png](dark/desktop-scrolled.png) — SHA256 `c3fbe7c78cacd73ce7dfa67ffa1f89d9ee1c1c219a8bbb1e969051248555900a`
- [light/desktop-expanded.png](light/desktop-expanded.png) — SHA256 `4e37fa7bf4f15031818f08d69cb94f96c16ee6da972800e3e974a673545a07d8`
- [light/desktop-scrolled.png](light/desktop-scrolled.png) — SHA256 `c01933a2ed6631d3a24eaef911f9a33ee82b8a1ab3659538df6ae756173ffb3d`
- [ios-devices.png](ios-devices.png) — SHA256 `8c41394bc70d56f84b50fee027ddcac0dafd1a34bf061d0b2208525d597bd9ad`
- [ios-updates-dark.png](ios-updates-dark.png) — SHA256 `82e21d5703de5faf0116839c107e1661721a7a82f2cd7dbb360825342a5e9eff`

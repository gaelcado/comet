# Agent updates in the rewritten iOS app

Status: approved by the user on 2026-09-27; full implementation requested.
Branch: `feat/ios-agent-updates`, explicitly based on #389 at `d3a66f32`
with user approval to implement before the dependency merges.
Dependency: https://github.com/zeronsh/zeron/pull/389 (open when inspected).

## Outcome

An iPhone user can inspect and control coding-agent CLI updates on a selected
execution host from Settings. No agent or installer runs on the phone. The host
owns policy, scheduling, installation, cancellation, and persistence.

PR #389's maintainer removed its old Swift implementation after the UIKit/Rust
rewrite (#570). This contribution rebuilds that experience using the new client
architecture. It preserves the previously requested explicit policy selection,
glyph-only activity, readable agent/device rows, native clipping, and reliable
cancellation and reconnect behavior.

## Dependency and scope

The user approved using #389 as an explicit dependency before its merge.
Implement and verify against `d3a66f32`; rebase onto main once #389 lands and
verify the final protocol then. Do not copy its engine changes or restore the
old Swift app.

Reuse `HarnessUpdateStatus`, `HarnessUpdatePolicy`, and the six existing RPCs:
`WatchHarnessUpdates`, `CheckHarnessUpdates`, `ApplyHarnessUpdate`,
`CancelHarnessUpdate`, `DismissHarnessUpdate`, and `SetHarnessUpdatePolicy`.
Gate access on the host's `harness-updates-v1` capability and fresh connectivity.
No protocol or engine semantic changes are planned.

Desktop behavior, app self-updates, push notifications, all-device aggregation,
and unrelated Settings redesign are outside this contribution.

## Approach

Use the existing Rust client relay, coalesced event delivery, and UniFFI facade.
A focused update module owns per-device state and requests; UIKit renders it.
This follows the rewrite's ownership rules and makes lifecycle tests independent
of view controllers. Swift-owned networking would duplicate reconnect and state
logic. Restoring the former SwiftUI screen would depend on deleted app models.
Neither alternative is selected.

### Client ownership and interface

Add a focused updates module to `crates/client`. Opening updates for a device
returns a handle with an immutable snapshot, revision, and typed actions. Share
one subscription per device across consumers. Last-view detach cancels the
observation subscription; accepted host updates continue. Keep pending action
ownership in the account-scoped client, independently of the view handle.
Sign-out shuts down local observation and requests without issuing Cancel to
the host. Reopening fetches current host state.

A snapshot includes device identity, load/connection state, status rows, and
separate watch/action errors. Row identity is device plus harness. Preserve the
last successful rows during transient failures, explicitly mark them stale, and
disable mutation until a fresh snapshot arrives. Reject unknown/removed hosts;
capability removal stops the watch. Never infer support from host version.

Reuse the relay's cancellable subscription and bounded reconnect backoff pattern.
Retry must be interruptible during connection setup, stream receive, and backoff.
Reject late frames from an obsolete subscription generation. Malformed frames
surface a recoverable error rather than leaving controls enabled with stale data.
Reconnection alone does not establish freshness.

Publish device-keyed, revision-only updates through the client's event pump;
consumers pull the current snapshot. Add a typed UniFFI handle, records, actions,
and event mapping in `crates/mobile/src/client_ffi`, then regenerate Swift bindings
with the repository build script. Keep transport types and raw JSON out of UIKit.

### Actions and correctness

The host is authoritative. Prevent duplicate in-flight actions for the same
operation/agent without blocking Cancel behind the long-running Apply request.
Derive action availability in Rust and enforce it again before sending.

- Apply requires a fresh supported host and the host's `can_apply` permission.
- Cancel is offered only in cancellable pre-install phases. A host rejection at
  the install boundary is authoritative; the client must not claim cancellation.
- An explicit host cancellation result is benign. Other failures remain visible.
- Policy is an explicit Notify / Automatic when idle / Off menu. Selecting Off
  sends Off directly. Host semantics decide which automatic work is cancelled.
- Versionless Available states, including Hermes commit updates, remain available
  and actionable. Never synthesize a version or use version presence as phase.
- Dismiss is offered only when the final RPC contract supports that release;
  versionless updates do not receive a fabricated dismissal version.
- Retry a watch safely. Never automatically replay an installation or policy
  mutation after an ambiguous transport failure; refresh status first.

## UIKit flow and layout

Make existing Settings device rows navigable to an Agent Updates screen, using
the selected host's name as visible context. Preserve online/offline information.
Offline and unsupported hosts open an explanatory state with disabled actions.

Use the existing inset-grouped UIKit list, Palette colors, Fonts, agent marks,
and status glyph. Each agent row has a clear name, compact installed-to-available
version line when known, readable status, and a primary action. Put policy and
secondary actions in a labeled menu; long errors/manual commands expand into
readable details with a copy action. Keep touch targets at least 44 points.

Use native list scrolling and indicators. Respect native grouped corners and
safe areas; avoid nested background layers bleeding outside the container.
Use glyph animation as the sole activity indicator, with status text and no
loading bar. Preserve row identity and scroll position during updates. Use small
list transitions, honor Reduce Motion, and support Dynamic Type and VoiceOver.
Do not impose the desktop card's 3.5-row cap on a full-screen mobile list.

States include initial loading, empty, current, available, waiting, preparing,
downloading, installing, verifying, updated, manual action, failed, disabled
monitoring, stale/offline, unsupported host, and watch error with Retry.

## Verification and acceptance

Extend the Rust demo host with deterministic update fixtures, using the same
client interface as live mode. Cover available versioned and versionless agents,
a waiting update, install/verify phases, retryable failure, manual instructions,
offline device, and unsupported host. Demo changes must not invoke real installers.

Rust client tests exercise host routing, capability/freshness gates, duplicate
requests, cancellation while Apply is pending, view teardown, reconnect and stale
frames, policy selection, versionless availability, and ambiguous transport errors.
Use existing test transport seams. Add focused UniFFI mapping checks where loss of
optional fields or action state could change behavior.

Run relevant client/mobile tests, `cargo fmt --all -- --check`, and a fresh Xcode
simulator build. Report any pre-existing failures separately and record tested SHA.
Simulator UI checks must cover device navigation, policy selection, Update/Cancel,
failure/Retry, and dismissal/reopening. Verify light/dark appearance, a narrow
phone, accessibility text size, long device/version/error strings, rounded
surfaces, VoiceOver labels, and Reduce Motion behavior.

Capture screenshots from the actual new build and identify source SHA, executable,
simulator/device, appearance, and demo conditions. Fixture evidence establishes
layout and interactions; it does not establish successful live installation.
A live smoke check may verify read-only watch/check against a capable host;
actual installation requires an explicitly selected test host and agent.

Acceptance: the new iOS app builds; the screen targets only the selected host;
state/actions survive navigation and reconnect correctly; focused tests pass;
and captured rows remain readable and clipped correctly in the specified states.

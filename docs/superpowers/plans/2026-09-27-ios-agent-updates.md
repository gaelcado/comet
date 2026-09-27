# iOS agent updates implementation plan

Design: ../specs/2026-09-27-ios-agent-updates-design.md
Execution: implement sequentially in this worktree, per the user's request to
implement the approved design entirely. The referenced writing-plans skill is
not installed; this document records the concrete steps directly.

1. Resolve the backend dependency. Main lacks #389's updater protocol. The
   approved design waits for merge; using the PR head instead requires the
   pending user decision. Record the dependency SHA if approved.
2. Add a focused Rust client update handle and snapshots. Reuse host capability
   and connectivity data, relay subscriptions, and the event pump. Preserve
   cached status with explicit freshness. Separate Cancel from pending Apply.
   Add a unary relay path without automatic mutation replay.
3. Extend the Rust demo backend through the same client interface. Add focused
   tests for targeting, freshness, lifecycle, policy, versionless availability,
   duplicate requests, and cancellation.
4. Add typed UniFFI records, handle methods, and event conversion. Regenerate
   bindings with the existing build tool.
5. Connect Settings device navigation to a native UIKit updates list. Reuse
   palette/fonts/glyphs. Add policy menu, primary actions, readable errors and
   manual instructions, empty/offline/unsupported states, and accessibility.
6. Run focused Rust tests and formatting checks; build the new iOS simulator
   app. Exercise update/cancel/retry and navigation in demo mode. Inspect and
   capture light/dark, narrow-phone and accessibility-text layouts.
7. Review the final diff, commit the implementation, and report tested SHA,
   artifacts, and remaining dependency/live-install limitations. External push
   or PR creation is not part of the current authorization.

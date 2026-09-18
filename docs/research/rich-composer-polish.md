# Rich composer polish

The composer uses Zeron's existing `zeron-syntax` Markdown grammar and embedded-language queries. Code spans are parsed in the background, remain relative to the original UTF-8 Markdown, and are mapped through the existing display projection. Changing the caret, width, or appearance does not reparse the draft. Unknown languages retain code styling; drafts over 128 KiB fall back to ordinary Markdown styling. Syntax work is capped at 16,000 spans.

This follows the architecture observed in [Zed's message editor](https://github.com/zed-industries/zed/blob/main/crates/agent_ui/src/message_editor.rs): its regular editor buffer is assigned Markdown from the language registry. Zeron reuses its own bundled grammars rather than importing Zed's editor or promising support for every language extension.

File, command, and skill completions share card, row typography, truncation, scroll insets, and overflow-dependent fades through `popover.rs`. Floating scrollbars remain outside the fade. Inline chips use proportional UI text, narrow nonbreaking padding, 16px icons, and a neutral file-icon seat. Their Markdown transport and atomic selection/undo behavior are unchanged.

## Companion ZUI change

The composer, user-message bubbles, tool-call/subagent/question cards, and Cmd+K palette share `Theme::SURFACE_CORNER_SMOOTHING` at `0.2` for gentle superellipse shaping. Larger radii preserve visible roundness: composer 32px (27px docked, clamped to fit), user messages 20px, tool cards 12px, question controls 13px, and Cmd+K 18px. Composer and palette backdrop masks use that same value. Attachment and Send share the same animated outer inset (8px compact, 12px expanded). The companion commit is `db1b72aea2d7c32a38c9db29f9afd29db9f395dc` on `feat/composer-corner-smoothing`, based on `c2d273dc3dadcb260b0fa7c35fc2fe02a14f5add`.

[gpui-ce PR #235](https://github.com/gpui-ce/gpui-ce/pull/235) demonstrates why corner treatment belongs in the renderer: fill, border, shadows, and blur need consistent masks. That PR uses Figma's Bézier construction and smoothing preservation. This implementation instead uses a continuous superellipse, interpolating exponent 2 (circle) to 4 (squircle). It preserves the existing radius footprint and the unsmoothed fast paths. It is not an exact port or an exact Figma match.

ZUI provides `.corner_smoothing()` for styled surfaces and explicit paint methods for custom elements. Existing `PaintQuad` literals and paint methods remain source-compatible. This API shapes surface chrome; it does not introduce descendant clipping or change image masks. The composer keeps its content inset from the corners.

The local `.cargo/config.toml` patches ZUI to the companion checkout. Before publishing the composer changes, publish the ZUI companion, pin its revision in the three workspace GPUI dependencies, remove the local patch, and regenerate `Cargo.lock`. The local patch must not be included in a public PR.

## Verification

Use `cargo check -p zeron-ui` and the composer/Markdown/popover unit tests. ZUI's Metal tests cover the changed corner footprint and matching backdrop mask; its WGSL test validates shader code and host struct offsets. Windows rendering needs its CI runner. Application screenshots and interaction QA must come from a rebuilt composer branch; an already-running older app is not evidence for these changes.

## Local handoff

- Composer checkout: `/Users/gaelcado/.zeron/worktrees/comet/quiet-onyx`, branch `zeron/composer-workstream-i-would-like-to`, base tip `28062b6f9674ef1a20cc736d679d4dcd1162769e`, with the follow-up work split into topic commits.
- ZUI checkout: `/Users/gaelcado/zeron/zui-corner-smoothing`, branch `feat/composer-corner-smoothing`, base tip `c2d273dc3dadcb260b0fa7c35fc2fe02a14f5add`, with companion changes committed as `db1b72aea2d7c32a38c9db29f9afd29db9f395dc`.
- The parallel review found a code-context boundary bug at EOF and with double-backtick spans; it is fixed with regression tests. A final review also caught unnecessary label truncation when a completion has no detail; labels now use the remaining row width in that case.
- No remote branches, issues or PRs were created. The prepared ZUI PR text lives in the workspace output directory at `output/composer-polish/zui-pr.md`.
- The already-running application was not rebuilt or relaunched. Full application appearance and interaction remain to be checked in a requested preview.

Validated final code with 109 composer-related tests, 26 picker tests, and 17 popover tests. The companion passed all 17 Metal tests and its WGSL validation/ABI test. Scoped rustfmt and `git diff --check` pass in both checkouts. Workspace-wide `cargo fmt --all -- --check` also reports pre-existing formatting differences outside this scope; those files were preserved.

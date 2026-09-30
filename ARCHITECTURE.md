# zeron — Architecture

A ground-up native rewrite of [zeron](../zeron) — a multi-device controller for coding agents
(Claude Code, Codex, Cursor, opencode, Pi, and ACP-native agents) — in Rust, with a gpui UI.
Fresh app; no backwards compatibility required.

**Pillars (from the goal):**
- Optional sync uses Loro CRDT docs (loro-mirror model) through Cloudflare Durable Objects; the same docs persist locally when sync is disabled.
- Durable Objects stay **TypeScript** (decision + evidence: `docs/research/durable-objects-language.md`).
  Everything device-side is Rust.
- Feature parity with zeron **except token-usage display** (poor fit for CRDTs; excluded).
- Frontend is **gpui** (pinned rev of `zeronsh/zui`, our fork of Zed's gpui). Virtualization +
  markdown techniques ported from **mugen + pretext** (`docs/research/mugen-pretext.md`).
- One binary, **headed or headless**. Smooth transitions/animations matching the original
  (catalog in `docs/research/feature-inventory.md` §1.12).

## 1. Topology (unchanged shape, new materials)

```
gpui UI ─ in-proc/localhost RPC ─ engine A ══ DeviceRoom DO relay ══ engine B ─ RPC ─ gpui UI
                    │       optional edge Worker: auth, rooms, R2        │
                    └── optional chat2 sync ──  ChatRoom DO (per chat) ──┘
                                          └─ Workspace registry room ────┘
```

- **Engine = backend** (was `@zeron/backend`): runs agents, owns auth, terminals, repos/worktrees,
  diff sync, doc hosting, project previews. Pure Rust daemon, fully functional headless.
- **UI = viewport** (was Electron): gpui app rendering engine state. Talks the same typed RPC whether the engine is in-process or a separate daemon. Organized around **spaces** — (device, folder) pairs, local or synced according to the active profile. The sidebar is the data: a recency-sorted Sessions list (status dots carry attention; pins and custom sections sync as registry rows), filtered by a searchable spaces dropdown ("All spaces" included) that also hosts space management. The horizontal tabs are a **device-local viewport** onto that list (`ui-settings.json` `openTabs`, cross-space): closing a tab is local-only — archiving is an explicit sidebar action — and a sidebar click (re)opens a session as a tab. The new-session canvas carries a space picker (defaulting to the sidebar filter, else the last selected space); new sessions are minted onto the picked space's device via relay-forwardable RPCs.
- **Viewer devices** (`apps/ios` on `zeron-mobile` → `zeron-client`): engine-free peers on the
  same mesh. They mirror the registry, join chat2 rooms, render session docs, and drive remote
  engines through the durable command ledger plus relay-forwarded host RPCs; no agent runs on
  them (`docs/mobile-rewrite.md`).
- **Edge (TypeScript, ported from zeron `apps/edge`)**: Worker + ChatRoom DO (per chat, the
  chat2 row protocol; the legacy SessionRoom DO remains deployed only for pre-cutover clients —
  no current client dials it) + RegistryRoom DO (per user) + DeviceRoom DO (per device) +
  PreviewRoom DO (preview signaling) + R2 attachments and release artifacts + WorkOS JWKS auth.
  Absorbs the old `apps/server` responsibilities (WorkOS code exchange/refresh, orgs) so
  **Postgres and the Hono server are gone**, and device control rides the DeviceRoom relay
  instead of WebRTC (only project previews use WebRTC DataChannels, signaled through the edge).

### Headed / headless
Single binary `zeron`:
- `zeron` — headed. If a local engine daemon is already listening on the IPC port, connect to it;
  otherwise run the engine **in-process** (RPC over an in-memory duplex — same protocol, zero
  serialization shortcuts, so the boundary stays honest) **and serve that same engine on the IPC
  port**. The embedded engine is not private: any other viewport can attach to the running app
  without it first being restarted as a daemon. Binding is best-effort — if the port is taken the
  window still opens, having lost only the ability to host peers.
- `zeron headless` — engine only. A clean installation immediately serves its local profile over localhost IPC; when a saved account selects the synced profile at startup and a bearer is available, it also hosts its DeviceRoom for remote control. A VPS can run this while a laptop's UI drives it.
- Other subcommands talk to the persisted session or the running engine's IPC and exit:
  `login`/`logout`/`status`, `sync` (live room introspection), `daemon` (launchd / systemd
  `--user` service), `update`, and `mcp` (a stdio MCP server proxying to the engine —
  `docs/mcp.md`).

### Local-first workspace profiles

Authentication and workspace selection are deliberately separate state machines:

- `AuthState` is live credential state: `SignedOut`, `NeedsOrganization`, or `SignedIn`. It may change after login, refresh, revocation, or logout.
- `WorkspaceScope` is the immutable storage and transport boundary captured once at engine startup: `Local`, `Synced`, or explicit `Development`.

The engine never re-resolves an open store because `AuthState` changed. This prevents a sign-in, token refresh, or revocation from silently swapping databases or attaching online transports to a runtime that started local-only.

| Startup condition | `WorkspaceScope` | Online transports |
| --- | --- | --- |
| WorkOS enabled, no parseable saved `session.json` | `Local` | Disabled |
| Parseable saved WorkOS session | `Synced` | Enabled when a bearer is available; organization onboarding completes before opening the store when needed |
| WorkOS disabled without a dev bearer | `Development` | Disabled |
| Explicit non-empty dev bearer | `Development` | Enabled |

`zeron login` and `zeron logout` operate on `session.json` while the engine is stopped. Login selects `Synced` for the next start; logout selects `Local` for the next start. The UI may update live authentication status, but a `WorkspaceScope` change always means a new engine: the headed app's sign-in wizard and sign-out stop the running engine and boot one on the other profile in place, falling back to quit-and-reopen when that fails.

The resolved profile selects the session snapshots, registry snapshot, run journals, and attachment cache that may contain workspace data:

| Scope | Store and journals | Uploads |
| --- | --- | --- |
| `Local` | `{data_dir}/profiles/local/` | `{data_dir}/profiles/local/uploads/` |
| `Synced` | `{data_dir}/orgs/{org_id}/{user_id}/` | `{data_dir}/orgs/{org_id}/{user_id}/uploads/` |
| `Development` | `{data_dir}/orgs/{org_id}/{user_id}/` | `{data_dir}/orgs/{org_id}/{user_id}/uploads/` |

The synced and development store roots preserve the historical cloud layout while their attachment caches are account-scoped. Local identity lives in `{data_dir}/local-profile.json`; its UUID is stable across restarts and is not an account or development identity.

Older releases wrote every synced and development attachment to `{data_dir}/uploads/`, and persisted those absolute paths in transcripts. On upgrade, the first synced or development account that opens this legacy cache claims it in `{data_dir}/legacy-uploads-owner.json`. That account may read the cache as a compatibility fallback, but all new staging and commits use its account-scoped uploads root; other accounts cannot read or write the legacy cache.

Device identity and machine resources remain device-scoped under the common data directory: `device-id`, repository registration, managed worktrees, agent credentials/accounts, and UI settings. They are available across profiles, but they do not contain or expose another profile's transcripts or attachments.

#### Privacy boundary and follow-ups

Signing in never uploads, links, or deletes local sessions by itself. The sign-in wizard offers a one-time import of the whole local profile into the synced one (`LocalImportStatus` / `ImportLocalWorkspace`, `crates/engine/src/local_import.rs`): chats, spaces, run journals, and command-ledger claims land through the live write paths, and re-running imports only what is missing. Attachments are not copied — the local upload root becomes a read-only jail root of that synced profile, re-armed at boot by `{data_dir}/local-import.json` — and otherwise stay jailed under the local upload root. Returning to local-only mode reopens the same local identity and data.

##### Remote workspace file trust boundary

Devices authenticated to the same synced account are trusted peers for remote workspace control. A peer may send relay-forwarded workspace file requests to the device that owns a checkout; the owning engine resolves the target and enforces workspace-relative path, containment, symlink, and write-conflict checks before touching its filesystem.

Ignored-file visibility is not an authorization boundary. A remote peer may request ignored entries and then read or write them, including potentially sensitive files such as `.env`, when `includeIgnored` is enabled. `.git` remains unavailable regardless of that option. Zeron intentionally does not maintain a filename denylist because it would be incomplete and could imply a security guarantee it cannot provide.

If authenticated devices must no longer trust one another with the full workspace, that policy must be enforced by the owning engine for remote requests. Hiding entries only in the UI is not a security control.

The following product work is intentionally deferred:

1. Per-session selection and copy between local and synced profiles (the existing import is whole-profile and one-way), including attachment copying, provenance, and conflict behavior.
2. Browsing both scopes simultaneously or switching the visible scope without restarting the engine.
3. A supported self-hosted backend contract covering authentication modes, room APIs, authorization, persistence, and blob storage. Current endpoint and bearer overrides remain development/deployment seams, not a promised compatibility surface.

## 2. Data model — Loro sessions, row-table registry

Two persistent state kinds, both snapshotted in the local `DocsStore`. When sync is enabled, session docs ride the chat2 row protocol (loro updates as append-only rows + Range-resumable checkpoints, ChatRoom DO — `docs/chat2-sync.md`) and the registry rides its own row-frame protocol (`docs/registry-sync.md`); local-only profiles persist the same state without joining rooms:

1. **Session doc** (per chat, Loro) — the transcript, the durable command ledger, and the
   editable message queue. Schema is a Rust port of `packages/session-doc` (same container
   names/shapes, so TS peers and the legacy SessionRoom's tail materializer stay compatible):
   `meta` map, `messages` list (parts as list-of-maps with **LoroText bodies** — the measured
   1.03× oplog shape; never LWW value rewrites), `commands` list with ledger rules 1–3
   (append-only per-device entries; host-only outcomes; dedupe/TTL/supersede evaluation), and
   `queue`, a movable list any device may edit or reorder while only the host takes from it
   (`docs/reference/message-queue.md`). Continuation splitting at 256KB, render-only tool parts
   (outputs folded to ≤160-char summaries; full inputs stay in the host's local run journal),
   host-published tail/diff sidecars. Constants live in `crates/doc/src/constants.rs`
   (`STREAM_COMMIT_MS=120`, tail 64, command TTL 24h); its flush/compaction/retention constants
   serve only the legacy SessionRoom.

2. **Workspace registry** (per profile) — not a CRDT: the `registry1` snapshot is a local replica of the per-user room, holding authoritative server rows plus pending local op batches stamped with an HLC and merged per field, last writer wins (`crates/doc/src/registry.rs` mirrors `edge/src/registry-core.ts`). Row kinds: spaces (id, deviceId, path, name?, gitDetected, checkoutId), the chats index (id, deviceId, title, archived, cwd, branch, checkoutId, spaceId, lastSeenAt, lastMessagePreview/At, config), devices, session-status rows, and sidebar state (`preferences`, `sidebarPins`, `sidebarSections`). A space is a device+folder pair in the active profile; the owning device's `SpacesSync` stamps git presence so branch pickers and the diff sidebar can gate without another RPC. Local scope keeps the registry entirely in its profile store. Synced and development scopes join `/registry/{orgId}/ws`, backed by the private per-user room `reg1/{orgId}/{userId}`; rows are never visible to every member of an organization.

   Writer discipline: each device writes its own device and session-status rows, rows for chats it hosts, and git stamps for spaces it owns. Creates, renames, archives, and seen marks are LWW sets accepted from any device. `deleteSpace` tombstones the space and every chat/session row in it in one commit. Presence uses ephemeral room frames rather than durable heartbeat writes.

   *Why one registry and not N tiny docs:* the sidebar needs one subscription for the whole list (grouping, resort animations, unseen markers). Its rows contain indexes rather than transcripts, so one local snapshot and, when enabled, one room connection remain bounded and cheap.

3. **Incremental projection** — `zeron-doc` owns hand-rolled typed read/write helpers over the
   schema (no `lorosurgeon`), and consumers project from `subscribe_root` events instead of
   re-hydrating per change — this is what fixes zeron's known O(transcript) re-projection
   inefficiency, remaining-work item 1a. `WatchDocMessages` ships `transcript_delta` frames (a
   full `reset`, then only changed entries — one per streaming tick); the desktop transcript
   rebuilds only rows whose entry fingerprint changed, and `zeron-client` keeps per-entry `Arc`s
   that stay pointer-equal while unchanged — the "endgame" the TS implementation documented but
   never reached.

### Command plane
Send/steer/interrupt/respondInput = durable command entries in the session doc (`QueueCommand`),
executed by the chat's **host** device (executor gated on chat ownership; mark-processed BEFORE
execute; steer with no live run dispatches as the next turn). Offline sends queue in the doc.
This is zeron's proven design, kept verbatim.

## 3. Cargo workspace

```
zeron/
  Cargo.toml                 # workspace
  crates/
    proto/        zeron-proto    # wire types: AgentEvent, ToolCall, RunRequest, Model,
                                 # entities, RPC envelopes (serde; ndjson framing);
                                 # `view` = the pure derivations every frontend shares
                                 # (sort orders, staleness gating, grouping, boot gate)
    doc/          zeron-doc      # session-doc schema, parts fold, continuations, command
                                 # ledger, message queue, sidecars, transcript deltas;
                                 # workspace registry row table (HLC, per-field LWW)
    sync/         zeron-sync     # edge room clients (chat2 rows: checkpoint + backfill +
                                 # backoff; registry rows + presence), DocsStore (SQLite
                                 # snapshots + processed-command ledger)
    harness/      zeron-harness  # Harness trait + native drivers (claude-code, codex,
                                 # cursor, opencode, pi), shared ACP harness (grok, devin,
                                 # hermes, antigravity), mock; steering mailbox,
                                 # requestInput, models/reasoning/options catalogs,
                                 # agent CLI installs
    engine/       zeron-engine   # sessions engine (pub/sub, run journal, recovery, idle
                                 # reaper), doc host + command executor, repos/worktrees,
                                 # checkout-diff sync, terminals (portable-pty), uploads,
                                 # agent accounts (cred swap), auth (WorkOS via edge),
                                 # profiles + local import, device-room host/peers, identity
    rpc/          zeron-rpc      # UiRpc/ControlRpc: typed req/resp/stream over WS (tokio-
                                 # tungstenite) + in-memory transport; device-room virtual
                                 # sockets ({s,k,to,from} frames)
    preview/      zeron-preview  # project previews: HTTP discovery, stable *.localhost
                                 # routing, WebRTC peers + edge signaling
    update/       zeron-update   # release checks + self-update (managed install, macOS
                                 # app, Windows portable)
    mcp/          zeron-mcp      # `zeron mcp`: stdio MCP server proxying to engine IPC
    theme/        zeron-theme    # source-neutral theme schema + built-in/custom registry,
                                 # validation, provenance, and local VS Code compiler
    markdown/     zeron-markdown # block-level incremental markdown over pulldown-cmark,
                                 # shared by desktop and mobile
    syntax/       zeron-syntax   # Tree-sitter highlighting contracts (no UI/engine deps)
    text/         zeron-text     # analytic text measurement + line layout (pretext port)
    ui/           zeron-ui       # gpui app: shell, sidebar, conversation, composer,
                                 # terminal view, diff pane, files, browser, settings,
                                 # animation kit
    client/       zeron-client   # engine-free thin client ("viewer device"): registry
                                 # mirror, chat2 sessions, command plane, demo mode
    mobile/       zeron-mobile   # UniFFI surface for the mobile apps (wraps zeron-client,
                                 # zeron-markdown, zeron-text)
  apps/
    zeron/                       # the binary (headed default; headless, login/logout,
                                 # status, sync, daemon, update, mcp subcommands)
    ios/                         # iOS app (UIKit) on zeron-mobile
    landing/                     # zeron.sh website (+ www-redirect/ Worker)
  edge/                          # TypeScript Worker + DOs (ported from zeron/apps/edge,
                                 # + auth-exchange routes absorbed from apps/server)
  docs/                          # research reports, design notes, PARITY.md, ADRs
```

Engine async runtime: **tokio** throughout; the UI bridges via `gpui_tokio` (`Tokio::spawn`
futures surfaced as gpui `Task`s). In-process mode runs the engine on the desktop's own tokio
runtime, off the UI thread; the UI never blocks on it.

## 4. UI plan (gpui) — parity + smoothness

Reference: `docs/research/gpui.md`, `docs/research/mugen-pretext.md`,
feature spec `docs/research/feature-inventory.md` §1.

- **Deps**: `gpui` + `gpui_platform` + `gpui_tokio` pinned to one `zeronsh/zui` rev (Apache-2.0;
  fork history in `Cargo.toml`). **We do not use Zed's GPL crates** (`markdown`, `ui`, `theme`,
  `editor`) — markdown, components, and theme are ours.
- **Transcript**: gpui `list()` + `ListState::new(n, ListAlignment::Bottom, overdraw)` (sum-tree
  offsets, follow-tail). On top of it, port the mugen behaviors that gpui doesn't give us:
  - stick-to-bottom **spring** with feed-forward tracking of streaming growth; interrupt from
    *user input* (wheel-up / drag), re-engage within a 70px band; own-send re-engages + smooth
    scrolls;
  - **block-granularity rows** (one row = one markdown block / tool group, not one message) with
    stable ids `msgId#blockId`; live entries split per block exactly like completed ones and
    keep their ids on completion; optimistic echo rows share the client-minted id so
    persistence never flickers;
  - rows cached per entry by content fingerprint, and row-set changes diffed by (id, version)
    into one minimal splice, so a streamed token re-renders one row;
  - scroll-anchor absorption for above-viewport height changes.
- **Markdown** (`zeron-markdown` parse, `zeron-ui::markdown` render): `pulldown-cmark` block
  tree with block-level incremental re-parse of the streaming tail (incremark's O(delta) idea:
  only re-parse from the last stable block boundary), monochrome
  theme where **numbers drive layout, colors are paint**. Code blocks: monospace, no wrap ⇒
  height = lines × line-height (layout independent of highlight); syntax highlighting via
  Tree-sitter (`zeron-syntax`, `docs/syntax-highlighting.md`) run on the background executor,
  colors applied as text runs (paint-only). Streaming **fade-in veil** on newly appended text via
  `with_animation` opacity (paint-layer, never affects layout). Reduced motion honored.
- **Composer**: hand-rolled gpui text input (start from Zed's `examples/input.rs`: IME, selection,
  clipboard, key actions), compact↔expanded auto-flip by measured text width, auto-grow 76–260px,
  Enter/Shift+Enter, Send→Steer→Stop morph, drafts + attachments per chat, drag-drop/paste
  images, QuestionPanel (paged, 1-9 keys, 220ms auto-advance) replacing the composer while input
  is requested. Pickers (harness/model, traits, repo w/ folder browser, branch w/ worktree
  toggle) as gpui popovers with `menu-in` scale/fade.
- **Terminal**: `alacritty_terminal` (vte state machine, MIT/Apache) + `portable-pty` on the
  engine side; custom gpui grid element; tabs w/ drag-reorder (150ms sliding transforms), height
  drag 160px–55vh, 12ms input coalescing / 80ms resize debounce, 1MB replay, detach ≠ close.
- **Diff pane**: unified-patch parser → virtualized file/hunk/line rows, per-file collapse
  (180ms height tween), time-sliced highlight, 200ms width transition on the pane itself.
- **Animation kit** (`zeron-ui::motion`): small helpers over gpui `Animation` reproducing the
  zeron catalog — `fade-in` (0.5s, cubic-bezier(0.16,1,0.3,1), translateY 4→0), `splash-out`,
  `zeron-pulse` staggered cell wave (boot splash + loaders), `gradient-spin-pulse` matrix
  spinner (WorkingIndicator + rotating flavour word), `menu-in`/`dialog-in` scale-fades, 200ms
  ease-out width/height transitions for sidebar/panes, sidebar-resort **slide animation**
  (we own the list, so animate row positions directly — the View Transitions equivalent, 260ms
  cubic-bezier(0.22,1,0.36,1)), reduced motion through gpui's `App::reduce_motion` (follows the
  OS setting unless Appearance pins it).
- **Theme**: independent light/dark resolved variants, theme-owned semantic/syntax/terminal
  palettes, optional interaction-accent overlays, and a device-local surface preference that
  resolves each variant's recommended frost/opaque treatment without changing theme selection.
  Forced frost derives contrast-checked tints from mapped theme surfaces. Local VS Code
  file/package compilation and imported/linked custom families retain last-known-good
  persistence. Colors remain paint-only; hairline borders and bundled Geist/Geist Mono remain
  shared presentation foundations.

## 5. Engine plan

Direct ports of zeron behaviors (spec: feature-inventory §3):
- **Sessions engine**: per-session broadcast hub; on-disk run journal (resumable `seq` replay,
  crash auto-resume); persistent steerable sessions (steering mailbox at step/turn boundary;
  30min idle reaper; a 15s liveness heartbeat instead of a stall watchdog, since agents may
  legitimately stay quiet; a turn-quiesce backstop only for harnesses without a deterministic
  turn end); recovery stamps `aborted`.
- **Doc host**: per-chat handle (join the chat2 room with checkpoint + row backfill, write user
  entries + stream assistant segments at 120ms commits, drain commands and the message queue
  host-only with processed-ledger idempotence, publish tail/diff sidecars); nudge-driven cold
  open; warm-doc LRU (12 docs + byte budget) over the SQLite snapshot store.
- **Harness** (`docs/research/harness.md`, `docs/research/acp.md`): trait mirroring zeron's
  `HarnessShape`, with a native driver per agent wire — Claude Code via `claude` CLI stream-json
  in/out (control protocol for permissions/AskUserQuestion→requestInput, resume, steering); Codex
  via `codex app-server` JSON-RPC; Cursor via the pinned `@cursor/sdk` behind a Node shim (JSONL
  over stdio); opencode via its HTTP/SSE server protocol; Pi via native JSONL RPC
  (`docs/pi.md`). A shared ACP harness serves only ACP-native agents (Grok, Devin, Hermes,
  Antigravity); adapter-mediated ACP for the others is retired. Model/reasoning/option catalogs
  ported from `packages/harness`.
- **Repos/diffs**: `git` subprocess (matches zeron, avoids libgit2 edge cases); worktrees under
  `~/.zeron/worktrees`; fs watchers (`notify`) + 2min repair; diff capture (patch + numstat +
  untracked, 3MiB cap, sha256) → local `WatchCheckoutDiffs` stream + diff sidecar per syncing
  chat.
- **Agent accounts**: credential-slot swap (macOS Keychain via the `security` CLI, files
  elsewhere), plan labels, usage probes, paste-code/browser-poll OAuth flows.
- **Auth**: WorkOS through edge routes (`/auth/exchange`, `/auth/refresh`, orgs); loopback
  callback server headed, paste-code headless; dev mode (no WorkOS client id ⇒ `user@org` dev
  bearer; see the profile table in §1).
- **Project previews** (`zeron-preview`): discovers a project's HTTP servers, routes stable
  `*.localhost` names, and reaches other devices' services over WebRTC DataChannels signaled
  through the PreviewRoom DO (`docs/preview-networking.md`).

## 6. Edge plan (TypeScript, `edge/`)

Ported from `zeron/apps/edge` (Loro-native, smoke-tested: device room byte relay + nudges +
sidecar slots, R2 attachments, JWKS auth), then reshaped so no Durable Object parses CRDT
bytes. Additions:
1. ChatRoom DOs (`/chat2/{chatId}/ws`): an authenticated append-only log relay with one
   client-built checkpoint and host-published sidecars served verbatim (`docs/chat2-sync.md`).
   The loro-protocol SessionRoom stays deployed only for pre-cutover clients.
2. Private per-user registry rooms (`/registry/{orgId}/ws` → `reg1/{orgId}/{userId}`) with authenticated row sync, ephemeral device presence, and APNs session notifications for iOS.
3. `/auth/*` routes absorbed from `apps/server` (WorkOS API key in Worker secret).
4. PreviewRoom signaling for project previews, and release artifacts (`/releases/*`, `install.sh`).
5. Drop `/seed` migration path and legacy sync anything (fresh app).
Hibernation hygiene: no idle timers (flush timer only while dirty), auto-response ping/pong —
per `docs/research/durable-objects-language.md`. Pushes to `main` that touch `edge/` deploy
through `.github/workflows/deploy.yml`.

## 7. Parity exclusions & deliberate changes

- **Excluded**: token-usage display (profile heatmap, lifetime stats, per-message token columns,
  `WatchUsage`). Rate-limit meters on agent accounts are *kept* (separate concern; probed from
  CLIs, not CRDT-synced), and so is the context-window ring: one atomic `meta.contextUsage`
  value per chat, not a token history (`docs/context-usage.md`).
- **Changed**: Postgres entity sync/server → workspace registry + edge; loro-aware session/
  workspace rooms → ChatRoom log relay + RegistryRoom row table; Electron/React/mugen → gpui with
  ported techniques; Node harness SDKs → subprocess protocols (Cursor keeps its pinned SDK
  behind a Node shim); WebRTC → device-room relay (zeron had already made this move; project
  previews alone use WebRTC); mobile app → `apps/ios` in this repo on a shared Rust core.
- **Kept verbatim**: session-doc schema shape, command ledger rules, render-parts privacy
  policy, UX behaviors and animation timings.

## 8. Milestones

Status legend: ✅ shipped · 🟡 shipped with named gaps (see `docs/PARITY.md`).

- ✅ **M0 Scaffold** — workspace builds; `proto`/`doc` crates with ledger + parts + continuation
  unit tests; gpui hello-window runs.
- ✅ **M1 Doc + sync core** — `zeron-doc` mirror over loro 1.13; room client syncs with the edge
  running under `wrangler dev`; Rust⇄edge⇄Rust convergence test (M1 exit: two Rust peers converge
  through a real SessionRoom DO, tail endpoint serves).
- ✅ **M2 Engine core** — Claude harness end-to-end headless: `zeron headless` + dev auth runs a
  turn, journal + doc writes, recovery test.
- ✅ **M3 UI core** — shell (sidebar/panes/header), transcript (virtualized, markdown, streaming,
  stick-to-bottom), composer (send/steer/stop, question panel); local chat fully usable headed.
- ✅ **M4 Multi-device** — device-room host/client virtual sockets, remote device control, workspace
  registry sync, WorkOS auth + org gate, presence. Proven live by `scripts/e2e-smoke.sh`:
  two headless engines against a real edge — B queues a run into the chat doc, the durable
  nudge wakes host A, A executes (mock harness), transcript + session status sync back to B.
- ✅ **M5 Full surface** — terminals, diff pane, repo/branch/folder pickers + worktrees,
  agent accounts UI, settings (devices/shortcuts/archived), composer attachments, Codex and
  Cursor harnesses.
- 🟡 **M6 Polish** — wire reconciliation (proto AuthState on the wire, `LocalDevice`),
  two-device e2e smoke, keyboard map, clippy/fmt sweep, reduced motion, single-instance lock,
  release packaging for Linux, macOS (dmg, optional notarization) and Windows
  (`.github/workflows/release.yml`, `scripts/package-linux.sh`, `scripts/package-macos.sh`,
  `scripts/package-windows.ps1`), edge production deploy. Gaps: engine hardening (parent-PID
  watchdog, crash shield, boot warm-open of recent chats).

## 9. Open questions (tracked, non-blocking)

1. Text shaping performance for analytic row heights: the desktop transcript measures shaped
   text through gpui `list()` + per-entry row caching, while the mobile core lays out
   analytically with `zeron-text` (pretext's kernel, ported); revisit the desktop only if
   cold-open of huge transcripts measures slow.

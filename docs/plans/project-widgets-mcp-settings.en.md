# Project widgets and Zeron MCP control — implementation plan

Status: detailed proposal, **no feature implemented**. Inspection of 15 October 2026.

## 1. Working frame and expected result

- Single worktree: `/Users/gaelcado/zeron/work/project-widgets-mcp-settings`.
- Single branch: `feat/project-widgets-mcp-settings`.
- HEAD and the local `origin/main` base inspected: `6ac8859998909bc5afd55838afb0635c281f31c3`. No branch-specific commit at the start of the inspection; clean tree.
- Both aspects stay on **this same branch**, as an ordered series of commits. The generic "one behavior per branch" rule of the contribution skill is replaced here by the user's explicit instruction. No other checkout receives writes.
- This task produces only this document: no feature code, branch switch, fetch/rebase, commit, push, PR, Cargo test, build or application launch.
- The user brief is the product reference. It picks up the conversation analyzed in chat `d75d1fe7-2a75-4661-8f03-940340a36d3e`; that chat was not resumed. Its old multi-branch split is obsolete.
- Agent context is deferred and excluded. Do not restart Voice Glyph, the CI panels or the voice-context diagnosis.

Goal of the first scope: keep a dashboard created in a chat, find and manage it in its project, then refresh it deliberately through an agent; allow inspecting and configuring the tools of the **built-in Zeron MCP**, with an effective refusal of disabled tools.

## 2. Verified state of this codebase

Paths below are relative to the root. Symbols are the entry points; line numbers are landmarks at the SHA above and may move during implementation.

| Topic | Existing code and integration point | Consequence for the plan |
| --- | --- | --- |
| Visual runtime | `crates/visual/src/lib.rs`, `Visual`, `check_with_assets`, `seed_for`; `assets.rs`, `node.rs`, `prelude.luau`; `crates/visual-gpu` | Reuse Luau and native rendering. Do not reintroduce an HTML engine or build a parallel widget engine. Sandbox with no network/I/O/`require`, bounded memory and time. |
| Publication bounds | `crates/proto/src/agent.rs:418`, `VISUAL_MAX_*` | Script 256 KiB, title 200 characters, 16 assets, 8 MiB per asset and 10 MiB total. Reuse the validations and the 16 MiB RPC frame constraint, without inflating the watchers. |
| MCP catalog | `crates/mcp/src/tools.rs:34`, `ToolDef`, `catalog()`; `Tools::list/has/call` around 660 | Static catalog, `&'static str` descriptions, JSON schemas; central dispatch. Today only `show_visual` is filtered outside a chat. No per-tool preferences. |
| MCP protocol | `crates/mcp/src/jsonrpc.rs`, `handle_request`, `serve_stdio` | Synchronous `tools/list`, `tools/call` checks `has` then calls `call`. `listChanged:false`. Incoming notifications, including cancellation, are ignored. The dispatch guard must change, not just the list. |
| MCP → engine link | `crates/mcp/src/zeron.rs`, `Origin`, `Zeron`, `call`, `resolve_space`, `resolve_target`; `crates/mcp/src/lib.rs`, `McpConfig::from_env` | Reconnecting IPC proxy. Keep the existing project/device resolution; no preferences owner in the stdio subprocess. |
| Agent injection | `crates/engine/src/sessions.rs:1575`, `Inner::zeron_mcp`, injection before dispatch around 2220; `apps/zeron/src/main.rs`, `mcp` subcommand | Server injected with port, chat and origin device. Persistent sessions may keep an old catalog. Do not modify the user's third-party MCPs. |
| Cached catalog example | `crates/harness/src/pi/mcp.mjs:70` | Pi lists once then registers `zeron_<tool>`. A new description therefore does not necessarily appear in an already-open runtime. |
| Inline publication | `crates/mcp/src/tools.rs:1209`, `Tools::show_visual`, `read_visual_assets`; `crates/engine/src/rpc.rs:3759`, `PUBLISH_VISUAL` | The bridge reads files in the agent's cwd and sends the bytes; the engine checks, stores and injects `AgentEvent::Visual`. It requires a live run on this device before/after validation. This RPC is not a widget CRUD and must not implicitly become one. |
| Current visual storage | `crates/engine/src/uploads.rs:903`, `store_visual`, `store_visual_assets`, `remove_visual` | Script `{id}-visual.luau`, manifest `{id}-assets.json`, files `{id}-asset-{name}`. Publication with local rollback; absolute upload paths in the transcript. No durable widget ownership, revision or project index. |
| Reading/rendering | `crates/ui/src/visuals.rs:175`, `VisualFrame::new`, `start`, `VisualLayoutChanged`; `crates/ui/src/transcript.rs:6709`, `render_visual`; `crates/ui/src/attachments.rs:634`, `read_attachment_bytes` | Reuse the renderer and bounded loading. The seed is currently derived from the row/visual ID; a widget must pass its content seed explicitly. Tasks/images are released when the entity drops. |
| Chat part | `crates/proto/src/agent.rs`, `AgentEvent::Visual`; `crates/doc/src/parts.rs`, `MessagePart::Visual`; `crates/doc/src/schema.rs` | The part holds ID, path, title; it is not a registered widget. Keep this format readable by old clients. |
| Project identity | `crates/proto/src/entities.rs:112`, `Space`; `crates/proto/src/view.rs:137`, `project_key`, `representative_space`; `crates/engine/src/spaces.rs` | A space is `(deviceId, path)`, stable ID and immutable owner. Sidebar grouping by `repositoryId` gathers spaces without giving them a common execution owner. |
| Host-local precedent | `crates/engine/src/project_actions.rs`, `ProjectActionsStore::open(profile_store_root)`, `project-actions.json`; `crates/engine/src/rpc.rs`, `local_project_action_space` | Existing model of engine-private project data, addressed by space and reachable through relay. Reuse the owner resolution/validation, not the Actions shell commands. |
| Profiles and assembly | `crates/engine/src/profile.rs`, `EngineProfile`; `crates/engine/src/lib.rs:114`, `EngineConfig`, then `EngineCore::assemble_with_profile_locked` | `EngineConfig` is not an MCP preferences file. EngineCore assembles the owners. Stores/journals/uploads are isolated per profile; credentials, device identity and provider preferences are device-local. |
| Real sync | `crates/doc/src/registry.rs`, `RegistryDoc`; `crates/engine/src/workspace_host.rs`; `crates/sync/src/store.rs`, `DocsStore`; `crates/engine/src/rpc.rs`, `forwardable`, `is_stream_method` | Registry and chats have their own sync protocols. Adding a local file or table does not synchronize them. For the proposed V1, relay operations to the owner, without a new CRDT document family. |
| Remote assets | `crates/engine/src/rpc.rs:3739`, `READ_ATTACHMENT_CHUNK`; `crates/ui/src/attachments.rs`; `edge/src/index.ts:396` | Visuals are read on the host engine, by chunks. The old `/attachments/...` mirror accepts then discards PUTs: do not promise R2 replication of assets. R2 tool sidecars are another mechanism. |
| Profile import | `crates/engine/src/local_import.rs`, `LocalImporter`; uploads assembly in `crates/engine/src/lib.rs` | A local→synced import already exists, with upload grants. A new widget store will not be imported automatically. `ARCHITECTURE.md` is partly historical here: the code wins. |
| Deletions | `crates/engine/src/rpc.rs:1325`, `DeleteChat`; `:1259`, `DeleteSpace`; `WorkspaceHost::delete_chat/delete_space`; `RegistryDoc::delete_space:1187` | DeleteChat purges the doc; DeleteSpace tombstones the space and its chats. No widget relation to maintain today. Plan their handling even when the space deletion arrives by sync. |
| Agent lifecycle | `crates/engine/src/sessions.rs`, `dispatch`, `subscribe`, `interrupt`, `stop_turn`, `terminate`, `recover_stale`; `crates/proto/src/entities.rs:308`, `Session.last_completed_turn`; `crates/engine/src/run_journal.rs`; `crates/doc/src/commands.rs` | Reuse the engine, the existing success marker and cancellation. `TurnListener` only signals the **start** of a `(chat_id,cwd)` turn and serves the diff. `JournaledEvent` holds seq/event, no commandId. Reliable attachment of an attempt to the executed turn is missing. |
| Queue and restart | `crates/engine/src/doc_host.rs:3407`, `queue_command`, `queue_command_with_transfers`; `crates/engine/src/lib.rs:257`, call to `recover_stale` | The queue currently generates the commandId internally and unarchives a chat when a message is sent to it. Recovery can relaunch an abandoned run. These behaviors must be adapted narrowly for refreshes, not assumed compatible. |
| Navigation/Settings | `crates/ui/src/shell.rs:539`, `SettingsSection`, `:690`, `Route`, new-chat outlet around 11120; `crates/ui/src/settings.rs`, `SettingsStore`; `settings/harnesses.rs` and `settings/accounts.rs` | No Widgets route or MCP section. `settings/widgets.rs` contains the settings primitives, not project widgets. Add dedicated views; SettingsStore must not write engine preferences. |
| Compatibility | `crates/proto/src/workspace.rs`, `EngineInfo.capabilities`, `capabilities::current`; `crates/rpc/src/lib.rs`, `methods`, `RpcError` | Add explicit capabilities and stable errors; no inference from the application version number. |

### Verified GitHub overlap

[PR #882 — Show GitHub activity on the new-chat canvas](https://github.com/zeronsh/zeron/pull/882) is **open**, not merged at inspection time. GitHub redirects `zeronsh/comet` to `zeronsh/zeron` for this PR. Its scope: a panel under the empty composer, GitHub account and 30-day metrics/heatmap, `github-activity.json` cache, last success kept, a General setting and hiding when the set does not fit.

Its files include `crates/ui/src/github_activity.rs`, `crates/engine/src/github_activity.rs`, the proto/RPC contracts, `shell.rs` and Settings. These modules are not present in the inspected base.

**Recommended decision:** a dedicated project route for widgets; no new mandatory panel under the composer. #882 remains independent and is neither adopted, modified nor a prerequisite. Possibly reuse a "last success + refresh error" state convention, not its per-`gh`-account cache. Before implementing on a later base, re-read `shell.rs` and the state of #882; do not graft its code onto this branch blindly.

### Verified MCP overlap

Contrary to a first reading, several **open** PRs touch the Zeron MCP server and the files this plan modifies (state verified on 15 October 2026):

| PR | Scope | Files shared with this plan | Consequence |
| --- | --- | --- | --- |
| [#878 — Zeron MCP: async delegation with background turn-end events](https://github.com/zeronsh/zeron/pull/878) | Asynchronous `create_chat(s)`/`send_message(s)`, engine turn-end watch (`NotifyTurnEnd`), event delivery to the harness (`DeliverEvent`) | `sessions.rs`, `doc_host.rs`, `rpc.rs`, `workspace_host.rs`, `tools.rs`, `zeron.rs`, `jsonrpc.rs`, `proto`, `rpc` | It builds precisely the "correlated turn end" seam whose absence §7 notes. Before step 7, re-read its state: if merged, reuse its turn-end watch rather than adding a second mechanism; otherwise decide explicitly whether to wait or design a compatible seam. |
| [#898 — Visual replies: agents show HTML pages inline](https://github.com/zeronsh/zeron/pull/898) | Old HTML variant of `show_visual`/`PublishVisual`, predating the merged Luau version (#900) | `visuals.rs`, `transcript.rs`, `uploads.rs`, `rpc.rs`, `tools.rs`, `agent.rs`, `parts.rs`, `docs/mcp.md` | Do not adopt it: this plan relies on HEAD's native Luau runtime. Check that it is closed or rebased before considering `show_visual` stable. |
| [#904 — MCP: add_project](https://github.com/zeronsh/zeron/pull/904) | New `add_project` tool and non-forwardable `AddProject` RPC | `tools.rs`, `zeron.rs`, `jsonrpc.rs`, `rpc.rs`, `proto` | Certain textual conflict with the catalog extraction (step 1). Keep that extraction a pure move to ease rebases; its tool enters the canonical catalog once merged. |
| [#640 — MCP: search_chats](https://github.com/zeronsh/zeron/pull/640) | New `search_chats` tool and `search.rs` module | `tools.rs`, `lib.rs`, `docs/mcp.md` | Same remark: conflict on `tools.rs` and `docs/mcp.md`. |

No open PR concerns persistent widgets or per-tool settings for the Zeron MCP. The four PRs above are neither adopted nor prerequisites; they require a targeted re-read of `crates/mcp/src/tools.rs`, `sessions.rs` and `doc_host.rs` before steps 1, 2 and 7.

## 3. Proposed structuring decisions

### 3.1 Scope and synchronization

V1: a widget belongs to **one existing project space, on its host engine, in the active profile**. Ownership key `(profile, ownerDeviceId, spaceId)`. Its own ID changes neither when the widget is renamed nor when the space is renamed. The folder path is not its key.

The "project" sidebar grouping may contain several spaces of the same repository. The page shows the selected space and device; if several are possible, it offers the same explicit choice as the current project flows. No silent merge between clones/worktrees, no owner election from the representative_space. Within a group, a Widgets entry first opens the current space, otherwise a space choice, without forcing the creation of a chat.

| Situation | Recommended V1 behavior |
| --- | --- |
| Local profile | Persistence under `profiles/local`, no cloud transport, no access from another account. |
| Synced/Development profile | Store under the existing account root, widget data always host-local. Authorized peers can view/modify through the relay of the online owner engine. |
| Owner offline | Keep already-loaded results in the view and indicate unavailability. On a new client with nothing loaded: unavailable, not "no widgets". No offline-queued mutations/refreshes in this V1. |
| Engine stop/restart | Reopen the store and the last validated content; interrupted refreshes become `interrupted`, with no automatic resumption of a paid call. |
| Profile switch | Widgets and MCP overrides of the previous profile are not visible. No automatic migration at login/logout. |

This is multi-device access to the owner, **not offline replication of widgets**. The compromise follows the Project Actions and avoids inventing CAS consensus on an LWW registry. Future replication would need a durable index, hash-addressed blobs, documented ownership and conflicts; it is not obtained by adding a field to the registry. If the product requires viewing from another device while the owner is off from V1, that extension changes the scope and must be decided before the storage work.

MCP settings also belong to the joined engine and the active profile, without V1 sync. Recommendation: `mcp-tool-settings.json` in `EngineProfile::store_root()`, rather than `ui-settings.json`, a versioned project file, or the global data_dir. Description overrides may contain account-specific instructions; profile isolation is preferable to global inheritance. The Settings device selector targets the owner by RPC. Both stores use the same profile captured at boot and do not change mid-runtime.

### 3.2 Common interface, no cross-cutting rewrite

- An engine module **ProjectWidgets** owns metadata, durable bundles, CAS, deduplication and refresh together. All UI and MCP callers go through its interface; the invariants are not reimplemented in the views.
- An engine module **McpToolPolicy** owns persistence, policy revision and call admission. It owns neither third-party accounts nor network configuration of external servers.
- Extract the current static definitions from `tools.rs` into `crates/mcp/src/catalog.rs`, exported by the crate. The engine and the bridge consume this single source; separate owned/serializable definitions for the effective description. The dispatch keeps its existing handlers.
- Minimal graph recommendation: move the engine's `zeron-mcp` dependency from dev-dependencies to dependencies to consume **only** this pure catalog. The MCP crate currently depends on proto/doc/rpc, not on the engine: no cycle observed. Check the full graph during the work. Do not make an IPC round trip from the engine to its own server for inspection. A separate catalog crate would only be justified if this dependency creates a real cycle or weighs down an actually affected target.
- `proto` receives the owned contracts and errors/capabilities, `rpc` the methods; no engine type must leak to the UI. Native rendering stays in `ui::visuals`.

## 4. User journeys

### 4.1 Save and find a visual

Example: "Show PostHog events and Google Analytics sessions for the last 30 days". The agent uses the integrations its harness has, produces the script and assets, then `show_visual` displays the result in the chat as today.

A **Save to project** action is placed in the Zeron chrome of the visual, outside the Luau tree. It proposes a title and the chat's project; for a chat without a project or in an ambiguous context, an explicit space choice. First version: save to the same engine as the source; another device implies a future explicit copy, not a mere ID change.

Saving does not launch the agent and does not request any integration. The engine re-reads the source part and copies its content into a self-contained bundle; no raw path supplied by the view suffices. After success: **Saved · Open widgets**. Re-clicking/replaying a request does not create a duplicate. Any additional copy is an explicit action, with a new title/ID.

Access: project/sidebar menu → **Widgets**, available even without a selected chat. The route has a selected space and its own navigation state; it does not reuse the fake empty chat as an identifier. No automatic move of the composer or of the #882 area.

The page shows titles, visuals, **Refresh**, last success date, a **Rename / Move before-after / Delete** menu, and **Open source chat**. Drag reorder may complement the keyboard actions, not replace them. Deleting removes the widget; it does not delete the chat. A missing source link shows "Source chat deleted" and the visual remains usable. For an archived chat still present, the link opens it without implicitly unarchiving it.

Do not expose revisions, paths, manifests, leases or hashes to the user. Concurrency appears as "This widget changed elsewhere. Reload it before retrying" with the current state; no silent forced replacement.

### 4.2 Refresh the metrics

The first refresh asks only for what is missing: refresh instruction, agent/model, and execution device already fixed by the project. Prefill a recipe proposed at agent creation, but make visible what the user is about to relaunch. Do not infer a recipe by replaying the whole chat history. Example: "Recompute these PostHog and GA metrics for the last 30 days, keep the definitions and comparisons".

The action indicates that an agent run is launched. No network in Luau, no credential in the widget. Reuse the harness configuration and permissions on the owner; a missing PostHog/GA integration leads to a help state or to that agent's normal authentication request. Do not promise to discover all third-party servers globally in this V1.

During the refresh, the last successful result stays visible, with "Refreshing", **Cancel** and **Open run**. An agent input request becomes "Response needed" with access to the refresh chat. An error shows an actionable sentence and **Retry**; the last success date does not move. Cancellation shows "Refresh cancelled", keeps the result, and allows a new attempt.

### 4.3 Placement and states

- Project page: vertical list of titled sections, visual at available width, consistent with the existing inline rendering; no free grid, no card resizing/docking for V1. Title and actions belong to the page, content to the renderer.
- Confirmed empty: "No widgets in this project" and an action opening a new chat in this space with a dashboard request. Do not launch an agent automatically.
- Initial load: light skeleton in the flow if the wait exceeds about 200 ms; do not show the empty state during the request.
- Missing or corrupted asset: error local to this widget, other widgets visible; re-read/retry and access to the source. Do not launch an agent refresh to repair a mere read without an explicit click.
- Owner offline: state distinct from integration failure and from empty; disable mutations and keep what is already displayed.
- Render/runtime failure: keep the validated bundle; offer reload, avoid an automatic retry loop.
- Small window: titles and actions wrap, menu reachable without hover, content scrolls; do not hide persistent widgets because they do not fit under a composer. Large tables keep their existing native behaviors.
- Stable focus per widgetId, focus return after menu/dialog/deletion, keyboard reorder, explicit labels for icon buttons and status announcements that do not depend on color. Respect theme tokens, text size and reduced motion; animations and GPU suspended for non-visible visuals.

### 4.4 Inspect and configure the built-in MCP

Settings → **MCP** → **Zeron**, with device selector and active profile indication. Search by name/description; one row per tool, switch and availability. Distinguish "enabled", "disabled", "available only from a chat", "engine unavailable"; Settings also lists disabled tools.

Expanding a tool shows the editable effective description, **View original**, **Reset description**, and the read-only input schema. Also present the output schema/annotations if the catalog provides them, without inventing any. Editing with **Save/Cancel**, inline validation, no autosave per keystroke. Reset removes the override and makes the original description follow future versions. The switch does not modify the description draft.

A concise note explains: "New calls use this setting. Some agents must reconnect the MCP to refresh their tool list." It must never suggest that a disabled tool remains executable with an old catalog. No automatic restart of a busy agent; offer reconnection at the next safe point when needed. The page remains operable if all tools are disabled: the Settings RPCs are not themselves MCP tools.

## 5. Widget model and persistence

### 5.1 Proposed data

Create `crates/proto/src/project_widgets.rs` and `crates/engine/src/project_widgets.rs` (private storage/refresh submodules if that simplifies reading).

| Data | Proposed contract |
| --- | --- |
| Widget | opaque UUID `id`, `spaceId`, `ownerDeviceId`, `title`, `revision:u64`, `createdAt`, `updatedAt`, `lastSuccessfulAt?`, `contentRevision?`, `source?`, `refreshRecipe?`. No secret. |
| Provenance | `sourceChatId`, visual part identifier, entry/message identifier if available, frozen source title for display and `sourceDeviceId`. Optional, non-owning links. The server resolves the part by ID, not by a blindly accepted path. |
| Immutable content | `bundleId`, `formatVersion`, `rendererFormat:luau-v1`, `renderSeed`, script and named assets with size/hash. New `bundleId` for each published content. The save seed is that of the source visual to preserve the same random data. |
| Recipe | instruction text, harness, optional model/reasoning, already-supported allowed parameters. The cwd is resolved from the owner space, not an arbitrary persisted cwd. No token and no automatic transcript copy. |
| Collection | `spaceId`, `collectionRevision`, ordered list of IDs. Authoritative order in the store, not a render-dependent index. |
| Refresh | `refreshId`, `widgetId`, `baseRevision`, `attemptRevision`, `commandId`, `executionChatId`, turn correlation, state, timestamps, `candidateBundleId?`, bounded and redacted error. A single active attempt per widget. |
| Replay | `operationId`/`requestId` and payload fingerprint, durable response; save uniqueness `(spaceId, sourceChatId, sourceVisualId)` and agent `creationKey`. |

`revision` represents a durable modification of the widget (title/recipe/content/deletion), not progress ticks. Refresh transitions have their own version; reorder only changes `collectionRevision`. This separation prevents a "running" status from failing a rename. Any widget modification during an attempt makes its `baseRevision` stale: do not overwrite, end in conflict and invite a relaunch. Fine-grained title/content merging can come later if needed.

### 5.2 Store and assets

Recommendation: a dedicated engine SQLite `project-widgets.sqlite3` under `profile.store_root()`, with versioned transactional migrations. The engine already depends on rusqlite. Do not extend `DocsStore`'s responsibility to host-local jobs/widgets or turn a JSON blob into a CRDT document without a protocol.

Logical tables: widgets (with tombstone), order/collection, attempts, idempotency receipts; unique constraints and transactional CAS. SQLite calls, copy/hash/visual check run off the GPUI threads and outside an async lock held during I/O. A single engine owner thanks to InstanceLock; transactions remain necessary for several concurrent windows/calls.

Bundles under `profile.store_root()/project-widgets/assets/<bundleId>/`, closed manifest, script and assets independent of the chat uploads. Write into an adjacent temporary directory, validate names/sizes/hash and `check_with_assets`, publish immutably by rename, then a transaction that points the widget at the complete bundle. If the transaction fails, the bundle becomes a collectable orphan; the previous reference remains. Never replace the files of a bundle being read by a view.

To save a part: read the source in the profile's authorized uploads, including the already-authorized legacy/import fallbacks, then **copy** script and all assets into the new bundle. Do not keep a symlink, a source path, or a hardlink that could be modified. A manifest naming a missing file fails the whole save. Revalidate the copied bundle with the original seed.

Add a dedicated, bounded/chunked bundle-read RPC, resolved by `(widgetId, contentRevision, assetName)` and owner. This RPC only opens the active profile's widgets root, rejects traversal/symlinks and keeps the current uploads jail unchanged. Reuse the chunk conventions of `ReadAttachmentChunk`; do not widen its root list to client paths. The script does not travel in `WatchProjectWidgets`.

The renderer receives an explicit content source and seed: current uploads read adapter for the transcript, bundle read adapter for widgets. Reuse `VisualFrame`/its elements without duplicating its thousands of lines; check cache behavior by `(widgetId, contentRevision)` and the release of images/tasks on revision/route change.

### 5.3 Concurrency, deduplication and deletion

1. Create requires a stable `operationId`; for the agent, a stable `creationKey` within the space is also mandatory. Replaying the same key/payload returns the same ID/revision. The same key with another payload returns a conflict, never an implicit update.
2. Saving the same part twice in the same space returns the existing widget, even if the two windows generated different operationIds. If that widget was deleted, a new explicit click can recreate it with a new ID and a new key; an old retry does not resurrect it.
3. Update/Delete require `expectedRevision`; atomic mutation `WHERE revision = expected AND deleted = false`, then increment. A conflict returns a stable code and the current metadata, never a silent last-writer-wins. New creations are not deduplicated by title.
4. Reorder requires the exact permutation of live IDs and `expectedCollectionRevision`; refuses duplicate/foreign ID/partial list. A concurrent add/delete invalidates this permutation; reload before retry.
5. Atomic refresh admission: uniqueness of an active attempt, click/retry `requestId`; return the active attempt rather than creating a second one. Idempotence controls the launch, not only the final replacement.
6. Delete sets a tombstone, invalidates the active refreshId and removes the ID from the collection in one transaction. Agent cancellation follows; a late result can no longer publish. Receipts prevent recreation through an old retry. Keep tombstones/creation receipts as long as the store exists in this V1, rather than a silent expiry that would compromise the replay contract.
7. Cleanup: bundles not referenced by current content or an active attempt, with a grace period for in-flight reads. Conservative GC at first, never deleting uploads/transcripts from the widgets store. A full disk leaves the last bundle and pointer intact.
8. Deletion of a source or refresh chat: no cascade to widgets. The source becomes a missing link; an attempt whose execution chat disappears ends interrupted. Space deletion: recommended to delete its widgets and cancel their attempts, with information in the existing confirmation. Owner-side reconciliation at startup and after registry changes, because the space tombstone may come from a peer; not only a hook on the local button. No implicit move to another clone.

## 6. Proposed RPC and MCP contracts

Names are proposed for implementation; do not add empty methods in this task. camelCase serialization, opaque IDs, bounded sizes and typed errors compatible with `RpcError`/the capability errors. Extend `crates/rpc/src/lib.rs::methods`, the proto DTOs and `EngineRpc`; add the remote methods to `forwardable` and the watchers to `is_stream_method`.

### 6.1 Widgets interface

| Proposed RPC | Significant inputs | Output / invariants |
| --- | --- | --- |
| `WatchProjectWidgets` | `spaceId`, `targetDeviceId?` | Initial snapshot then bounded snapshots of metadata, order, collectionRevision and refresh state. Deletions are observable. |
| `GetProjectWidget` | `widgetId`, `targetDeviceId?` | Metadata, readable recipe, revisions and manifest; no mandatory transcript retrieval. |
| `ReadProjectWidgetAssetChunk` | `widgetId`, `contentRevision`, reserved or named `assetName`, offset, target | Bounded bytes, nextOffset/done and integrity; immutable content, error if missing/retired version. |
| `SaveProjectVisual` | `spaceId`, `sourceChatId`, `sourceVisualId`, optional title, `operationId`, target | Reliable part resolution, bundle copy, deduplicated create. No live run required; no arbitrary path. |
| `CreateProjectWidget` | `spaceId`, `creationKey`, `operationId`, title, full content, optional recipe, target | Validation then idempotent creation; Luau content and base64 assets like PublishVisual. |
| `UpdateProjectWidget` | `widgetId`, `expectedRevision`, `operationId`, metadata patch or new full content, `refreshId?`, target | Outside refresh: CAS and commit. Inside refresh: candidate validated and staged, **not yet published**; no arbitrary modification of another widget. |
| `ReorderProjectWidgets` | `spaceId`, ordered IDs, `expectedCollectionRevision`, operationId, target | New authoritative collection. |
| `DeleteProjectWidget` | widgetId, expectedRevision, operationId, target | Tombstone and publication cancellation, idempotent. |
| `RefreshProjectWidget` | widgetId, expectedRevision, requestId, target | Fast admission → refreshId/status. The RPC does not wait for the agent to finish. |
| `CancelProjectWidgetRefresh` | widgetId, refreshId, requestId, target | Durable invalidation and targeted stop request; repeated cancellation has no additional effect. |

Avoid a `Mutate { op: arbitrary-json }` for this domain: the CAS/idempotence invariants must be visible in the DTOs. Validate that the space is still alive, the owner device, the profile and the revision on the destination engine; `targetDeviceId` is routing, not authorization. A widget ID read on another owner grants no access to its filesystem.

MCP V1: `list_widgets {project?,device?}`, `get_widget {widgetId,device?}`, `create_widget {project,creationKey,operationId,title,script,assets?,refreshRecipe?}`, `update_widget {widgetId,expectedRevision,operationId,...,refreshId?}`. Project resolution reuses that of `Zeron::resolve_target`/`resolve_space_in`, including ambiguity errors. In a project chat, omitting the project for the list may use that space; outside a chat, the project is required to write, never an implicit "first project".

`get_widget` returns the revision and, on explicit request, the script/manifest/assets within documented limits; not all data of all widgets in `list_widgets`. To allow an update without losing assets, a content update supplies a full bundle, or an **exact** reference to the current bundle validated by the engine to recopy the unchanged assets. Never interpret an omission as "erase all assets" by accident. A title-only change does not require re-reading the bundle.

MCP asset paths are read in the subprocess cwd, as for `show_visual`, then sent as bytes. The engine must not read a remote agent's cwd. Responses include `widgetId`, `revision`, `contentRevision`, `created/replayed/staged`, and stable errors such as `widgets.conflict`, `widgets.key_reused`, `widgets.not_found`, `widgets.owner_unavailable`, `widgets.refresh_inactive`, `widgets.invalid_visual`.

No agent delete/reorder/refresh tool in the first catalog: they are not needed to list/read/create/update and would notably encourage recursive refresh. The engine module already exposes these actions to the UI; any future exposure goes through the same catalog and the same rules. `show_visual` remains an ephemeral chat publication; it never implicitly creates a widget. An agent only saves a widget on request or in the planned explicit product flow.

### 6.2 MCP settings interface

Proposed DTOs in `crates/proto/src/mcp.rs`:

- `McpToolDescriptor`: stable name, original description, schemas and intrinsic contextual constraints; owned form without static lifetime in responses.
- `McpToolOverride`: name, `enabled?` and `descriptionOverride?`. Omission = follow the default, not a persisted copy of the original value.
- `McpCatalogSnapshot`: `catalogVersion`, `policyRevision`, full list of descriptors + effective state and availability reasons; Settings receives the originals even if disabled.
- Versioned file `{version, revision, tools:{name:{enabled?,descriptionOverride?}}}`. Keep, without activating, overrides for names absent after a downgrade; never let them create a tool. Removing a field resets to default.

| Proposed RPC | Role |
| --- | --- |
| `WatchZeronMcpCatalog {targetDeviceId?}` | Full inspection snapshot then policy/catalog changes; state for Settings, no duplicated schemas in the UI. |
| `SetZeronMcpToolOverride {name, expectedPolicyRevision, enabled?, descriptionPatch, operationId, targetDeviceId?}` | Atomic patch; descriptionPatch distinguishes unchanged/set/reset. Returns the actually persisted policy, conflict if stale. |
| `GetZeronMcpDiscovery {originChatId?, bridgeVersion}` | Local to the bridge: effective catalog filtered by policy **and** real context, state/version. Settings does not use this filtered catalog. |
| `AdmitZeronMcpToolCall {name, originChatId?, bridgeVersion}` | Local to the bridge: current admission, under the policy owner; stable refusal if the tool is disabled, the context invalid or the policy unreadable. No authorization cache in the bridge. |

The last two methods are not relay-forwardable: the tools of an injected bridge follow the policy of the engine it joins; afterwards their operations may target another device through the usual RPCs. Remote Settings uses the first two methods to configure the explicitly chosen engine. B's policy does not arbitrarily filter the list of the MCP hosted by A; it filters the bridges attached to B. Explain this point in the technical documentation without burdening the product flow.

## 7. Refresh: execution, success and cancellation

The refresh is orchestrated by the engine, never by the Luau widget nor by the window lifetime. A widget saved without a recipe remains viewable; Refresh offers to configure the recipe.

1. Preflight: live space and online owner, valid recipe, harness installed/enabled, engine able to serve the MCP, widget read/update tools authorized. The effective availability of external integrations is checked by the agent; neither Zeron MCP Settings nor a fake global probe guarantees it. A preflight error does not touch the content.
2. Transactional admission: fix `refreshId`, baseRevision, requestId and `queued` state. Reuse an existing active attempt for the same widget, even after a client network timeout. Persist **before** launching the run.
3. Use an ordinary refresh chat, dedicated to the widget, distinct from the source chat. Create it on the first refresh, then reuse it when idle; recreate it if deleted. Keep it archived outside the current list with an "Open run" link, reusing the existing archiving. Caution: `queue_command_with_transfers` currently unarchives sent chats; plan an internal refresh command path that explicitly preserves the archive while keeping the default user behavior. Do not introduce a new parent/child hierarchy or an agent context system for this. Never steer/interrupt a busy user chat to refresh a dashboard.
4. Send a Run command with a durable commandId and the recipe, widgetId/baseRevision/refreshId, and bounded reference content. The commandId → attempt link must be persisted before the drain. Reuse the `DocHost` ledger; handle a crash between admission, chat creation and enqueue with a local attempt outbox and preallocated IDs, not a second blind launch. Since `queue_command` currently generates the ID internally, add an internal engine entry that accepts a preallocated ID and idempotently replays the same payload; the current user entry keeps its defaults.
5. The harness keeps its normal integrations/configuration/permissions. It collects the data then calls `update_widget` for this widget. The engine checks the origin chat, the active attempt, the revision and the correlated execution; it validates the bundle with the existing runtime and stores a **candidate**. A call without refreshId from this chat cannot bypass staging and publish directly.
6. When a newer candidate is submitted within the same attempt, replace it in staging with its own operationId and attemptRevision; only the last validated candidate is eligible. An invalid candidate never replaces the last valid one. For a refresh run, limit writing to its widget; the rest remains subject to the agent's normal rules and is not obtained through an implicit extra permission.
7. Publish only after successful end of the **correlated turn**, candidate present and attempt active. In one transaction: check baseRevision/tombstone/refreshId, change contentRevision, increment revision, lastSuccessfulAt, state succeeded. A "turn finished without update" is a `no_result` error, not a success.
8. Tool/agent error, timeout, abandoned response, cancellation, deletion, CAS conflict: appropriate terminal state, previous content unchanged. An `awaitingInput` attempt is visible and resumes only after a response; proposed bounded global delay of 15 minutes, with a timeout error then a targeted stop. Distinguish the time of the last success from the last attempt and from the age of the data displayed by the visual.

Attempt states: `queued → running → awaitingInput → running → succeeded`, with terminal exits `failed`, `cancelled`, `interrupted`, `conflict`. `validating`/`cancelling` may be visible phases, without serving as truth for accepting a result. The initial last-success date is that of the validated creation/save; do not display it as a guaranteed collection date if the visual is older.

**Correlation to complete:** `Session.last_completed_turn` already exists and keeps the last successfully completed assistant entry ID; `DoneStatus` distinguishes completed/interrupted/errored. Reuse this marker, but do not confuse the last successful turn with the one of the current refresh commandId. An idle status or a timestamp change is not enough, and the transcript may arrive after the status. The `SessionsEngine::subscribe` journal gives seq/event; add to the engine/DocHost seam the commandId → run/turn attachment and its reliable terminal notification. Prefer a local engine receipt if that avoids an unnecessary new wire field; any serialized field added remains compatible. A persistent session parks between turns: wait for the end of the turn, never for process exit. The existing `TurnListener` callback is not suitable. Keep the existing diff callbacks, do not repurpose them. Test the transitions through this public interface, without depending on an agent's response text.

**Cancellation:** in one transaction, make the attempt non-publishable before requesting the interruption. The Cancel ack means "no new result of this attempt can be published", even if a subprocess is slow to stop. Then use `interrupt/stop_turn` on the dedicated chat and its commandId correlate so as not to stop a following attempt; reuse `CommandBasedOn.turn_id` when the Interrupt command goes through the ledger, with attempt validation at the coordinator. If the successful end already won the transaction, Cancel returns `alreadyCompleted`, without lying about a non-existent rollback. A new attempt gets a new refreshId. Late callbacks/updates fail without modifying the widget.

Closing the page does not cancel the engine refresh; deleting the widget cancels it. An engine restart marks non-terminal attempts interrupted before resuming the drains; invalidate their ability to publish even if an old command/run is found. `recover_stale` is currently called early in assembly and may relaunch abandoned runs: open/reconcile the attempt index before this recovery and give it the exclusion of the concerned refresh chats/commands. Keep the usual recovery for other chats. No auto-retry of a paid analytics request. Diagnostics limited to IDs, phases, duration and error code, without analytics data or tokens.

## 8. MCP policy: discovery, dispatch and sessions

### 8.1 Effective enforcement

`Tools::list/has` become operations that obtain the current policy by RPC (or use a snapshot only for display, never to admit). `jsonrpc::handle_request` awaits discovery. `Tools::call` has its **own** admission guard before parsing, file reads, project resolution and side effects; the `has` check in JSON-RPC is not enough, because internal callers/tests may call `call` directly.

The linearization point is engine admission: a call admitted before the disable is already in flight and may finish. As soon as the disable save is acknowledged, any subsequent admission of the name fails, including from an old list. Do not promise retroactive cancellation of an already accepted `send_message`. No automatic retry of an admission whose transport result is uncertain may duplicate a business effect.

Batches are a mandatory case: disabling `create_chat` also forbids its composition in `create_chats`; likewise `send_message` in `send_messages`. The batch admits its own name, then each child operation goes through the same guard at the moment it starts. No direct call to the private handler that bypasses the rule. Conversely, disabling only the batch name does not forbid the still-enabled unit tool. The engine's underlying permissions still apply.

A description override is catalog text, never a privileged instruction for the engine. It changes neither name, schema, "chat only" constraints, sandbox, permissions nor dispatch. Validate UTF-8/length (proposal 16 KiB), refuse empty/blank and inappropriate control characters; keep the long original description of `show_visual` without truncation. An override limit does not apply retroactively to existing native descriptions.

Policy write: validation and CAS under the owner, adjacent temporary file + atomic replacement, publication of the new revision only after successful persistence. Disk failure = setting not applied, clear message and previous state kept. Missing file = historical defaults. Corrupted file/unknown version = do not silently restart with all tools enabled; keep the last valid state in memory, or at boot make the MCP unavailable until explicit repair from Settings. Preserve the file for diagnosis, without logging the full overrides.

The scope of this feature is the tool offering of the Zeron MCP. It removes neither the corresponding UI functions nor an agent's general filesystem/shell rights; the UI RPCs remain available. Do not present this setting as security isolation of an agent that can already act otherwise.

### 8.2 Cached catalogs and versions

- `tools/list` always returns the current policy and context for a compatible bridge. Disable: the tool disappears. Re-enable and changed description: the next discovery reflects the new state.
- Recommended V1: honestly keep `listChanged:false` and signal the possible reconnection, rather than promise a notification that not all adapters handle. Per-call enforcement does not depend on the list refresh. Add `notifications/tools/list_changed` later only with engine subscription, stdio writer, lifecycle and adapter-by-adapter evidence.
- Provide `policyRevision`, `catalogVersion` and bridge version in internal exchanges to diagnose a lagging session. The UI may indicate "list to refresh" if a known session exposes an earlier revision, otherwise "may require a reconnection", without inventing an update confirmation.
- Pi registers its tools at runtime start; at minimum a safe relaunch of that runtime is needed to see new names/descriptions. For Claude/Codex/ACP/Cursor/OpenCode, check the behavior of the actually installed versions; do not assume that resuming a chat re-reads its list or changes its descriptions.
- No forced relaunch mid-tool, no automatic `terminate` for a text change. Effective disabling via admission applies without this relaunch on compatible bridges.
- Distinguish **old catalog in a compatible bridge** (covered without delay) from **old MCP binary predating the feature** (it contains no guard). Do not claim to make the latter safe with a mere list change. At upgrade, require the renewal of Zeron-managed bridges before declaring their settings supported; persistent runtimes must take part in the version check at the next safe point. An old externally launched `zeron mcp` must be replaced/reconnected; document this compatibility limit, without equating MCP control with a global ACL of old RPCs.
- Add the proposed capabilities `project_widgets_v1` and `zeron_mcp_tool_policy_v1` in the relevant engine/device info. Old engine: Settings announces the feature as unsupported, mutations unavailable; no fake local success. Recent bridge on a too-old engine: clear error for this feature, no fallback that ignores a claimed policy. If a legacy mode of historical tools is kept, it must be explicitly identified as unmanaged, never displayed as controlled.

The protocol behavior above follows the [MCP tools specification 2025-06-18](https://modelcontextprotocol.io/specification/2025-06-18/server/tools) already recognized by the current server: `listChanged` capability, the associated optional notification, distinction between JSON-RPC errors and execution errors. A known but disabled tool returns a stable business error `mcp.tool_disabled` with `isError:true`; an unknown name keeps the protocol error. A refresh cancellation uses its business RPC, without claiming that [MCP cancellation notifications](https://modelcontextprotocol.io/specification/2025-06-18/basic/utilities/cancellation), currently ignored, stop a run.

### 8.3 Articulation with the new widget tools

The four widget tools enter the canonical catalog as soon as they are added, with the same defaults and overrides as the others. No hidden widget-specific switch. If `update_widget` is disabled, a refresh cannot bypass it through a reserved RPC or `show_visual`; preflight and dispatch signal the block. If the disable arrives during the refresh, any new submission is refused. An already admitted/validated candidate may finish according to the in-flight rule; display this semantics without promising a rollback. The Cancel button remains available through the UI.

The `show_visual` setting does not forbid displaying an already-saved widget; disabling it forbids new publications through that tool. Read/edit rights from the project page remain those of the UI and the engine. Enabling a widget tool grants it no additional access to the PostHog/GA integrations.

## 9. Planned steps and commits — single branch

All future commits below are on `feat/project-widgets-mcp-settings`, in the declared worktree. They form an integrated series, not two stacked branches. Titles are indicative. None is created by this task.

### Step 1 — Catalog and shared contracts

Proposed commit: `refactor(mcp): expose canonical tool descriptors and policy contracts`.

- Extract catalog and descriptions into `crates/mcp/src/catalog.rs`, keep exactly the current schemas/names/behaviors; export owned definitions for the effective override.
- Add MCP DTOs and capabilities in `crates/proto/src/mcp.rs`, `workspace.rs`, exports; methods in `crates/rpc/src/lib.rs`. Update the engine dependency after checking the graph.
- Define intrinsic availability and the exhaustive name→handler mapping; no copy/paste of the catalog into Settings or the engine. Distinguish schema version from policyRevision.
- Acceptance: identical default catalog, `show_visual` still absent outside a chat, unique names, all handlers cover exactly the descriptors, DTO round-trip and old EngineInfo defaults.
- Planned checks: catalog/proto and `zeron-mcp` tests; targeted check of the crate graph and exports. No application build at this step.

### Step 2 — Persistence and admission of the MCP policy

Commit: `feat(engine): persist and enforce Zeron MCP tool policy`.

- Add `crates/engine/src/mcp_settings.rs`, owner in EngineCore and EngineRpc, profile file, getters/watch/setters/admission, typed errors, atomic revision.
- Modify `Tools::list/has/call`, `jsonrpc::handle_request`, `Zeron` for current discovery/admission; keep the announced protocol consistent.
- Route the unit operations of batches through the guard. Define migration of persistent bridges and support detection; do not guarantee enforcement on an old binary without renewal.
- Acceptance: a disabled tool disappears from discovery and fails before any effect, even from an old catalog or a direct Tools::call; override respects schema/permissions; change seen by all compatible bridges of the engine; no cross-profile leak.
- Checks: in-memory tests with two MCP connections against the same engine owner, disable between list/call, concurrent admission with save, batch/unit, IPC/persistence failure, restart, corruption and reset. Check that a description alone changes no serialized schema.

### Step 3 — Usable MCP Settings

Commit: `feat(ui): inspect and configure built-in Zeron MCP tools`.

- Add `crates/ui/src/settings/mcp.rs`, Settings module, `SettingsSection::Mcp` and render/navigation/focus branch in `shell.rs`, compatible persistent slug in `settings.rs`.
- Reuse the device selector and task patterns of `AccountsPage`/`HarnessesPage`, `settings/widgets.rs` primitives. Read and write through the engine; do not write preference files from the UI.
- Search, details/schemas, original/reset, draft, errors, scope display and reconnection note. Keep the draft on conflict/failure or on an explicitly cancelled device change.
- Acceptance: independent edit/reset/toggle, old engines read-only/unsupported, all tools disableable, Settings still operable, no focus loss or write to the wrong device after a late response.
- Checks: useful reducer/contract and focus tests; visual fixture for long list/long description/schema; later native review only on preview request, not automatically.

### Step 4 — Widget store, bundles and RPC

Commit: `feat(engine): persist project widgets with immutable assets and revision checks`.

- Add the §6.1 DTOs and methods, SQLite store/migrations, bundles and conservative GC; assemble the owner in EngineCore/EngineRpc.
- Save of a source part, create/update/reorder/delete, deduplication and CAS, watchers and chunked read, owner routing and space/profile control.
- Handle DeleteChat without widget cascade and DeleteSpace with owner reconciliation; widen no uploads jail. Do not modify the registry protocol to sync data that is not synced.
- Acceptance: save then delete the source leaves script/assets readable after restart; two clients lose no update silently; retry adds no duplicate; a bundle is never visible incomplete; remote owner and wrong profile correctly refused.
- Checks: module tests on a temporary store and two RPC clients; crash/fault injection around copy→rename→transaction; missing assets/traversal/symlink/sizes, full disk, reorder concurrent with create/delete, tombstone and restart. Focused forwarding/stream cleanup tests. No new tests that merely recopy the field list.

### Step 5 — Policy-governed agent widget tools

Commit: `feat(mcp): add idempotent project widget read and write tools`.

- Add the four §6 descriptors/handlers to the single catalog and the Zeron adapters; share the file read/validation of `show_visual` without coupling their effects.
- Document creationKey/operationId, expected revision, metadata patch vs bundle, conflicts and the need to re-read; no deduplication by title. Include new tools in Settings automatically.
- Acceptance: create via agent, list/read, update with revision, refuse stale revision and divergent replay; disabling `update_widget` produces the same refusal as for the other tools. A call without an identifiable project creates nothing.
- Checks: MCP tests against real in-memory engine/RPC and temporary assets; owner/profile isolation and name ambiguities; the catalog carries the new schemas without breaking show_visual.

### Step 6 — Save and project Widgets space

Commit: `feat(ui): save chat visuals and browse project widgets`.

- Add `crates/ui/src/project_widgets.rs` and route `Route::ProjectWidgets` with explicit space state; project/sidebar menu entry and navigation return. Integrate the save action into `TranscriptView::render_visual` in the row chrome, not in `ui.*` Luau.
- Adapt `VisualFrame` to the explicit source/seed without inline regression, cache by revision and resource release. List, rename, keyboard/drag reorder, delete, source link and complete states.
- Acceptance: PostHog/GA flow saved from a historical part outside a run, independent of the chat and its deletion; access without a selected chat; several spaces of the same repository explicitly distinguished; no collision with canvas/composer/#882.
- Checks: analytics content fixtures, visual with assets, long list and errors; relevant navigation/focus tests, late response from the wrong space, distinct loading/offline/empty. When a preview is requested: small/large viewport, themes and reduced motion per `zeron-ui-evidence`; no automatic capture/build in this plan.

### Step 7 — Manual engine refresh and UI commands

Commit: `feat(widgets): refresh through cancellable agent runs and keep last success`.

- Add coordinator and durable states, recipe and minimal dialog, outbox/commandId-turn correlation, dedicated chat and staging with `refreshId`. Extend the turn-end seam only if necessary; do not modify agent context.
- Wire launch/cancel onto the DocHost/Sessions primitives; handle crash/restart, awaitingInput, timeout, end without result, invalid candidate, space/chat disappearance, revision conflict and late publication.
- Acceptance: a single run per replayed click, success only after a successful turn; failure/cancellation keeps bytes and lastSuccessfulAt; double refresh and crossed cancellation do not touch the new run; closing the view does not lose the job; disabling the update tool is respected.
- Checks: deterministic mock harness with execution barriers to test races; candidate then terminal error, deletion/conflict during validation, cancellation before/after commit, restart before/after enqueue, awaitingInput and timeout. PostHog/GA user integration test with real integrations only during a later, explicitly authorized validation; not needed for the CAS/cancellation invariants.

### Step 8 — Compatibility and delivery documentation

Commit: `docs(widgets): document ownership refresh and MCP session compatibility` (with small compatibility fixes if revealed).

- Update `docs/mcp.md`, add widget documentation (and the state of this plan), specify profiles, absence of offline replication, import, schemas/errors, defaults and reconnection. (Verified: at HEAD, no comment in `engine`, `proto`, `mcp` or `doc` still mentions HTML for `PublishVisual`; nothing to fix there.)
- Redo the targeted review of #882 if the base changed; keep the dedicated Widgets route. Do not import the other work in progress.
- Acceptance: old/new engine-client-bridge matrix documented; additive migrations, downgrade without data deletion; explicit limitation of old binaries; key scenarios verified and limits declared.
- Final planned checks: `cargo fmt --all -- --check` (judge only the branch hunks if upstream drifts), targeted tests `zeron-proto`, `zeron-mcp`, concerned engine modules then targeted `zeron-ui`; `cargo check` of the affected packages. Widen only if a remaining risk justifies it. Heavy operations then require `zeron-storage-watcher`. None of these checks is run while writing this plan.

Dependency order: **1 → 2 → 3** for control; **1 → 4 → 5 → 6 → 7**, with **2 and 5 required before 7**; **8** closes the whole. The widgets UI of 6 can be designed from 4, but no external merge or second branch is needed. The common contracts are stabilized in 1/4 and exercised by all callers.

## 10. Cross-cutting checks and exit criteria

The first scope is complete only if the following scenarios pass:

1. In a project chat, show a dashboard with an asset; save it twice from two views; a single widget. Close then delete the chat, restart: identical visual and missing source link handled cleanly.
2. Rename/reorder/delete from two clients connected to the same owner: explicit conflicts, no lost update and no widget resurrected by retry.
3. Run a mock then an authorized real refresh with PostHog/GA data: new content on success; an integration outage, render error, end without candidate, cancellation or crash leaves the last content/success date intact. The result of N cannot overwrite N+1.
4. From another authorized device, view/modify the widgets of the online owner; owner off: unavailable state and already-displayed data kept. No fake cloud cache and nothing sent into another profile.
5. Inspect all Zeron tools, including the four widget tools, see schemas/original, override then reset and restart: conforming state; descriptions do not change rights.
6. List a tool, disable it from Settings, call through that old catalog on a compatible bridge: refusal before effect. Same via direct `Tools::call` and batch; disabling everything then re-enabling from Settings remains possible.
7. Unreadable policy or unreachable engine: no fallback to tools enabled by default while claiming they are controlled. Old engine/bridge explicitly unsupported or renewed, no fake "applied" indicator.
8. Native UI usable with keyboard/small sizes/large typography and themes; no task/GPU leak when leaving the project or replacing content. The possible presence of #882 does not change access to widgets.

No build, feature test or analytics agent was run to validate these behaviors in this task: they are the criteria of the future implementation. Validation of this deliverable: consistency with the inspected sources, existing links/paths, `git diff --check`, and a check that only this plan was added.

## 11. Migrations and compatibility

- New, versioned stores: absence = no widgets / historical settings; append-only transactional SQLite migrations. Unknown future version: disable writes without overwriting the store. Unreadable MCP file: explicit recovery and fail-closed managed MCP, no permissive automatic reset.
- No migration of existing `visual` parts: they stay in their chats until explicitly saved. Keep the existing tool names, the inline `show_visual` behavior and the current limits.
- Do not delete the authorized legacy paths/uploads during the copy; the new bundle becomes independent. No access to another profile's cache.
- New optional DTOs have appropriate defaults/skip; new capabilities for all mutations. Do not serialize a new mandatory status into existing rows if old clients do not tolerate it; widget states stay in their own contract.
- Rollback to an old binary: it ignores the new stores and must not erase them. New widgets are then not exposed by the old client. MCP control is not retroactively guaranteed by the old server; show/document this fact on a downgrade and require a compatible bridge for the feature.
- Local→synced import: recommended not to copy widgets/recipes/overrides automatically in the first scope. The existing mechanism imports sessions, not this new store. Show this limitation in the import help if needed; keep the local store intact. A future import option must remap spaceId, provenance, bundles and keys, without importing overrides silently.
- No edge/registry change or deployment needed for the recommended host-local scope. Do not add empty tables "for future sync".

## 12. Real risks and still-open product decisions

| Decision / risk | Recommendation and reason | When to decide |
| --- | --- | --- |
| "Project" = logical repository or host space? | V1 explicit space/device. The engine already has this owner; grouping clones does not elect a place to run the integrations. | Before step 4; default of this plan. |
| Is offline multi-device essential? | V1 connected relay access; durable replication is an extension with a blobs/conflicts protocol. Do not sell it as acquired through the registry. | Before step 4 if the product requires more. |
| Widgets location | Dedicated project route. Avoids competition with #882 and keeps access when the composer changes or a chat is open. | Design of 6; routine decision proposed. |
| Refresh chat and visible cost | Dedicated chat reused, archived and reachable, agent/model visible during configuration; no replay of the source chat. | Step 7. |
| Save all visuals automatically? | No. Explicit action or requested agent creation, otherwise project pollution and duplicates. | Product default. |
| MCP scope device-global or profile? | Profile + joined engine; overrides potentially account-specific, no sync. Mention in Settings. | Before step 2. |
| All tools disableable? | Yes, since recovery goes through the UI RPCs; no implicit permission for `whoami`, create/update widgets or batch. | Product default. |
| Live catalog notification | V1 honest reconnection, live admission; later notification only with harness evidence. Old binary ≠ old catalog, renewal required. | Step 2 and check 8. |
| Stopping a slow process | Durable cancellation cuts publication first; the harness stop follows its existing guarantees. Avoid proclaiming an immediate physical cancellation of external requests. | Tests of 7. |
| Editing during refresh | Conservative global CAS: explicit conflict rather than overwriting the rename/recipe or an implicit merge. | Default of 4/7. |
| Large storage and assets | Complete immutable bundle, current bound, conservative GC and initial proposed quota of 50 widgets per space; measure before optimization/global deduplication. Quota/full-disk failure without losing the last success. | Quota before 4; adjustable value. |
| Synced space deletion | Widget cascade on the owner with reconciliation, user information, no inheritance to another clone. | Before 4/6. |
| Sensitive analytics data | Isolated profile, no raw data/secret in logs or project file, same peer trust model as today. The recipe embeds no credential. | Invariant at every step. |

These recommendations allow progress without blocking the plan on labels, menu order or ID shape. The two questions that would really change the architecture are multi-owner repository-level ownership and offline viewing from any device; the other choices resolve within the described scope.

## 13. Explicitly out of scope

- Agent context/memory, Voice Glyph, CI panels and voice-context diagnosis.
- Scheduling, cron, automatic refresh at launch/open, analytics push subscriptions.
- Template/plugin store, creation of PostHog/GA integrations or management of their credentials in Zeron.
- General management of third-party MCP servers: add/remove/install/authenticate/configure a server, inspect every MCP of each harness.
- Cloud/offline replication of bundles, new widget CRDT document, multi-owner resolution, automatic import between profiles or copy between devices.
- Free-form dashboard canvas, resizable grid, persistence of Luau interactions, history/version browser, public sharing and cross-account collaboration.
- Automatic paid resumption after crash, recursive refresh by agents and retroactive cancellation of an already-accepted external effect.
- Rewrite of #882, Project Actions, the Luau/GPU renderer, the sessions queue or general agent permissions.
- Any implementation, commit, GitHub publication, build, preview or analytics agent run in this planning task.

## 14. Sources and method

Instructions inspected: no `AGENTS.md` found in the worktree's parent chain or its versioned files; `.agents` is a link to the shared local skills, read only. `CONTEXT.md`, `ARCHITECTURE.md`, `docs/mcp.md`, `docs/plans` examples and the Rust/JS/edge files cited above served as starting points. The behavior of the code at the fixed SHA wins over the historical sentences of the documents.

Skills applied: `zeron-contribution-branch`, `codebase-design`, `zeron-rust`/`zeron-gpui` rules and `better-layout` for the plan of interfaces, owners, cancellation and journeys. No subagent launched. Git/GitHub reads only for the base and the overlap; no side work restarted. Official MCP specification consulted in the version already supported by this server, no protocol migration proposed.

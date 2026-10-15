# Project widgets

A project widget is a visual (a `show_visual` Luau script and its assets) kept
in a project space so it can be found, managed and refreshed outside the chat
that produced it. The design plan is
`docs/plans/project-widgets-mcp-settings.en.md`.

Code: `crates/proto/src/project_widgets.rs` (contracts),
`crates/engine/src/project_widgets/` (`mod.rs` owner and invariants, `store.rs`
SQLite, `bundles.rs` content files, `refresh.rs` agent runs),
`crates/mcp/src/widgets.rs` (agent tools, see [mcp.md](mcp.md#project-widgets)),
`crates/ui/src/project_widgets.rs` (Save action and Widgets page).

## Ownership

A widget belongs to one space, on the engine of the device that owns that
space, in that engine's active profile. Its id is an opaque UUID; renaming the
widget or the space changes nothing, and the folder path is not a key. Clones
and worktrees of one repository are distinct spaces: the sidebar's project
group never merges them or elects an owner.

The owner serves a space only while it is live in the registry and owned by
its device. `targetDeviceId` routes a request through the device relay; it is
never authorization. Other devices of the account read and write through the
owner while it is online. There is no replication: with the owner offline,
the page keeps what it already loaded and pauses changes, and a client that
loaded nothing shows the owner as unavailable, not as empty.

## Storage

Everything lives under the profile store root, so profiles never see each
other's widgets:

- `project-widgets.sqlite3` (WAL, mode 0600). Tables `widgets` (tombstones
  kept), `collections` and `collection_items` (per-space order and its
  `collectionRevision`), `refresh_attempts`, `receipts` (operation replay).
  Migrations are append-only, one transaction each, counted in `user_version`:
  1 creates the tables, 2 adds `widgets.refresh_chat_id` and
  `refresh_attempts.candidate_content`. A store from a newer build is served
  read-only (`widgets.store_unavailable` on writes) and never rewritten; an
  unopenable one refuses everything without touching the file.
- `project-widgets/assets/<bundleId>/` holds one immutable bundle:
  `manifest.json`, `script.luau`, `assets/<name>`. A bundle is written to an
  adjacent `.staging-*` directory, synced, read back against its SHA-256
  hashes, checked with the visual runtime and the widget's render seed, then
  renamed into place. Only then does a transaction point the widget at it. A
  failed transaction leaves an orphan bundle and the previous pointer.

Limits are those of `show_visual` (script 256 KiB, title 200 characters, 16
assets, 8 MB each, 10 MB together) plus 50 live widgets per space
(`widgets.quota_exceeded`), keys and operation ids of 1–128 bytes, and refresh
instructions of 16 KiB.

**Garbage collection.** After writes, deletions, successful refreshes and once
per engine start, bundles referenced by no live widget and no active refresh
candidate are removed once older than 10 minutes, as are abandoned staging
directories. Reads name the `contentRevision`, and a superseded one gets
`widgets.conflict` whether or not its bundle is still on disk. Collection
never leaves the bundles root, so chat uploads and transcripts are never
touched.

**Deletion.** Deleting a widget writes a tombstone, removes it from the order,
and cancels its active refresh in one transaction. Tombstones and receipts are
kept for the store's lifetime, so an old retry never resurrects a widget.
Deleting the source chat does not touch the widget; the link reads as "Source
chat deleted" and the content keeps working. Deleting the space, here or from a
peer, deletes its widgets: the owner reconciles at start and after every
registry change.

## RPC

Served by the owner and relay-forwardable; `WatchProjectWidgets` is a stream.
Capability `project-widgets-v1` covers everything but the two refresh
methods, which need `project-widget-refresh-v1`.

| Method | Contract |
| --- | --- |
| `WatchProjectWidgets {spaceId}` | The space's ordered live widgets with `collectionRevision`, then a snapshot per change. Metadata and manifests only, never script bytes. |
| `GetProjectWidget {widgetId}` | `ProjectWidgetDetail`: the widget and `sourceMissing`. The source check happens only here. |
| `ReadProjectWidgetAssetChunk {widgetId, contentRevision, assetName, offset}` | 45,000-byte base64 chunks with `nextOffset`, `done`, `size`, `sha256`. `assetName` `@script` reads the script. A `contentRevision` other than the current one is `widgets.conflict`. |
| `SaveProjectVisual {spaceId, sourceChatId, sourceVisualId, title?, operationId}` | Copies a chat visual the owner hosts into a new widget. No client path is accepted and no live run is needed. A visual already saved in that space returns `existing`. The bundle keeps the source's render seed. |
| `CreateProjectWidget {spaceId, creationKey, operationId, title, content, refreshRecipe?}` | Idempotent creation from a script and base64 assets. A `creationKey` is never reused in its space. |
| `UpdateProjectWidget {widgetId, expectedRevision, operationId, title?, refreshRecipe, content?, refreshId?, originChatId?}` | Compare-and-set on `revision`. `content.keepAssetsFromBundle` must name the current bundle. With `refreshId`, stages a refresh candidate. |
| `ReorderProjectWidgets {spaceId, widgetIds, expectedCollectionRevision, operationId}` | `widgetIds` must be exactly the live widgets, each once. |
| `DeleteProjectWidget {widgetId, expectedRevision, operationId}` | Tombstone; deleting again answers `alreadyDeleted`. |
| `RefreshProjectWidget {widgetId, expectedRevision, requestId}` | Admits a refresh and answers once it is queued. |
| `CancelProjectWidgetRefresh {widgetId, refreshId, requestId}` | Makes the attempt unpublishable, then stops its run. |

`revision` changes with every durable change of a widget (title, recipe,
content, deletion), never with refresh progress or reordering;
`contentRevision` changes with the content; reordering changes only
`collectionRevision`. Every write carries an `operationId`: the same id and
payload return the first answer (`replayed`), the same id with another payload
is `widgets.key_reused`. Conflicts carry only their code; clients reload
before retrying. Unknown enum values from a newer engine (states, outcomes)
read as `Other`.

### Errors

Stable codes in `zeron_rpc::capability_errors`:

| Code | Meaning |
| --- | --- |
| `widgets.conflict` | Changed since the revision sent; nothing changed. |
| `widgets.key_reused` | `operationId` or `creationKey` already used with another payload. |
| `widgets.not_found` | No such live widget on this owner. |
| `widgets.source_not_found` | The chat or visual to save is gone. |
| `widgets.owner_unavailable` | Space unknown, deleted or owned elsewhere, or the owner is unreachable. |
| `widgets.refresh_inactive` | `refreshId` is not the widget's active refresh for this chat. |
| `widgets.refresh_scope` | A refresh run wrote outside its own attempt. |
| `widgets.invalid_visual` | Content failed validation; the text continues with the reason and line. |
| `widgets.refresh_unavailable` | A refresh cannot start; the text continues with what is missing. |
| `widgets.quota_exceeded` | The space already holds 50 widgets. |
| `widgets.write_failed` | Not saved (for example a full disk); the previous state is intact. |
| `widgets.store_unavailable` | The store is unreadable or from a newer build. |

## In the app

- **Save.** Under a chat's visual, **Save to project** saves into the chat's
  space, or offers an explicit choice of that device's spaces. It runs no
  agent. The result is "Saved" or "Already saved", then **Open widgets**.
- **Open.** The Widgets route (`Route::ProjectWidgets {space_id, device_id}`)
  is reached from a project row's context menu ("Widgets", or one "Widgets ·
  <place>" entry per clone), the command palette's "Open project widgets",
  and **Open widgets** after a save. It needs no selected chat.
- **Browse.** Widgets render one after another at the page width with the
  inline visual renderer, one frame per `(widgetId, contentRevision)`.
  Off-screen widgets don't paint, and leaving the page drops every frame. Each
  widget has Refresh, **Open source chat** (it does not unarchive an archived
  chat), and a menu with Rename…, Move up, Move down and Delete…. Loading,
  empty ("No widgets in this project"), offline, unsupported and per-widget
  read errors are distinct states. A refused change keeps what is shown and
  says "This widget changed elsewhere. Reload it before retrying."

Agents create and update widgets with the MCP tools; they have no delete,
reorder or refresh tool.

## Refresh

A refresh asks an agent to rebuild one widget. The widget's recipe holds the
instruction, harness and optional model and reasoning, never a credential;
**Set up refresh** collects it the first time. The Luau script never gets
network access: data comes from the agent's own integrations on the owner.

**Admission** (`RefreshProjectWidget`). Preflight requires a recipe, its
harness installed and enabled on the owner, an engine that serves the Zeron
MCP to its agents, and `get_widget` and `update_widget` switched on in the
owner's tool policy; otherwise `widgets.refresh_unavailable` and nothing
changes. Then one transaction stores a `queued` attempt with its base
revision, `requestId`, preallocated command id and the widget's refresh chat.
The same `requestId`, or any request while an attempt is active, returns that
attempt (`existing`). The RPC answers before the agent starts.

**The run.** Each widget has one ordinary chat in its space, created on first
use, titled "Refresh · <title>", kept archived and reachable with **Open
run**; it is recreated if deleted. The Run command is queued under the
preallocated id without unarchiving the chat, with `workspace-write` sandbox
and no auto-approval. Its prompt carries the instruction, the ids
`update_widget` needs, and the current script for reference, truncated at
24 KiB, with asset names only. If a user turn is running in that chat, the
refresh waits up to 60 seconds, then fails with `chat_busy`; it never steers
or interrupts it. Attempts of one widget take turns in its chat.

**Candidates.** The agent calls `update_widget` with the `refreshId`. The
engine checks that the call comes from the attempt's chat (`originChatId`),
for this widget, at the base revision, and with the whole script; it
validates the content into a bundle and keeps it as the attempt's candidate.
A later valid candidate replaces it; an invalid one changes nothing. A
refresh run cannot publish directly or write to another widget.

**Publication.** When the correlated turn completes, one transaction publishes
the last candidate if the attempt is still active and the widget is still at
its base revision: new content and `contentRevision`, `revision` + 1,
`lastSuccessfulAt` set. Otherwise nothing changes.

States: `queued` → `running` ⇄ `awaitingInput` → `succeeded`, or a terminal
`failed`, `cancelled`, `interrupted` or `conflict` with an error code:

| Code | State | Cause |
| --- | --- | --- |
| `no_result` | failed | The turn completed without a candidate. |
| `agent_error` | failed | The turn ended in error. |
| `timeout` | failed | Over 15 minutes in total, queued and awaiting-input time included; the run is then stopped. |
| `chat_busy` | failed | The refresh chat stayed busy for 60 seconds. |
| `launch_failed` | failed | The run could not be prepared, queued or published. |
| `cancelled` | cancelled | Cancelled by the user. |
| `deleted` | cancelled | The widget was deleted. |
| `conflict` | conflict | The widget changed during the refresh. |
| `run_stopped` | interrupted | The turn was stopped before it finished. |
| `chat_deleted` | interrupted | The refresh chat was deleted. |
| `engine_restarted` | interrupted | The engine restarted. |

No failure moves the content or `lastSuccessfulAt`. The page keeps the last
good content visible with "Refreshing" and Cancel, shows "Response needed"
while the agent awaits input, and offers Retry after a failure.

**Cancellation.** `CancelProjectWidgetRefresh` first marks the attempt
`cancelled` in the store; from its answer on, nothing from that attempt can
publish. Then the run is stopped: a command that has not started is withdrawn,
otherwise the attempt's own turn is stopped and followed for up to 30
seconds. Cancelling again answers `alreadyCancelled`; after publication it
answers `alreadyCompleted`, with no rollback. Closing the page does not
cancel; deleting the widget does.

**Restart.** When the store opens, every active attempt becomes `interrupted`
(`engine_restarted`) and its command is withdrawn if it never ran. Refresh
chats are excluded from boot recovery, so a crashed refresh is never resumed:
paid agent work is not retried on its own.

**Tool policy.** Switching off `update_widget` (or `get_widget`) blocks new
refreshes at preflight. During a run, the bridge refuses new submissions, so
the attempt ends `no_result`; a candidate staged before the change may still
publish when the turn completes.

## Compatibility

All of it is host-local and additive; nothing changes the registry, the edge,
or existing chat parts. Visual parts already in chats stay there until saved.

| Owner engine | Client | Behavior |
| --- | --- | --- |
| With `project-widgets-v1` | Current | Full feature. Refresh needs `project-widget-refresh-v1` on the owner, otherwise the button explains the owner must be updated. |
| Without it | Current | No Save action for its chats; the Widgets page reads "unsupported", not empty. |
| Any | Without widgets | Nothing shown; the store is ignored. |
| Offline | Current | Already-loaded widgets stay visible, changes are paused; nothing loaded reads as unavailable. |

A bridge without `originChatId` (built before refreshes) cannot stage a
candidate (`widgets.refresh_inactive`) until its harness restarts with a
current one. MCP versions are covered in [mcp.md](mcp.md#versions).

- **Downgrade.** An older build ignores `project-widgets.sqlite3`, the bundles
  and `mcp-tool-settings.json` and never deletes them; its clients do not show
  widgets, and its MCP server enforces no policy.
- **Import.** The local-to-synced import copies sessions, not widgets, recipes
  or tool overrides; the local profile's store stays intact.
- **Profile switch.** The other profile's widgets and overrides are not
  visible; nothing migrates.

## Non-goals

Offline or cloud replication of widgets, a widget CRDT document, multi-owner
or repository-level ownership, copying between devices or profiles;
scheduled or automatic refresh, automatic retry after a crash, recursive
refresh by agents; a free-form grid or resizable cards, drag reordering,
version history, public sharing; creating or authenticating analytics
integrations; managing third-party MCP servers.

## Tests

`crates/engine/tests/project_widgets.rs` (save, restart, deleted source, two
clients, deleted space), `project_widget_refresh.rs` (publication only at turn
completion, cancellation, deletion, restart), `mcp_widget_tools.rs` (agent
tools), plus unit tests in each module and the `WidgetsModel` tests in
`crates/ui/src/project_widgets.rs`. The `project-widgets-fixture` and
`mcp-settings-fixture` examples (each behind the feature of the same name)
render the Widgets page and Settings → MCP offscreen.

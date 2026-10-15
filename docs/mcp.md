# Zeron MCP server

`zeron mcp` serves the Model Context Protocol on stdin/stdout and proxies every
tool into the running engine's localhost IPC (`ws://127.0.0.1:$ZERON_IPC_PORT`,
default 27654) — the same `zeron_rpc` surface the headed app and `zeron sync`
dial. It is a subcommand of the one `zeron` binary: no Node runtime, no extra
install, a few MB resident.

Crate: `crates/mcp` (`zeron-mcp`). The protocol layer is hand-rolled
(`initialize`, `ping`, `tools/list`, `tools/call`; newline-delimited JSON-RPC
2.0) — the repo already owns JSON-RPC framing for the Codex and ACP drivers and
the stdio tool-server subset is tiny, so no SDK dependency was taken.

## Identity

The engine can inject this server into a harness's MCP config with the
originating chat in the environment:

| Variable          | Meaning                                                       |
| ----------------- | ------------------------------------------------------------- |
| `ZERON_IPC_PORT`  | Engine to proxy (default 27654).                              |
| `ZERON_CHAT_ID`   | The chat whose agent spawned this server.                     |
| `ZERON_DEVICE_ID` | That chat's host device.                                      |

When `ZERON_CHAT_ID` is set, every `send_message` is prefixed with a
`[Message from Zeron chat <title> (<id8>) …]` line so the receiving agent and the
human reading that transcript can tell an agent-to-agent message from a typed
one, and the server refuses to message its own chat. The transcript renders
this routing header as “Message from **chat name**”, keeping the full routing
instructions in the stored prompt for agents.

### Parent links

`create_chat` and each request in `create_chats` accept `kind`:

| Input | Placement |
| --- | --- |
| `kind: "chat"` | Standalone session, visible in the left **Sessions** sidebar; no `parentChatId` is written. |
| `kind: "side"` | Child of the explicit `parent` or origin chat. Requires one of those. |
| Kind omitted, origin or parent supplied | Child, preserving the previous default. |
| Kind omitted, no origin or parent | Standalone, preserving terminal usage. |

`kind: "chat"` with a nonempty `parent`, `kind: "side"` without an origin/parent,
and unknown kinds fail before any writes. Empty `parent` uses the default.
The creation result includes the effective `kind`, `chatId`, `deviceId`,
`project` and `parentChatId`. Kind is derived from the parent; it is not persisted.

Children record `Chat.parent_chat_id` (proto) ⇄ `parentChatId` on the registry
row (`Mutate createChat { parentChatId? }`). Chats with a parent cannot create
chats through MCP, including standalone and batch creation or an explicit parent
override. A side chat cannot be selected as a parent; only one level is supported.

For children, `parent` (id, prefix, or title) overrides the origin (`ZERON_CHAT_ID`).
`list_chats { parent }` returns children, and chat summaries carry `parentChatId`.
Rows from older engines read as parentless; a dangling parent id is tolerated.

The left sidebar hides chats that have a parent: `AppState::visible_chats`
(the Sessions list, project tabs, jump slots) and the Archived section both
require `parent_chat_id == None`. Children remain addressable by id, deep
link, and every MCP tool; `list_chats { parent }` is how an orchestrator
finds them.

### Injection

The host engine stamps this server onto every run it drives
(`RunRequest.mcp`, additive): the same `zeron` binary with `args: ["mcp"]`
and the three variables above, pointed at the port the engine itself serves
(never a port it lost the bind race for). Each driver spells it in its own
dialect and leaves the user's configured servers alone:

| Harness | Where |
| ------- | ----- |
| Claude  | `--mcp-config <inline json>` (no `--strict-mcp-config`)             |
| ACP (Devin, Grok, Hermes, Antigravity) | `session/new` and `session/load` → `mcpServers: [{name, command, args, env}]` |
| Pi | Per-run `--extension` bridges stdio MCP into Pi tools in the native RPC process |
| OpenCode | Child-only `OPENCODE_CONFIG_CONTENT`: `mcp.zeron` on 1.x, `mcp.servers.zeron` on 2.x |
| Codex   | `thread/start` config overrides `mcp_servers.zeron.{command,args,env}` |
| Cursor  | SDK `Agent.create` / `Agent.resume` → inline `mcpServers.zeron`, plus `local.settingSources: ["user", "team", "mdm", "plugins"]` so `~/.cursor/mcp.json` and plugin servers load (not `project`: the SDK skips MCP approvals, so repo-defined servers would run unprompted) |

OpenCode preserves inherited inline configuration and other servers. Its config
shape follows the installed binary's major version. Cursor uses the SDK's
[inline MCP configuration](https://cursor.com/docs/sdk/typescript); OpenCode's
[1.x config layer](https://opencode.ai/docs/config/) and
[2.x MCP format](https://opencode.ai/v2/docs/mcp-servers) differ.
Pi's temporary extension lives only for the native RPC process lifetime;
user settings, extensions, and session arguments remain intact. The bridge
registers `zeron_<tool>` tools, propagates cancellation and errors, and closes
the MCP child when the Pi session shuts down.

Title runs never carry it. A run with no served port (embedded engine that
lost the bind) gets no Zeron tools rather than a dead server.

### Forks and the explorer

`ForkSideChat { chatId, sourceChatId, parentChatId? }` copies a chat's
history through its latest completed response into a new chat and appends
a `fork` part (a system entry: `sourceChatId`, `sourceTitle`) as the seam;
the transcript draws it as "This chat was forked from <title>". `parentChatId`
defaults to the source; a side chat's own fork button passes its parent so
the copy lists as a sibling. The file explorer's footer lists a chat's
**Subagents** (its spawn chips) and **Chats** (its children: forks and
`kind: "side"` spawns) and opens either in the right pane.

## Tools

Chats are referenced by full id, a unique id prefix, or an exact title.
Projects by id, path, display name, or unique path suffix. Devices by id or
name (default: the local engine's device).

| Tool               | Engine calls                                              |
| ------------------ | --------------------------------------------------------- |
| `whoami`           | `LocalDevice`, `EngineInfo`, origin chat summary          |
| `list_devices`     | `WatchDevices` snapshot                                   |
| `list_projects`    | `WatchSpaces` snapshot                                    |
| `list_harnesses`   | `ListHarnesses {targetDeviceId?}`                          |
| `list_models`      | `ListModels {harness, targetDeviceId?}`                                    |
| `list_chats`       | `WatchChats` + `WatchSessions` snapshots (status merged)  |
| `get_chat`         | above + `WatchDocMessages` opening frame (pending input)  |
| `create_chat`      | `Mutate createChat` (+ `renameChat`; optional first send) |
| `create_chats`     | Concurrent `create_chat` requests with per-request results |
| `send_messages`    | Concurrent `send_message` requests with per-request results |
| `read_chat`        | `WatchDocMessages` opening `reset` frame, rendered        |
| `send_message`     | `QueueCommand` Run / Steer, or `QueueMessage`             |
| `wait_for_turn`    | `WatchSessions` until the chat settles                    |
| `interrupt_chat`   | `QueueCommand` Interrupt                                  |
| `respond_to_input` | `QueueCommand` RespondInput                               |
| `archive_chat`     | `Mutate setChatArchived`                                  |
| `show_visual`      | `PublishVisual {chatId, title, script, assets}` (origin chat only; assets base64 by name, read by the MCP server from the agent's cwd) |
| `list_widgets`     | `WatchProjectWidgets` snapshot (metadata only; project defaults to the origin chat's) |
| `get_widget`       | `GetProjectWidget` (+ `ReadProjectWidgetAssetChunk` per file with `includeContent`) |
| `create_widget`    | `CreateProjectWidget` (project required; assets read like `show_visual`'s) |
| `update_widget`    | `GetProjectWidget` when a script is sent, then `UpdateProjectWidget` |

Watch streams are the engine's only read surface (there is no one-shot "get
transcript" RPC); a snapshot is "subscribe, take the first item, drop" — drop
cancels server-side, exactly what the sidebar does on attach.

Every tool, the four widget tools included, is subject to the
[tool policy](#tool-policy) before it runs.

`send_message` mode `auto` starts idle chats and steers working chats through
their live mailbox without interrupting tools or child processes. Providers
consume it at their next supported input boundary. Only explicit `queue` mode
creates a held queue row. `awaitingInput` refuses and points at
`respond_to_input`. Quiet, long-running turns still receive steering; the host
falls back to starting a turn if the live runtime has already exited.

Claude uses `priority: "next"` and confirms consumption through replayed user
messages. Cursor uses the SDK's native `Run.steer`, waits for active tools to
finish, and retains input until its delivery acknowledgment. New injections do
not wait for earlier delivery acknowledgments; that wait serializes responses
even when the SDK reports a single active turn. These are mid-turn
paths; they do not terminate the harness process. Adapters that only accept input
between turns are labeled **Send next** in the composer.

The opt-in `steering_live` engine test checks a rapid burst, foreground and
background process survival, retained context, exactly-once effects, and ordered
normal queue delivery. For Claude, Cursor and Codex it additionally requires the
burst to finish within the original turn. Set `ZERON_TEST_BURST=6` to reproduce a
six-message burst; select an inexpensive model with `ZERON_TEST_MODEL` and the
harness with `ZERON_TEST_HARNESS`. Codex is checked for child survival during
the active tool: its runtime cleans up background jobs on normal tool completion
even without steering. Other providers also check a job that outlives that tool.

For conversational redirection, run `cargo run -p zeron-harness --example
cursor_steering_probe -- gemini-3-flash`. It sends six bare digits during streamed
prose and requires only the latest requested answer. Counting turn completions
alone cannot distinguish real steering from serial responses inside one run.

`wait_for_turn` after a send is edge-triggered on the `Session` row captured
before the send: it returns on a new `last_completed_turn`, an
`awaitingInput`/`errored` stamp newer than the baseline, or a working→idle
edge. A brand-new chat has no session row until the host picks the run up, so
the wait keeps waiting in that case rather than reporting the unstarted run as
done (this was the one bug the first live run found).

### Visual replies

`show_visual` is listed only when `ZERON_CHAT_ID` is set: it shows an
interactive visual (chart, table, dashboard, diagram, simulation, 3D view)
inline in the calling chat, above the agent's final reply. A visual is a Luau
script, not a web page. The script builds its data and state once and returns a
view function; the view returns a tree of `ui.*` nodes (rows, columns, grids,
text, stats, tables, charts, tabs, toggles, sliders, buttons, links, 2D
canvases, 3D scenes, and GPU shaders). Controls call the script's handlers, and Zeron renders
the view again.

`crates/visual` (`zeron-visual`) is the runtime. Scripts run in a Luau sandbox
with only the `string`, `table`, `math`, `utf8` and `bit32` libraries (no I/O,
network, or `require`), a 64 MB memory cap, and a time budget per call (250 ms
for setup, renders and handlers, 60 ms per canvas or scene frame), so a runaway
script fails with an error instead of freezing the app. `math.random` is
reseeded from the visual's id before every call, so random data is identical on
every render and device. The style rules are the API rather than advice: colors
are theme tokens (`text`, `muted`, `faint`, `accent`, `on-accent`, `success`,
`warning`, `danger`, `border`, `chart1`…`chart6`), text sizes are steps on the
transcript's type scale, the font is always the interface font, and layout nodes
have no fill, border, shadow, padding, or font props; using one is an error that
names the rule. 3D scenes render on the GPU (`zeron-visual-gpu`: physically
based materials, shadows, glass with dispersion, glTF models, textures,
environment maps, sprites and bloom), with the runtime's software rasterizer
(`raster.rs`) as the fallback where there's no GPU: Zeron frames an orbit
camera on the geometry and draws it into one transparent image. A visual's
files (`assets`: at most 16, 8 MB each, 10 MB together) are stored beside its
script as `{id}-asset-{name}` with an `{id}-assets.json` list, and travel to
other devices like attachments. `ui.shader` takes Shadertoy-style GLSL
(`mainImage`, `iResolution`, `iTime`, `iMouse`, plus `iCamera` for an orbit
camera, named float inputs bound to the script's state, `iNoise`, and up to four
Shadertoy-style buffer passes read as `iChannel0`-`3`, with feedback from their
own previous frame); `zeron_visual::shader` wraps each pass in a GLSL 4.50
program and validates it with naga, so a compile error reaches the agent with
its pass and line. A shader composites onto the chat by `blend` (`alpha`,
`light` for emitted light where black is clear, or `opaque`) with optional
bloom, so it never sits in a black box.

The engine runs every script before showing it (`zeron_visual::check`: setup,
the first render, every canvas and scene draw, then each tab, toggle, slider
end, and button once) and returns the first error with its line, so the agent
fixes it before the reader sees anything. A passing script is stored in uploads
(`{id}-visual.luau`) and a `visual` doc part is folded into the chat's live turn
through the run's engine event channel; with no live turn on this device the
call fails and nothing is stored. The doc carries only the path and title, so
clients read the script with `ReadAttachmentChunk` like any attachment; old
clients see an empty part.

The desktop (`crates/ui/src/visuals.rs`) renders the tree with the transcript's
own GPUI elements: markdown's table treatment, the settings tab, switch, and
button idioms, canvases as GPUI paths and shaped text in theme colors, and
scenes and shaders as images. Shaders run offscreen on the GPU through wgpu
(`crates/visual-gpu`, one shared device, pipelines cached by source) and are
read back like a scene's image. Nothing foreign is hosted, so a visual scrolls, clips, blurs under
the glass composer, and takes input like any other row, identically on Linux,
macOS, and Windows, and it follows theme, appearance, and text-size changes
without rerunning the script.

### Project widgets

`list_widgets`, `get_widget`, `create_widget` and `update_widget` read and
write the widgets kept in a project ([project-widgets.md](project-widgets.md)).
A widget lives on the device that hosts its project: pass the `deviceId` that
`list_widgets` returns as `device` to the other three. `show_visual` never saves
anything; an agent creates a widget only when the user asks to keep a visual.

| Tool | Contract |
| --- | --- |
| `list_widgets {project?, device?}` | Metadata only: `id`, `title`, `revision`, `contentRevision`, `lastSuccessfulAt`, `updatedAt`, `hasSource`, `hasRefreshRecipe`. Without `project`, the origin chat's project; outside a project chat `project` is required. Never an implicit first project. |
| `get_widget {widgetId, device?, includeContent?}` | The widget (revisions, manifest with sizes and SHA-256, recipe, source) and `sourceMissing`. With `includeContent`, also the script and assets base64 by name, up to 1 MB of assets in total; the rest are named in `omittedAssets`. |
| `create_widget {project, creationKey, operationId, title, script, assets?, refreshRecipe?}` | `project` is required, even in a project chat. `creationKey` (1–128 bytes, chosen by the agent) names the widget in its project and is never reused there, even after deletion: the same key and arguments return the first widget (`replayed`); other arguments fail with `widgets.key_reused`. Widgets are never matched by title. |
| `update_widget {widgetId, expectedRevision, operationId, …}` | Compare-and-set on `revision`; see below. |

`operationId` (1–128 bytes) identifies one request: resending it after a lost
reply returns the first answer (`outcome: replayed`), reusing it with other
arguments fails with `widgets.key_reused`. Asset paths are read from the
agent's working directory, as for `show_visual`; the engine never reads a
remote agent's files.

`update_widget` applies only against the widget's current `revision`. Any
change since then (a rename in the app, a refresh, another agent) fails with
`widgets.conflict` and changes nothing: call `get_widget`, reapply the change,
retry with the new revision and a new `operationId`.

- `title`, `refreshRecipe` and `clearRefreshRecipe` change metadata only and
  keep the content.
- `script` replaces the content and must be the whole script. With
  `keepExistingAssets: true`, current assets not replaced by `assets` are kept
  (the bridge names the current bundle, which the engine checks); with
  `false`, the assets become exactly `assets`. When the widget has assets,
  `keepExistingAssets` is required with `script`, so leaving out `assets` never
  deletes them.
- `refreshId` is only for a run Zeron started to refresh that widget. The
  content is then staged (`outcome: staged`) and replaces the widget only when
  the run's turn completes. Outside that run, or after the refresh ended, it
  fails with `widgets.refresh_inactive`. A refresh run that omits `refreshId`,
  or names another widget, fails with `widgets.refresh_scope`.

Results carry `outcome` (`created`, `updated`, `staged`, `replayed`),
`widgetId`, `projectId`, `deviceId`, `revision` and `contentRevision`. Errors
start with their stable code (`widgets.*`, listed in
[project-widgets.md](project-widgets.md#errors)); `widgets.invalid_visual` keeps
the script's error and line after it. An engine without `project-widgets-v1`
is reported as such.

## Tool policy

Settings → **MCP** → Zeron lists every built-in tool of the chosen device's
engine: switch, effective and original description, input schema, and whether
it is available only from a chat. Every tool can be switched off; the page
uses app RPCs, not MCP tools, so it stays usable with all of them off.

The policy belongs to one engine profile, in `mcp-tool-settings.json` at the
profile store root:

```json
{ "version": 1, "revision": 3,
  "tools": { "send_message": { "enabled": false, "descriptionOverride": "…" } } }
```

An omitted field follows the default (enabled, original description). Defaults
are never copied in, so a reset description follows later catalog versions.
Entries for names this build does not know (written by a newer build) are kept
and never create or enable a tool. An override must be non-blank, at most
16 KiB of UTF-8, without control characters other than tab and line breaks. It
is catalog text only: name, schema, chat-only constraint and the agent's
permissions do not change.

`SetZeronMcpToolOverride` is a compare-and-set on `policyRevision`. It writes a
temporary file, renames it into place, and only then publishes the new
revision; a failed write keeps the previous policy. Resending an applied
`operationId` returns the current policy instead of a conflict.

The policy fails closed. A missing file means the defaults. An unreadable file
or unknown `version` refuses every tool and Settings shows all of them off; the
first change sent against the published revision keeps a copy
(`mcp-tool-settings.json.unreadable-<ms>`) and rebuilds the policy from the
defaults plus that change. Override text is never logged.

### Enforcement

The bridge asks its engine on every `tools/list` (`GetZeronMcpDiscovery`) and
every call (`AdmitZeronMcpToolCall`), before parsing arguments, reading files or
any effect. It caches neither answer.

- Admission is the cut-off. Once a change that switches a tool off is
  acknowledged, no later call to it is admitted, including from a list fetched
  earlier. A call admitted before runs to its end; nothing is undone.
- `create_chats` and `send_messages` are admitted under their own name, then
  each item as `create_chat` or `send_message` when it starts. A refused item
  reports `isError` and its `code`; the others still run. Switching off only
  the batch name leaves the single tool available.
- A known tool that is switched off returns `isError: true` with
  `structuredContent.error.code` `mcp.tool_disabled`. A tool unknown to the
  engine, or chat-only outside a chat, keeps the JSON-RPC "unknown tool" error.
- `tools/list` adds `_meta` for diagnosing lagging sessions:
  `zeron/catalogVersion` (the engine's `catalog::CATALOG_VERSION`, now 3),
  `zeron/policyRevision` and `zeron/bridgeVersion`. The bridge serves its own
  schemas and offers only names both sides know.
- `listChanged` stays `false`. A session keeps the list it fetched until it
  reconnects the MCP: Pi registers its tools when its runtime starts, and other
  harnesses may cache too. New names and descriptions appear after a
  reconnection; a switched-off tool is refused at once either way. Settings
  says: "New calls use this setting. Some agents must reconnect the MCP to
  refresh their tool list."
- MCP cancellation notifications are still ignored.

| Code | Where | Meaning |
| --- | --- | --- |
| `mcp.tool_disabled` | call result | Switched off in Settings; it did not run. |
| `mcp.policy_unreadable` | call result, `tools/list` error, Settings | Stored policy unusable; nothing runs until a change rebuilds it. |
| `mcp.policy_unsupported` | call result, `tools/list` error | The engine predates `zeron-mcp-tool-policy-v1`. |
| `mcp.admission_unavailable` | call result, `tools/list` error | The engine could not be asked. |
| `mcp.tool_refused` | call result | A refusal reason this bridge does not know. |
| `mcp.policy_conflict` | Settings | Changed since `expectedPolicyRevision`; reload. |
| `mcp.policy_write_failed` | Settings | Not saved; the previous policy applies. |

### Scope

A bridge follows the policy of the engine it joined (`ZERON_IPC_PORT`).
Discovery and admission are never relay-forwarded; `WatchZeronMcpCatalog` and
`SetZeronMcpToolOverride` are, so Settings can configure another device's
engine. Device B's policy filters the bridges attached to B, not those of A,
even when A's agents act on B's chats with `device`. The policy is not synced
between devices, not shared between profiles, and not copied by the
local-to-synced import.

It governs what the Zeron MCP offers, nothing else: the app's RPCs, the agent's
shell and file access, its other MCP servers and the harness's own permissions
are unchanged. It is not a security boundary.

### Versions

- New bridge, engine without `zeron-mcp-tool-policy-v1`: `tools/list` is a
  JSON-RPC error and every call fails with `mcp.policy_unsupported`; nothing
  runs unmanaged. Settings shows "Not available on this device" and offers no
  change there. The
  engine injects its own executable (`current_exe`), so this happens when that
  file is replaced by a newer build while the older engine keeps running,
  until the engine restarts.
- A `zeron mcp` built before the policy has no guard: it lists and runs every
  tool whatever Settings says, and reconnects to a newer engine after a
  restart. An injected bridge starts with its harness process, so a runtime
  still running from before the update keeps its old bridge until it
  restarts. An externally configured `zeron mcp` (another app's MCP config)
  must be restarted to be managed.

## Selecting a host and project

`list_projects {device?}` filters by device id or exact name; without arguments
it lists all projects. `list_harnesses {device?}` and
`list_models {harness, device?}` query the selected host with `targetDeviceId`;
without `device` they retain the local engine catalogs.

For creation, project alone determines the host. Device alone creates a session
without a project on that device. With both, the project is resolved **within**
the selected device, and a project id belonging to another device is rejected.
Neither argument means a projectless session on the local engine. Repeated names
or paths are errors with candidate ids and devices; use ids from discovery.

Before writing, creation checks the chosen engine's harness catalog, chooses
its available default (Claude Code when available, otherwise the first available
non-mock harness), and validates any explicit model against that host's catalog.
An explicit harness must be offered, installed and enabled. Catalog failures,
including model lookup failures, are returned with the device id; there is no
fallback to the local catalog. A successful remote query checks reachability;
`lastSeenAt` alone does not establish that an engine is online.

Discover and launch a standalone session:

```text
list_devices {}
list_projects { device: "<device-id>" }
list_harnesses { device: "<device-id>" }
list_models { device: "<device-id>", harness: "codex" }
create_chat {
  kind: "chat",
  device: "<device-id>",
  project: "<project-id>",
  harness: "codex",
  model: "<model-id from that host's catalog>",
  title: "Implement feature",
  prompt: "Implement the feature...",
  wait: false
}
```

Without a project (the host uses its home directory unless `cwd` is given):

```json
{"kind":"chat", "device":"<device-id>", "harness":"codex", "prompt":"Reply with pong", "wait":true}
```

A mixed `create_chats` batch, with an origin chat or explicit top-level parent:

```json
{
  "requests": [
    {"kind":"chat", "device":"<device-id>", "project":"<project-id>", "title":"Feature", "prompt":"Implement the feature"},
    {"kind":"side", "parent":"<parent-chat-id>", "project":"<project-id>", "prompt":"Review the API"}
  ]
}
```

The first prompt and later sends use durable chat commands drained by the host.
Transcript reads target the chat's host; session status comes from the shared
registry. Sends remember the session and existing message ids on the MCP connection before
sending, so a subsequent `wait_for_turn` after `wait:false` waits for that send,
even before a session row appears. A separate MCP server has no send baseline
and reports the chat's current posture. Creation records the send time before dispatching its first
prompt. If completion arrives before the transcript, the wait allows the new
assistant response to arrive within the **same timeout**. A previous response
is never substituted for a missing response to a new send. An unfinished
transcript or missing response at the deadline returns `timedOut`.

## Parallel side chats

Use `create_chats` with a `requests` array to launch independent workers in
one tool call, including when the harness executes its tool calls sequentially:

```json
{
  "requests": [
    {"project": "/repo", "title": "Review tests", "prompt": "Review test coverage"},
    {"project": "/repo", "title": "Review API", "prompt": "Review API compatibility"}
  ]
}
```

Use `send_messages` with the same envelope for existing chats; each item uses
`send_message` arguments. Both accept 1–32 requests, run them concurrently,
and return `results` in input order with `index`, `isError`, and either `result`
or `error`. A failed request does not cancel or roll back successful requests.
Each request defaults to `wait: false`; explicit waits also run concurrently.
Only batch independent work, not ordered messages to the same chat.

When using individual tools, launch **all** chats/messages with `wait: false`
first, then collect replies with `wait_for_turn`. Waiting on each individual
launch before issuing the next serializes work at the caller, even though
both the MCP transport and engine support concurrent chat runs.

## Smoke recipe

```sh
BIN=target/debug/zeron
{ echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}'
  echo '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"list_chats","arguments":{"limit":5}}}'
  sleep 5; } | ZERON_IPC_PORT=27655 $BIN mcp
```

`create_chat` with `"prompt": "Reply with exactly the word pong", "wait": true`
against a live daemon returns the assistant's `pong` in a few seconds; archive
the chat afterwards with `archive_chat`. Unit tests (`cargo test -p zeron-mcp`)
drive the whole tool set against an in-memory stub `RpcService`. The engine
tests `mcp_tool_policy` and `mcp_widget_tools` run the bridge against a real
in-memory engine: a tool switched off between list and call, batches, and the
widget tools' replay and conflict rules. Persistence, compare-and-set and the
corrupt-file path are unit tests in `crates/engine/src/mcp_settings.rs`.

An isolated stdio instance is available with
`cargo run -p zeron-engine --example mcp_standalone_smoke`. It provides a temporary
project, an origin coordinator and a scripted `codex` adapter (`smoke-1`) returning
`pong`. Discover its ids with the list tools, create `kind: "chat"` with a prompt
and `wait: true`, then check `read_chat`, `send_message` and a mixed batch. The
profile is removed on exit. This checks MCP dispatch and engine execution without
changing your running app's sessions or requiring provider credentials.

The `mcp_standalone_session_executes_on_the_selected_device` test in
`device_routing` additionally runs two engines with a shared registry and device
relay, verifies execution on the selected host (and absence of execution on the
caller), and retrieves responses through MCP.

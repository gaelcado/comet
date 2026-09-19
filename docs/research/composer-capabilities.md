# Composer capability mapping

Audited 2026-09-19. These are integration contracts, not promises that every
installed agent version or account offers the same modes. The UI uses the host's
`agent-modes-v1` capability and the selected model's discovered options. An
advertised native mode keeps its exact ID and label; no prompt simulates a mode.

| Harness | Plan / native modes | Persistent goal | User input above composer |
| --- | --- | --- | --- |
| Codex | `collaborationMode/list`; only advertised presets | Only when `experimentalFeature/list` advertises enabled `goals`; `thread/goal/get,set` | `item/tool/requestUserInput` (blocking or asynchronous), plus completed assistant-message questions |
| Claude Code | `--permission-mode plan`; explicit approval of `ExitPlanMode` | Not exposed | `AskUserQuestion` control request and plan approval |
| Cursor | Pinned `@cursor/sdk@1.0.31`: `mode` on create and send, including resume | Not exposed | Not exposed: pinned public SDK has no answer channel; `askQuestion` remains disabled |
| OpenCode | V1 `/agent` must advertise visible primary `build` and `plan` agents; V2 plan is not exposed by this integration | Not exposed | `question.asked` with reply/reject; explicit permissions while planning |
| Devin | ACP live `configOptions` or legacy `modes` only | No dedicated goal lifecycle API; no fabricated Goal option | ACP question choices and native mode permissions |
| Grok | ACP live `configOptions` or legacy `modes` only | Same ACP restriction | Same ACP input bridge |
| Hermes | ACP live `configOptions` or legacy `modes` only | Same ACP restriction | Same ACP input bridge |
| Pi | `pi-acp` live `configOptions` or legacy `modes` only | Same ACP restriction | Same ACP input bridge |
| Antigravity | ACP live modes only; pinned fixture advertises Default/Auto Edit/YOLO, **not Plan** | Same ACP restriction | Same ACP input bridge |
| Mock | Test-only harness; no production mode selector | Test events only | Test callback |

ACP mode IDs are opaque: `architect` is not rewritten as `plan`. Semantic mode
configurations use a shared UI key but are sent through their original config ID.
Grouped select choices are supported. Legacy modes use `session/set_mode`.
Missing or rejected explicit modes stop before prompting; native defaults are
preserved instead of silently selecting a permissive mode. For agents advertising
modes, permission requests go through the question tray. `switch_mode` requests
always require an answer, even when every option has a standard allow/reject kind.
For agents without modes, existing unattended tool permissions remain unchanged;
question-shaped choices still use the tray.

Codex fallback model lists intentionally contain no mode claims. Runtime discovery
must establish support first. Claude's CLI supports the native plan permission
mode. Cursor's pinned published type definitions include `AgentModeOption`, but
no public question response operation. OpenCode V2 plan is an integration gap,
not a claim that the product itself cannot plan.

Native plans/checklists, Codex goals, and questions use the existing durable
transcript events and composer tray. Cursor `createPlan` and Claude plan-exit
content map to the same plan representation. Native mode choices apply to the
next message; they are not immediate commands to interrupt or change a running
turn. A plain-text question from an agent is not treated as a structured request.

## Evidence and verification

- Installed Codex app-server: read-only `collaborationMode/list` and
  `experimentalFeature/list`; generated experimental protocol types.
- Installed Claude CLI `--help`: plan permission mode.
- Published Cursor SDK 1.0.31 package: `options.d.ts`, `agent.d.ts`, and
  `createPlan` tool delta types. [Official SDK documentation](https://cursor.com/docs/sdk/typescript).
- ACP [session modes](https://agentclientprotocol.com/protocol/v1/session-modes)
  and [configuration options](https://agentclientprotocol.com/protocol/v1/session-config-options).
- OpenCode [agents](https://opencode.ai/docs/agents/), plus the integration's
  V1/V2 HTTP/SSE fixtures.
- Regression coverage: native mode discovery, disabled/missing Codex goal support,
  opaque/grouped ACP mode IDs, stale intent rejection, plan-exit question round
  trip, Cursor plan/create and build/resume, and existing harness input tests.

No hosted model turns were started for this audit. Fixture coverage verifies the
adapter contracts; it does not establish live end-to-end behavior for every
installed provider version, account, or optional ACP extension.

## Plan and task projection

Plan prose and task snapshots are independent. Failed or still-unconfirmed todo
writes do not replace the last successful snapshot. An authoritative empty list
clears the list. Native in-progress, blocked and cancelled states survive document
storage; old `done`-only documents remain readable. Cancellation never counts as
successful completion.

- Codex: streamed/completed plan items, `turn/plan/updated`, and legacy todo items.
- Claude: `TodoWrite`, plan-exit prose when supplied, and successful `TaskCreate`,
  `TaskUpdate`, `TaskList` results. Task-list JSON and the CLI's numbered text form
  are accepted; unrecognized results preserve the prior snapshot. Successful
  create/update/delete operations retain native task IDs as durable patches, so
  restarting or resuming the Claude process does not lose existing task state.
- Cursor: `createPlan` and `updateTodos`, including camel-case `inProgress`.
- OpenCode: `todowrite` and session-scoped `todo.updated`, including empty lists.
- All five ACP harnesses: native `plan.entries` snapshots when emitted. No
  checklist is inferred from arbitrary assistant text or slash-command names.

The tray starts with a compact summary. Details expand without replacing the
composer; goal status, plan prose, and task states remain independently visible.
The expansion retargets from its current height and respects reduced motion.
Questions and queues reduce the context budget. Switching conversations resets
expansion and scroll state, and collapsed streaming plan text is not repeatedly
parsed. Automated checks cover state projection, storage and animation math;
live appearance and frame timing still require a focused preview.

## Question contracts and command presentation

Mode choices are slash commands, not a permanent toolbar. Only choices advertised
by the selected model are offered; remote hosts also need `agent-modes-v1`.
Opaque ACP IDs survive unchanged. A provider's existing command wins its name;
the local mode command then uses `/zeron:<name>`. Selecting one changes the next
message's mode and consumes only the command token, preserving the draft.

Question constraints travel with each request, rather than being guessed from a
harness badge. The engine validates IDs, cardinality and allowed labels before
resolving a pending request. Invalid responses leave it pending; an empty response
is cancellation. Impossible choice-only requests with no options cancel immediately.

| Integration | Options | Custom text | Multiple choices | Descriptions | Non-blocking |
| --- | --- | --- | --- | --- | --- |
| Claude AskUserQuestion | Yes | Yes | `multiSelect` | Preserved | No |
| Claude plan exit | Implement / keep planning | Feedback keeps planning | No | N/A | No |
| Codex requestUserInput | Yes | `isOther`, or text-only question; legacy missing flag remains permissive | Legacy `multiSelect` only | Preserved | `isBlocking: false` |
| Codex assistant questions | Yes | Yes, sent as a follow-up/steer | No | Wire carries labels only | Yes |
| Codex approvals | Yes / No | No | No | N/A | No |
| OpenCode questions | Yes | `custom` (defaults true) | `multiple` | Preserved | No |
| OpenCode permissions | Yes / No | No | No | N/A | No |
| Devin / Grok / Hermes / Pi / Antigravity | ACP permission options | No generic free-text reply in the integrated ACP transport | No | Wire carries option names | No |
| Cursor | No public answer channel in pinned SDK | Unsupported | Unsupported | Unsupported | Unsupported |

Codex `isSecret` requests are explicitly rejected with a protocol error; the
composer does not claim to be a credential input. ACP extension-specific custom
answers are not inferred or fabricated. [Codex app-server protocol](https://developers.openai.com/ja-JP/docs/app-server)
, [OpenCode question schema](https://github.com/anomalyco/opencode/blob/dev/packages/schema/src/v1/question.ts),
and [ACP permission schema](https://docs.rs/agent-client-protocol-schema/latest/src/agent_client_protocol_schema/v2/client.rs.html)
are the source contracts; the installed generated Codex types were also inspected.

Question pages retain their typed answers when navigating back. A choice-only page
makes the text editor read-only and explains that an option is required. Questions
borrow and restore the current rich draft, including selection, rather than silently
discarding it. Empty pages cannot advance. Option descriptions wrap below labels.

## Transport contracts versus availability

`crates/harness/src/interaction_contract.rs` is the exhaustive adapter-level
contract for all nine production harnesses. The engine checks question shapes
against the adapter's response transport. ACP cannot accidentally advertise free
text/multiple replies through an option-ID response. Cursor cannot accidentally
present a question without an implemented response channel. Adding a HarnessId
requires updating this exhaustive mapping and its tests.

This contract intentionally does **not** advertise model tools as globally
available. Keep three layers separate:

1. Adapter transport: which native request/response operations Zeron implements.
2. Installed-session discovery: modes, enabled goal features, primary agents,
   model options and protocol-version limits.
3. Individual request: native IDs, allowed choices, custom text, multiple answers,
   blocking behavior and descriptions.

The observed Codex session rejected `request_user_input` in Default mode and
accepted it in Plan mode. Therefore “Codex question transport supported” must not
be read as “the tool is available in every execution mode.” Live protocol/session
behavior wins over a static product-level feature list.

Every new mapping should include a captured/synthetic native request, normalized
UI state, the exact native response payload, cancellation, invalid/stale IDs,
and turn-end/restart behavior. Pure capability booleans or mocked callbacks alone
cannot establish round-trip compatibility. The typed-answer fixture specifically
asserts what the fake Codex app-server receives on stdin; mismatched IDs now fail
explicitly rather than being turned into successful empty answers.

## Compaction lifecycle

Codex `contextCompaction` item start/completion becomes a typed `Compaction` part
with one stable native ID. The transcript renders a standalone divider with a
shimmering center label while active, then a static completion label. Reduced
motion removes the shimmer. Interrupted or failed turns stop the animation even
when the provider omits the completion item. Compaction does not get hidden inside
a generic “Called tools” fold and does not fabricate an assistant message.

Other adapters currently have no mapped compaction lifecycle in this contract.
Do not synthesize an indeterminate duration from a `/compact` command or assume a
completion notification supplies a start event. Their native signals need explicit
adapter mappings and round-trip fixtures before claiming equivalent progress UI.

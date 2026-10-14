# Native Codex subscription voice

Zeron connects a local Codex chat to the installed Codex voice runtime. Voice
uses ChatGPT authentication and the normal Codex plan budget. Codex may use
additional credits according to the user's account settings and usage limits.
There is no OpenAI API-key fallback, separate public Realtime API client, or
promise of an included-quota-only spending cap.

Sources checked 2026-10-01:

- [App-server](https://learn.chatgpt.com/docs/app-server)
- [Voice](https://learn.chatgpt.com/docs/features/voice)
- [Pricing](https://learn.chatgpt.com/docs/pricing#how-much-does-voice-cost)
- [Official Codex source, rust-v0.159.0](https://github.com/openai/codex/tree/377f7f557a6bdea0f3a2d26d4d899c66db4789d0/codex-rs),
  commit `377f7f557a6bdea0f3a2d26d4d899c66db4789d0`: app-server realtime schemas,
  `core/src/realtime_conversation.rs`, `realtime-webrtc`, and `voice-host`.

## Transport and authentication

Codex 0.159's WebSocket audio path calls `realtime_api_key` and requires API-key
credentials, including an environment-key fallback. That path is unsuitable for
this integration. Native subscription voice uses **WebRTC V3** instead:

1. Prepare or reuse an idle native thread with the normal Zeron MCP server.
   No initial `turn/start`, empty user message, title request, or submitted draft.
   Load the initial `account/read` snapshot before attaching the voice router:
   Codex announces initial authentication with `account/updated`, which must not
   be mistaken for a later identity change. Subsequent updates still retire voice.
2. Check `account/read` on that same process for `chatgpt` authentication, reject
   custom providers and realtime backend overrides in the effective project
   configuration (`config/read` with the resolved thread cwd), validate the
   provider returned by `thread/start` or `thread/resume`, inspect ordinary
   usage permission, and obtain `thread/realtime/listVoices`. Exhausted ordinary
   quota alone does not reject permitted credits; the native backend makes the
   final usage and spend-control decision. These reads run concurrently. A start
   skips the usage read (a network round trip that gates nothing) and its call
   reuses the engine's probe for ten seconds instead of repeating it; an
   identity change drops that result.
3. Resolve the helper inside the physical standalone Codex package (layout 1,
   version 0.159 or later). Its protocol/build handshake must succeed.
4. Initialize the helper and gather its SDP offer. Call `thread/realtime/start`
   with WebRTC, V3, audio output, the lease session id, startup context, and
   `clientManagedHandoffs: false`. Codex owns background task delegation.
5. The request response acknowledges submission. Independently wait for both
   `thread/realtime/started` and `thread/realtime/sdp`; validate session/version.
6. Apply the answer and wait for transport readiness. Open default local audio
   devices while muted/suppressed. Only enable them for an attached owner.

Audio, Opus, resampling, echo cancellation, reference audio, native interruption,
and bounded playout remain in Codex's helper. No PCM or SDP reaches the UI,
document, sync journal, or owner RPC stream. Helper stderr is discarded; errors
and diagnostics use typed rejection reasons. Child processes are killed and
reaped on cancellation, including when a helper control reply is stalled.

The earlier `zeron-audio` PCM/AEC prototype remains isolated and tested offline;
it is not a dependency of the released UI or the subscription voice path.
`voice-experimental` is retained as an empty compatibility build feature. The
Codex voice control is available in ordinary desktop builds.

## Lifecycle, transcript and MCP

One engine lease owns local voice. Tokens are redacted from Debug. Ownership is
exclusive, generation checked, ephemeral, and cancelled when its scoped stream
is dropped. A five-second unattached-owner watchdog prevents abandoned starts
from creating a provider call. A successor waits for native stop and its terminal
`requested` notification from that single stop; a spontaneous terminal cannot
satisfy the barrier and leave our terminal queued for a successor. Late events
cannot acquire the successor's generation. Native
capture termination is independent of the actor's pending control I/O. The
stdout reader also cancels capture on identity changes, overflow and EOF; it
does not wait for a blocked audio-control reply. Identity retirement invalidates
pending start reservations as well as existing leases. The physical Codex
release is pinned before spawning app-server so an installer symlink change
cannot select a different helper for a warm runtime. A reader identity change
also permanently invalidates the voice bridge before any new token can be
installed. A fresh explicit request can renew that idle runtime after cleanup;
continuing delegated work is preserved and must finish before renewal.

Voice protects the warm runtime from idle reaping and updates. Voice stop leaves
delegated Codex work running. Account/profile retirement, engine/window owner
loss, audio failure and provider termination close voice; navigating between
threads does not. An agent question keeps voice open: the stage reports it and
links to the session transcript, where the usual input UI answers it. Reconnection and
microphone resumption require a fresh user action. Changes to the chat host,
configuration or checkout invalidate pending starts and active ownership;
workspace monitoring also covers synced host/configuration changes.

Canonical `thread/realtime/item/completed` transcript segments are committed by
the serialized document owner, deduplicated by session/item identity. Legacy
transcript events and partial text never become durable messages. Only a newly
inserted canonical final updates sidebar activity and preview; replay does not
bump the timestamp. Native BEM
promotion uses the existing Codex task transcript; it does not resubmit a spoken
request or create another task response. Native `turn/started` publishes the
engine turn boundary before its text/tools, including on an idle bootstrap.

`thread/realtime/start` carries one short English instruction, as
`realtimeStartInstructions` for the backing Codex model and as a developer
`initialItems` entry for the voice model: work in this conversation, and use
the Zeron MCP to create, message and monitor other chats for work that belongs
elsewhere.

The voice agent uses the unchanged upstream Zeron MCP. Creating chats, sending
messages and delegating tasks use the normal provider selection, authentication
and device routing. A Codex voice session can create a Grok, Claude Code or other
available provider session; those children use their own provider credentials.
Codex/ChatGPT authentication and local-host checks apply to starting voice itself,
not to the sessions it controls through MCP. Ending voice does not restrict
continuing tasks or their children.

## Interface and packaging

A voice chat is a conversation, as in Codex's own apps ("Start voice chat" in
a task, "Start new voice chat" in a new one; a voice chat can be reopened and
resumed; one is active at a time,
[learn.chatgpt.com/docs/features/voice](https://learn.chatgpt.com/docs/features/voice),
checked 2026-10-02). It is not a global orchestrator beside the conversations:

- From a Codex conversation on this device, voice continues **that**
  conversation: its thread, project, model and history. What is said lands in
  its transcript, and the conversation stays where it was in the sidebar.
- From the new-session canvas (or the shortcut, from a conversation voice
  cannot run in), voice starts a **new** conversation in the selected project
  when that project is on this device, otherwise with none. It is an ordinary,
  visible session titled "Voice session" and is removed again if voice never
  starts in it. Its model follows the composer when Codex is selected,
  otherwise Codex's default.
- A conversation on another harness or another device does not offer voice.

Either way it can be left and returned to, and opening it later and pressing
the voice button resumes by voice. Conversations created by the earlier hidden
orchestrator keep their `voice-orchestrator-` id and stay out of the lists.

Voice starts from the composer: while the input is empty and nothing is running,
the send slot is a voice button, and it becomes Send again as soon as there is
something to send. The shortcut does the same. While a voice chat is live the
microphone is taken: no composer offers voice or dictation.

Each model is told what is its own (`crates/harness/src/codex/realtime.rs`):

- The backing Codex model gets `realtimeStartInstructions`. The parameter
  replaces Codex's own framing of a realtime session (seen in
  `~/.codex/sessions`: the developer message holds Zeron's text alone), so the
  text restates it (a backend executor behind a voice intermediary, a
  transcript that may be misrecognized), says where the conversation works
  (its project, or the home directory with no codebase to work on), and routes
  by what the user wants to end up with: do contained work here; use the Zeron
  MCP tools, named so Codex need not search for them, for anything about other
  chats; start a chat with `wait: false` for work that outlives the call, runs
  in parallel or belongs elsewhere, asking it to report back. It must never
  block on another chat, and a message from another chat is a report to pass
  on, never something to acknowledge.
- The realtime voice model gets one developer item in `initialItems`. It has
  no MCP tools, so it is told none: only that the user cannot read the
  backend's text on the call screen, to name chats rather than read ids, and to
  say each report from another chat when it arrives.
- `realtimeEndInstructions` is Codex's own text for the end of a call, plus the
  rule that outlives it: a chat the call started may still report afterwards.

Two start options are on trial behind the environment, off by default:
`ZERON_VOICE_REASONING_STATUS=1` (`backendReasoningStatus`, reasoning summaries
as quiet context for the voice model, which may fill long turns) and
`ZERON_VOICE_ACK_FILLER=0|1` (`delegationAckFiller`).
`ZERON_VOICE_STARTUP_CONTEXT=0` starts without Codex's startup context, to
measure what building it costs a call's start.

A message one chat sends another carries a header naming the sender
(`zeron-mcp`'s `attribution`). A sender waiting on the turn (`wait: true`)
receives the answer as its tool's result, so the receiver is told not to
message it back; otherwise it is told how to answer and not to reply only to
acknowledge. The earlier header always asked for a reply, which kept two chats
acknowledging each other after a call had ended.

The live voice chat's conversation is shown as the call screen, over the
conversation area (the whole window when the sidebar is collapsed); the titlebar
cluster stays above it. The screen paints no surface of its own: the
conversation, its panes and its title bar fade out beneath it, so it stands on
the window's material (frosted or opaque, as configured) like any conversation. Opening that conversation opens the screen; moving to
another thread, a new session or Settings leaves it without ending voice. There
is nothing to minimize it to, and Escape only closes a failure shown there.
The screen is not a sheet that owns the keyboard: whatever is open over it (the
command palette, a menu, a dialog) takes Escape first, Escape never stops the
call's turn, and the session shortcuts work from it as from any conversation.

What the voice chat is doing is never spelled out and never an icon: the orb
says it (`crate::voice::orb`). The orb is a sphere of glass with a fluid in it.

The glass is the optical envelope of the Glass Orb reference
(github.com/gaelcado/glass-orb), ported from its shader, and it is there to
see whatever is in it:

- The fluid fills the glass almost to its edge (98.5% of its radius, fading
  over its outer sixth), so what is left clear round it is the glass's own
  edge.
- The fluid has clear water in it: where there is little dye there is none to
  see, and the surface behind shows through. The voice feeds it dye slowly
  and the dye returns to its mist within a couple of seconds, so a voice
  shapes the fluid instead of filling the vessel.
- A thin rim of light runs round the silhouette, brighter towards the upper
  left, at the reference's own strength (half white at its brightest on a
  dark surface: nearer full white it read as a ring drawn round the orb). On a dark surface it is the glass's only edge, as on the reference:
  nothing else is drawn inside it (a sheen there read as a second border, and
  a highlight as a smudge on an empty glass).
- A Fresnel atmosphere (`pow(1 - facing, 2.8)`) of the accent gathers at the
  limb, in the glass itself, behind the fluid. On a dark surface it gathers
  opposite the light, where a glass ball glows with what passes through it,
  and leaves the lit side to the rim; all round, it too read as a second edge.
- On a light surface the light on the glass lies over whatever is in it: a
  faint bloom on the sphere's shoulder and a sheen just inside the edge (not
  on the bar's small orb, where it would thicken the rim).

Nothing is cast around it: no glow, no shadow.

The fluid is simulated, not drawn from a recipe: an incompressible flow in a
round vessel, on a 48-cell grid (24 in the bar), carrying three dyes and seen
through the sphere as through a lens. Each step the flow is pushed, has its
small eddies put back (vorticity confinement), carries itself along, thickens,
comes to rest at the glass, and is made incompressible again (a pressure
solve). The dyes ride it: how much fluid shows, which of its colours, and what
has just been lit. So it has inertia: a push curls into eddies that take
seconds to settle.

Its colour is the accent as the rest of the app shows it: the theme's own
tones of the chosen accent (`Theme::glyph`, the family the app's mark is drawn
in), namely the fill its accent controls carry, the accent, and its light
tone. The orb never turns the hue, so an orange accent gives an orange fluid,
not a yellow one. On a dark surface the fluid at rest is dark: its body lies
between that fill and a shade of it (62% of its light, so the same colour and
not a more vivid one), the accent itself is what Codex's voice lights, and the
light tone is kept for the peaks. On a light surface, where a dark veil reads
as mud and light cannot add, the fluid is ink in clear glass: a tint of the
accent where it is thin, the accent where it is thick, and the accent's fill
where the voice is (never darker than what the app's accent controls carry),
slightly denser (110%), and shaded towards its own limb so a ball of it reads
as a ball. How much fluid there is goes into the image's opacity. Waiting on
an answer uses tones of the warning colour the same way.

The two voices are two fluids and do not share a colour. Codex's is the accent.
The user's is the surface's own ink (near white on a dark surface, a soft graphite on a light
one, with a third of the accent's light tone in it), so one is never taken for
the other.

What moves the fluid is the two voices themselves, whatever the call's state
says (a state is confirmed a moment late; a voice is not). Each voice reaches
the orb as how loud five bands of it are (`crate::voice::hearing`):

- The user's voice is measured from the microphone by Zeron itself
  (`zeron_voice::spectrum`), beside the call's own capture: 90 to 280 Hz (the
  pitch), 280 to 750 and 750 to 2000 (the two vowel resonances), 2 to 4.5 kHz
  (the consonants' presence) and 4.5 to 9 kHz (sibilants). Each band is a
  fourth-order filter; all five read on one scale that follows the voice (its
  top is the loudest any band has lately been, each band's floor the quietest
  it has been), so the reading is the same whatever the microphone's gain and
  a steady noise reads as silence. Levels only: no sample is kept, sent or
  written. The input is open only while a call this window runs is connected,
  is paused while the microphone is muted, and is closed when the call ends.
  `ZERON_VOICE_SPECTRUM=0` turns it off, and the user's voice is then followed
  as Codex's is.
- Codex's voice never leaves the call's audio process (Codex's packaged WebRTC
  helper), which reports only a peak, so its five bands are the rhythms of
  that loudness instead: the phrase, the syllable, the level, each syllable's
  attack and its flutter, heavy to fine as a spectrum runs low to high. The
  helper is asked every 40 ms (it was 100), often enough to tell one syllable
  from the next. A true spectrum of Codex's voice would need a tap on that
  process's audio output, which macOS grants only behind its own recording
  permission.

A voice acts on the fluid in two ways.

At once, before the fluid has moved (`Touch`): the fluid shows fuller, in
every live state, swells towards the eye and has rings sent out through it
from the centre, a long wave for a low band and short ripples for a high one.
The glass itself holds still: only what is in it moves. These rise in 30 to
60 ms and fall in 160 to 350.

Then as a force: each band pushes from its own place in the vessel, as hard
as it is loud, with a kick at each onset. A low band is broad and
thickens the fluid; a high one is narrow, spins small eddies and lights it,
and puts back more of the fine eddies while it sounds. The bands go straight
out from the centre, curling one way and the other: a bloom from the middle,
which does not set the fluid turning.

Both voices move the fluid alike; what tells them apart is the light each
brings into it. Codex's lights the accent; the user's lights the surface's own
ink (a pale light on a dark surface, a soft graphite on a light one), which
mixes with the fluid rather than covering it. The user's bands push from the
places between Codex's, so two voices at once do not lie on one another. Each
light fades within a couple of seconds of silence. An earlier version made the
user's voice a pale sand lying at the bottom of the glass, raised by their
voice; it read as a different object rather than a second voice. Speeds are
in vessel radii a second: at rest 0.17; Codex speaking, peaks of 1.4.

On top of that, each state stirs the fluid its own way, and the material
glides between states over about a second:

- Connecting: the orb being born. The glass is empty but for a seed of the
  accent's light fed just off its centre, which grows for as long as the call
  takes to connect; once it does, the fluid fills the glass from it within a
  couple of seconds. Ending is the same the other way: the fluid thins back
  to the seed.
- Working: the whole fluid turns about its centre and winds a lit thread into
  a spiral (0.5).
- Looking something up (the call's running tool is a web or project search or
  a fetch): one stirrer circles the vessel and leaves a lit wake (0.9).
- Waiting on an answer: the warning colour, with a puff rising from the bottom
  every 1.6 s.
- Muted: grey and thick, brought to a stand.

A state has to last before the orb follows it: it stays with a speaker for
900 ms of silence, so the pauses inside a sentence do not flick it between
speaking and listening; a speaker starting is followed after 120 ms; a mute and
the call's own phases are immediate. Under reduced motion the fluid stands
still in each state's colours and brightness.
The state's words stay available to assistive technology, and as the floating
bar's tooltip.

GPUI has no per-element shader, so each frame is computed on the CPU and
painted as an image whose predecessor is dropped from the atlas. The fluid is
painted at 64 px on the call screen and 44 px in the bar: it is soft, and its
grid is coarser still. It is then given its grain (`sand`): the fluid is a
little sandy, each pixel up to 26% more or less of it than its neighbours, on
an image redrawn twice as fine where the disc has the pixels for it. The grain
is fixed in the glass, so it shows where the fluid passes, and part of it
shimmers while a voice sounds. The image fills its square and the painter clips it to
the disc at the display's own resolution, so the silhouette is crisp at any
size; the rim, which has to be crisp too, is computed once at that resolution
and laid over it, and kept between entrances (at the call screen's size it
costs a frame). On a light surface, where light cannot add, the glass is told
by its dark: a shade inside the edge and a hairline on it, both deeper away
from the light, with the same band as a white glint and the light's bloom on
the sphere's shoulder. The band is a fixed few pixels wide, right
for the call screen's orb and a white collar on the bar's 22pt one, so under
240 device pixels across it narrows and fades, down to 45% of its width and
strength at 48: a hairline. In the unoptimised dev profile a frame costs about 8 ms
on the call screen (3.0 ms of simulation, 2.7 ms of painting with both voices
sounding, 2.4 ms of grain) and 2.3 ms in the bar, and the microphone's bands about 5 ms for each second of sound;
release builds are faster.

The orb and what is said under it are one group. The orb is 24% of the room
between the title bar and the controls (104 to 176pt); the words start 12pt
under its slot. With a usual turn under it (three lines) the group stands 60%
of the way down that room, below the middle, so the artwork has the upper half
of the screen to itself and the group sits nearer the controls. The orb holds
that place
from the moment the call is placed: a longer turn takes the room under it. The
controls are a group of their own, 24pt from the window's edge, and what is
said never comes within 32pt of them. While the call connects it is held
back (92% of its size, 60% of its presence); when the call connects it comes
alive over 360 ms where it stands and the feed appears under it. Nothing on the
screen moves. The orb rides the bar's 30 fps clock. A voice chat that could
not start, or that dropped, stays on the screen to say why, with Try again and
Close; leaving the screen puts the failure away.

The screen comes and goes on one linear clock (420 ms in, the app's own page
transition, and 220 ms out), which a
change of mind turns around from where it is. Placing a call takes a longer way
in (`PLACING`, 1.6 s): Codex takes two to three seconds to answer, and the
screen spends the first of them arriving. The page leaves at once (190 ms), the
orb grows from half its size as its glass and its seed of light appear (the
seed goes on growing until the call connects, so the arrival and the wait are
one movement), and the controls come over the last 600 ms. Coming back to a
call already under way keeps the short way in. One frame counts for 34 ms at
most on that clock, and on the floating bar's: a frame that took long (the
page under the screen being built, the call starting) holds the motion up
instead of swallowing it. Any frame of a call transition longer than 80 ms is
logged (`zeron_ui::voice`, "slow frame in a call transition", with where the
screen and the bar were), so what holds a transition up can be read from the
log of the run it happened in. Its parts take their turn on it:
what it replaces leaves first (gone after 170 ms, and unmounted from then on),
the orb arrives, and the context row and the controls follow it by about
85 ms, each travelling 12pt. Over the new-session screen the artwork does not
cross-fade: the call screen's copy stands in at full strength from the first
frame, in the same crop, so the picture holds still while that screen leaves
from under it. Over a conversation there is no artwork yet and it comes in
with the screen. Every clock here stretches with `ZERON_MOTION_SCALE`, the
app's slow-motion knob, for review. A conversation created for the call is entered only once the
screen covers the new-session screen, so that route's own travel (the composer
docking, the artwork dissolving) never shows through. A failure stands where
the orb stood.

The conversation shows what is said as it is said. The engine records an
utterance only once it is finished (`zeron_doc::voice::commit_voice_transcript`
takes final text, never partials), which left the transcript a sentence behind
the call. Until that record arrives each utterance is a local echo, updated as
its words come in and carrying the id its record will have
(`zeron_doc::voice::entry_id`), so the recorded entry takes the echo's place
without a seam. Only the transcript is told of each update, not everything that
observes the workspace. What was still being said when the call ends is never
recorded and leaves with the call.

The screen is a call, not a second view of the conversation. Where the call
runs is said at the bottom's leading edge, on the controls' own line, where a
conversation keeps its status strip: the device and the project ("No project"
when it runs at home) in the new-session screen's quiet chips, and the
conversation's name before them when it had one before the call. It steps
aside when the screen is too narrow to hold it beside the controls. The title
bar carries nothing, and nothing stands between the feed and the controls.
Right under the orb sits one
feed: the turn the call is in, top to bottom in the order it happens, with no
surface around any of it.

- What the user asked heads the turn, two lines at most. It is set exactly as
  what Codex says (18pt on a 28pt line) and stands straight above it, so the
  two read as one block of text, told apart by their ink alone: the user's
  rests faint, Codex's ends in full ink.
- What Codex does about it follows: one line per tool call, written as the
  transcript writes it (`transcript::call_line`: its icon, or a spinner while
  it runs; its label; what it acted on; a file's badge), up to three, the
  latest in full ink and the earlier ones receding. More than three are
  counted on a leading "N earlier" line that leads to the conversation. The list keeps the calls made since the user last spoke, so
  they stay in view while Codex reports on them, plus any call still running.
- What Codex says closes it, as the caption: a subtitle's measure, four lines
  at most (a longer one pushes its oldest line out through the top). The two
  share one room of five lines: under a long answer a long question gives up
  its first line through the top, instead of each keeping a room of its own. While it is being said its newest three words are in full ink and the
  rest rests behind them; a pause of 1.2 s lets them all rest; once finished it
  reads as one.

The feed only grows downwards, so nothing in it moves except to make room for a
new tool line, which the list eases open over 240 ms. It has one life: a new
turn takes its place with a short fade, and ten seconds after the turn's last
sign of life (a word, a tool call starting or finishing) the whole feed fades
away as one. It stays while Codex is working or speaking and while the pointer
is on it. Codex announces who is speaking when an
utterance starts (`thread/realtime/item/started`); the harness passes that on
as a text-less partial with a `role`, so live text is attributed before its
final segment arrives. The conversation's transcript keeps everything.

Each tool line is a control of its own, with no surface: it answers the
pointer with its ink and with a mark for where it leads.

- A fetch opens its page in the browser. Only `http` and `https` addresses
  leave the app.
- A Zeron call goes to the chat it concerns, the call carrying on in the
  floating bar; a side chat opens beside the voice chat's conversation. The
  record keeps only a summary of a tool's result, which rarely holds the new
  chat's id, so a created chat is told by what appeared since the turn began,
  in the order it was created (a `create_chats` batch tells none apart);
  other single-chat tools are told by the id their result names.
- Any other call is shown in the conversation, where it opens as usual. A
  line does not open in place on the call screen.

With hero artwork configured, the artwork is the new-session screen's own crop
(`new_thread_background_mask::paint_above`), dissolving from the top of the
window down to the orb's centre: one fade across the whole background, with
nothing cut out of it. The orb stands on the horizon that leaves and everything
the call says is below it, on the window's own material.

The controls are one surface in the composer's own material
(`crate::voice::VoiceDock`): its tint, its hairline, a full round pill, and its
shadow where the theme has no blur. On the call screen it is 44pt tall and no
wider than what it holds (the orb above it carries the call), with 32pt round
controls inset 6pt, so its ends are concentric with them; it is not blurred
there, since nothing is under it. The bar holds three groups, told by spacing alone:
the two neutral controls together on the leading side (mute, then the
conversation, 6pt apart), the clock, and the hang-up on the trailing side, the
clock 14pt from each. The clock is text on the bar, in no box of its own (its
place kept before there is one).
Mute and the conversation control are
dictation's raised neutral glass plate (`crate::dictation::glass::light`: lit
from above with a soft drop under it, opaque in light appearances), not the
compact model picker's, whose light-appearance plate is sunk into the surface
and read as a hole in the bar; the surface itself is the track. Mute keeps its place from the
first frame (inert until the microphone is open, so nothing shifts when the
call connects); its two icons hand over with a scale and a fade over 300 ms
while the plate inverts. The hang-up is the filled glass plate on the theme's
destructive fill (`danger_strong`, as every destructive button) and holds
still. Controls dim on hover and further while pressed. On the call screen a
plate before the hang-up shows the conversation: the call sets its screen aside
and the ordinary transcript and composer are in view, with the compact row
above the composer as the way back; it turns to the warning colour while Codex
waits on an answer there.

Outside the call screen the same surface is one floating bar, always in reach
(`crate::voice::float`): mute, the orb, the clock and the hang-up, so a call
being run from several chats keeps its controls in view on every screen. In a
conversation it stands above the composer, centred, in room the composer keeps
for it. On a screen with no composer (the new-session screen, Settings) it
stands in the top trailing corner, 14pt from the top and from the trailing
edge (centred on the title bar's line it had no air above it). It passes from one to the other as the screen changes without crossing the
screen: it leaves the place it stands at over the first half of the composer's
own travel (420 ms to dock, 470 ms back) and arrives at the other over the
second, moving 8pt as it does. Leaving a conversation its room above the
composer goes at once, so the bar goes with it and then arrives in the corner.
The floating bar and the call screen's own bar never show together: the
floating one leaves where it stands as the screen is asked for, and comes back
only once the screen's controls have left. A call screen opened by moving into
its conversation does not fade that conversation out from under it (nobody has
seen it yet): the page left behind is cut and the screen's parts arrive, the
artwork holding still when the page left behind showed it. It is 36pt tall
with dictation's 28pt controls inset 4pt, blurred like the composer and no
wider than what it holds. While it is docked there, the transcript's "Scroll to
bottom" pill, whose place above the composer it has taken, stands beside it on
its trailing side and on its line (above it, as before, when the composer is
too narrow). Its orb is the same one at 22pt, so nothing repeats
the controls beside it. The state's
words stay as the orb's tooltip and for assistive technology. What is said is
not captioned here: it is heard, and read in the conversation or on the call
screen. Pressing the orb (which answers the pointer with a wash) returns to the
call screen, as does the conversation's sidebar row. A start that failed while
the call screen was set aside explains itself in the composer's room instead.

Settings → Voice has a Realtime voice switch (on by default). Off hides the
composer's voice button and disables the shortcut (`mod-shift-d`, rebindable); a
live session is unaffected. Native voice ids from the provider catalog
can be selected for the next session. Devices use the operating system defaults:
Codex's current helper protocol does not expose device selection, so the
Microphone row names the system input instead of offering a choice.

A call sounds at both ends (`docs/sound-design`): one cue when it connects and
Codex is listening, one when a connected call ends, under Settings → Voice →
Call sounds. A failed start or a dropped call plays the shared attention cue
under its Notifications setting; a start abandoned before connecting is silent.
The cues play through the system output, outside the helper's echo
cancellation, so the start cue can reach the open microphone on speakers.

Each startup stage logs its duration (`zeron_engine::voice` for the runtime and
probe, `zeron_harness::codex::voice` for the helper offer, provider answer,
transport and devices), so a slow start names its stage in the engine log.

The native helper/runtime is supplied by the standalone Codex installation,
not redistributed by Zeron. npm/CLI-only installations without those resources
report `nativeRuntimeUnavailable`. macOS production/dev bundles declare microphone
usage and audio-input entitlements. The viewport verifies microphone authorization
and requests it when voice is explicitly started, before creating a provider
session or opening devices. Denied access shows a settings hint. Cargo builds
embed the microphone purpose string in the executable's `__TEXT,__info_plist`
section, so direct development launches can also request access. Missing
metadata shows a rebuild hint instead of requesting permission and being
terminated by macOS. `scripts/run-macos-dev.sh` builds the
signed development bundle; `ZERON_DEV_BUILD_ONLY=1` prepares it without launching.
No new DSP DLLs or C++ build dependency are
added to the production UI. Apache-2.0 attribution for the adapted native helper
protocol is included in `THIRD_PARTY_NOTICES.md`.

## Rendering cost during a call

A child view's `notify` dirties its ancestors, and only the sidebar is a cached
view, so every animation tick of a call redraws the window. What keeps a frame
cheap (read from the code on 2026-10-02, not profiled):

- Once the call screen fully covers the conversation, the conversation, its
  panes and its title bar are not mounted: at opacity 0 they would still be
  laid out, painted and blurred on every frame.
- The shell notifies its own observers (the sidebar first) only when the
  call's phase, screen or conversation changes. Levels and transcript deltas
  redraw through the call's own rows, which notify themselves.
- Spinners on the call screen pulse through the control bar's view, not the
  shell, and entrance fades are the short opacity-only one: a 500 ms entrance
  redraws the window at display rate for as long as it runs.
- On the call screen the control plates have no backdrop blur (each one is a
  render pass of its own, and nothing scrolls under them there).
- The tool activity reads the end of the conversation's record instead of
  merging and sorting all of it on every frame.
- A full event queue drops meter readings rather than ending the call.

- The settled transcript is a cached view again (it draws fresh only while it
  fades or glides between routes), so a tick of the compact row or of the
  composer's caret redraws the window without rebuilding it.
- The orb's frames are counted on the display's own clock
  (`on_next_frame`): one every 33 ms, evenly spaced at 60 and 120 Hz, instead
  of a free-running timer that landed between refreshes.
- Render paths read settings in place (`settings::read`) instead of cloning
  the whole struct, and the shell merges its working copy with the store only
  when the store's revision moved.

`ZERON_FRAME_STATS=1` logs per-view render cadence on a development build.

## Validation and practical limits

Offline CI never starts a provider session or microphone. Fake app-server and
helper peers exercise actual engine RPC, independent start acknowledgements,
SDP signaling, owner attachment, native controls, transcript replay, task
continuation, stop/restart and authentication rejection. Synthetic audio tests
remain offline. Cross-platform checks compile the native bridge without loading
hardware; signed package/device trials require their actual operating systems.

Read-only discovery:

```sh
python3 scripts/codex-voice-probe.py
```

Explicit opt-in connectivity smoke (consumes normal Codex voice usage, does not
open microphone devices):

```sh
python3 scripts/codex-voice-smoke.py --live
```

Optional device/playout check opens default devices with the microphone **muted**
and requests a short greeting:

```sh
python3 scripts/codex-voice-smoke.py --live --devices
```

Evidence on 2026-10-01, Linux, installed Codex 0.159.0: ChatGPT authentication,
packaged helper initialization, real V3 WebRTC negotiation, transport readiness
and native stop passed. No API fallback was used. The optional device trial
failed at opening audio devices: this environment exposes a network output
monitor, without a usable default microphone. No real speech or acoustic echo
trial is claimed. macOS/Windows signed package and physical-device trials also
remain unverified; protocol/fixture success does not establish those results.

Implementation review and local test results are recorded in the ignored
`.personal/codex-voice/` working notes. No push, deployment or remote PR is part
of this task.

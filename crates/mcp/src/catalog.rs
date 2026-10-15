//! The built-in Zeron MCP tool catalog: every tool's name, description and
//! input schema, plus where it may be offered.
//!
//! Pure data with no engine connection, so the stdio bridge ([`crate::Tools`])
//! and the engine (tool policy, Settings) read one source. [`ToolDef`] is the
//! static form served by `tools/list`; [`descriptors`] gives the owned
//! [`McpToolDescriptor`] that crosses RPC. Each name maps to exactly one
//! [`ToolName`], which `Tools::call` dispatches on exhaustively.

use serde::Serialize;
use serde_json::{Value, json};
use zeron_proto::{McpToolAvailability, McpToolDescriptor};

/// Version of the catalog's names, descriptions and schemas. Bump it whenever
/// any of them changes; it is unrelated to the policy revision.
pub const CATALOG_VERSION: u32 = 1;

/// Most requests one `create_chats` / `send_messages` call takes.
pub const MAX_BATCH: usize = 32;

/// A tool exactly as `tools/list` serves it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}

impl ToolDef {
    /// Where this tool may be offered, before any policy applies.
    pub fn availability(&self) -> McpToolAvailability {
        // Every catalog name parses (see the catalog tests).
        ToolName::parse(self.name).map_or(McpToolAvailability::Always, ToolName::availability)
    }

    /// The owned form that crosses RPC.
    pub fn to_descriptor(&self) -> McpToolDescriptor {
        McpToolDescriptor {
            name: self.name.to_owned(),
            description: self.description.to_owned(),
            input_schema: self.input_schema.clone(),
            availability: self.availability(),
        }
    }
}

/// One built-in tool. `Tools::call` matches on this exhaustively, so a new
/// variant cannot ship without a handler; the catalog tests tie each variant
/// to exactly one [`ToolDef`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolName {
    Whoami,
    ListDevices,
    ListProjects,
    ListHarnesses,
    ListModels,
    ListChats,
    GetChat,
    CreateChat,
    ReadChat,
    SendMessage,
    WaitForTurn,
    InterruptChat,
    RespondToInput,
    ArchiveChat,
    CreateChats,
    SendMessages,
    ShowVisual,
}

impl ToolName {
    /// Every tool, in catalog order.
    pub const ALL: &[ToolName] = &[
        Self::Whoami,
        Self::ListDevices,
        Self::ListProjects,
        Self::ListHarnesses,
        Self::ListModels,
        Self::ListChats,
        Self::GetChat,
        Self::CreateChat,
        Self::ReadChat,
        Self::SendMessage,
        Self::WaitForTurn,
        Self::InterruptChat,
        Self::RespondToInput,
        Self::ArchiveChat,
        Self::CreateChats,
        Self::SendMessages,
        Self::ShowVisual,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Whoami => "whoami",
            Self::ListDevices => "list_devices",
            Self::ListProjects => "list_projects",
            Self::ListHarnesses => "list_harnesses",
            Self::ListModels => "list_models",
            Self::ListChats => "list_chats",
            Self::GetChat => "get_chat",
            Self::CreateChat => "create_chat",
            Self::ReadChat => "read_chat",
            Self::SendMessage => "send_message",
            Self::WaitForTurn => "wait_for_turn",
            Self::InterruptChat => "interrupt_chat",
            Self::RespondToInput => "respond_to_input",
            Self::ArchiveChat => "archive_chat",
            Self::CreateChats => "create_chats",
            Self::SendMessages => "send_messages",
            Self::ShowVisual => SHOW_VISUAL,
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|tool| tool.as_str() == name)
    }

    /// Intrinsic context constraint, independent of policy.
    pub const fn availability(self) -> McpToolAvailability {
        match self {
            Self::ShowVisual => McpToolAvailability::ChatOnly,
            Self::Whoami
            | Self::ListDevices
            | Self::ListProjects
            | Self::ListHarnesses
            | Self::ListModels
            | Self::ListChats
            | Self::GetChat
            | Self::CreateChat
            | Self::ReadChat
            | Self::SendMessage
            | Self::WaitForTurn
            | Self::InterruptChat
            | Self::RespondToInput
            | Self::ArchiveChat
            | Self::CreateChats
            | Self::SendMessages => McpToolAvailability::Always,
        }
    }
}

/// The full catalog as owned descriptors, in catalog order.
pub fn descriptors() -> Vec<McpToolDescriptor> {
    catalog().iter().map(ToolDef::to_descriptor).collect()
}

fn chat_key_schema(extra: Value) -> Value {
    let mut properties = json!({
        "chat": {
            "type": "string",
            "description": "Chat id, unique id prefix, or exact title."
        }
    });
    if let (Some(base), Some(more)) = (properties.as_object_mut(), extra.as_object()) {
        for (k, v) in more {
            base.insert(k.clone(), v.clone());
        }
    }
    json!({ "type": "object", "properties": properties, "required": ["chat"] })
}

fn device_schema() -> Value {
    json!({ "type": "string", "description": "Host device id or exact name. Omit for the local engine." })
}

/// Every built-in tool, in the order `tools/list` reports them.
pub fn catalog() -> Vec<ToolDef> {
    let mut tools = vec![
        ToolDef {
            name: "whoami",
            description: "Which chat and device this server speaks for, plus the engine's workspace mode. Call this first when you need to know your own chat id.",
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        ToolDef {
            name: "list_devices",
            description: "Devices in this workspace (the local engine's device is flagged). Chats and projects are hosted on a device.",
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        ToolDef {
            name: "list_projects",
            description: "Projects: folders on specific devices. Filter by device (id or name); omit for all projects. Use ids when names or paths repeat.",
            input_schema: json!({ "type": "object", "properties": { "device": device_schema() } }),
        },
        ToolDef {
            name: "list_harnesses",
            description: "Agent harnesses (claude-code, codex, cursor, …) available on the chosen device. Omit device for the local engine.",
            input_schema: json!({ "type": "object", "properties": { "device": device_schema() } }),
        },
        ToolDef {
            name: "list_models",
            description: "Models a harness offers on the chosen device (local engine when omitted). Model ids are harness-specific strings; pass one to create_chat.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "harness": { "type": "string", "description": "Harness id, e.g. claude-code or codex." },
                    "device": device_schema()
                },
                "required": ["harness"]
            }),
        },
        ToolDef {
            name: "list_chats",
            description: "Chats with their project, host device, harness/model, live status, and last activity. Newest activity first.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project": { "type": "string", "description": "Only chats in this project (id, path, or name)." },
                    "device": { "type": "string", "description": "Only chats hosted on this device (id or name)." },
                    "include_archived": { "type": "boolean", "default": false },
                    "parent": { "type": "string", "description": "Only chats created by this chat (id, prefix, or title) — e.g. your own id to list the chats you spawned." },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 500, "default": 50 }
                }
            }),
        },
        ToolDef {
            name: "get_chat",
            description: "One chat's metadata, live status, and any question its agent is currently blocked on.",
            input_schema: chat_key_schema(json!({})),
        },
        ToolDef {
            name: "create_chat",
            description: "Create a standalone session visible in Sessions with kind chat, or a child with kind side. Omitted kind preserves the default: side with an origin/parent, standalone otherwise. A side chat cannot create chats. Select a project and/or device; catalogs are validated on that host. Optionally send a first prompt and wait for the reply. Returns the new chat id. For parallel delegation use create_chats, or leave wait=false on every launch and wait only after all chats have been started.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "kind": { "type": "string", "enum": ["chat", "side"], "description": "chat: standalone, no parent allowed. side: requires parent or origin. Omit for legacy defaults." },
                    "project": { "type": "string", "description": "Project id, path, or name. Belongs to a specific device; when device is supplied, search only there. Optional." },
                    "device": { "type": "string", "description": "Host device (id or name). With project, it must own that project. Without either, use the local engine." },
                    "parent": { "type": "string", "description": "Parent chat to record (id, prefix, or title). Defaults to the chat you are speaking from." },
                    "harness": { "type": "string", "description": "Harness id (see list_harnesses with the chosen device). Defaults to claude-code when available." },
                    "model": { "type": "string", "description": "Model id from list_models on the chosen device. Omit for the harness default." },
                    "reasoning": { "type": "string", "description": "Reasoning level the model supports (e.g. low, medium, high, max)." },
                    "sandbox": { "type": "string", "enum": ["read-only", "workspace-write", "danger-full-access"], "default": "workspace-write" },
                    "title": { "type": "string", "description": "Sidebar title. Otherwise the engine titles it from the first exchange." },
                    "branch": { "type": "string", "description": "Branch label to record on the chat." },
                    "cwd": { "type": "string", "description": "Working directory override (an existing worktree path). Defaults to the project folder." },
                    "prompt": { "type": "string", "description": "First message to send right away." },
                    "wait": { "type": "boolean", "default": false, "description": "With prompt: block until the first turn finishes and return the reply." },
                    "timeout_secs": { "type": "integer", "minimum": 1, "maximum": 3600, "default": 600 }
                }
            }),
        },
        ToolDef {
            name: "read_chat",
            description: "Read a chat's transcript as plain messages (newest window by default). Tool calls are summarized one per line.",
            input_schema: chat_key_schema(json!({
                "limit": { "type": "integer", "minimum": 1, "maximum": 500, "default": 40, "description": "How many messages, counted from the newest." },
                "offset": { "type": "integer", "minimum": 0, "default": 0, "description": "Skip this many newest messages (paging backwards)." },
                "include_reasoning": { "type": "boolean", "default": false },
                "include_tools": { "type": "boolean", "default": true }
            })),
        },
        ToolDef {
            name: "send_message",
            description: "Send a message to a chat. Messages are attributed to your chat. mode 'auto' starts a turn when the chat is idle, steers a running turn through its live mailbox (at the next supported input boundary without interrupting the agent). Use mode 'queue' only to explicitly hold a message for later. With wait=true, blocks until the turn finishes and returns the assistant's reply. For parallel work use send_messages, or send to all chats with wait=false before waiting.",
            input_schema: chat_key_schema(json!({
                "text": { "type": "string" },
                "mode": { "type": "string", "enum": ["auto", "run", "steer", "queue"], "default": "auto" },
                "wait": { "type": "boolean", "default": false },
                "timeout_secs": { "type": "integer", "minimum": 1, "maximum": 3600, "default": 600 }
            })),
        },
        ToolDef {
            name: "wait_for_turn",
            description: "Block until a chat is no longer working: returns completed, awaitingInput (answer with respond_to_input), errored, or timedOut, with the newest assistant message.",
            input_schema: chat_key_schema(json!({
                "timeout_secs": { "type": "integer", "minimum": 1, "maximum": 3600, "default": 600 }
            })),
        },
        ToolDef {
            name: "interrupt_chat",
            description: "Stop the chat's running turn.",
            input_schema: chat_key_schema(json!({})),
        },
        ToolDef {
            name: "respond_to_input",
            description: "Answer a question the chat's agent is blocked on (see get_chat / wait_for_turn pendingInput). Labels are the option strings, or free text for open questions.",
            input_schema: chat_key_schema(json!({
                "request_id": { "type": "string", "description": "The pending request id. Defaults to the chat's current pending question." },
                "answers": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "question_id": { "type": "string" },
                            "labels": { "type": "array", "items": { "type": "string" } }
                        },
                        "required": ["question_id", "labels"]
                    }
                }
            })),
        },
        ToolDef {
            name: "archive_chat",
            description: "Archive a chat (hide it from the active sidebar list); pass archived=false to restore. Use it to tidy up chats you created.",
            input_schema: chat_key_schema(json!({
                "archived": { "type": "boolean", "default": true }
            })),
        },
    ];
    for (name, single, description) in [
        (
            "create_chats",
            "create_chat",
            "Create multiple standalone or side chats concurrently (kind per request). Put each chat's prompt in its request to start all work together. Prefer this for parallel delegation, including harnesses that execute tool calls sequentially. Each request has create_chat arguments; wait defaults to false. Results preserve request order and include per-request errors; successful requests are not rolled back.",
        ),
        (
            "send_messages",
            "send_message",
            "Send messages to multiple independent chats concurrently. Prefer this to delegate parallel work to existing chats. Each request has send_message arguments; wait defaults to false. If wait=true, requests still run concurrently. Results preserve request order and include per-request errors; successful sends are not rolled back. Do not include dependent messages to the same chat.",
        ),
    ] {
        let item_schema = tools
            .iter()
            .find(|tool| tool.name == single)
            .unwrap()
            .input_schema
            .clone();
        tools.push(ToolDef {
            name, description,
            input_schema: json!({
                "type": "object",
                "properties": {"requests": {"type": "array", "minItems": 1, "maxItems": MAX_BATCH, "items": item_schema}},
                "required": ["requests"],
            }),
        });
    }
    tools.push(ToolDef {
        name: SHOW_VISUAL,
        description: SHOW_VISUAL_DESCRIPTION,
        input_schema: json!({
            "type": "object",
            "properties": {
                "script": {
                    "type": "string",
                    "description": "The Luau visual script: setup at the top level, ending with `return function() return ui.column { ... } end`."
                },
                "title": { "type": "string", "description": "Short name for the visual." },
                "assets": {
                    "type": "object",
                    "additionalProperties": { "type": "string" },
                    "description": "Files the script uses, by the name it uses them: { \"prism.glb\": \"models/prism.glb\", \"sky.hdr\": \"/tmp/sky.hdr\" }. Paths are on this machine, relative to your working directory. glTF models (.glb, or .gltf with everything embedded), PNG, JPEG, WebP, and Radiance .hdr images; at most 16 files, 8 MB each, 10 MB together."
                }
            },
            "required": ["script", "title"]
        }),
    });
    tools
}

/// Shows a page in the CALLING chat, so it is offered only to an agent that
/// Zeron launched (`ZERON_CHAT_ID` set).
const SHOW_VISUAL: &str = "show_visual";

pub(crate) const SHOW_VISUAL_DESCRIPTION: &str = "\
Show an interactive visual (chart, table, dashboard, diagram, simulation, 3D view) inline in this \
chat, above your final text reply; call it before writing that reply. Use it when a visual says more \
than prose. The reader already sees it, so the reply should not announce, locate, or restate it: add \
only what the visual doesn't say.\n\
\n\
A visual is a Luau script (Lua 5.1 plus `+=`, `continue`, and `if` expressions). Zeron runs it in a \
sandbox (no I/O or network; math.random is seeded, so data is the same on every render) and renders \
what it returns with Zeron's own native interface. Build data and state at the top level, then return \
a view function. Zeron calls the view again after every handler, so handlers just assign to state:\n\
\n\
local s = ui.state { metric = \"p95\" }\n\
local data = { { region = \"us-east\", p95 = 182, p99 = 240 }, { region = \"eu-west\", p95 = 140, p99 = 210 } }\n\
return function()\n\
  return ui.column { gap = 16,\n\
    ui.text { \"Latency by region\", size = \"lg\", weight = 600 },\n\
    ui.tabs { options = { \"p95\", \"p99\" }, value = s.metric, on_change = function(v) s.metric = v end },\n\
    ui.chart { type = \"bar\", height = 220, unit = \"ms\",\n\
      x = ui.map(data, function(d) return d.region end),\n\
      series = { { name = s.metric, values = ui.map(data, function(d) return d[s.metric] end) } } },\n\
  }\n\
end\n\
\n\
Zeron runs the script before showing it (setup, render, every draw, and each tab, toggle, slider, and \
button once) and returns the first error with its line; fix it and call again.\n\
\n\
Nodes are props tables: children are the array part, strings are plain text, nil and false children \
are skipped, and lists of nodes flatten (so ui.map results can be children). Unknown props are errors.\n\
- ui.column { gap = 8, align = \"start|center|end\", ... }, ui.row { gap = 12, align, justify = \
\"start|center|end|between\", wrap = true, ... }, ui.grid { columns = 3, gap = 16, ... }. Any node \
takes width = px or grow = true inside a row (sliders, charts, tables, canvases, and scenes fill a \
row's spare width by default), and key = \"name\" to keep its hover or camera when the tree changes \
shape. A row or column with on_click = function() end is clickable; hover = true gives \
it the hover wash.\n\
- ui.text { \"plain \", ui.span { \"bold\", weight = 600, color = \"accent\", mono = true }, size = \
\"xs|sm|body|lg|xl|2xl\", weight = 400-700, color, mono, italic, align, truncate }, or ui.text \"string\".\n\
- ui.stat { value = \"182 ms\", label = \"p95 latency\", delta = \"-12%\", tone = \"success\" }: a headline \
figure over a muted label; tone colors the delta.\n\
- ui.table { columns = { \"Region\", { label = \"p95\", align = \"end\" } }, rows = { { \"us-east\", 182 }, \
... } }: laid out like a markdown table (bold header, hairline row rules), numbers right-aligned; cells \
are strings, numbers, or nodes.\n\
- ui.chart { type = \"bar|line|area|scatter\", height = 220, x = { labels }, series = { { name, values = \
{ ... }, color } }, stacked, horizontal (bar), min, max, unit = \"ms\", prefix = \"$\", decimals, legend }. \
Scatter series take points = { { x, y }, ... } (x_unit, x_decimals). Axes, gridlines, the legend, and a \
hover readout are built in.\n\
- ui.rule {} is a hairline divider; ui.space { size = 16 }; ui.link { label, url = \"https://...\" } \
opens like a chat link.\n\
- Controls: ui.tabs { options = { ... }, value, on_change = function(option) end }, ui.toggle { label, \
value = bool, on_change = function(on) end }, ui.slider { label, value, min, max, step, unit, on_change \
= function(n) end }, ui.button { label, on_click = function() end, primary = bool }.\n\
- ui.canvas { height = 240, draw = function(g, w, h, t) ... end, animate = bool, on_click = \
function(x, y) end }: 2D drawing in pixels, origin top-left, w = the reply column's width. \
g:line(x1, y1, x2, y2, { color, width, dash }), g:polyline(pts, { color, width }), g:polygon(pts, \
{ fill, stroke, width }), g:rect(x, y, w, h, { fill, stroke, radius }), g:circle(x, y, r, { fill, \
stroke }), g:arc(x, y, r, a0, a1, { color } or { fill, inner = radius } for pie and donut sectors; \
radians clockwise from 3 o'clock), g:text(x, y, str, { size, color, weight, align = \
\"start|center|end\", baseline = \"top|middle|bottom\", mono }). pts are { x1, y1, x2, y2, ... } or \
{ { x, y }, ... }. g.pointer is { x, y, down } while the pointer is over the canvas (draw hover \
effects with it); t is seconds, and animate = true redraws every frame while visible.\n\
- ui.scene { height = 320, draw = function(s, t) ... end, orbit = true, spin = 0.3 (turntable \
radians per second), camera = { yaw = 0.6, pitch = 0.45, zoom = 1 }, animate = bool }: real 3D, y up, \
rendered on the GPU with depth, soft shadows from a key light, and bloom. Zeron frames the camera on \
your geometry and the reader drags to orbit. Solids (box, sphere, cylinder, surface, mesh) take \
material props: roughness (0 mirror to 1 matte), metal (0-1), glow (emits its color and blooms; \
use for lamps, beams, highlights), glass = true (refracts what's behind it; ior = 1.5, dispersion \
= 0-1 splits it into a rainbow, as a prism does). Build real forms from several primitives (legs, \
tops, rims), not single boxes, and give materials so surfaces read as wood, metal, or glass. s:box { at = { x, y, z }, size = { w, h, d } or n, color }, s:sphere { at, radius, color }, \
s:cylinder { at (base center), radius, height, color }, s:surface { x = { min, max }, z = { min, max }, \
f = function(x, z) return y end or values = { { ... } }, detail = 32, colors = { \"chart1\", \"chart3\" } \
(a ramp by height), wire = bool }, s:mesh { vertices = { { x, y, z }, ... }, faces = { { 1, 2, 3 }, ... }, \
color }, s:line(a, b, { color, width }), s:path(points, { color, width }), s:point(at, { radius, \
color, hint = \"shown on hover\" }), s:label(at, text, { color, size }), s:grid { size = 10, step = 1 }, \
s:axes { size = 1, labels = { \"x\", \"y\", \"z\" } }. Physical colors may be \"#rrggbb\" in scenes \
(a material's own color: gold, glass, wood), and only there.\n\
- Assets: pass real files in show_visual's assets (name -> path) and scripts use them by name. \
s:model { asset = \"prism.glb\", at, rotate = { x, y, z } degrees, scale, fit = 2 (center it and size \
its longest side), color and material props override the model's own } draws a glTF model with its \
materials and textures. Solids take texture = \"wood.jpg\" (multiplied by color; mesh takes uvs = { { u, v \
}, ... } per vertex). s:environment { asset = \"studio.hdr\", intensity = 1, background = false } lights \
the scene from an equirectangular image (an HDR is best) that metal and glass reflect and glass bends; \
background = true also draws it behind the scene. s:sprite { at, size = 48 (pixels), color, glow = 1, \
asset = \"flare.png\" } adds a camera-facing light (a flare or glint; a soft round glow without an \
asset) that blooms. ui.shader takes textures = { \"rock.jpg\", ... } as iTexture0-iTexture3 \
(repeating, mipmapped). Find or make assets with your own tools (download, convert KTX2 or EXR to PNG \
or HDR) before calling.\n\
- ui.shader { height = 360, code = [[ GLSL ]], buffers = { [[ GLSL ]], ... }, inputs = { speed = \
s.speed }, blend = \"alpha|light|opaque\", bloom = 0-3, orbit = bool, spin, camera = { yaw, pitch, \
zoom }, animate = true }: GPU shaders for what geometry can't express: raymarched SDFs and \
fractals, light and optics (beams, refraction, dispersion), volumes, fluids, particles, generative \
art. Write each pass as on Shadertoy: void mainImage(out vec4 fragColor, in vec2 fragCoord), with \
iResolution (vec3: width and height in pixels, then the display's pixel density), iTime, \
iTimeDelta, iFrame, iMouse (pixels from the bottom-left; xy where the pointer is, zw where it went \
down while it's held, negative otherwise), iCamera (vec4 yaw, pitch, zoom, 0; orbit = true lets the reader drag \
it), every inputs name as a float (bind them to state so sliders and toggles drive the shader), \
iNoise (a tiling 256x256 random RGBA texture), and iChannel0-iChannel3 with iChannelResolution: \
buffers run first, in order, each into a float texture that every pass reads as iChannel<i> (an \
earlier buffer's current frame; a buffer's own previous frame), for multi-pass work like blur, \
trails, accumulation, and simulation. The shader sits on the chat, never in a box: don't paint a \
background, vignette, or floor color. By default (blend = \"light\") the output is emitted light: \
brightness is opacity and black is clear, right for beams, glows, sparks, and light through glass. \
For solid shaded objects (fractals, SDF bodies) set blend = \"alpha\" and write alpha 0 where the ray \
misses. bloom = 0.6-1.5 adds a soft glow around bright light. \
The width varies with the reader (a phone is about 350px wide, often narrower than the shader is \
tall): scale the scene by min(iResolution.x, iResolution.y), not one axis, and keep the subject \
centered with room on both sides, so it fits and stays centered at any aspect. \
GLSL errors come back with the pass and line number.\n\
- Helpers: ui.scale(d0, d1, r0, r1) returns a mapping function; ui.ticks(lo, hi, n); ui.fmt(n, { unit, \
prefix, decimals }) groups digits; ui.map(list, function(v, i) end); ui.range(a, b, step); ui.clamp; \
ui.lerp; ui.series(i) is \"chart<i>\"; ui.color(token, alpha); ui.mix(a, b, t). print(...) output comes \
back to you.\n\
\n\
Colors are theme tokens, never hex or color names (except scene materials): text; muted (labels, axes, captions); faint \
(tertiary notes); accent (the one thing to notice); on-accent (text on an accent mark); success, \
warning, danger (status only); border (hairlines, gridlines); chart1, chart2, chart3, chart4, \
chart5, chart6 (series, in order; \
chart1 is the accent). They follow the reader's theme, light or dark, on frosted glass or an opaque \
surface, so never assume a background color.\n\
\n\
Style, enforced: layout nodes have no fill, border, shadow, padding, or font props. A visual is part \
of the reply, not a widget, so lay content directly on the chat with type, spacing, and alignment: no \
cards, tiles, boxed panels, banners, or canvas backgrounds. Stats are a row of plain ui.stat figures; \
a chart is a plain heading, then the chart; separate sections with gap or one ui.rule. Text renders \
in Zeron's interface font at the reader's text size: body for most text, sm and xs for labels and \
notes, lg and xl for headings, 2xl for a headline figure. The width is the reply column (about 720px, \
as narrow as 360px), so lay out fluidly with grow, wrap, and grid, and give charts, canvases, and \
scenes heights of 160-420px.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_well_formed() {
        let defs = catalog();
        let mut names: Vec<&str> = defs.iter().map(|d| d.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), defs.len(), "tool names must be unique");
        for def in &defs {
            assert_eq!(def.input_schema["type"], "object", "{}", def.name);
            assert!(!def.description.is_empty());
        }
        // One tool name per descriptor and one descriptor per tool name.
        let mut tools: Vec<&str> = ToolName::ALL.iter().map(|t| t.as_str()).collect();
        tools.sort_unstable();
        assert_eq!(names, tools);
        let chat_only: Vec<String> = descriptors()
            .into_iter()
            .filter(|d| d.availability == McpToolAvailability::ChatOnly)
            .map(|d| d.name)
            .collect();
        assert_eq!(chat_only, [SHOW_VISUAL]);
    }

    #[test]
    fn the_show_visual_example_passes_its_own_check() {
        let start = SHOW_VISUAL_DESCRIPTION.find("local s = ui.state").unwrap();
        let end = start
            + SHOW_VISUAL_DESCRIPTION[start..]
                .find("\n\nZeron runs the script")
                .unwrap();
        let report = zeron_visual::check(&SHOW_VISUAL_DESCRIPTION[start..end], 1).unwrap();
        assert_eq!(report.interactions, 2, "both tabs");
    }
}

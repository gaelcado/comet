//! Settings → MCP → Zeron: inspect and configure the built-in Zeron MCP tools
//! of one engine.
//!
//! The policy lives on the engine, in the active profile: this page reads it
//! through `WatchZeronMcpCatalog` and changes it only through
//! `SetZeronMcpToolOverride`, retargeted at any registered device like the
//! Providers page. Names, descriptions, schemas and availability all come
//! from the engine's catalog; nothing here lists tools by name.
//!
//! [`McpModel`] decides what is shown and sent, so the draft lifecycle and
//! the late-reply guard are testable without a window:
//! - a description draft is keyed by device and tool. It survives conflicts,
//!   failures and device switches, and only Cancel or a confirmed save of
//!   that very text discards it;
//! - every write remembers the device generation it was sent for. A reply
//!   for a device no longer shown never touches the page.

use std::collections::HashMap;

use gpui::{
    AnyElement, Context, Entity, FocusHandle, IntoElement, Render, SharedString, Subscription,
    Task, Window, div, prelude::*, px,
};
use zeron_proto::{
    McpCatalogSnapshot, McpDescriptionError, McpDescriptionPatch, McpToolAvailability,
    McpToolState, SetZeronMcpToolOverrideParams, WatchZeronMcpCatalogParams, WorkspaceScope,
    validate_description_override,
};
use zeron_rpc::{RpcError, capability_errors, methods};

use crate::composer::{ComposerInput, ComposerInputEvent};
use crate::motion;
use crate::popover;
use crate::settings::widgets::{self, ActionTone};
use crate::state::AppState;
use crate::theme::Theme;

/// The reconnection note. It promises nothing about lists an agent already
/// holds: calls are checked when made, lists refresh on reconnect.
const RECONNECT_NOTE: &str =
    "New calls use this setting. Some agents must reconnect the MCP to refresh their tool list.";

/// A description edit belongs to one tool on one device (`None`: this one).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct DraftKey {
    device: Option<String>,
    tool: String,
}

#[derive(Debug, Clone, PartialEq)]
struct Draft {
    text: String,
    /// The effective description the edit started from. A different one at
    /// save time means someone changed it meanwhile.
    base: String,
    error: Option<SharedString>,
}

#[derive(Debug, Clone, PartialEq)]
enum Catalog {
    Loading,
    Ready(McpCatalogSnapshot),
    /// The engine predates per-tool control.
    Unsupported,
    /// Nothing loaded and the engine can't be reached.
    Unavailable(String),
}

#[derive(Debug, Clone, PartialEq)]
enum WriteKind {
    Enable(bool),
    Save(String),
    Reset,
    /// Rebuild an unreadable policy from defaults.
    Repair,
}

#[derive(Debug, Clone)]
struct Write {
    generation: u64,
    device: Option<String>,
    tool: String,
    kind: WriteKind,
    operation_id: String,
}

#[derive(Debug, Default, PartialEq)]
struct WriteOutcome {
    /// The row whose trigger takes focus back (the control used is gone).
    focus: Option<String>,
    /// The write lost a race: subscribe again for the current policy.
    reload: bool,
}

struct McpModel {
    /// Target device; `None` is this device (no passthrough).
    device: Option<String>,
    /// Bumped on every retarget; replies carry the one they were sent for.
    generation: u64,
    catalog: Catalog,
    /// The watch is delivering. Without it, shown settings may be stale and
    /// nothing is written.
    live: bool,
    query: String,
    expanded: Option<String>,
    show_original: bool,
    drafts: HashMap<DraftKey, Draft>,
    in_flight: Option<Write>,
    /// A failed switch, reset or repair, shown as a toast.
    notice: Option<String>,
}

impl McpModel {
    fn new() -> Self {
        Self {
            device: None,
            generation: 0,
            catalog: Catalog::Loading,
            live: false,
            query: String::new(),
            expanded: None,
            show_original: false,
            drafts: HashMap::new(),
            in_flight: None,
            notice: None,
        }
    }

    fn snapshot(&self) -> Option<&McpCatalogSnapshot> {
        match &self.catalog {
            Catalog::Ready(snapshot) => Some(snapshot),
            _ => None,
        }
    }

    fn tool(&self, name: &str) -> Option<&McpToolState> {
        self.snapshot()?
            .tools
            .iter()
            .find(|tool| tool.descriptor.name == name)
    }

    fn key(&self, tool: &str) -> DraftKey {
        DraftKey {
            device: self.device.clone(),
            tool: tool.to_owned(),
        }
    }

    fn draft(&self, tool: &str) -> Option<&Draft> {
        self.drafts.get(&self.key(tool))
    }

    /// The draft differs from what the engine currently describes.
    fn dirty(&self, tool: &str) -> bool {
        match (self.draft(tool), self.tool(tool)) {
            (Some(draft), Some(state)) => draft.text != state.effective_description(),
            _ => false,
        }
    }

    /// What the description field shows: the draft, else the effective text.
    fn editor_text(&self, tool: &str) -> Option<String> {
        let state = self.tool(tool)?;
        Some(
            self.draft(tool)
                .map(|draft| draft.text.clone())
                .unwrap_or_else(|| state.effective_description().to_owned()),
        )
    }

    /// Point at another device. Its drafts wait under their own key.
    fn retarget(&mut self, device: Option<String>) -> bool {
        if self.device == device {
            return false;
        }
        self.device = device;
        self.generation += 1;
        self.catalog = Catalog::Loading;
        self.live = false;
        self.expanded = None;
        self.show_original = false;
        self.in_flight = None;
        self.notice = None;
        true
    }

    fn watched(&mut self, generation: u64, snapshot: McpCatalogSnapshot) -> bool {
        if generation != self.generation {
            return false;
        }
        self.catalog = Catalog::Ready(snapshot);
        self.live = true;
        if let Some(name) = &self.expanded
            && self.tool(name).is_none()
        {
            self.expanded = None;
        }
        true
    }

    fn watch_lost(&mut self, generation: u64, message: String) {
        if generation != self.generation {
            return;
        }
        self.live = false;
        if !matches!(self.catalog, Catalog::Ready(_)) {
            self.catalog = Catalog::Unavailable(message);
        }
    }

    fn unsupported(&mut self, generation: u64) {
        if generation == self.generation {
            self.catalog = Catalog::Unsupported;
            self.live = false;
        }
    }

    /// Switches and edits can be sent: live, readable, nothing pending.
    fn writable(&self) -> bool {
        self.live
            && self.in_flight.is_none()
            && self
                .snapshot()
                .is_some_and(|snapshot| !snapshot.policy_unreadable)
    }

    /// Tools whose name or description (effective or original) holds every
    /// word of the query, in catalog order.
    fn filtered(&self) -> Vec<&McpToolState> {
        let Some(snapshot) = self.snapshot() else {
            return Vec::new();
        };
        let terms: Vec<String> = self
            .query
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        snapshot
            .tools
            .iter()
            .filter(|tool| {
                let haystack = format!(
                    "{}\n{}\n{}",
                    tool.descriptor.name,
                    tool.effective_description(),
                    tool.descriptor.description
                )
                .to_lowercase();
                terms.iter().all(|term| haystack.contains(term))
            })
            .collect()
    }

    /// The switch as shown: a pending change shows at once.
    fn shown_enabled(&self, tool: &McpToolState) -> bool {
        match &self.in_flight {
            Some(Write {
                tool: name,
                kind: WriteKind::Enable(on),
                ..
            }) if *name == tool.descriptor.name => *on,
            _ => tool.enabled,
        }
    }

    fn toggle_expanded(&mut self, tool: &str) {
        self.show_original = false;
        self.expanded = if self.expanded.as_deref() == Some(tool) {
            None
        } else {
            Some(tool.to_owned())
        };
    }

    /// The description field changed. Text equal to the effective
    /// description is no edit at all.
    fn edit(&mut self, tool: &str, text: String) {
        let Some(effective) = self
            .tool(tool)
            .map(|state| state.effective_description().to_owned())
        else {
            return;
        };
        let key = self.key(tool);
        if text == effective {
            self.drafts.remove(&key);
            return;
        }
        let draft = self.drafts.entry(key).or_insert_with(|| Draft {
            text: String::new(),
            base: effective,
            error: None,
        });
        draft.text = text;
        draft.error = None;
    }

    /// Discard the draft. Returns the row that takes focus back.
    fn cancel(&mut self, tool: &str) -> String {
        let key = self.key(tool);
        self.drafts.remove(&key);
        tool.to_owned()
    }

    /// Validate a change and record it as in flight, or explain inline why
    /// it can't be sent.
    fn begin(
        &mut self,
        tool: &str,
        kind: WriteKind,
    ) -> Option<(Write, SetZeronMcpToolOverrideParams)> {
        let snapshot = self.snapshot()?;
        let repair = matches!(kind, WriteKind::Repair);
        if !self.live || self.in_flight.is_some() || snapshot.policy_unreadable != repair {
            return None;
        }
        let expected_policy_revision = snapshot.policy_revision;
        let effective = self.tool(tool)?.effective_description().to_owned();
        let (enabled, description_patch) = match &kind {
            WriteKind::Enable(on) => (Some(*on), McpDescriptionPatch::Unchanged),
            WriteKind::Reset => (None, McpDescriptionPatch::Reset),
            WriteKind::Repair => (None, McpDescriptionPatch::Unchanged),
            WriteKind::Save(text) => {
                let key = self.key(tool);
                let draft = self.drafts.get_mut(&key)?;
                if let Err(error) = validate_description_override(text) {
                    draft.error = Some(description_error_copy(error).into());
                    return None;
                }
                if draft.base != effective {
                    // Saving would silently replace someone else's text:
                    // say so once, then the next Save is deliberate.
                    draft.base = effective;
                    draft.error = Some(
                        "This description changed elsewhere since you started. Save again to replace it, or Cancel to keep it."
                            .into(),
                    );
                    return None;
                }
                (None, McpDescriptionPatch::Set(text.clone()))
            }
        };
        let write = Write {
            generation: self.generation,
            device: self.device.clone(),
            tool: tool.to_owned(),
            kind,
            operation_id: uuid::Uuid::new_v4().to_string(),
        };
        let params = SetZeronMcpToolOverrideParams {
            name: tool.to_owned(),
            expected_policy_revision,
            enabled,
            description_patch,
            operation_id: write.operation_id.clone(),
            target_device_id: self.device.clone(),
        };
        self.in_flight = Some(write.clone());
        Some((write, params))
    }

    /// Settle a write. A reply for another device or an older generation
    /// only retires a draft it saved, under that draft's own key.
    fn finish(
        &mut self,
        write: Write,
        result: Result<McpCatalogSnapshot, RpcError>,
    ) -> WriteOutcome {
        let key = DraftKey {
            device: write.device.clone(),
            tool: write.tool.clone(),
        };
        if let (Ok(_), WriteKind::Save(text)) = (&result, &write.kind)
            && self
                .drafts
                .get(&key)
                .is_some_and(|draft| draft.text == *text)
        {
            self.drafts.remove(&key);
        }
        let current = self
            .in_flight
            .as_ref()
            .is_some_and(|pending| pending.operation_id == write.operation_id);
        if !current || write.generation != self.generation {
            return WriteOutcome::default();
        }
        self.in_flight = None;
        match result {
            Ok(snapshot) => {
                // A watch item may already have delivered something newer.
                if self
                    .snapshot()
                    .is_none_or(|shown| snapshot.policy_revision >= shown.policy_revision)
                {
                    self.catalog = Catalog::Ready(snapshot);
                }
                WriteOutcome {
                    focus: matches!(write.kind, WriteKind::Save(_) | WriteKind::Reset)
                        .then_some(write.tool),
                    reload: false,
                }
            }
            Err(error) => {
                let conflict = matches!(&error, RpcError::Capability(code)
                    if code == capability_errors::MCP_POLICY_CONFLICT);
                let message = write_error_copy(&error);
                match (&write.kind, self.drafts.get_mut(&key)) {
                    (WriteKind::Save(_), Some(draft)) => draft.error = Some(message.into()),
                    _ => self.notice = Some(message),
                }
                WriteOutcome {
                    focus: None,
                    reload: conflict,
                }
            }
        }
    }
}

fn description_error_copy(error: McpDescriptionError) -> String {
    match error {
        McpDescriptionError::Blank => {
            "Enter a description, or Cancel to keep the current one.".into()
        }
        McpDescriptionError::TooLong => format!(
            "Shorten the description to {} KiB or less.",
            zeron_proto::MCP_DESCRIPTION_OVERRIDE_MAX_BYTES / 1024
        ),
        McpDescriptionError::ControlCharacter => {
            "Remove the control characters from the description.".into()
        }
    }
}

fn write_error_copy(error: &RpcError) -> String {
    match error {
        RpcError::Capability(code) if code == capability_errors::MCP_POLICY_CONFLICT => {
            "These settings changed elsewhere. The latest ones are loaded; try again.".into()
        }
        RpcError::Capability(code) if code == capability_errors::MCP_POLICY_UNREADABLE => {
            "Zeron can't read the saved tool settings. Restore the defaults first.".into()
        }
        RpcError::Capability(code) if code == capability_errors::MCP_POLICY_WRITE_FAILED => {
            "Zeron couldn't save this setting. Nothing changed.".into()
        }
        RpcError::UnknownMethod(_) => {
            "This device's Zeron can't change MCP tools. Update it to make changes.".into()
        }
        RpcError::Transport(_) | RpcError::Closed => {
            "Couldn't reach the device. Check the setting once it reconnects.".into()
        }
        RpcError::BadParams(message)
        | RpcError::Failed(message)
        | RpcError::Capability(message) => message.clone(),
    }
}

fn availability_label(availability: McpToolAvailability) -> Option<&'static str> {
    match availability {
        McpToolAvailability::Always => None,
        McpToolAvailability::ChatOnly => Some("Only from a chat"),
        McpToolAvailability::Unknown => Some("Needs a newer Zeron"),
    }
}

fn scope_label(scope: Option<WorkspaceScope>) -> &'static str {
    match scope {
        Some(WorkspaceScope::Local) => "local",
        Some(WorkspaceScope::Synced) => "synced",
        Some(WorkspaceScope::Development) => "development",
        None => "current",
    }
}

/// The open description field and the draft it edits.
struct Editor {
    key: DraftKey,
    input: Entity<ComposerInput>,
    _events: Subscription,
}

pub struct McpSettingsPage {
    state: Entity<AppState>,
    model: McpModel,
    scroll: widgets::PageScroll,
    device_select: widgets::SelectState,
    search: Entity<ComposerInput>,
    editor: Option<Editor>,
    /// Stable focus per tool trigger, so focus can come back to a row.
    row_focus: HashMap<String, FocusHandle>,
    pending_focus: Option<String>,
    /// The engine and workspace the model describes; another one resets it.
    engine_key: Option<(String, WorkspaceScope)>,
    watch_task: Option<Task<()>>,
    write_task: Option<Task<()>>,
    _search_events: Subscription,
    _state: Subscription,
}

impl McpSettingsPage {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| {
            ComposerInput::with_context("Search tools", "PaletteSearch", cx)
                .with_accessibility_role(gpui::Role::SearchInput)
                .with_single_line()
                .with_tab_stop()
        });
        let search_events = cx.subscribe(&search, |page: &mut Self, input, event, cx| {
            if matches!(event, ComposerInputEvent::Edited) {
                page.model.query = input.read(cx).text().to_owned();
                cx.notify();
            }
        });
        // Settings can open before the engine connects, and a profile switch
        // replaces it: follow both, and peers' capabilities as they sync.
        let state_sub = cx.observe(&state, |page: &mut Self, _, cx| {
            let key = page.current_engine_key(cx);
            if key != page.engine_key {
                page.engine_key = key;
                page.model = McpModel::new();
                page.editor = None;
                page.write_task = None;
                page.watch(cx);
            } else if matches!(page.model.catalog, Catalog::Unsupported)
                // Undeclared until now. An engine that declared it and still
                // answered "unknown method" keeps its finished task.
                && page.watch_task.is_none()
                && page.supported(cx)
            {
                page.watch(cx);
            }
            cx.notify();
        });
        let mut page = Self {
            state,
            model: McpModel::new(),
            scroll: widgets::PageScroll::default(),
            device_select: widgets::SelectState::default(),
            search,
            editor: None,
            row_focus: HashMap::new(),
            pending_focus: None,
            engine_key: None,
            watch_task: None,
            write_task: None,
            _search_events: search_events,
            _state: state_sub,
        };
        page.engine_key = page.current_engine_key(cx);
        page.watch(cx);
        page
    }

    fn current_engine_key(&self, cx: &Context<Self>) -> Option<(String, WorkspaceScope)> {
        let info = self.state.read(cx).engine()?.engine_info();
        Some((info.device_id.clone(), info.workspace_scope))
    }

    /// The target engine declares per-tool control. Undeclared is treated as
    /// unsupported: never offer changes an old engine would ignore.
    fn supported(&self, cx: &Context<Self>) -> bool {
        let state = self.state.read(cx);
        let capability = zeron_proto::capabilities::ZERON_MCP_TOOL_POLICY_V1;
        match &self.model.device {
            Some(device) => state.device_supports(device, capability),
            None => state
                .engine()
                .is_none_or(|engine| engine.engine_info().supports(capability)),
        }
    }

    /// Subscribe to the target's catalog, reconnecting with backoff. Each
    /// attempt reads the current engine, so a reconnected one is used.
    fn watch(&mut self, cx: &mut Context<Self>) {
        let generation = self.model.generation;
        if !self.supported(cx) {
            self.watch_task = None;
            self.model.unsupported(generation);
            return;
        }
        let params = serde_json::to_value(WatchZeronMcpCatalogParams {
            target_device_id: self.model.device.clone(),
        })
        .unwrap_or_default();
        self.watch_task = Some(cx.spawn(async move |this, cx| {
            let mut retry = 1;
            loop {
                let Ok(engine) = this.update(cx, |page, cx| page.state.read(cx).engine().cloned())
                else {
                    return;
                };
                let failure = match engine {
                    None => "Zeron isn't connected yet.".to_owned(),
                    Some(engine) => match engine
                        .client()
                        .subscribe_checked(methods::WATCH_ZERON_MCP_CATALOG, params.clone())
                        .await
                    {
                        Ok(mut stream) => {
                            while let Some(value) = stream.recv().await {
                                retry = 1;
                                let parsed = serde_json::from_value::<McpCatalogSnapshot>(value);
                                let applied = this.update(cx, |page, cx| {
                                    match parsed {
                                        Ok(snapshot) => {
                                            if page.model.watched(generation, snapshot) {
                                                page.sync_editor(cx);
                                            }
                                        }
                                        Err(error) => {
                                            page.model.watch_lost(generation, error.to_string())
                                        }
                                    }
                                    cx.notify();
                                });
                                if applied.is_err() {
                                    return;
                                }
                            }
                            "Connection closed. Reconnecting…".to_owned()
                        }
                        Err(RpcError::UnknownMethod(_)) => {
                            this.update(cx, |page, cx| {
                                page.model.unsupported(generation);
                                cx.notify();
                            })
                            .ok();
                            return;
                        }
                        Err(error) => error.to_string(),
                    },
                };
                if this
                    .update(cx, |page, cx| {
                        page.model.watch_lost(generation, failure);
                        cx.notify();
                    })
                    .is_err()
                {
                    return;
                }
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(retry))
                    .await;
                retry = (retry * 2).min(15);
            }
        }));
    }

    pub(crate) fn set_target_device(&mut self, device: Option<String>, cx: &mut Context<Self>) {
        widgets::close_select(self, |page: &mut Self| &mut page.device_select, cx);
        if self.model.retarget(device) {
            self.editor = None;
            self.watch(cx);
        }
        cx.notify();
    }

    /// Keep the description field on the expanded tool's draft or effective
    /// text. Never overwrites a draft.
    fn sync_editor(&mut self, cx: &mut Context<Self>) {
        let Some(tool) = self.model.expanded.clone() else {
            self.editor = None;
            return;
        };
        let Some(text) = self.model.editor_text(&tool) else {
            self.editor = None;
            return;
        };
        let key = self.model.key(&tool);
        if self.editor.as_ref().is_none_or(|editor| editor.key != key) {
            let input = cx.new(|cx| {
                ComposerInput::new("Description", cx)
                    .with_viewport_height(180.0)
                    .with_tab_stop()
            });
            let events = cx.subscribe(&input, move |page: &mut Self, input, event, cx| {
                match event {
                    ComposerInputEvent::Edited => {
                        let text = input.read(cx).text().to_owned();
                        page.model.edit(&tool, text);
                        cx.notify();
                    }
                    // Enter saves; Shift+Enter adds a line.
                    ComposerInputEvent::Submitted => page.save(&tool, cx),
                    _ => {}
                }
            });
            self.editor = Some(Editor {
                key,
                input,
                _events: events,
            });
        }
        if let Some(editor) = &self.editor
            && editor.input.read(cx).text() != text
        {
            editor
                .input
                .update(cx, |input, cx| input.set_text(text, cx));
        }
    }

    fn toggle_expanded(&mut self, tool: &str, cx: &mut Context<Self>) {
        self.model.toggle_expanded(tool);
        self.sync_editor(cx);
        cx.notify();
    }

    fn cancel(&mut self, tool: &str, cx: &mut Context<Self>) {
        self.pending_focus = Some(self.model.cancel(tool));
        self.sync_editor(cx);
        cx.notify();
    }

    fn save(&mut self, tool: &str, cx: &mut Context<Self>) {
        if !self.model.dirty(tool) {
            return;
        }
        let Some(text) = self.model.draft(tool).map(|draft| draft.text.clone()) else {
            return;
        };
        self.send(tool, WriteKind::Save(text), cx);
    }

    fn send(&mut self, tool: &str, kind: WriteKind, cx: &mut Context<Self>) {
        let Some(engine) = self.state.read(cx).engine().cloned() else {
            return;
        };
        let Some((write, params)) = self.model.begin(tool, kind) else {
            cx.notify();
            return;
        };
        let params = serde_json::to_value(params).unwrap_or_default();
        self.write_task = Some(cx.spawn(async move |this, cx| {
            let result = engine
                .client()
                .call_as::<McpCatalogSnapshot>(methods::SET_ZERON_MCP_TOOL_OVERRIDE, params)
                .await;
            this.update(cx, |page, cx| {
                let outcome = page.model.finish(write, result);
                if let Some(tool) = outcome.focus {
                    page.pending_focus = Some(tool);
                }
                if outcome.reload {
                    page.watch(cx);
                }
                page.sync_editor(cx);
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    /// Escape that reached Settings unclaimed closes the open tool (its
    /// draft kept), then clears the search. Returns whether it did.
    pub(crate) fn dismiss_on_escape(&mut self, cx: &mut Context<Self>) -> bool {
        if let Some(tool) = self.model.expanded.clone() {
            self.toggle_expanded(&tool, cx);
            self.pending_focus = Some(tool);
            return true;
        }
        if !self.model.query.is_empty() {
            self.model.query.clear();
            self.search.update(cx, |input, cx| input.set_text("", cx));
            cx.notify();
            return true;
        }
        false
    }

    fn device_name(&self, cx: &Context<Self>) -> String {
        let state = self.state.read(cx);
        self.model
            .device
            .as_deref()
            .or(state.local_device_id.as_deref())
            .and_then(|id| state.device_name(id))
            .filter(|name| !name.trim().is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| "this device".into())
    }

    /// The Providers page's device switcher, retargeting reads and writes.
    fn render_device_switcher(&mut self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        use crate::icons::{self, icon};
        let (mut devices, local_id) = {
            let state = self.state.read(cx);
            (state.devices.clone(), state.local_device_id.clone())
        };
        devices.sort_by(|a, b| {
            a.created_at
                .cmp(&b.created_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        let effective = self.model.device.clone().or_else(|| local_id.clone());
        let glyph = |platform: &str| match platform {
            "macos" | "darwin" => icons::LAPTOP,
            "ios" | "android" => icons::SMARTPHONE,
            _ => icons::MONITOR,
        };
        let mut targets: Vec<Option<String>> = Vec::new();
        let mut options = Vec::new();
        for device in &devices {
            let is_local = local_id.as_deref() == Some(device.id.as_str());
            let path = glyph(&device.platform);
            let muted = theme.text_muted;
            let option = widgets::SelectOption::new(device.name.clone()).leading(move || {
                icon(path)
                    .size(px(16.0))
                    .flex_none()
                    .text_color(muted)
                    .into_any_element()
            });
            options.push(if is_local {
                option.detail("You")
            } else {
                option
            });
            targets.push((!is_local).then(|| device.id.clone()));
        }
        let selected = match devices
            .iter()
            .position(|device| Some(device.id.as_str()) == effective.as_deref())
        {
            Some(ix) => ix,
            None => {
                let muted = theme.text_muted;
                options.push(widgets::SelectOption::new("This device").leading(move || {
                    icon(icons::LAPTOP)
                        .size(px(16.0))
                        .flex_none()
                        .text_color(muted)
                        .into_any_element()
                }));
                targets.push(self.model.device.clone());
                options.len() - 1
            }
        };
        widgets::select("mcp-device-switcher", "Device", theme, |page: &mut Self| {
            &mut page.device_select
        })
        .options(options, selected)
        .menu_width(260.0)
        .heading("Devices")
        .on_select(move |page, ix, _, cx| {
            if let Some(target) = targets.get(ix) {
                page.set_target_device(target.clone(), cx);
            }
        })
        .render(&self.device_select, cx)
        .into_any_element()
    }

    fn render_search(&self, theme: &Theme) -> AnyElement {
        div()
            .h(px(widgets::SELECT_HEIGHT))
            .px(px(10.0))
            .rounded(px(8.0))
            .bg(theme.input_glass_bg())
            .border_1()
            .border_color(theme.border)
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.0))
            .text_size(crate::typography::ui_rems(13.0))
            .child(
                crate::icons::icon(crate::icons::MAGNIFER)
                    .size(px(14.0))
                    .flex_none()
                    .text_color(theme.text_muted),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .child(self.search.clone()),
            )
            .into_any_element()
    }

    fn render_rows(&mut self, theme: &Theme, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let tools: Vec<McpToolState> = self.model.filtered().into_iter().cloned().collect();
        let writable = self.model.writable();
        let unreadable = self
            .model
            .snapshot()
            .is_some_and(|snapshot| snapshot.policy_unreadable);
        let mut rows = Vec::with_capacity(tools.len());
        for (ix, tool) in tools.iter().enumerate() {
            let name = tool.descriptor.name.clone();
            let enabled = self.model.shown_enabled(tool);
            let expanded = self.model.expanded.as_deref() == Some(name.as_str());
            let focus = self
                .row_focus
                .entry(name.clone())
                .or_insert_with(|| cx.focus_handle())
                .clone();
            let mut badges: Vec<&'static str> = Vec::new();
            if !enabled {
                badges.push("Off");
            }
            badges.extend(availability_label(tool.descriptor.availability));
            if tool.description_override.is_some() {
                badges.push("Edited description");
            }
            if self.model.dirty(&name) {
                badges.push("Unsaved edit");
            }
            let group = format!("mcp-tool-{ix}");
            let trigger_tool = name.clone();
            let key_tool = name.clone();
            let trigger = div()
                .id(("mcp-tool-trigger", ix))
                .group(group.clone())
                .flex_1()
                .min_w(px(180.0))
                .min_h(px(44.0))
                .px(px(4.0))
                .rounded(px(8.0))
                .flex()
                .flex_row()
                .items_center()
                .gap(px(12.0))
                .role(gpui::Role::Button)
                .aria_label(format!("{name} details"))
                .aria_expanded(expanded)
                .track_focus(&focus.tab_stop(true))
                .tab_index(0)
                .cursor_pointer()
                .focus_visible(|s| s.border_2().border_color(theme.accent))
                .on_click(
                    cx.listener(move |page, _, _, cx| page.toggle_expanded(&trigger_tool, cx)),
                )
                .on_key_down(cx.listener(move |page, event: &gpui::KeyDownEvent, _, cx| {
                    if !event.is_held && matches!(event.keystroke.key.as_str(), "enter" | "space") {
                        page.toggle_expanded(&key_tool, cx);
                        cx.stop_propagation();
                    }
                }))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .flex_wrap()
                                .items_center()
                                .gap(px(6.0))
                                .child(
                                    widgets::row_title(theme, name.clone())
                                        .font_family(theme.font_mono.clone())
                                        .when(!enabled, |title| title.text_color(theme.text_muted)),
                                )
                                .children(
                                    badges.into_iter().map(|label| widgets::badge(theme, label)),
                                ),
                        )
                        .child(
                            div()
                                .mt(px(Theme::TEXT_STACK_GAP))
                                .min_w_0()
                                .text_size(crate::typography::ui_rems(
                                    widgets::ROW_DESCRIPTION_SIZE,
                                ))
                                .line_height(crate::typography::ui_rems(16.0))
                                .text_color(theme.text_muted)
                                .truncate()
                                .child(SharedString::from(
                                    tool.effective_description()
                                        .lines()
                                        .next()
                                        .unwrap_or_default()
                                        .to_owned(),
                                )),
                        ),
                )
                .child(
                    crate::icons::icon(if expanded {
                        crate::icons::ALT_ARROW_DOWN
                    } else {
                        crate::icons::ALT_ARROW_RIGHT
                    })
                    .size(px(14.0))
                    .flex_none()
                    .text_color(theme.text_muted)
                    .group_hover(group, |s| s.text_color(theme.text)),
                );
            let switch_tool = name.clone();
            let key_switch_tool = name.clone();
            let switch = widgets::toggle_switch(theme, enabled, format!("mcp-switch-{name}"))
                .id(("mcp-tool-switch", ix))
                .role(gpui::Role::Switch)
                .aria_label(name.clone())
                .aria_toggled(if enabled {
                    gpui::Toggled::True
                } else {
                    gpui::Toggled::False
                })
                .when(!writable && (unreadable || !self.model.live), |el| {
                    el.opacity(0.55)
                })
                .when(writable || self.model.in_flight.is_some(), |el| {
                    el.cursor_pointer()
                        .tab_index(0)
                        .focus_visible(|s| s.border_2().border_color(theme.accent).opacity(1.0))
                        .on_click(cx.listener(move |page, _, _, cx| {
                            page.send(&switch_tool, WriteKind::Enable(!enabled), cx)
                        }))
                        .on_key_down(cx.listener(move |page, event: &gpui::KeyDownEvent, _, cx| {
                            if !event.is_held
                                && matches!(event.keystroke.key.as_str(), "enter" | "space")
                            {
                                page.send(&key_switch_tool, WriteKind::Enable(!enabled), cx);
                                cx.stop_propagation();
                            }
                        }))
                });
            let header = widgets::card_row(theme, ix == 0)
                .id(("mcp-tool-row", ix))
                .child(trigger)
                .child(switch);
            let details = expanded.then(|| self.render_details(tool, ix, theme, cx));
            rows.push(
                div()
                    .flex()
                    .flex_col()
                    .child(header)
                    .children(details)
                    .into_any_element(),
            );
        }
        rows
    }

    fn render_details(
        &self,
        tool: &McpToolState,
        ix: usize,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = tool.descriptor.name.clone();
        let dirty = self.model.dirty(&name);
        let writable = self.model.writable();
        let overridden = tool.description_override.is_some();
        let draft = self.model.draft(&name);
        let message: Option<SharedString> = draft
            .filter(|_| dirty)
            .and_then(|draft| validate_description_override(&draft.text).err())
            .map(|error| description_error_copy(error).into())
            .or_else(|| draft.and_then(|draft| draft.error.clone()));
        let can_save = dirty
            && writable
            && draft.is_some_and(|draft| validate_description_override(&draft.text).is_ok());
        let input = self
            .editor
            .as_ref()
            .filter(|editor| editor.key == self.model.key(&name))
            .map(|editor| editor.input.clone());

        let original_tool = name.clone();
        let reset_tool = name.clone();
        let description_actions = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(4.0))
            .when(overridden, |actions| {
                actions.child(keyboard_button(
                    widgets::text_action(
                        theme,
                        ActionTone::Quiet,
                        if self.model.show_original {
                            "Hide original"
                        } else {
                            "View original"
                        },
                    )
                    .id(("mcp-view-original", ix)),
                    theme,
                    cx,
                    move |page, cx| {
                        if page.model.expanded.as_deref() == Some(original_tool.as_str()) {
                            page.model.show_original = !page.model.show_original;
                            cx.notify();
                        }
                    },
                ))
            })
            // Reset never discards an unsaved edit: Save or Cancel it first.
            .when(overridden && !dirty && writable, |actions| {
                actions.child(keyboard_button(
                    widgets::text_action(theme, ActionTone::Quiet, "Reset description")
                        .id(("mcp-reset-description", ix)),
                    theme,
                    cx,
                    move |page, cx| page.send(&reset_tool, WriteKind::Reset, cx),
                ))
            });

        let field = div()
            .w_full()
            .px(px(12.0))
            .py(px(8.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(if message.is_some() {
                theme.danger.opacity(0.5)
            } else {
                theme.border
            })
            .bg(theme.input_glass_bg())
            .text_size(crate::typography::ui_rems(13.0))
            .children(input);

        let cancel_tool = name.clone();
        let save_tool = name.clone();
        let edit_actions = dirty.then(|| {
            div()
                .mt(px(8.0))
                .flex()
                .flex_row()
                .justify_end()
                .gap(px(8.0))
                .child(keyboard_button(
                    widgets::text_action(theme, ActionTone::Quiet, "Cancel")
                        .id(("mcp-description-cancel", ix)),
                    theme,
                    cx,
                    move |page, cx| page.cancel(&cancel_tool, cx),
                ))
                .child(
                    keyboard_button(
                        widgets::text_action(theme, ActionTone::Solid, "Save")
                            .id(("mcp-description-save", ix)),
                        theme,
                        cx,
                        move |page, cx| page.save(&save_tool, cx),
                    )
                    .when(!can_save, |button| button.opacity(0.45).cursor_default()),
                )
        });

        let original = (overridden && self.model.show_original).then(|| {
            div()
                .mt(px(12.0))
                .flex()
                .flex_col()
                .child(widgets::details_label(theme, "Original description"))
                .child(
                    div()
                        .px(px(12.0))
                        .py(px(8.0))
                        .rounded(px(8.0))
                        .bg(theme.wash(0.04))
                        .text_size(crate::typography::ui_rems(12.5))
                        .line_height(crate::typography::ui_rems(18.0))
                        .text_color(theme.text_muted)
                        .child(SharedString::from(tool.descriptor.description.clone())),
                )
        });

        let schema = serde_json::to_string_pretty(&tool.descriptor.input_schema)
            .unwrap_or_else(|_| tool.descriptor.input_schema.to_string());

        let content = div()
            .mx(px(16.0))
            .pl(px(4.0))
            .pb(px(16.0))
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(widgets::details_label(theme, "Description"))
                    .child(description_actions),
            )
            .child(field)
            .when_some(message, |el, message| {
                el.child(
                    div()
                        .mt(px(6.0))
                        .text_size(crate::typography::ui_rems(12.0))
                        .text_color(theme.danger_muted)
                        .child(message),
                )
            })
            .children(edit_actions)
            .children(original)
            .child(
                div()
                    .mt(px(12.0))
                    .flex()
                    .flex_col()
                    .child(widgets::details_label(theme, "Input schema"))
                    .child(
                        div()
                            .id(("mcp-input-schema", ix))
                            .role(gpui::Role::Group)
                            .aria_label(format!("{name} input schema"))
                            .px(px(12.0))
                            .py(px(10.0))
                            .rounded(px(8.0))
                            .bg(theme.wash(0.04))
                            .font_family(theme.font_mono.clone())
                            .text_size(crate::typography::ui_rems(11.5))
                            .line_height(crate::typography::ui_rems(17.0))
                            .text_color(theme.text_muted)
                            .child(SharedString::from(schema)),
                    ),
            );
        if motion::reduced_motion(cx) {
            content.into_any_element()
        } else {
            motion::menu_in(("mcp-tool-details", ix), content).into_any_element()
        }
    }

    fn render_body(&mut self, theme: &Theme, device: &str, cx: &mut Context<Self>) -> AnyElement {
        match &self.model.catalog {
            Catalog::Loading => widgets::section_card(theme)
                .p(px(16.0))
                .child(popover::skeleton_rows(
                    "mcp-skeleton",
                    theme,
                    6,
                    cx.entity_id(),
                    cx,
                ))
                .into_any_element(),
            Catalog::Unsupported => widgets::section_card(theme)
                .child(
                    widgets::card_row(theme, true).child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(widgets::row_title(theme, "Not available on this device"))
                            .child(widgets::meta_line(
                                theme,
                                vec![
                                    div()
                                        .child(SharedString::from(format!(
                                            "Update Zeron on {device} to inspect and configure its MCP tools."
                                        )))
                                        .into_any_element(),
                                ],
                            )),
                    ),
                )
                .into_any_element(),
            Catalog::Unavailable(message) => {
                let message = message.clone();
                div()
                    .child(widgets::error_strip(theme, message))
                    .child(keyboard_button(
                        widgets::ghost_action(theme).id("mcp-retry").mt(px(8.0)).child("Retry"),
                        theme,
                        cx,
                        |page, cx| {
                            page.watch(cx);
                            cx.notify();
                        },
                    ))
                    .into_any_element()
            }
            Catalog::Ready(snapshot) => {
                let unreadable = snapshot.policy_unreadable;
                let repair_tool = snapshot.tools.first().map(|tool| tool.descriptor.name.clone());
                let count = snapshot.tools.len();
                let live = self.model.live;
                let rows = self.render_rows(theme, cx);
                let empty = rows.is_empty();
                let mut column = div().flex().flex_col();
                if !live {
                    column = column.child(widgets::warning_strip(
                        theme,
                        format!(
                            "Can't reach {device}. These are its last known settings; changes resume when it reconnects."
                        ),
                    ));
                }
                if unreadable {
                    column = column.child(
                        div()
                            .child(widgets::warning_strip(
                                theme,
                                format!(
                                    "Zeron can't read the saved tool settings on {device}, so every tool is off. Restore the defaults to turn them back on."
                                ),
                            ))
                            .when_some(repair_tool.filter(|_| live), |el, tool| {
                                el.child(keyboard_button(
                                    widgets::text_action(theme, ActionTone::Filled, "Restore defaults")
                                        .id("mcp-repair")
                                        .mt(px(8.0)),
                                    theme,
                                    cx,
                                    move |page, cx| page.send(&tool, WriteKind::Repair, cx),
                                ))
                            }),
                    );
                }
                let card = if empty {
                    widgets::section_card(theme)
                        .mt(px(12.0))
                        .child(
                            widgets::card_row(theme, true).child(
                                div()
                                    .text_size(crate::typography::ui_rems(13.0))
                                    .text_color(theme.text_muted)
                                    .child(SharedString::from(format!(
                                        "No tools match “{}”.",
                                        self.model.query.trim()
                                    ))),
                            ),
                        )
                } else {
                    widgets::section_card(theme).mt(px(12.0)).children(rows)
                };
                column
                    .child(widgets::section(
                        theme,
                        format!("Zeron · {count} tools"),
                        div()
                            .flex()
                            .flex_col()
                            .child(self.render_search(theme))
                            .child(card)
                            .child(
                                div()
                                    .mt(px(10.0))
                                    .px(px(8.0))
                                    .text_size(crate::typography::ui_rems(12.0))
                                    .line_height(crate::typography::ui_rems(16.0))
                                    .text_color(theme.text_muted)
                                    .child(RECONNECT_NOTE),
                            ),
                    ))
                    .into_any_element()
            }
        }
    }

    fn on_scroll_hovered(&mut self, hovered: &bool, _: &mut Window, cx: &mut Context<Self>) {
        if self.scroll.set_list_hovered(*hovered) {
            cx.notify();
        }
    }
}

/// A settings action reachable by Tab and activated by Enter or Space.
fn keyboard_button(
    button: gpui::Stateful<gpui::Div>,
    theme: &Theme,
    cx: &mut Context<McpSettingsPage>,
    action: impl Fn(&mut McpSettingsPage, &mut Context<McpSettingsPage>) + 'static,
) -> gpui::Stateful<gpui::Div> {
    let action = std::rc::Rc::new(action);
    let on_key = action.clone();
    button
        .role(gpui::Role::Button)
        .tab_index(0)
        .focus_visible(|s| s.border_2().border_color(theme.accent))
        .on_click(cx.listener(move |page, _, _, cx| action(page, cx)))
        .on_key_down(cx.listener(move |page, event: &gpui::KeyDownEvent, _, cx| {
            if !event.is_held && matches!(event.keystroke.key.as_str(), "enter" | "space") {
                on_key(page, cx);
                cx.stop_propagation();
            }
        }))
}

#[cfg(feature = "mcp-settings-fixture")]
impl McpSettingsPage {
    /// Show `snapshot` as a live catalog without an engine, with `expanded`
    /// open on its original and `draft` typed into its description.
    pub fn fixture_show(
        &mut self,
        snapshot: McpCatalogSnapshot,
        expanded: &str,
        draft: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        self.watch_task = None;
        self.model.watched(self.model.generation, snapshot);
        self.model.expanded = Some(expanded.to_owned());
        self.model.show_original = true;
        self.sync_editor(cx);
        if let (Some(text), Some(editor)) = (draft, &self.editor) {
            editor
                .input
                .update(cx, |input, cx| input.set_text(text, cx));
        }
        cx.notify();
    }
}

impl popover::ScrollRailHost for McpSettingsPage {
    fn rail_bar(&mut self) -> &mut popover::MenuScrollbarState {
        self.scroll.rail_bar()
    }

    fn rail_scroll(&self) -> Option<gpui::ScrollHandle> {
        self.scroll.rail_scroll()
    }
}

impl Render for McpSettingsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(tool) = self.pending_focus.take()
            && let Some(handle) = self.row_focus.get(&tool)
        {
            window.focus(handle, cx);
        }
        crate::toast::take_error(&mut self.model.notice, cx);
        let theme = Theme::of(cx).for_settings_surface();
        let device = self.device_name(cx);
        let scope = scope_label(self.state.read(cx).workspace_scope);
        let switcher = self.render_device_switcher(&theme, cx);
        let body = self.render_body(&theme, &device, cx);
        let scrollbar = popover::rail(self, "mcp-page-scrollbar", &theme, cx);
        div()
            .id("mcp-page-host")
            .relative()
            .size_full()
            .on_hover(cx.listener(Self::on_scroll_hovered))
            .child(
                crate::edge_fade::edge_faded(
                    16.0,
                    true,
                    true,
                    div()
                        .id("mcp-page")
                        .size_full()
                        .overflow_y_scroll()
                        .track_scroll(&self.scroll.scroll)
                        .child(
                            widgets::page_column()
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .justify_between()
                                        .gap(px(12.0))
                                        .child(widgets::page_header(&theme, "MCP", None))
                                        .child(switcher),
                                )
                                .child(widgets::page_subtitle(
                                    &theme,
                                    format!(
                                        "Built-in tools Zeron offers to agents on {device}. Saved for this {scope} workspace only."
                                    ),
                                ))
                                .child(body),
                        ),
                )
                .fade_overflow_y(&self.scroll.scroll),
            )
            .children(scrollbar)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tool(name: &str, description: &str) -> McpToolState {
        McpToolState {
            descriptor: zeron_proto::McpToolDescriptor {
                name: name.into(),
                description: description.into(),
                input_schema: json!({ "type": "object", "properties": {} }),
                availability: McpToolAvailability::Always,
            },
            enabled: true,
            description_override: None,
        }
    }

    fn snapshot(revision: u64, tools: Vec<McpToolState>) -> McpCatalogSnapshot {
        McpCatalogSnapshot {
            catalog_version: 2,
            policy_revision: revision,
            tools,
            policy_unreadable: false,
        }
    }

    fn ready(tools: Vec<McpToolState>) -> McpModel {
        let mut model = McpModel::new();
        assert!(model.watched(0, snapshot(3, tools)));
        model
    }

    #[test]
    fn search_matches_every_word_across_name_and_descriptions() {
        let mut edited = tool("send_message", "Send a message to a chat.");
        edited.description_override = Some("Deliver text to a teammate".into());
        let mut model = ready(vec![
            tool("create_chat", "Create a chat on a device."),
            edited,
            tool("list_widgets", "List the widgets of a project."),
        ]);
        let names = |model: &McpModel| -> Vec<String> {
            model
                .filtered()
                .iter()
                .map(|tool| tool.descriptor.name.clone())
                .collect()
        };
        assert_eq!(names(&model).len(), 3);
        model.query = "CHAT create".into();
        assert_eq!(names(&model), ["create_chat"]);
        model.query = "teammate".into();
        assert_eq!(names(&model), ["send_message"]);
        // The original stays searchable under an override.
        model.query = "send a message".into();
        assert_eq!(names(&model), ["send_message"]);
        model.query = "nothing like this".into();
        assert!(names(&model).is_empty());
    }

    #[test]
    fn toggle_and_description_writes_stay_independent() {
        let mut model = ready(vec![tool("whoami", "Who is calling.")]);
        model.edit("whoami", "Identify the calling chat.".into());
        let (_, params) = model.begin("whoami", WriteKind::Enable(false)).unwrap();
        assert_eq!(params.enabled, Some(false));
        assert_eq!(params.description_patch, McpDescriptionPatch::Unchanged);
        assert_eq!(params.expected_policy_revision, 3);
        // The pending switch shows at once; the draft is untouched.
        let shown = model.tool("whoami").unwrap().clone();
        assert!(!model.shown_enabled(&shown));
        assert_eq!(
            model.draft("whoami").unwrap().text,
            "Identify the calling chat."
        );
        // One write at a time.
        assert!(model.begin("whoami", WriteKind::Reset).is_none());
    }

    #[test]
    fn conflict_and_failure_keep_the_draft_and_save_retires_only_its_text() {
        let mut model = ready(vec![tool("whoami", "Who is calling.")]);
        model.edit("whoami", "   ".into());
        assert!(
            model
                .begin("whoami", WriteKind::Save("   ".into()))
                .is_none()
        );
        assert!(model.draft("whoami").unwrap().error.is_some());

        model.edit("whoami", "Identify the calling chat.".into());
        let (write, params) = model
            .begin(
                "whoami",
                WriteKind::Save("Identify the calling chat.".into()),
            )
            .unwrap();
        assert_eq!(
            params.description_patch,
            McpDescriptionPatch::Set("Identify the calling chat.".into())
        );
        let outcome = model.finish(
            write,
            Err(RpcError::Capability(
                capability_errors::MCP_POLICY_CONFLICT.into(),
            )),
        );
        assert!(outcome.reload);
        let draft = model.draft("whoami").unwrap();
        assert_eq!(draft.text, "Identify the calling chat.");
        assert!(draft.error.is_some());

        // Someone else edits the description meanwhile: the first Save says
        // so instead of silently replacing it, the second one goes through.
        let mut changed = tool("whoami", "Who is calling.");
        changed.description_override = Some("Their text".into());
        assert!(model.watched(0, snapshot(4, vec![changed])));
        let text = "Identify the calling chat.".to_owned();
        assert!(
            model
                .begin("whoami", WriteKind::Save(text.clone()))
                .is_none()
        );
        let (write, _) = model
            .begin("whoami", WriteKind::Save(text.clone()))
            .unwrap();
        let mut saved = tool("whoami", "Who is calling.");
        saved.description_override = Some(text.clone());
        let outcome = model.finish(write, Ok(snapshot(5, vec![saved])));
        assert_eq!(outcome.focus.as_deref(), Some("whoami"));
        assert!(model.draft("whoami").is_none());
        assert_eq!(model.tool("whoami").unwrap().effective_description(), text);
    }

    #[test]
    fn a_late_reply_for_another_device_changes_nothing_shown() {
        let mut model = ready(vec![tool("whoami", "Who is calling.")]);
        model.edit("whoami", "Edited on this device".into());
        let (write, _) = model
            .begin("whoami", WriteKind::Save("Edited on this device".into()))
            .unwrap();

        assert!(model.retarget(Some("peer".into())));
        assert!(model.watched(1, snapshot(9, vec![tool("whoami", "Peer text.")])));
        model.edit("whoami", "Peer draft".into());

        let mut saved = tool("whoami", "Who is calling.");
        saved.description_override = Some("Edited on this device".into());
        let outcome = model.finish(write, Ok(snapshot(4, vec![saved])));
        assert_eq!(outcome, WriteOutcome::default());
        assert_eq!(model.snapshot().unwrap().policy_revision, 9);
        assert_eq!(model.draft("whoami").unwrap().text, "Peer draft");
        // Its own draft, under the first device's key, is retired.
        assert!(!model.drafts.contains_key(&DraftKey {
            device: None,
            tool: "whoami".into()
        }));
        // A stale watch item for the old device is ignored too.
        assert!(!model.watched(0, snapshot(10, vec![])));
        // The peer can be written to now.
        assert!(
            model
                .begin("whoami", WriteKind::Enable(false))
                .is_some_and(|(_, params)| params.target_device_id.as_deref() == Some("peer"))
        );
    }

    #[gpui::test]
    fn long_catalog_renders_and_cancel_returns_focus_to_the_row(cx: &mut gpui::TestAppContext) {
        use gpui::AppContext;
        let dir = tempfile::tempdir().unwrap();
        cx.update(|cx| {
            crate::settings::init(Default::default(), dir.path(), cx);
            gpui_base::init(cx);
            cx.set_global(crate::theme::Theme::default());
        });
        let window = cx.add_window(|_, cx| {
            let state = cx.new(|_| crate::state::AppState::new());
            McpSettingsPage::new(state, cx)
        });
        let long = "Publish a visual into the chat. ".repeat(200);
        let mut tools: Vec<McpToolState> = (0..24)
            .map(|ix| tool(&format!("tool_{ix}"), "A short description."))
            .collect();
        tools[3] = McpToolState {
            descriptor: zeron_proto::McpToolDescriptor {
                name: "show_visual".into(),
                description: long.clone(),
                input_schema: json!({
                    "type": "object",
                    "required": ["script"],
                    "properties": {
                        "script": { "type": "string", "description": long },
                        "assets": { "type": "array", "items": { "type": "string" } }
                    }
                }),
                availability: McpToolAvailability::ChatOnly,
            },
            enabled: false,
            description_override: Some("Edited".into()),
        };
        window
            .update(cx, |page, _, cx| {
                assert!(page.model.watched(0, snapshot(1, tools)));
                page.toggle_expanded("show_visual", cx);
                page.model.show_original = true;
                let input = page.editor.as_ref().unwrap().input.clone();
                input.update(cx, |input, cx| input.set_text("Edited again", cx));
            })
            .unwrap();
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
            .unwrap();
        window
            .update(cx, |page, _, cx| {
                assert!(page.model.dirty("show_visual"));
                page.cancel("show_visual", cx);
                assert!(page.model.draft("show_visual").is_none());
            })
            .unwrap();
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
            .unwrap();
        window
            .update(cx, |page, window, _| {
                assert!(page.row_focus["show_visual"].is_focused(window));
                assert_eq!(page.model.expanded.as_deref(), Some("show_visual"));
            })
            .unwrap();
    }
}

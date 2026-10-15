//! Project widgets in the app: "Save to project" under a chat's visual, and
//! the Widgets page of one project space.
//!
//! A widget belongs to one space on the engine of the device that owns it
//! (`project-widgets-v1`). Everything here reads and writes through that
//! engine's `*ProjectWidget*` RPCs, relay-forwarded to a remote owner; the
//! engine checks revisions and the space. The page shows one explicit space
//! and device: the clones of a repository are distinct places, never merged.
//!
//! [`WidgetsModel`] decides what is shown and sent, so the late-reply guard,
//! deletions, conflicts and the loading/empty/unavailable split are testable
//! without a window:
//! - every request remembers the target generation it was sent for, and a
//!   snapshot must name the space shown: a reply for another space or device
//!   never touches the page;
//! - a refused change keeps what is shown and says so in plain words; nothing
//!   is retried or forced.
//!
//! Widget content renders through [`crate::visuals::VisualFrame`] with a
//! [`VisualSource::Widget`] bundle source, one frame per
//! `(widgetId, contentRevision)`. The page lists widgets in a virtual list,
//! so off-screen visuals don't paint or animate, and it drops every frame
//! when it is left.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, FocusHandle, Focusable as _, ListAlignment,
    ListOffset, ListState, SharedString, Subscription, Task, Window, div, list, prelude::*, px,
};
use sha2::Digest as _;
use zeron_proto::{
    CancelProjectWidgetRefreshParams, DeleteProjectWidgetParams, GetProjectWidgetParams, HarnessId,
    PROJECT_WIDGET_INSTRUCTION_MAX_BYTES, PROJECT_WIDGET_SCRIPT_ASSET, ProjectWidget,
    ProjectWidgetAssetChunk, ProjectWidgetContent, ProjectWidgetDeletion, ProjectWidgetDetail,
    ProjectWidgetRecipe, ProjectWidgetRecipePatch, ProjectWidgetRefresh,
    ProjectWidgetRefreshAdmission, ProjectWidgetRefreshCancellation, ProjectWidgetRefreshState,
    ProjectWidgetWrite, ProjectWidgetWriteOutcome, ProjectWidgetsSnapshot,
    ReadProjectWidgetAssetChunkParams, RefreshProjectWidgetParams, ReorderProjectWidgetsParams,
    SaveProjectVisualParams, Space, UpdateProjectWidgetParams, VISUAL_MAX_ASSET_BYTES,
    VISUAL_MAX_ASSETS, VISUAL_MAX_BYTES, VISUAL_MAX_TITLE_CHARS, WatchProjectWidgetsParams,
    WorkspaceScope, capabilities,
};
use zeron_rpc::{RpcError, capability_errors, methods};

use crate::composer::{ComposerInput, ComposerInputEvent};
use crate::icons::{self, icon};
use crate::popover;
use crate::settings::widgets::{self, ActionTone};
use crate::state::{AppState, EngineHandle};
use crate::theme::Theme;
use crate::visuals::{VisualFrame, VisualLayoutChanged, VisualSource};

/// What a stale write is answered with: the change was refused, nothing
/// moved, and the current state must be loaded before trying again.
const CONFLICT_COPY: &str = "This widget changed elsewhere. Reload it before retrying.";
/// Why Refresh is disabled on an owner that predates refreshes.
const REFRESH_UNSUPPORTED_COPY: &str =
    "This device's Zeron can't refresh widgets. Update it first.";
/// What a running refresh is, for assistive technology and the tooltip.
const REFRESH_RUN_COPY: &str =
    "An agent run is making a new version. The current one stays until it succeeds.";
/// What the empty state's new chat asks for. Sent only by the user.
const DASHBOARD_REQUEST: &str =
    "Build a dashboard of this project's key metrics and show it as a visual.";
/// The skeleton waits this long, so a quick load never flashes it.
const SKELETON_DELAY: Duration = Duration::from_millis(200);
const READ_CHUNK_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_READ_CHUNKS: usize = 1_000;
/// The page's content measure: wider than Settings, so visuals keep their
/// inline proportions.
const PAGE_MAX_WIDTH: f32 = 920.0;

// ---- reading a bundle ------------------------------------------------------

/// Read a widget's script and assets at `content_revision` and verify each
/// file against the manifest. The error is the line the widget shows.
pub(crate) async fn read_bundle(
    engine: &EngineHandle,
    executor: &gpui::BackgroundExecutor,
    target: Option<&str>,
    widget_id: &str,
    content_revision: u64,
    content: &ProjectWidgetContent,
) -> Result<(String, zeron_visual::Assets), String> {
    let script = read_asset(
        engine,
        executor,
        target,
        widget_id,
        content_revision,
        PROJECT_WIDGET_SCRIPT_ASSET,
        content.script_size,
        &content.script_sha256,
        VISUAL_MAX_BYTES,
    )
    .await?;
    let script = String::from_utf8(script).map_err(|_| DAMAGED_COPY.to_owned())?;
    let mut files = HashMap::new();
    for asset in content.assets.iter().take(VISUAL_MAX_ASSETS) {
        if !zeron_visual::assets::valid_name(&asset.name) {
            return Err(DAMAGED_COPY.to_owned());
        }
        let bytes = read_asset(
            engine,
            executor,
            target,
            widget_id,
            content_revision,
            &asset.name,
            asset.size,
            &asset.sha256,
            VISUAL_MAX_ASSET_BYTES,
        )
        .await?;
        files.insert(asset.name.clone(), std::sync::Arc::<[u8]>::from(bytes));
    }
    Ok((script, zeron_visual::Assets::new(files)))
}

const UNREADABLE_COPY: &str = "This widget couldn't be loaded.";
const DAMAGED_COPY: &str = "This widget's content is damaged and can't be shown.";

#[allow(clippy::too_many_arguments)]
async fn read_asset(
    engine: &EngineHandle,
    executor: &gpui::BackgroundExecutor,
    target: Option<&str>,
    widget_id: &str,
    content_revision: u64,
    name: &str,
    size: u64,
    sha256: &str,
    max: usize,
) -> Result<Vec<u8>, String> {
    if size > max as u64 {
        return Err(DAMAGED_COPY.to_owned());
    }
    let limit = (size as usize).div_ceil(3) * 4;
    let mut encoded = String::new();
    let mut offset = 0u64;
    for _ in 0..MAX_READ_CHUNKS {
        let params = serde_json::to_value(ReadProjectWidgetAssetChunkParams {
            widget_id: widget_id.to_owned(),
            content_revision,
            asset_name: name.to_owned(),
            offset,
            target_device_id: target.map(str::to_owned),
        })
        .unwrap_or_default();
        let value = crate::attachments::call_with_timeout(
            engine,
            executor,
            methods::READ_PROJECT_WIDGET_ASSET_CHUNK,
            params,
            READ_CHUNK_TIMEOUT,
        )
        .await
        .map_err(|_| UNREADABLE_COPY.to_owned())?;
        let chunk: ProjectWidgetAssetChunk =
            serde_json::from_value(value).map_err(|_| UNREADABLE_COPY.to_owned())?;
        if chunk.size != size || chunk.sha256 != sha256 {
            return Err(DAMAGED_COPY.to_owned());
        }
        if encoded.len() + chunk.data.len() > limit {
            return Err(DAMAGED_COPY.to_owned());
        }
        encoded.push_str(&chunk.data);
        if chunk.done {
            let expected = sha256.to_owned();
            // Decoding and hashing up to 8 MiB stays off the UI thread.
            return executor
                .spawn(async move {
                    let bytes = BASE64.decode(encoded.as_bytes()).ok()?;
                    let digest = sha2::Sha256::digest(&bytes);
                    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
                    (bytes.len() as u64 == size && hex == expected).then_some(bytes)
                })
                .await
                .ok_or_else(|| DAMAGED_COPY.to_owned());
        }
        if chunk.next_offset <= offset {
            return Err(UNREADABLE_COPY.to_owned());
        }
        offset = chunk.next_offset;
    }
    Err(UNREADABLE_COPY.to_owned())
}

// ---- spaces ----------------------------------------------------------------

/// The space and device a Widgets page shows.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WidgetsTarget {
    pub space_id: String,
    pub device_id: String,
}

/// One space offered by an explicit choice, named so clones and worktrees of
/// the same repository read apart: device, then folder.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SpaceChoice {
    pub target: WidgetsTarget,
    pub name: String,
    pub place: String,
}

impl SpaceChoice {
    fn of(state: &AppState, space: &Space) -> Self {
        let device = state
            .device_name(&space.device_id)
            .filter(|name| !name.trim().is_empty())
            .unwrap_or("Unknown device");
        Self {
            target: WidgetsTarget {
                space_id: space.id.clone(),
                device_id: space.device_id.clone(),
            },
            name: space.display_name().to_owned(),
            place: format!("{device} · {}", space.path),
        }
    }
}

/// How a project's Widgets entry opens.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum WidgetsEntry {
    Open(WidgetsTarget),
    /// Several spaces share the repository and none is current: the user
    /// picks one. No owner is elected.
    Choose(Vec<SpaceChoice>),
}

/// The space the user is in: the selected chat's, else the new-chat canvas's.
pub(crate) fn current_space(state: &AppState) -> Option<String> {
    match state.selected_chat.as_deref() {
        Some(chat_id) => state
            .chats
            .iter()
            .find(|chat| chat.id == chat_id)
            .and_then(|chat| chat.space_id.clone()),
        None => state.selected_space.clone(),
    }
}

/// The Widgets entry of `space_id`'s project: the current space when it is
/// one of the project's, the only space when there is one, else a choice.
pub(crate) fn widgets_entry(
    state: &AppState,
    space_id: &str,
    current: Option<&str>,
) -> Option<WidgetsEntry> {
    let space = state.space_row(space_id)?;
    let members = state.project_members(space);
    let target = |space: &Space| WidgetsTarget {
        space_id: space.id.clone(),
        device_id: space.device_id.clone(),
    };
    if let Some(current) = current.and_then(|id| members.iter().find(|space| space.id == id)) {
        return Some(WidgetsEntry::Open(target(current)));
    }
    Some(match members.as_slice() {
        [only] => WidgetsEntry::Open(target(only)),
        _ => WidgetsEntry::Choose(
            members
                .iter()
                .map(|space| SpaceChoice::of(state, space))
                .collect(),
        ),
    })
}

/// `targetDeviceId` for an owner: absent for this device.
fn route_to(state: &AppState, device_id: &str) -> Option<String> {
    (state.local_device_id.as_deref() != Some(device_id)).then(|| device_id.to_owned())
}

fn device_label(state: &AppState, device_id: &str) -> String {
    state
        .device_name(device_id)
        .filter(|name| !name.trim().is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| "this device".into())
}

/// A failed widget request in plain words. Never codes, revisions or paths.
fn error_copy(error: &RpcError) -> String {
    let code = match error {
        RpcError::Capability(code) | RpcError::Failed(code) => code.as_str(),
        _ => "",
    };
    match error {
        _ if code == capability_errors::WIDGETS_CONFLICT => CONFLICT_COPY.into(),
        _ if code == capability_errors::WIDGETS_NOT_FOUND => {
            "This widget was deleted elsewhere.".into()
        }
        _ if code == capability_errors::WIDGETS_SOURCE_NOT_FOUND => {
            "This visual is no longer in its chat, so it can't be saved.".into()
        }
        _ if code == capability_errors::WIDGETS_OWNER_UNAVAILABLE => {
            "The project's device can't be reached, or the project was removed. Nothing changed."
                .into()
        }
        _ if code == capability_errors::WIDGETS_QUOTA_EXCEEDED => format!(
            "This project already holds {} widgets. Delete one first.",
            zeron_proto::PROJECT_WIDGETS_MAX_PER_SPACE
        ),
        _ if code == capability_errors::WIDGETS_WRITE_FAILED => {
            "Zeron couldn't save this change. Nothing changed.".into()
        }
        _ if code == capability_errors::WIDGETS_STORE_UNAVAILABLE => {
            "Zeron can't change widgets on this device right now.".into()
        }
        _ if code.starts_with(capability_errors::WIDGETS_INVALID_VISUAL) => {
            "This visual can't be kept as a widget.".into()
        }
        // The owner says what is missing: no recipe, an agent that isn't
        // available there, or update_widget switched off in Settings → MCP.
        _ if code.starts_with(capability_errors::WIDGETS_REFRESH_UNAVAILABLE) => {
            match code.split_once(": ") {
                Some((_, reason)) => format!("Can't refresh: {reason}."),
                None => "This widget can't be refreshed on its device.".into(),
            }
        }
        _ if code == capability_errors::WIDGETS_REFRESH_INACTIVE => {
            "This refresh already ended.".into()
        }
        RpcError::UnknownMethod(_) => {
            "This device's Zeron can't keep widgets. Update it first.".into()
        }
        RpcError::Transport(_) | RpcError::Closed => {
            "Couldn't reach the device. Nothing changed; try again once it's online.".into()
        }
        _ => "Something went wrong. Nothing changed.".into(),
    }
}

fn is_code(error: &RpcError, code: &str) -> bool {
    matches!(error, RpcError::Capability(found) if found == code)
}

/// A settings-style action reachable by Tab and activated by Enter or Space.
fn keyboard_button<V: 'static>(
    button: gpui::Stateful<gpui::Div>,
    theme: &Theme,
    cx: &mut Context<V>,
    action: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
) -> gpui::Stateful<gpui::Div> {
    let action = std::rc::Rc::new(action);
    let on_key = action.clone();
    button
        .role(gpui::Role::Button)
        .tab_index(0)
        .focus_visible(|s| s.border_2().border_color(theme.accent))
        .on_click(cx.listener(move |view, _, window, cx| action(view, window, cx)))
        .on_key_down(
            cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                if !event.is_held && matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    on_key(view, window, cx);
                    cx.stop_propagation();
                }
            }),
        )
}

// ---- "Save to project" under a chat visual --------------------------------

/// Where a chat's visual can be saved. V1 keeps it on the engine that holds
/// the chat: a remote copy would be a separate, explicit action.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SaveTarget {
    device_id: String,
    /// The chat's own space, when it has a live one on that device.
    space: Option<SpaceChoice>,
    /// Otherwise the device's spaces, for an explicit choice.
    choices: Vec<SpaceChoice>,
}

pub(crate) fn save_target(state: &AppState, chat_id: &str) -> Option<SaveTarget> {
    let chat = state.chats.iter().find(|chat| chat.id == chat_id)?;
    if !state.device_supports(&chat.device_id, capabilities::PROJECT_WIDGETS_V1) {
        return None;
    }
    let space = chat
        .space_id
        .as_deref()
        .and_then(|id| state.space_row(id))
        .filter(|space| space.device_id == chat.device_id)
        .map(|space| SpaceChoice::of(state, space));
    let choices = if space.is_some() {
        Vec::new()
    } else {
        state
            .spaces_sorted()
            .into_iter()
            .filter(|space| space.device_id == chat.device_id)
            .map(|space| SpaceChoice::of(state, space))
            .collect()
    };
    Some(SaveTarget {
        device_id: chat.device_id.clone(),
        space,
        choices,
    })
}

pub enum SaveVisualEvent {
    /// The chrome changed height: the transcript remeasures the row.
    LayoutChanged,
    OpenWidgets {
        space_id: String,
        device_id: String,
    },
}

struct SaveForm {
    title: Entity<ComposerInput>,
    /// The explicit choice for a chat without a project.
    choice: Option<WidgetsTarget>,
    error: Option<SharedString>,
    saving: Option<Task<()>>,
    _events: Subscription,
}

/// The "Save to project" chrome under one chat visual, outside its Luau tree.
pub struct SaveVisual {
    state: Entity<AppState>,
    chat_id: String,
    visual_id: String,
    visual_title: String,
    form: Option<SaveForm>,
    /// Where it was saved, and whether it already was.
    saved: Option<(WidgetsTarget, bool)>,
    space_select: widgets::SelectState,
    trigger_focus: FocusHandle,
    open_focus: FocusHandle,
    pending_focus: Option<FocusHandle>,
}

impl EventEmitter<SaveVisualEvent> for SaveVisual {}

impl SaveVisual {
    pub fn new(
        state: Entity<AppState>,
        chat_id: String,
        visual_id: String,
        visual_title: String,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            state,
            chat_id,
            visual_id,
            visual_title,
            form: None,
            saved: None,
            space_select: widgets::SelectState::default(),
            trigger_focus: cx.focus_handle(),
            open_focus: cx.focus_handle(),
            pending_focus: None,
        }
    }

    fn changed(&mut self, cx: &mut Context<Self>) {
        cx.emit(SaveVisualEvent::LayoutChanged);
        cx.notify();
    }

    fn open_form(&mut self, cx: &mut Context<Self>) {
        let title = cx.new(|cx| {
            ComposerInput::new("Widget title", cx)
                .with_single_line()
                .with_tab_stop()
        });
        let initial = if self.visual_title.trim().is_empty() {
            "Visual".to_owned()
        } else {
            self.visual_title.clone()
        };
        title.update(cx, |input, cx| input.set_text(initial, cx));
        let events = cx.subscribe(&title, |this: &mut Self, _, event, cx| {
            if matches!(event, ComposerInputEvent::Submitted) {
                this.save(cx);
            }
        });
        self.pending_focus = Some(title.read(cx).focus_handle(cx));
        self.form = Some(SaveForm {
            title,
            choice: None,
            error: None,
            saving: None,
            _events: events,
        });
        self.changed(cx);
    }

    fn close_form(&mut self, cx: &mut Context<Self>) {
        self.form = None;
        widgets::close_select(self, |this: &mut Self| &mut this.space_select, cx);
        self.pending_focus = Some(self.trigger_focus.clone());
        self.changed(cx);
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let (engine, target, local) = {
            let state = self.state.read(cx);
            (
                state.engine().cloned(),
                save_target(state, &self.chat_id),
                state.local_device_id.clone(),
            )
        };
        let Some(form) = self.form.as_mut() else {
            return;
        };
        if form.saving.is_some() {
            return;
        }
        let (Some(engine), Some(target)) = (engine, target) else {
            form.error = Some("Zeron can't save this visual right now.".into());
            self.changed(cx);
            return;
        };
        let Some(space) = target
            .space
            .as_ref()
            .map(|space| space.target.clone())
            .or_else(|| form.choice.clone())
        else {
            form.error = Some("Choose a project for this widget.".into());
            self.changed(cx);
            return;
        };
        let title = form.title.read(cx).text().trim().to_owned();
        if title.chars().count() > VISUAL_MAX_TITLE_CHARS {
            form.error = Some(
                format!("Shorten the title to {VISUAL_MAX_TITLE_CHARS} characters or less.").into(),
            );
            self.changed(cx);
            return;
        }
        // One operation per click: a retried reply is answered as `replayed`,
        // and the same visual saved again as `existing`.
        let params = SaveProjectVisualParams {
            space_id: space.space_id.clone(),
            source_chat_id: self.chat_id.clone(),
            source_visual_id: self.visual_id.clone(),
            title: (!title.is_empty()).then_some(title),
            operation_id: uuid::Uuid::new_v4().to_string(),
            target_device_id: (local.as_deref() != Some(target.device_id.as_str()))
                .then(|| target.device_id.clone()),
        };
        let params = serde_json::to_value(params).unwrap_or_default();
        form.error = None;
        form.saving = Some(cx.spawn(async move |this, cx| {
            let result = engine
                .client()
                .call_as::<ProjectWidgetWrite>(methods::SAVE_PROJECT_VISUAL, params)
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(write) => {
                        this.form = None;
                        this.saved = Some((
                            WidgetsTarget {
                                space_id: write.widget.space_id,
                                device_id: write.widget.owner_device_id,
                            },
                            write.outcome == ProjectWidgetWriteOutcome::Existing,
                        ));
                        this.pending_focus = Some(this.open_focus.clone());
                    }
                    Err(error) => {
                        if let Some(form) = this.form.as_mut() {
                            form.saving = None;
                            form.error = Some(error_copy(&error).into());
                        }
                    }
                }
                this.changed(cx);
            })
            .ok();
        }));
        self.changed(cx);
    }

    fn render_form(
        &mut self,
        target: &SaveTarget,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let device = device_label(self.state.read(cx), &target.device_id);
        let Some(form) = self.form.as_ref() else {
            return gpui::Empty.into_any_element();
        };
        let saving = form.saving.is_some();
        let input = form.title.clone();
        let error = form.error.clone();
        let place: AnyElement = match (&target.space, target.choices.as_slice()) {
            (Some(space), _) => widgets::meta_line(
                theme,
                vec![
                    div()
                        .child(SharedString::from(format!(
                            "Saves to {} · {}",
                            space.name, space.place
                        )))
                        .into_any_element(),
                ],
            )
            .into_any_element(),
            (None, []) => widgets::meta_line(
                theme,
                vec![
                    div()
                        .child(SharedString::from(format!(
                            "This chat has no project. Add a project on {device} to keep widgets there."
                        )))
                        .into_any_element(),
                ],
            )
            .into_any_element(),
            (None, choices) => {
                let chosen = form.choice.clone();
                let mut options = vec![widgets::SelectOption::new("Choose a project")];
                options.extend(choices.iter().map(|choice| {
                    widgets::SelectOption::new(choice.name.clone()).detail(choice.place.clone())
                }));
                let selected = chosen
                    .as_ref()
                    .and_then(|chosen| choices.iter().position(|c| &c.target == chosen))
                    .map_or(0, |ix| ix + 1);
                let targets: Vec<Option<WidgetsTarget>> = std::iter::once(None)
                    .chain(choices.iter().map(|choice| Some(choice.target.clone())))
                    .collect();
                widgets::select("save-visual-project", "Project", theme, |this: &mut Self| {
                    &mut this.space_select
                })
                .options(options, selected)
                .menu_width(320.0)
                .heading("Projects")
                .open_rightward()
                .on_select(move |this, ix, _, cx| {
                    if let Some(form) = this.form.as_mut() {
                        form.choice = targets.get(ix).cloned().flatten();
                        form.error = None;
                    }
                    this.changed(cx);
                })
                .render(&self.space_select, cx)
                .into_any_element()
            }
        };
        let can_save = !saving && (target.space.is_some() || !target.choices.is_empty());
        div()
            .id("save-visual-form")
            .role(gpui::Role::Group)
            .aria_label("Save to project")
            .mt(px(6.0))
            .w_full()
            .max_w(px(520.0))
            .p(px(12.0))
            .rounded(px(10.0))
            .bg(widgets::block_fill(theme))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" && !this.space_select.is_open() {
                    this.close_form(cx);
                    cx.stop_propagation();
                }
            }))
            .child(widgets::field_label(theme, "Save to project"))
            .child(popover::dialog_field(input.into_any_element()))
            .child(place)
            .when_some(error, |el, error| {
                el.child(
                    div()
                        .id("save-visual-error")
                        .role(gpui::Role::Alert)
                        .text_size(crate::typography::ui_rems(12.0))
                        .text_color(theme.danger_muted)
                        .child(error),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .justify_end()
                    .gap(px(8.0))
                    .child(keyboard_button(
                        widgets::text_action(theme, ActionTone::Quiet, "Cancel")
                            .id("save-visual-cancel"),
                        theme,
                        cx,
                        |this, _, cx| this.close_form(cx),
                    ))
                    .child(
                        keyboard_button(
                            widgets::text_action(
                                theme,
                                ActionTone::Solid,
                                if saving { "Saving…" } else { "Save" },
                            )
                            .id("save-visual-confirm"),
                            theme,
                            cx,
                            |this, _, cx| this.save(cx),
                        )
                        .when(!can_save, |button| button.opacity(0.45).cursor_default()),
                    ),
            )
            .into_any_element()
    }
}

impl Render for SaveVisual {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(handle) = self.pending_focus.take() {
            window.focus(&handle, cx);
        }
        let Some(target) = save_target(self.state.read(cx), &self.chat_id) else {
            return gpui::Empty.into_any_element();
        };
        let theme = Theme::of(cx).clone();
        if self.form.is_some() {
            return self.render_form(&target, &theme, cx);
        }
        let row = div()
            .mt(px(6.0))
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap(px(4.0))
            .text_size(crate::typography::ui_rems(12.5));
        match self.saved.clone() {
            Some((saved, existing)) => row
                .child(
                    div()
                        .id("save-visual-status")
                        .role(gpui::Role::Status)
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .px(px(4.0))
                        .text_color(theme.text_muted)
                        .child(icon(icons::CHECK).size(px(14.0)).text_color(theme.success))
                        .child(if existing { "Already saved" } else { "Saved" }),
                )
                .child(div().text_color(theme.text_faint).child("·"))
                .child(
                    keyboard_button(
                        widgets::text_action(&theme, ActionTone::Quiet, "Open widgets")
                            .id("save-visual-open")
                            .track_focus(&self.open_focus.clone().tab_stop(true)),
                        &theme,
                        cx,
                        move |_, _, cx| {
                            cx.emit(SaveVisualEvent::OpenWidgets {
                                space_id: saved.space_id.clone(),
                                device_id: saved.device_id.clone(),
                            })
                        },
                    )
                    .role(gpui::Role::Link),
                )
                .into_any_element(),
            None => row
                .child(keyboard_button(
                    widgets::ghost_action(&theme)
                        .id("save-visual")
                        .track_focus(&self.trigger_focus.clone().tab_stop(true))
                        .aria_label("Save visual to project")
                        .child(
                            icon(icons::FLOPPY_DISK)
                                .size(px(14.0))
                                .text_color(theme.text_muted),
                        )
                        .child("Save to project"),
                    &theme,
                    cx,
                    |this, _, cx| this.open_form(cx),
                ))
                .into_any_element(),
        }
    }
}

// ---- the page's state ------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Collection {
    /// Nothing confirmed yet. Never shown as empty.
    Loading,
    Ready(ProjectWidgetsSnapshot),
    /// The owner predates project widgets.
    Unsupported,
    /// Nothing loaded and the owner can't be reached.
    Unavailable(String),
}

#[derive(Debug, Clone, PartialEq)]
enum MutationKind {
    Rename(String),
    /// Save the refresh recipe, then refresh.
    Recipe(ProjectWidgetRecipe),
    /// Move by this many places (−1 up, +1 down).
    Move(isize),
    Delete,
}

#[derive(Debug, Clone)]
struct Mutation {
    generation: u64,
    widget_id: String,
    kind: MutationKind,
    operation_id: String,
}

enum Request {
    Update(UpdateProjectWidgetParams),
    Reorder(ReorderProjectWidgetsParams),
    Delete(DeleteProjectWidgetParams),
}

/// What a successful mutation answered.
enum Applied {
    Widget(ProjectWidget),
    Collection(ProjectWidgetsSnapshot),
    Deleted,
}

#[derive(Debug, Default, PartialEq)]
struct MutationOutcome {
    /// The widget whose actions take focus back (`None` with `settled`:
    /// the page heading, the list being empty).
    focus: Option<String>,
    /// The reply was for what is shown, and was applied.
    settled: bool,
}

struct WidgetsModel {
    target: Option<WidgetsTarget>,
    /// Bumped on every retarget; replies carry the one they were sent for.
    generation: u64,
    collection: Collection,
    /// The watch is delivering. Without it the owner is unavailable: what is
    /// shown stays, and nothing is written.
    live: bool,
    in_flight: Option<Mutation>,
    /// A refused change or failed request, per widget, in plain words.
    problems: HashMap<String, SharedString>,
}

impl WidgetsModel {
    fn new() -> Self {
        Self {
            target: None,
            generation: 0,
            collection: Collection::Loading,
            live: false,
            in_flight: None,
            problems: HashMap::new(),
        }
    }

    fn retarget(&mut self, target: Option<WidgetsTarget>) -> bool {
        if self.target == target {
            return false;
        }
        self.target = target;
        self.generation += 1;
        self.collection = Collection::Loading;
        self.live = false;
        self.in_flight = None;
        self.problems.clear();
        true
    }

    fn snapshot(&self) -> Option<&ProjectWidgetsSnapshot> {
        match &self.collection {
            Collection::Ready(snapshot) => Some(snapshot),
            _ => None,
        }
    }

    fn widgets(&self) -> &[ProjectWidget] {
        self.snapshot().map_or(&[], |snapshot| &snapshot.widgets)
    }

    fn widget(&self, id: &str) -> Option<&ProjectWidget> {
        self.widgets().iter().find(|widget| widget.id == id)
    }

    /// A watched snapshot. One for another generation or space is ignored.
    fn watched(&mut self, generation: u64, snapshot: ProjectWidgetsSnapshot) -> bool {
        if generation != self.generation
            || self.target.as_ref().map(|t| t.space_id.as_str()) != Some(&snapshot.space_id)
        {
            return false;
        }
        self.problems
            .retain(|id, _| snapshot.widgets.iter().any(|widget| &widget.id == id));
        self.collection = Collection::Ready(snapshot);
        self.live = true;
        true
    }

    fn watch_lost(&mut self, generation: u64, message: String) {
        if generation != self.generation {
            return;
        }
        self.live = false;
        if !matches!(self.collection, Collection::Ready(_)) {
            self.collection = Collection::Unavailable(message);
        }
    }

    fn unsupported(&mut self, generation: u64) {
        if generation == self.generation {
            self.collection = Collection::Unsupported;
            self.live = false;
        }
    }

    /// Changes can be sent: live, loaded, nothing pending.
    fn writable(&self) -> bool {
        self.live && self.in_flight.is_none() && self.snapshot().is_some()
    }

    /// Validate a change and record it as in flight. `Err` explains why it
    /// can't be sent (shown on the widget); `Ok(None)` changes nothing.
    fn begin(
        &mut self,
        widget_id: &str,
        kind: MutationKind,
    ) -> Result<Option<(Mutation, Request)>, String> {
        if !self.writable() {
            return Ok(None);
        }
        let (Some(_), Some(snapshot), Some(widget)) = (
            self.target.as_ref(),
            self.snapshot(),
            self.widget(widget_id),
        ) else {
            return Ok(None);
        };
        // `targetDeviceId` is the page's routing, set when it is sent.
        let operation_id = uuid::Uuid::new_v4().to_string();
        let request = match &kind {
            MutationKind::Rename(title) => {
                let title = title.trim();
                if title.is_empty() {
                    return Err("Enter a title.".into());
                }
                if title.chars().count() > VISUAL_MAX_TITLE_CHARS {
                    return Err(format!(
                        "Shorten the title to {VISUAL_MAX_TITLE_CHARS} characters or less."
                    ));
                }
                if title == widget.title {
                    return Ok(None);
                }
                Request::Update(UpdateProjectWidgetParams {
                    widget_id: widget.id.clone(),
                    expected_revision: widget.revision,
                    operation_id: operation_id.clone(),
                    title: Some(title.to_owned()),
                    refresh_recipe: Default::default(),
                    content: None,
                    refresh_id: None,
                    origin_chat_id: None,
                    target_device_id: None,
                })
            }
            MutationKind::Recipe(recipe) => {
                let instruction = recipe.instruction.trim();
                if instruction.is_empty() {
                    return Err("Say what the refresh should do.".into());
                }
                if instruction.len() > PROJECT_WIDGET_INSTRUCTION_MAX_BYTES {
                    return Err("Shorten the instruction.".into());
                }
                let recipe = ProjectWidgetRecipe {
                    instruction: instruction.to_owned(),
                    ..recipe.clone()
                };
                if widget.refresh_recipe.as_ref() == Some(&recipe) {
                    return Ok(None);
                }
                Request::Update(UpdateProjectWidgetParams {
                    widget_id: widget.id.clone(),
                    expected_revision: widget.revision,
                    operation_id: operation_id.clone(),
                    title: None,
                    refresh_recipe: ProjectWidgetRecipePatch::Set(recipe),
                    content: None,
                    refresh_id: None,
                    origin_chat_id: None,
                    target_device_id: None,
                })
            }
            MutationKind::Move(delta) => {
                let mut ids: Vec<String> = snapshot.widgets.iter().map(|w| w.id.clone()).collect();
                let Some(from) = ids.iter().position(|id| id == widget_id) else {
                    return Ok(None);
                };
                let to = from as isize + delta;
                if to < 0 || to as usize >= ids.len() {
                    return Ok(None);
                }
                let id = ids.remove(from);
                ids.insert(to as usize, id);
                Request::Reorder(ReorderProjectWidgetsParams {
                    space_id: snapshot.space_id.clone(),
                    widget_ids: ids,
                    expected_collection_revision: snapshot.collection_revision,
                    operation_id: operation_id.clone(),
                    target_device_id: None,
                })
            }
            MutationKind::Delete => Request::Delete(DeleteProjectWidgetParams {
                widget_id: widget.id.clone(),
                expected_revision: widget.revision,
                operation_id: operation_id.clone(),
                target_device_id: None,
            }),
        };
        let mutation = Mutation {
            generation: self.generation,
            widget_id: widget_id.to_owned(),
            kind,
            operation_id,
        };
        self.problems.remove(widget_id);
        self.in_flight = Some(mutation.clone());
        Ok(Some((mutation, request)))
    }

    /// Settle a mutation. A reply for another target or an older generation
    /// changes nothing shown.
    fn finish(&mut self, mutation: Mutation, result: Result<Applied, RpcError>) -> MutationOutcome {
        let current = self
            .in_flight
            .as_ref()
            .is_some_and(|pending| pending.operation_id == mutation.operation_id);
        if !current || mutation.generation != self.generation {
            return MutationOutcome::default();
        }
        self.in_flight = None;
        let id = mutation.widget_id.clone();
        let deleted = match (&mutation.kind, result) {
            (_, Ok(Applied::Widget(widget))) => {
                if let Collection::Ready(snapshot) = &mut self.collection
                    && let Some(shown) = snapshot.widgets.iter_mut().find(|w| w.id == widget.id)
                    && widget.revision >= shown.revision
                {
                    *shown = widget;
                }
                false
            }
            (_, Ok(Applied::Collection(snapshot))) => {
                // A watch item may already have delivered something newer.
                if self.snapshot().is_some_and(|shown| {
                    shown.space_id == snapshot.space_id
                        && snapshot.collection_revision >= shown.collection_revision
                }) {
                    self.collection = Collection::Ready(snapshot);
                }
                false
            }
            (_, Ok(Applied::Deleted)) => true,
            // Already gone elsewhere: what the user asked for holds.
            (MutationKind::Delete, Err(error))
                if is_code(&error, capability_errors::WIDGETS_NOT_FOUND) =>
            {
                true
            }
            (_, Err(error)) => {
                self.problems.insert(id.clone(), error_copy(&error).into());
                false
            }
        };
        if !deleted {
            return MutationOutcome {
                focus: Some(id),
                settled: true,
            };
        }
        // Focus moves to the widget that takes its place, else the previous.
        let widgets = self.widgets();
        let focus = widgets.iter().position(|w| w.id == id).and_then(|ix| {
            widgets
                .get(ix + 1)
                .or_else(|| ix.checked_sub(1).and_then(|ix| widgets.get(ix)))
                .map(|w| w.id.clone())
        });
        if let Collection::Ready(snapshot) = &mut self.collection {
            snapshot.widgets.retain(|w| w.id != id);
        }
        self.problems.remove(&id);
        MutationOutcome {
            focus,
            settled: true,
        }
    }

    /// An attempt the owner answered a refresh or cancel with. The watch
    /// may already show something newer: never step back from it.
    fn refresh_settled(&mut self, generation: u64, widget_id: &str, refresh: ProjectWidgetRefresh) {
        if generation != self.generation {
            return;
        }
        let Collection::Ready(snapshot) = &mut self.collection else {
            return;
        };
        let Some(widget) = snapshot.widgets.iter_mut().find(|w| w.id == widget_id) else {
            return;
        };
        let newer = match &widget.refresh {
            None => true,
            Some(shown) if shown.refresh_id == refresh.refresh_id => {
                refresh_active(shown.state) && !refresh_active(refresh.state)
            }
            Some(shown) => shown.started_at <= refresh.started_at,
        };
        if newer {
            widget.refresh = Some(refresh);
        }
    }
}

fn refresh_active(state: ProjectWidgetRefreshState) -> bool {
    matches!(
        state,
        ProjectWidgetRefreshState::Queued
            | ProjectWidgetRefreshState::Running
            | ProjectWidgetRefreshState::AwaitingInput
    )
}

/// What a widget's Refresh area offers, from its latest attempt. The last
/// good content stays shown whatever the attempt does.
#[derive(Debug, Clone, PartialEq)]
enum RefreshControl {
    /// The owner predates refreshes: shown disabled, saying why.
    Unsupported,
    /// Refresh can start; without a recipe it asks for one first.
    Start { configure: bool },
    /// The agent run is working: Cancel and Open run.
    Running {
        refresh_id: String,
        chat_id: Option<String>,
    },
    /// The agent asked a question: its run needs an answer.
    AwaitingInput {
        refresh_id: String,
        chat_id: Option<String>,
    },
    /// Cancelled; the content is unchanged and Refresh starts again.
    Cancelled,
    /// Ended without publishing: what happened, and Retry.
    Failed {
        message: String,
        chat_id: Option<String>,
    },
}

fn refresh_control(widget: &ProjectWidget, supported: bool) -> RefreshControl {
    if !supported {
        return RefreshControl::Unsupported;
    }
    let start = RefreshControl::Start {
        configure: widget.refresh_recipe.is_none(),
    };
    let Some(refresh) = &widget.refresh else {
        return start;
    };
    let refresh_id = refresh.refresh_id.clone();
    let chat_id = refresh.execution_chat_id.clone();
    match refresh.state {
        ProjectWidgetRefreshState::Queued | ProjectWidgetRefreshState::Running => {
            RefreshControl::Running {
                refresh_id,
                chat_id,
            }
        }
        ProjectWidgetRefreshState::AwaitingInput => RefreshControl::AwaitingInput {
            refresh_id,
            chat_id,
        },
        ProjectWidgetRefreshState::Succeeded => start,
        ProjectWidgetRefreshState::Cancelled => RefreshControl::Cancelled,
        ProjectWidgetRefreshState::Failed
        | ProjectWidgetRefreshState::Interrupted
        | ProjectWidgetRefreshState::Conflict
        | ProjectWidgetRefreshState::Other => RefreshControl::Failed {
            message: refresh
                .error_message
                .as_deref()
                .map(str::trim)
                .filter(|message| !message.is_empty())
                .map(|message| message.chars().take(300).collect())
                .unwrap_or_else(|| "The refresh didn't finish.".into()),
            chat_id,
        },
    }
}

// ---- the page --------------------------------------------------------------

pub enum ProjectWidgetsEvent {
    /// Open a chat as it is, archived or not.
    OpenChat(String),
    /// A new chat in this space, its composer holding a dashboard request.
    NewDashboardChat { space_id: String, request: String },
    /// Show another space of the same project.
    OpenSpace(WidgetsTarget),
}

/// One widget's renderer at one content revision.
struct WidgetFrame {
    content_revision: u64,
    frame: Entity<VisualFrame>,
    _subscriptions: [Subscription; 2],
}

/// The list's rows: the page header, then a state body or the widgets.
#[derive(Debug, Clone, PartialEq)]
enum Item {
    Header,
    Body,
    Widget(String),
}

struct RowMenu {
    widget_id: String,
    /// The keyboard highlight.
    active: Option<usize>,
}

struct RenameDialog {
    widget_id: String,
    input: Entity<ComposerInput>,
    error: Option<SharedString>,
    focus_pending: bool,
    _events: Subscription,
}

/// The first Refresh of a widget without a recipe: what to ask, and which
/// agent runs it. Nothing is prefilled from the chat it came from.
struct RecipeDialog {
    widget_id: String,
    instruction: Entity<ComposerInput>,
    model: Entity<ComposerInput>,
    /// The agents enabled on the widget's device; `None` while loading.
    harnesses: Option<Vec<(HarnessId, String)>>,
    harness: usize,
    error: Option<SharedString>,
    focus_pending: bool,
    _load: Task<()>,
}

pub struct ProjectWidgetsPage {
    state: Entity<AppState>,
    model: WidgetsModel,
    list: ListState,
    items: Vec<Item>,
    frames: HashMap<String, WidgetFrame>,
    /// Widgets whose source chat the owner reported gone.
    source_missing: HashSet<String>,
    /// Widgets whose source chat the owner still has, though it isn't
    /// listed here.
    source_present: HashSet<String>,
    /// Widgets whose source was looked up (here, or by asking the owner).
    source_checked: HashSet<String>,
    source_tasks: Vec<Task<()>>,
    menu: Option<RowMenu>,
    menu_focus: FocusHandle,
    /// Opened from the keyboard: the menu takes focus on the next frame.
    menu_focus_pending: bool,
    rename: Option<RenameDialog>,
    recipe: Option<RecipeDialog>,
    recipe_select: widgets::SelectState,
    /// Widgets with a refresh or cancel request on its way.
    refresh_pending: HashSet<String>,
    refresh_tasks: HashMap<String, Task<()>>,
    delete_confirm: Option<String>,
    /// Stable focus per widget: its section (kept rendered while it holds
    /// focus) and its actions trigger.
    section_focus: HashMap<String, FocusHandle>,
    trigger_focus: HashMap<String, FocusHandle>,
    heading_focus: FocusHandle,
    pending_focus: Option<Option<String>>,
    space_select: widgets::SelectState,
    show_skeleton: bool,
    visible: bool,
    engine_key: Option<(String, WorkspaceScope)>,
    skeleton_task: Option<Task<()>>,
    watch_task: Option<Task<()>>,
    write_task: Option<Task<()>>,
    _state: Subscription,
}

impl EventEmitter<ProjectWidgetsEvent> for ProjectWidgetsPage {}

impl ProjectWidgetsPage {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        // A profile switch or reconnect replaces the engine: start over on
        // it. Peers' capabilities arrive as they sync.
        let state_sub = cx.observe(&state, |page: &mut Self, _, cx| {
            if !page.visible {
                return;
            }
            let key = page.current_engine_key(cx);
            if key != page.engine_key {
                page.engine_key = key;
                let target = page.model.target.clone();
                page.reset(cx);
                page.open(target, cx);
            } else if matches!(page.model.collection, Collection::Unsupported)
                && page.watch_task.is_none()
                && page.supported(cx)
            {
                page.watch(cx);
            }
            page.check_sources(cx);
            cx.notify();
        });
        let mut page = Self {
            state,
            model: WidgetsModel::new(),
            list: ListState::new(0, ListAlignment::Top, px(400.0)),
            items: Vec::new(),
            frames: HashMap::new(),
            source_missing: HashSet::new(),
            source_present: HashSet::new(),
            source_checked: HashSet::new(),
            source_tasks: Vec::new(),
            menu: None,
            menu_focus: cx.focus_handle(),
            menu_focus_pending: false,
            rename: None,
            recipe: None,
            recipe_select: widgets::SelectState::default(),
            refresh_pending: HashSet::new(),
            refresh_tasks: HashMap::new(),
            delete_confirm: None,
            section_focus: HashMap::new(),
            trigger_focus: HashMap::new(),
            heading_focus: cx.focus_handle(),
            pending_focus: None,
            space_select: widgets::SelectState::default(),
            show_skeleton: false,
            visible: false,
            engine_key: None,
            skeleton_task: None,
            watch_task: None,
            write_task: None,
            _state: state_sub,
        };
        page.engine_key = page.current_engine_key(cx);
        page
    }

    fn current_engine_key(&self, cx: &App) -> Option<(String, WorkspaceScope)> {
        let info = self.state.read(cx).engine()?.engine_info();
        Some((info.device_id.clone(), info.workspace_scope))
    }

    /// Show `target` (the route's space). Same target: nothing reloads.
    pub fn open(&mut self, target: Option<WidgetsTarget>, cx: &mut Context<Self>) {
        let was_visible = std::mem::replace(&mut self.visible, true);
        self.engine_key = self.current_engine_key(cx);
        if self.model.retarget(target) || !was_visible {
            self.release(cx);
            self.pending_focus = Some(None);
            self.watch(cx);
        }
        cx.notify();
    }

    /// The route was left: let go of every renderer, read and watch.
    pub fn hide(&mut self, cx: &mut Context<Self>) {
        self.visible = false;
        self.reset(cx);
    }

    fn reset(&mut self, cx: &mut Context<Self>) {
        self.model.retarget(None);
        self.release(cx);
    }

    fn release(&mut self, cx: &mut Context<Self>) {
        self.frames.clear();
        self.source_missing.clear();
        self.source_present.clear();
        self.source_checked.clear();
        self.source_tasks.clear();
        self.menu = None;
        self.rename = None;
        self.recipe = None;
        // Leaving the page never cancels a refresh: the owner runs it.
        self.refresh_pending.clear();
        self.refresh_tasks.clear();
        self.delete_confirm = None;
        self.watch_task = None;
        self.write_task = None;
        self.skeleton_task = None;
        self.show_skeleton = false;
        self.sync_items(cx);
    }

    /// The owner declares project widgets. Undeclared is unsupported: never
    /// offer what an old engine would refuse.
    fn supported(&self, cx: &App) -> bool {
        let Some(target) = &self.model.target else {
            return false;
        };
        self.state
            .read(cx)
            .device_supports(&target.device_id, capabilities::PROJECT_WIDGETS_V1)
    }

    /// Subscribe to the space's widgets, reconnecting with backoff. Each
    /// attempt reads the current engine, so a reconnected one is used.
    fn watch(&mut self, cx: &mut Context<Self>) {
        let generation = self.model.generation;
        let Some(target) = self.model.target.clone() else {
            self.watch_task = None;
            return;
        };
        if !self.supported(cx) {
            self.watch_task = None;
            self.model.unsupported(generation);
            self.sync_items(cx);
            return;
        }
        if self.model.snapshot().is_none() {
            self.show_skeleton = false;
            self.skeleton_task = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(SKELETON_DELAY).await;
                this.update(cx, |page, cx| {
                    page.show_skeleton = true;
                    cx.notify();
                })
                .ok();
            }));
        }
        let params = serde_json::to_value(WatchProjectWidgetsParams {
            space_id: target.space_id.clone(),
            target_device_id: route_to(self.state.read(cx), &target.device_id),
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
                        .subscribe_checked(methods::WATCH_PROJECT_WIDGETS, params.clone())
                        .await
                    {
                        Ok(mut stream) => {
                            while let Some(value) = stream.recv().await {
                                retry = 1;
                                let parsed =
                                    serde_json::from_value::<ProjectWidgetsSnapshot>(value);
                                let applied = this.update(cx, |page, cx| {
                                    match parsed {
                                        Ok(snapshot) => {
                                            if page.model.watched(generation, snapshot) {
                                                page.synced(cx);
                                            }
                                        }
                                        Err(error) => {
                                            page.model.watch_lost(generation, error.to_string());
                                            page.sync_items(cx);
                                        }
                                    }
                                    cx.notify();
                                });
                                if applied.is_err() {
                                    return;
                                }
                            }
                            "The connection to the project's device closed.".to_owned()
                        }
                        Err(RpcError::UnknownMethod(_)) => {
                            this.update(cx, |page, cx| {
                                page.model.unsupported(generation);
                                page.sync_items(cx);
                                cx.notify();
                            })
                            .ok();
                            return;
                        }
                        Err(error) => error_copy(&error),
                    },
                };
                if this
                    .update(cx, |page, cx| {
                        page.model.watch_lost(generation, failure);
                        page.sync_items(cx);
                        cx.notify();
                    })
                    .is_err()
                {
                    return;
                }
                cx.background_executor()
                    .timer(Duration::from_secs(retry))
                    .await;
                retry = (retry * 2).min(15);
            }
        }));
    }

    /// Load the current state again (after a conflict, or to retry).
    fn reload(&mut self, cx: &mut Context<Self>) {
        self.model.problems.clear();
        self.watch(cx);
        cx.notify();
    }

    /// A new snapshot is shown: renderers follow content revisions.
    fn synced(&mut self, cx: &mut Context<Self>) {
        self.skeleton_task = None;
        self.show_skeleton = false;
        let widgets = self.model.widgets().to_vec();
        self.frames.retain(|id, frame| {
            widgets
                .iter()
                .any(|w| &w.id == id && w.content_revision == frame.content_revision)
        });
        for widget in &widgets {
            if !self.frames.contains_key(&widget.id) {
                self.load_frame(widget, cx);
            }
        }
        if let Some(menu) = &self.menu
            && self.model.widget(&menu.widget_id).is_none()
        {
            self.menu = None;
        }
        self.sync_items(cx);
        self.check_sources(cx);
    }

    /// (Re)create a widget's renderer; the previous one releases its images
    /// and tasks as it drops.
    fn load_frame(&mut self, widget: &ProjectWidget, cx: &mut Context<Self>) {
        let Some(target) = &self.model.target else {
            return;
        };
        let (engine, routed) = {
            let state = self.state.read(cx);
            (state.engine().cloned(), route_to(state, &target.device_id))
        };
        let source = VisualSource::Widget {
            widget_id: widget.id.clone(),
            content_revision: widget.content_revision,
            content: widget.content.clone(),
        };
        let id = SharedString::from(format!("widget:{}", widget.id));
        let title = widget.title.clone();
        let frame = cx.new(|cx| VisualFrame::new(id, title, source, engine, routed, cx));
        self.insert_frame(widget, frame, cx);
    }

    fn insert_frame(
        &mut self,
        widget: &ProjectWidget,
        frame: Entity<VisualFrame>,
        cx: &mut Context<Self>,
    ) {
        let list = self.list.clone();
        let subscriptions = [
            cx.subscribe(&frame, move |_, _, _: &VisualLayoutChanged, cx| {
                list.remeasure();
                cx.notify();
            }),
            // Loaded, failed: the section's own controls follow.
            cx.observe(&frame, |page, _, cx| {
                page.list.remeasure();
                cx.notify();
            }),
        ];
        self.frames.insert(
            widget.id.clone(),
            WidgetFrame {
                content_revision: widget.content_revision,
                frame,
                _subscriptions: subscriptions,
            },
        );
    }

    fn reload_frame(&mut self, widget_id: &str, cx: &mut Context<Self>) {
        self.frames.remove(widget_id);
        if let Some(widget) = self.model.widget(widget_id).cloned() {
            self.load_frame(&widget, cx);
        }
        cx.notify();
    }

    /// Keep the list's rows in step with what is shown, holding the scroll
    /// position and each widget's focus handle.
    fn sync_items(&mut self, cx: &mut Context<Self>) {
        let mut items = vec![Item::Header];
        match self.model.widgets() {
            [] => items.push(Item::Body),
            widgets => items.extend(widgets.iter().map(|w| Item::Widget(w.id.clone()))),
        }
        if items == self.items {
            self.list.remeasure();
            return;
        }
        let handles: Vec<Option<FocusHandle>> = items
            .iter()
            .map(|item| match item {
                Item::Widget(id) => Some(self.section_handle(id, cx)),
                _ => None,
            })
            .collect();
        let top = self.list.logical_scroll_top();
        self.list.splice_focusable(0..self.items.len(), handles);
        self.list.scroll_to(ListOffset {
            item_ix: top.item_ix.min(items.len().saturating_sub(1)),
            offset_in_item: top.offset_in_item,
        });
        let live: HashSet<&str> = items
            .iter()
            .filter_map(|item| match item {
                Item::Widget(id) => Some(id.as_str()),
                _ => None,
            })
            .collect();
        self.section_focus
            .retain(|id, _| live.contains(id.as_str()));
        self.trigger_focus
            .retain(|id, _| live.contains(id.as_str()));
        self.items = items;
    }

    fn section_handle(&mut self, id: &str, cx: &mut Context<Self>) -> FocusHandle {
        self.section_focus
            .entry(id.to_owned())
            .or_insert_with(|| cx.focus_handle())
            .clone()
    }

    fn trigger_handle(&mut self, id: &str, cx: &mut Context<Self>) -> FocusHandle {
        self.trigger_focus
            .entry(id.to_owned())
            .or_insert_with(|| cx.focus_handle())
            .clone()
    }

    /// Ask the owner about source chats this device doesn't know, once per
    /// widget, so a deleted one reads as deleted rather than silently absent.
    fn check_sources(&mut self, cx: &mut Context<Self>) {
        if !self.model.live {
            return;
        }
        let Some(target) = self.model.target.clone() else {
            return;
        };
        let fresh: Vec<&ProjectWidget> = self
            .model
            .widgets()
            .iter()
            .filter(|w| !self.source_checked.contains(&w.id))
            .collect();
        if fresh.is_empty() {
            return;
        }
        let (engine, routed, unknown) = {
            let state = self.state.read(cx);
            let unknown: Vec<String> = fresh
                .iter()
                .filter(|w| {
                    w.source.as_ref().is_some_and(|source| {
                        !state
                            .chats
                            .iter()
                            .any(|chat| chat.id == source.source_chat_id)
                    })
                })
                .map(|w| w.id.clone())
                .collect();
            (
                state.engine().cloned(),
                route_to(state, &target.device_id),
                unknown,
            )
        };
        let fresh: Vec<String> = fresh.into_iter().map(|w| w.id.clone()).collect();
        self.source_checked.extend(fresh);
        let Some(engine) = engine else {
            return;
        };
        let generation = self.model.generation;
        for widget_id in unknown {
            let params = serde_json::to_value(GetProjectWidgetParams {
                widget_id,
                target_device_id: routed.clone(),
            })
            .unwrap_or_default();
            let engine = engine.clone();
            self.source_tasks.push(cx.spawn(async move |this, cx| {
                let Ok(detail) = engine
                    .client()
                    .call_as::<ProjectWidgetDetail>(methods::GET_PROJECT_WIDGET, params)
                    .await
                else {
                    return;
                };
                this.update(cx, |page, cx| {
                    if page.model.generation != generation {
                        return;
                    }
                    let id = detail.widget.id;
                    if detail.source_missing {
                        page.source_missing.insert(id);
                    } else {
                        page.source_present.insert(id);
                    }
                    page.list.remeasure();
                    cx.notify();
                })
                .ok();
            }));
        }
    }

    fn mutate(&mut self, widget_id: &str, kind: MutationKind, cx: &mut Context<Self>) {
        let Some(engine) = self.state.read(cx).engine().cloned() else {
            return;
        };
        let rename = matches!(kind, MutationKind::Rename(_));
        let recipe = matches!(kind, MutationKind::Recipe(_));
        let (mutation, request) = match self.model.begin(widget_id, kind) {
            Ok(Some(begun)) => begun,
            Ok(None) => {
                if rename {
                    self.close_rename(cx);
                }
                if recipe && self.recipe.take().is_some() {
                    // The same recipe is already saved.
                    self.start_refresh(widget_id, cx);
                }
                return;
            }
            Err(message) => {
                if rename && let Some(dialog) = self.rename.as_mut() {
                    dialog.error = Some(message.into());
                } else if recipe && let Some(dialog) = self.recipe.as_mut() {
                    dialog.error = Some(message.into());
                } else {
                    self.model
                        .problems
                        .insert(widget_id.to_owned(), message.into());
                }
                cx.notify();
                return;
            }
        };
        let routed = self
            .model
            .target
            .as_ref()
            .and_then(|target| route_to(self.state.read(cx), &target.device_id));
        let (method, params) = match request {
            Request::Update(mut params) => {
                params.target_device_id = routed;
                (methods::UPDATE_PROJECT_WIDGET, serde_json::to_value(params))
            }
            Request::Reorder(mut params) => {
                params.target_device_id = routed;
                (
                    methods::REORDER_PROJECT_WIDGETS,
                    serde_json::to_value(params),
                )
            }
            Request::Delete(mut params) => {
                params.target_device_id = routed;
                (methods::DELETE_PROJECT_WIDGET, serde_json::to_value(params))
            }
        };
        let params = params.unwrap_or_default();
        self.write_task = Some(cx.spawn(async move |this, cx| {
            let client = engine.client();
            let result = match method {
                methods::UPDATE_PROJECT_WIDGET => client
                    .call_as::<ProjectWidgetWrite>(method, params)
                    .await
                    .map(|write| Applied::Widget(write.widget)),
                methods::REORDER_PROJECT_WIDGETS => client
                    .call_as::<ProjectWidgetsSnapshot>(method, params)
                    .await
                    .map(Applied::Collection),
                _ => client
                    .call_as::<ProjectWidgetDeletion>(method, params)
                    .await
                    .map(|_| Applied::Deleted),
            };
            this.update(cx, |page, cx| page.settle(mutation, result, cx))
                .ok();
        }));
        cx.notify();
    }

    fn settle(
        &mut self,
        mutation: Mutation,
        result: Result<Applied, RpcError>,
        cx: &mut Context<Self>,
    ) {
        let rename = matches!(mutation.kind, MutationKind::Rename(_));
        let recipe = matches!(mutation.kind, MutationKind::Recipe(_));
        let id = mutation.widget_id.clone();
        let outcome = self.model.finish(mutation, result);
        if !outcome.settled {
            return;
        }
        if rename && let Some(dialog) = self.rename.as_mut().filter(|d| d.widget_id == id) {
            match self.model.problems.remove(&id) {
                // Refused: the dialog keeps the text and says why.
                Some(problem) => {
                    dialog.error = Some(problem);
                    cx.notify();
                    return;
                }
                None => self.rename = None,
            }
        }
        if recipe && let Some(dialog) = self.recipe.as_mut().filter(|d| d.widget_id == id) {
            match self.model.problems.remove(&id) {
                Some(problem) => {
                    dialog.error = Some(problem);
                    cx.notify();
                    return;
                }
                None => {
                    // Saved: run it against the revision it produced.
                    self.recipe = None;
                    self.start_refresh(&id, cx);
                }
            }
        }
        self.frames.retain(|id, _| self.model.widget(id).is_some());
        self.pending_focus = Some(outcome.focus);
        self.sync_items(cx);
        cx.notify();
    }

    // ---- refresh ----

    /// The owner runs refreshes (`project-widget-refresh-v1`).
    fn refresh_supported(&self, cx: &App) -> bool {
        self.model.target.as_ref().is_some_and(|target| {
            self.state
                .read(cx)
                .device_supports(&target.device_id, capabilities::PROJECT_WIDGET_REFRESH_V1)
        })
    }

    /// Refresh: ask for a recipe first if there is none.
    fn request_refresh(&mut self, widget_id: &str, cx: &mut Context<Self>) {
        match self.model.widget(widget_id) {
            Some(widget) if widget.refresh_recipe.is_none() => self.open_recipe(widget_id, cx),
            Some(_) => self.start_refresh(widget_id, cx),
            None => {}
        }
    }

    /// One click, one request id. The owner joins an attempt that is
    /// already running instead of starting another.
    fn start_refresh(&mut self, widget_id: &str, cx: &mut Context<Self>) {
        let Some(widget) = self.model.widget(widget_id).cloned() else {
            return;
        };
        let params = RefreshProjectWidgetParams {
            widget_id: widget.id,
            expected_revision: widget.revision,
            request_id: uuid::Uuid::new_v4().to_string(),
            target_device_id: None,
        };
        self.send_refresh(
            widget_id,
            methods::REFRESH_PROJECT_WIDGET,
            serde_json::to_value(params),
            cx,
        );
    }

    fn cancel_refresh(&mut self, widget_id: &str, refresh_id: &str, cx: &mut Context<Self>) {
        let params = CancelProjectWidgetRefreshParams {
            widget_id: widget_id.to_owned(),
            refresh_id: refresh_id.to_owned(),
            request_id: uuid::Uuid::new_v4().to_string(),
            target_device_id: None,
        };
        self.send_refresh(
            widget_id,
            methods::CANCEL_PROJECT_WIDGET_REFRESH,
            serde_json::to_value(params),
            cx,
        );
    }

    fn send_refresh(
        &mut self,
        widget_id: &str,
        method: &'static str,
        params: serde_json::Result<serde_json::Value>,
        cx: &mut Context<Self>,
    ) {
        let Some(engine) = self.state.read(cx).engine().cloned() else {
            return;
        };
        if !self.model.live || !self.refresh_pending.insert(widget_id.to_owned()) {
            return;
        }
        let mut params = params.unwrap_or_default();
        if let Some(device) = self
            .model
            .target
            .as_ref()
            .and_then(|target| route_to(self.state.read(cx), &target.device_id))
        {
            params["targetDeviceId"] = device.into();
        }
        self.model.problems.remove(widget_id);
        let generation = self.model.generation;
        let id = widget_id.to_owned();
        let task = cx.spawn(async move |this, cx| {
            let client = engine.client();
            let result = if method == methods::REFRESH_PROJECT_WIDGET {
                client
                    .call_as::<ProjectWidgetRefreshAdmission>(method, params)
                    .await
                    .map(|admission| admission.refresh)
            } else {
                client
                    .call_as::<ProjectWidgetRefreshCancellation>(method, params)
                    .await
                    .map(|cancellation| cancellation.refresh)
            };
            this.update(cx, |page, cx| {
                page.refresh_pending.remove(&id);
                match result {
                    Ok(refresh) => page.model.refresh_settled(generation, &id, refresh),
                    Err(error) if generation == page.model.generation => {
                        page.model.problems.insert(id, error_copy(&error).into());
                    }
                    Err(_) => {}
                }
                cx.notify();
            })
            .ok();
        });
        self.refresh_tasks.insert(widget_id.to_owned(), task);
        cx.notify();
    }

    fn open_recipe(&mut self, widget_id: &str, cx: &mut Context<Self>) {
        let (Some(engine), Some(target)) = (
            self.state.read(cx).engine().cloned(),
            self.model.target.clone(),
        ) else {
            return;
        };
        let routed = route_to(self.state.read(cx), &target.device_id);
        let instruction = cx.new(|cx| {
            ComposerInput::new(
                "What should the agent refresh? For example: recompute these metrics for the last 30 days.",
                cx,
            )
        });
        let model = cx.new(|cx| ComposerInput::new("Model (optional)", cx).with_single_line());
        let load = cx.spawn(async move |this, cx| {
            let params = match routed {
                Some(device) => serde_json::json!({ "targetDeviceId": device }),
                None => serde_json::json!({}),
            };
            let listed = engine
                .client()
                .call_as::<Vec<zeron_engine::HarnessDescriptor>>(methods::LIST_HARNESSES, params)
                .await;
            this.update(cx, |page, cx| {
                let Some(dialog) = page.recipe.as_mut() else {
                    return;
                };
                match listed {
                    Ok(listed) => {
                        let agents: Vec<(HarnessId, String)> = listed
                            .into_iter()
                            .filter(|d| {
                                d.installed && zeron_engine::registry::descriptor_enabled(d)
                            })
                            .map(|d| (d.id, d.name))
                            .collect();
                        if agents.is_empty() {
                            dialog.error = Some("No agent is enabled on this device.".into());
                        }
                        dialog.harnesses = Some(agents);
                    }
                    Err(error) => {
                        dialog.harnesses = Some(Vec::new());
                        dialog.error = Some(error_copy(&error).into());
                    }
                }
                cx.notify();
            })
            .ok();
        });
        self.recipe = Some(RecipeDialog {
            widget_id: widget_id.to_owned(),
            instruction,
            model,
            harnesses: None,
            harness: 0,
            error: None,
            focus_pending: true,
            _load: load,
        });
        cx.notify();
    }

    fn close_recipe(&mut self, cx: &mut Context<Self>) {
        if let Some(dialog) = self.recipe.take() {
            self.pending_focus = Some(Some(dialog.widget_id));
        }
        cx.notify();
    }

    fn save_recipe(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = &self.recipe else {
            return;
        };
        let Some(harness) = dialog
            .harnesses
            .as_ref()
            .and_then(|agents| agents.get(dialog.harness))
            .map(|(id, _)| *id)
        else {
            return;
        };
        let model = dialog.model.read(cx).text().trim().to_owned();
        let recipe = ProjectWidgetRecipe {
            instruction: dialog.instruction.read(cx).text().to_owned(),
            harness,
            model: (!model.is_empty()).then_some(model),
            reasoning: None,
        };
        let id = dialog.widget_id.clone();
        self.mutate(&id, MutationKind::Recipe(recipe), cx);
    }

    // ---- menus and dialogs ----

    fn open_menu(&mut self, widget_id: &str, keyboard: bool, cx: &mut Context<Self>) {
        self.menu = Some(RowMenu {
            widget_id: widget_id.to_owned(),
            active: keyboard.then_some(0),
        });
        self.menu_focus_pending = keyboard;
        cx.notify();
    }

    /// Close the row menu; focus returns to its trigger.
    fn close_menu(&mut self, cx: &mut Context<Self>) {
        if let Some(menu) = self.menu.take() {
            self.pending_focus = Some(Some(menu.widget_id));
        }
        cx.notify();
    }

    /// The menu's actions, in order, and whether each can run now.
    fn menu_actions(&self, widget_id: &str) -> [(&'static str, bool); 4] {
        let widgets = self.model.widgets();
        let ix = widgets.iter().position(|w| w.id == widget_id);
        let writable = self.model.writable();
        [
            ("Rename…", writable),
            ("Move up", writable && ix.is_some_and(|ix| ix > 0)),
            (
                "Move down",
                writable && ix.is_some_and(|ix| ix + 1 < widgets.len()),
            ),
            ("Delete…", writable),
        ]
    }

    fn activate_menu(&mut self, action: usize, cx: &mut Context<Self>) {
        let Some(menu) = self.menu.take() else {
            return;
        };
        let id = menu.widget_id;
        if !self
            .menu_actions(&id)
            .get(action)
            .is_some_and(|(_, on)| *on)
        {
            self.menu = None;
            self.pending_focus = Some(Some(id));
            cx.notify();
            return;
        }
        match action {
            0 => self.open_rename(&id, cx),
            1 => self.mutate(&id, MutationKind::Move(-1), cx),
            2 => self.mutate(&id, MutationKind::Move(1), cx),
            _ => {
                self.delete_confirm = Some(id);
                cx.notify();
            }
        }
    }

    fn open_rename(&mut self, widget_id: &str, cx: &mut Context<Self>) {
        let Some(title) = self.model.widget(widget_id).map(|w| w.title.clone()) else {
            return;
        };
        let input = cx.new(|cx| ComposerInput::new("Widget title", cx).with_single_line());
        input.update(cx, |input, cx| input.set_text(title, cx));
        let id = widget_id.to_owned();
        let events = cx.subscribe(&input, move |page: &mut Self, input, event, cx| {
            if matches!(event, ComposerInputEvent::Submitted) {
                let title = input.read(cx).text().to_owned();
                page.mutate(&id, MutationKind::Rename(title), cx);
            }
        });
        self.rename = Some(RenameDialog {
            widget_id: widget_id.to_owned(),
            input,
            error: None,
            focus_pending: true,
            _events: events,
        });
        cx.notify();
    }

    fn close_rename(&mut self, cx: &mut Context<Self>) {
        if let Some(dialog) = self.rename.take() {
            self.pending_focus = Some(Some(dialog.widget_id));
        }
        cx.notify();
    }

    fn close_delete(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.delete_confirm.take() {
            self.pending_focus = Some(Some(id));
        }
        cx.notify();
    }

    fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.delete_confirm.take() {
            self.mutate(&id, MutationKind::Delete, cx);
        }
    }

    /// Escape not claimed by a control: close the open menu or dialog.
    fn on_escape(&mut self, cx: &mut Context<Self>) -> bool {
        if self.rename.is_some() {
            self.close_rename(cx);
        } else if self.recipe.is_some() {
            self.close_recipe(cx);
        } else if self.delete_confirm.is_some() {
            self.close_delete(cx);
        } else if self.menu.is_some() {
            self.close_menu(cx);
        } else {
            return false;
        }
        true
    }

    fn apply_pending_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self.pending_focus.take() else {
            return;
        };
        match target.and_then(|id| {
            let ix = self
                .items
                .iter()
                .position(|item| item == &Item::Widget(id.clone()))?;
            Some((ix, id))
        }) {
            Some((ix, id)) => {
                self.list.scroll_to_reveal_item(ix);
                let handle = self.trigger_handle(&id, cx);
                window.focus(&handle, cx);
            }
            None => window.focus(&self.heading_focus, cx),
        }
    }

    // ---- rendering ----

    fn target_names(&self, cx: &App) -> (String, String, String) {
        let state = self.state.read(cx);
        let Some(target) = &self.model.target else {
            return ("Widgets".into(), String::new(), "this device".into());
        };
        let device = device_label(state, &target.device_id);
        match state.space_row(&target.space_id) {
            Some(space) => (space.display_name().to_owned(), space.path.clone(), device),
            None => ("Project".into(), String::new(), device),
        }
    }

    fn render_item(
        &mut self,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let narrow = window.viewport_size().width < px(640.0);
        let content = match self.items.get(ix).cloned() {
            Some(Item::Header) => self.render_header(&theme, cx),
            Some(Item::Body) => self.render_body(&theme, cx),
            Some(Item::Widget(id)) => self.render_widget(&id, &theme, cx),
            None => gpui::Empty.into_any_element(),
        };
        div()
            .w_full()
            .child(
                div()
                    .w_full()
                    .max_w(px(PAGE_MAX_WIDTH))
                    .mx_auto()
                    .px(px(if narrow { 16.0 } else { 40.0 }))
                    .child(content),
            )
            .into_any_element()
    }

    fn render_header(&mut self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let (name, path, device) = self.target_names(cx);
        let count = self.model.snapshot().map(|s| s.widgets.len());
        let switcher = self.render_space_switcher(theme, cx);
        let offline = !self.model.live && self.model.snapshot().is_some();
        let place = if path.is_empty() {
            format!("On {device}")
        } else {
            format!("{path} on {device}")
        };
        div()
            .pt(px(Theme::TITLEBAR_HEIGHT + 16.0))
            .pb(px(16.0))
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        div()
                            .id("project-widgets-heading")
                            .role(gpui::Role::Heading)
                            .aria_label(format!("Widgets of {name}"))
                            .track_focus(&self.heading_focus)
                            .min_w_0()
                            .child(widgets::page_header(theme, "Widgets", count)),
                    )
                    .children(switcher),
            )
            .child(widgets::page_subtitle(theme, format!("{name} · {place}")))
            .when(offline, |el| {
                el.child(
                    div().id("project-widgets-offline").role(gpui::Role::Status).child(
                        widgets::warning_strip(
                            theme,
                            format!(
                                "Offline: {device} can't be reached. These widgets are as last loaded; changes are paused until it reconnects."
                            ),
                        ),
                    ),
                )
            })
            .into_any_element()
    }

    /// Several spaces of one repository: each named by device and folder.
    fn render_space_switcher(
        &mut self,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let target = self.model.target.clone()?;
        let choices: Vec<SpaceChoice> = {
            let state = self.state.read(cx);
            let space = state.space_row(&target.space_id)?;
            state
                .project_members(space)
                .into_iter()
                .map(|space| SpaceChoice::of(state, space))
                .collect()
        };
        if choices.len() < 2 {
            return None;
        }
        let selected = choices
            .iter()
            .position(|choice| choice.target == target)
            .unwrap_or(0);
        let options = choices
            .iter()
            .map(|choice| widgets::SelectOption::new(choice.place.clone()))
            .collect::<Vec<_>>();
        let targets: Vec<WidgetsTarget> = choices.into_iter().map(|c| c.target).collect();
        Some(
            widgets::select(
                "project-widgets-space",
                "Project space",
                theme,
                |page: &mut Self| &mut page.space_select,
            )
            .options(options, selected)
            .menu_width(360.0)
            .heading("Spaces of this project")
            .on_select(move |_, ix, _, cx| {
                if let Some(target) = targets.get(ix) {
                    cx.emit(ProjectWidgetsEvent::OpenSpace(target.clone()));
                }
            })
            .render(&self.space_select, cx)
            .into_any_element(),
        )
    }

    fn render_body(&mut self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let (_, _, device) = self.target_names(cx);
        let card = |title: String, copy: String| {
            widgets::section_card(theme).child(
                widgets::card_row(theme, true).child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(widgets::row_title(theme, title))
                        .child(widgets::meta_line(
                            theme,
                            vec![div().child(SharedString::from(copy)).into_any_element()],
                        )),
                ),
            )
        };
        match self.model.collection.clone() {
            // Quiet at first; a skeleton only for a load that takes a while.
            Collection::Loading => div()
                .id("project-widgets-loading")
                .role(gpui::Role::Status)
                .aria_label("Loading widgets")
                .min_h(px(120.0))
                .when(self.show_skeleton, |el| {
                    el.child(popover::skeleton_rows(
                        "project-widgets-skeleton",
                        theme,
                        4,
                        cx.entity_id(),
                        cx,
                    ))
                })
                .into_any_element(),
            Collection::Unsupported => card(
                "Widgets aren't available here".into(),
                format!("Update Zeron on {device} to keep visuals in its projects."),
            )
            .into_any_element(),
            Collection::Unavailable(message) => div()
                .flex()
                .flex_col()
                .child(card(format!("Can't reach {device}"), message))
                .child(
                    div().mt(px(8.0)).child(keyboard_button(
                        widgets::ghost_action(theme)
                            .id("project-widgets-retry")
                            .child("Try again"),
                        theme,
                        cx,
                        |page, _, cx| page.reload(cx),
                    )),
                )
                .into_any_element(),
            Collection::Ready(_) => {
                let space_id = self.model.target.as_ref().map(|t| t.space_id.clone());
                widgets::section_card(theme)
                    .child(
                        widgets::card_row(theme, true)
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(200.0))
                                    .child(widgets::row_title(theme, "No widgets in this project"))
                                    .child(widgets::meta_line(
                                        theme,
                                        vec![
                                            div()
                                                .child(
                                                    "Save a visual from one of its chats, or ask an agent for a dashboard.",
                                                )
                                                .into_any_element(),
                                        ],
                                    )),
                            )
                            .when_some(space_id, |row, space_id| {
                                row.child(keyboard_button(
                                    widgets::text_action(theme, ActionTone::Outlined, "New dashboard chat")
                                        .id("project-widgets-new-dashboard"),
                                    theme,
                                    cx,
                                    move |_, _, cx| {
                                        cx.emit(ProjectWidgetsEvent::NewDashboardChat {
                                            space_id: space_id.clone(),
                                            request: DASHBOARD_REQUEST.into(),
                                        })
                                    },
                                ))
                            }),
                    )
                    .into_any_element()
            }
        }
    }

    fn render_widget(&mut self, id: &str, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let Some(widget) = self.model.widget(id).cloned() else {
            return gpui::Empty.into_any_element();
        };
        let section = self.section_handle(id, cx);
        let trigger = self.trigger_handle(id, cx);
        let frame = self.frames.get(id).map(|f| f.frame.clone());
        let failure = frame.as_ref().and_then(|frame| frame.read(cx).failure());
        let problem = self.model.problems.get(id).cloned();
        let source = widget.source.clone();
        let source_known = source.as_ref().is_some_and(|source| {
            self.state
                .read(cx)
                .chats
                .iter()
                .any(|chat| chat.id == source.source_chat_id)
        });
        let source_missing = self.source_missing.contains(id);
        let updated = widget.last_successful_at.unwrap_or(widget.updated_at);
        let updated = crate::transcript::format_timestamp(updated, &chrono::Local);
        let menu_open = self.menu.as_ref().is_some_and(|m| m.widget_id == id);
        let title = widget.title.clone();

        let refresh = self.render_refresh(&widget, theme, cx);

        let source_control: Option<AnyElement> = match source {
            Some(_) if source_missing => Some(
                div()
                    .id(SharedString::from(format!("widget-source-{id}")))
                    .role(gpui::Role::Status)
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .px(px(10.0))
                    .text_size(crate::typography::ui_rems(12.5))
                    .text_color(theme.text_muted)
                    .child(
                        icon(icons::INFO_CIRCLE)
                            .size(px(14.0))
                            .text_color(theme.text_muted),
                    )
                    .child("Source chat deleted")
                    .into_any_element(),
            ),
            Some(source) if source_known || self.source_present.contains(id) => {
                let chat_id = source.source_chat_id.clone();
                Some(
                    keyboard_button(
                        widgets::ghost_action(theme)
                            .id(SharedString::from(format!("widget-source-{id}")))
                            .aria_label(format!("Open the chat {title} came from"))
                            .child(
                                icon(icons::CHAT_ROUND_LINE)
                                    .size(px(14.0))
                                    .text_color(theme.text_muted),
                            )
                            .child("Open source chat"),
                        theme,
                        cx,
                        move |_, _, cx| cx.emit(ProjectWidgetsEvent::OpenChat(chat_id.clone())),
                    )
                    .into_any_element(),
                )
            }
            _ => None,
        };

        let menu_id = id.to_owned();
        let key_id = id.to_owned();
        let mut actions_trigger = div()
            .id(SharedString::from(format!("widget-actions-{id}")))
            .relative()
            .size(px(32.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(8.0))
            .cursor_pointer()
            .hover(|s| s.bg(theme.glass_hover()))
            .role(gpui::Role::Button)
            .aria_label(format!("Actions for {title}"))
            .aria_expanded(menu_open)
            .track_focus(&trigger.tab_stop(true))
            .tab_index(0)
            .focus_visible(|s| s.border_2().border_color(theme.accent))
            .on_click(cx.listener(move |page, event: &gpui::ClickEvent, _, cx| {
                if page.menu.as_ref().is_some_and(|m| m.widget_id == menu_id) {
                    page.close_menu(cx);
                } else {
                    page.open_menu(&menu_id, event.is_keyboard(), cx);
                }
            }))
            .on_key_down(
                cx.listener(move |page, event: &gpui::KeyDownEvent, window, cx| {
                    if event.is_held {
                        return;
                    }
                    match event.keystroke.key.as_str() {
                        // Move with the keyboard without opening the menu.
                        "up" | "down" if event.keystroke.modifiers.alt => {
                            let delta = if event.keystroke.key == "up" { -1 } else { 1 };
                            page.mutate(&key_id, MutationKind::Move(delta), cx);
                            cx.stop_propagation();
                        }
                        "down" if page.menu.is_some() => {
                            window.focus(&page.menu_focus, cx);
                            if let Some(menu) = page.menu.as_mut() {
                                menu.active = Some(0);
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }
                        _ => {}
                    }
                }),
            )
            .child(
                icon(icons::MORE_HORIZONTAL)
                    .size(px(16.0))
                    .text_color(theme.text_muted),
            );
        if menu_open {
            actions_trigger = actions_trigger.child(self.render_menu(id, theme, cx));
        }

        let status = (!self.model.live).then(|| {
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .child(
                    icon(icons::WIFI_OFF)
                        .size(px(12.0))
                        .text_color(theme.warning),
                )
                .child("Offline · changes paused")
                .into_any_element()
        });
        let meta = widgets::meta_line(
            theme,
            std::iter::once(
                div()
                    .child(SharedString::from(format!("Last updated {updated}")))
                    .into_any_element(),
            )
            .chain(status)
            .collect(),
        );

        let problem_line = problem.map(|problem| {
            div()
                .id(SharedString::from(format!("widget-problem-{id}")))
                .role(gpui::Role::Alert)
                .mt(px(8.0))
                .flex()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap(px(8.0))
                .text_size(crate::typography::ui_rems(12.5))
                .text_color(theme.danger_muted)
                .child(
                    icon(icons::DANGER_TRIANGLE)
                        .size(px(14.0))
                        .text_color(theme.danger_muted),
                )
                .child(problem)
                .child(keyboard_button(
                    widgets::ghost_action(theme)
                        .id(SharedString::from(format!("widget-reload-{id}")))
                        .child("Reload"),
                    theme,
                    cx,
                    |page, _, cx| page.reload(cx),
                ))
        });

        // A widget that couldn't be read or failed to run keeps its place;
        // reloading is a click, never an automatic loop.
        let reload_frame = failure.map(|_| {
            let id = id.to_owned();
            keyboard_button(
                widgets::ghost_action(theme)
                    .id(SharedString::from(format!("widget-retry-{id}")))
                    .aria_label(format!("Reload {title}"))
                    .child(
                        icon(icons::RESTART)
                            .size(px(14.0))
                            .text_color(theme.text_muted),
                    )
                    .child("Reload widget"),
                theme,
                cx,
                move |page, _, cx| page.reload_frame(&id, cx),
            )
        });

        div()
            .id(SharedString::from(format!("widget-{id}")))
            .track_focus(&section)
            .role(gpui::Role::Group)
            .aria_label(title.clone())
            .pb(px(32.0))
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(160.0))
                            .text_size(crate::typography::ui_rems(15.0))
                            .line_height(crate::typography::ui_rems(21.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme.text)
                            .child(SharedString::from(title.clone())),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .flex_wrap()
                            .items_center()
                            .gap(px(4.0))
                            .child(refresh)
                            .children(source_control)
                            .child(actions_trigger),
                    ),
            )
            .child(meta)
            .children(problem_line)
            .child(div().mt(px(12.0)).w_full().children(frame))
            .children(reload_frame.map(|button| div().mt(px(6.0)).child(button)))
            .into_any_element()
    }

    /// The Refresh area: a button, or what the running attempt needs.
    fn render_refresh(
        &mut self,
        widget: &ProjectWidget,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = widget.id.clone();
        let title = widget.title.clone();
        let pending = self.refresh_pending.contains(&id);
        let live = self.model.live;
        let status = |label: &'static str, tooltip: Option<&'static str>| {
            let element = div()
                .id(SharedString::from(format!("widget-refresh-status-{id}")))
                .role(gpui::Role::Status)
                .px(px(6.0))
                .text_size(crate::typography::ui_rems(12.5))
                .text_color(theme.text_muted)
                .child(label);
            match tooltip {
                Some(copy) => element
                    .aria_description(copy)
                    .tooltip(widgets::text_tooltip(copy))
                    .into_any_element(),
                None => element.into_any_element(),
            }
        };
        let button = |label: &'static str, glyph, key: &str| {
            widgets::ghost_action(theme)
                .id(SharedString::from(format!("widget-{key}-{id}")))
                .child(icon(glyph).size(px(14.0)).text_color(theme.text_muted))
                .child(label)
        };
        let open_run = |chat_id: Option<String>, cx: &mut Context<Self>| {
            chat_id.map(|chat_id| {
                keyboard_button(
                    button("Open run", icons::CHAT_ROUND_LINE, "open-run")
                        .aria_label(format!("Open the run refreshing {title}")),
                    theme,
                    cx,
                    move |_, _, cx| cx.emit(ProjectWidgetsEvent::OpenChat(chat_id.clone())),
                )
                .into_any_element()
            })
        };
        let start = |label: &'static str, cx: &mut Context<Self>| {
            let id = id.clone();
            let element =
                button(label, icons::REFRESH, "refresh").aria_label(format!("{label} {title}"));
            if pending || !live {
                element.opacity(0.5).cursor_default().into_any_element()
            } else {
                keyboard_button(element, theme, cx, move |page, _, cx| {
                    page.request_refresh(&id, cx)
                })
                .into_any_element()
            }
        };
        let cancel = |refresh_id: String, cx: &mut Context<Self>| {
            let id = id.clone();
            let element = button("Cancel", icons::STOP, "cancel-refresh")
                .aria_label(format!("Cancel refreshing {title}"));
            if pending || !live {
                element.opacity(0.5).cursor_default().into_any_element()
            } else {
                keyboard_button(element, theme, cx, move |page, _, cx| {
                    page.cancel_refresh(&id, &refresh_id, cx)
                })
                .into_any_element()
            }
        };
        let row = |children: Vec<AnyElement>| {
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap(px(4.0))
                .children(children)
                .into_any_element()
        };
        match refresh_control(widget, self.refresh_supported(cx)) {
            RefreshControl::Unsupported => div()
                .id(SharedString::from(format!("widget-refresh-{id}")))
                .role(gpui::Role::Button)
                .aria_label(format!("Refresh {title}"))
                .aria_description(REFRESH_UNSUPPORTED_COPY)
                .tab_index(0)
                .focus_visible(|s| s.border_2().border_color(theme.accent))
                .child(
                    widgets::action_button(theme, ActionTone::Quiet)
                        .opacity(0.5)
                        .cursor_default()
                        .child(
                            icon(icons::REFRESH)
                                .size(px(14.0))
                                .text_color(theme.text_muted),
                        )
                        .child("Refresh"),
                )
                .tooltip(widgets::text_tooltip(REFRESH_UNSUPPORTED_COPY))
                .into_any_element(),
            RefreshControl::Start { .. } if pending => {
                status("Starting refresh…", Some(REFRESH_RUN_COPY))
            }
            RefreshControl::Start { .. } => start("Refresh", cx),
            RefreshControl::Running {
                refresh_id,
                chat_id,
            } => {
                let mut children = vec![status("Refreshing", Some(REFRESH_RUN_COPY))];
                children.push(cancel(refresh_id, cx));
                children.extend(open_run(chat_id, cx));
                row(children)
            }
            RefreshControl::AwaitingInput {
                refresh_id,
                chat_id,
            } => {
                let mut children = vec![status(
                    "Response needed",
                    Some("The agent asked a question. Open the run to answer it."),
                )];
                children.extend(open_run(chat_id, cx));
                children.push(cancel(refresh_id, cx));
                row(children)
            }
            RefreshControl::Cancelled => row(vec![
                status("Refresh cancelled", None),
                start("Refresh", cx),
            ]),
            RefreshControl::Failed { message, chat_id } => {
                let mut children = vec![
                    div()
                        .id(SharedString::from(format!("widget-refresh-failed-{id}")))
                        .role(gpui::Role::Alert)
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .px(px(6.0))
                        .text_size(crate::typography::ui_rems(12.5))
                        .text_color(theme.danger_muted)
                        .child(
                            icon(icons::DANGER_TRIANGLE)
                                .size(px(14.0))
                                .text_color(theme.danger_muted),
                        )
                        .child(SharedString::from(message))
                        .into_any_element(),
                    start("Retry", cx),
                ];
                children.extend(open_run(chat_id, cx));
                row(children)
            }
        }
    }

    fn render_menu(&mut self, id: &str, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let theme = theme.for_popup();
        let actions = self.menu_actions(id);
        let active = self.menu.as_ref().and_then(|m| m.active);
        let glyphs = [
            icons::PEN,
            icons::ALT_ARROW_UP,
            icons::ALT_ARROW_DOWN,
            icons::TRASH_BIN_MINIMALISTIC,
        ];
        let mut card = popover::popover_card(&theme)
            .id("widget-actions-menu-card")
            .w(px(200.0))
            .flex()
            .flex_col()
            .track_focus(&self.menu_focus)
            .role(gpui::Role::Menu)
            .aria_label("Widget actions")
            .on_key_down(cx.listener(|page, event: &gpui::KeyDownEvent, _, cx| {
                match event.keystroke.key.as_str() {
                    "escape" => page.close_menu(cx),
                    "up" | "down" => {
                        if let Some(menu) = page.menu.as_mut() {
                            menu.active = popover::menu_step(
                                menu.active,
                                4,
                                if event.keystroke.key == "up" { -1 } else { 1 },
                            );
                        }
                    }
                    "enter" | "space" => {
                        if let Some(action) = page.menu.as_ref().and_then(|m| m.active) {
                            page.activate_menu(action, cx);
                        }
                    }
                    _ => return,
                }
                cx.stop_propagation();
                cx.notify();
            }))
            .on_mouse_down_out(cx.listener(|page, _, _, cx| page.close_menu(cx)));
        for (action, ((label, enabled), glyph)) in actions.into_iter().zip(glyphs).enumerate() {
            let danger = action == 3;
            let color = if !enabled {
                theme.text_faint
            } else if danger {
                theme.danger
            } else {
                theme.text_muted
            };
            card = card.child(
                popover::menu_row_nav(
                    &theme,
                    false,
                    active == Some(action),
                    format!("widget-menu-{id}-{action}"),
                )
                .id(("widget-menu-action", action))
                .role(gpui::Role::MenuItem)
                .aria_label(label)
                .when(!enabled, |row| row.opacity(0.5).cursor_default())
                .when(danger && enabled, |row| row.text_color(theme.danger))
                .on_click(cx.listener(move |page, _, _, cx| {
                    cx.stop_propagation();
                    page.activate_menu(action, cx);
                }))
                .child(icon(glyph).size(px(16.0)).text_color(color))
                .child(label),
            );
        }
        popover::anchored_menu_below_end("widget-actions-menu", card.into_any_element(), None)
    }

    fn render_dialogs(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = Theme::of(cx).for_popup();
        let viewport = window.viewport_size();
        let mut overlays = Vec::new();
        if let Some(dialog) = &mut self.rename {
            if std::mem::take(&mut dialog.focus_pending) {
                window.focus(&dialog.input.focus_handle(cx), cx);
            }
            let input = dialog.input.clone();
            let error = dialog.error.clone();
            let id = dialog.widget_id.clone();
            let card = popover::dialog_card(&theme)
                .id("rename-widget-dialog-card")
                .role(gpui::Role::Dialog)
                .aria_label("Rename widget")
                .on_key_down(cx.listener(|page, event: &gpui::KeyDownEvent, _, cx| {
                    if event.keystroke.key == "escape" {
                        page.close_rename(cx);
                        cx.stop_propagation();
                    }
                }))
                .child(popover::dialog_title(&theme, "Rename widget"))
                .child(
                    div()
                        .mt(px(12.0))
                        .child(popover::dialog_field(input.clone().into_any_element())),
                )
                .when_some(error, |el, error| {
                    el.child(
                        div()
                            .id("rename-widget-error")
                            .role(gpui::Role::Alert)
                            .mt(px(8.0))
                            .text_size(crate::typography::ui_rems(12.5))
                            .text_color(theme.danger_muted)
                            .child(error),
                    )
                })
                .child(
                    div()
                        .mt(px(16.0))
                        .flex()
                        .flex_row()
                        .justify_end()
                        .gap(px(8.0))
                        .child(keyboard_button(
                            popover::btn_ghost(&theme, "Cancel", "rename-widget-cancel")
                                .id("rename-widget-cancel"),
                            &theme,
                            cx,
                            |page, _, cx| page.close_rename(cx),
                        ))
                        .child(keyboard_button(
                            popover::btn_primary(&theme, "Rename").id("rename-widget-save"),
                            &theme,
                            cx,
                            move |page, _, cx| {
                                let title = input.read(cx).text().to_owned();
                                page.mutate(&id, MutationKind::Rename(title), cx);
                            },
                        )),
                )
                .into_any_element();
            overlays.push(popover::modal("rename-widget-dialog", viewport, card));
        }
        if let Some(dialog) = &mut self.recipe {
            if std::mem::take(&mut dialog.focus_pending) {
                window.focus(&dialog.instruction.focus_handle(cx), cx);
            }
            let instruction = dialog.instruction.clone();
            let model = dialog.model.clone();
            let error = dialog.error.clone();
            let agents = dialog.harnesses.clone();
            let selected = dialog.harness;
            let ready = agents.as_ref().is_some_and(|agents| !agents.is_empty());
            let agent: AnyElement = match agents {
                None => popover::dialog_body(&theme, "Looking for this device's agents…")
                    .into_any_element(),
                Some(agents) if agents.is_empty() => gpui::Empty.into_any_element(),
                Some(agents) => {
                    widgets::select("widget-recipe-agent", "Agent", &theme, |page: &mut Self| {
                        &mut page.recipe_select
                    })
                    .options(
                        agents
                            .iter()
                            .map(|(_, name)| widgets::SelectOption::new(name.clone())),
                        selected,
                    )
                    .heading("Agent on this device")
                    .on_select(|page, ix, _, cx| {
                        if let Some(dialog) = page.recipe.as_mut() {
                            dialog.harness = ix;
                        }
                        cx.notify();
                    })
                    .render(&self.recipe_select, cx)
                    .into_any_element()
                }
            };
            let save = popover::btn_primary(&theme, "Save and refresh").id("widget-recipe-save");
            let card = popover::dialog_card(&theme)
                .id("widget-recipe-dialog-card")
                .role(gpui::Role::Dialog)
                .aria_label("Set up refresh")
                .on_key_down(cx.listener(|page, event: &gpui::KeyDownEvent, _, cx| {
                    if event.keystroke.key == "escape" {
                        page.close_recipe(cx);
                        cx.stop_propagation();
                    }
                }))
                .child(popover::dialog_title(&theme, "Set up refresh"))
                .child(div().mt(px(6.0)).child(popover::dialog_body(
                    &theme,
                    "Refresh starts an agent run in this project with this instruction. It uses the \
                     agent's own tools and accounts, and the widget changes only if the run succeeds.",
                )))
                .child(
                    div()
                        .mt(px(12.0))
                        .child(popover::dialog_field(instruction.into_any_element())),
                )
                .child(
                    div()
                        .mt(px(8.0))
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .items_center()
                        .gap(px(8.0))
                        .child(agent)
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(160.0))
                                .child(popover::dialog_field(model.into_any_element())),
                        ),
                )
                .when_some(error, |el, error| {
                    el.child(
                        div()
                            .id("widget-recipe-error")
                            .role(gpui::Role::Alert)
                            .mt(px(8.0))
                            .text_size(crate::typography::ui_rems(12.5))
                            .text_color(theme.danger_muted)
                            .child(error),
                    )
                })
                .child(
                    div()
                        .mt(px(16.0))
                        .flex()
                        .flex_row()
                        .justify_end()
                        .gap(px(8.0))
                        .child(keyboard_button(
                            popover::btn_ghost(&theme, "Cancel", "widget-recipe-cancel")
                                .id("widget-recipe-cancel"),
                            &theme,
                            cx,
                            |page, _, cx| page.close_recipe(cx),
                        ))
                        .child(if ready {
                            keyboard_button(save, &theme, cx, |page, _, cx| page.save_recipe(cx))
                                .into_any_element()
                        } else {
                            save.opacity(0.5).cursor_default().into_any_element()
                        }),
                )
                .into_any_element();
            overlays.push(popover::modal("widget-recipe-dialog", viewport, card));
        }
        if let Some(id) = self.delete_confirm.clone() {
            let title = self
                .model
                .widget(&id)
                .map(|w| w.title.clone())
                .unwrap_or_else(|| "this widget".into());
            let card = popover::dialog_card(&theme)
                .id("delete-widget-dialog-card")
                .role(gpui::Role::AlertDialog)
                .aria_label("Delete widget?")
                .on_key_down(cx.listener(|page, event: &gpui::KeyDownEvent, _, cx| {
                    if event.keystroke.key == "escape" {
                        page.close_delete(cx);
                        cx.stop_propagation();
                    }
                }))
                .child(popover::dialog_title(&theme, "Delete widget?"))
                .child(div().mt(px(6.0)).child(popover::dialog_body(
                    &theme,
                    format!(
                        "“{title}” is removed from this project. The chat it came from is kept."
                    ),
                )))
                .child(
                    div()
                        .mt(px(16.0))
                        .flex()
                        .flex_row()
                        .justify_end()
                        .gap(px(8.0))
                        .child(keyboard_button(
                            popover::btn_ghost(&theme, "Cancel", "delete-widget-cancel")
                                .id("delete-widget-cancel"),
                            &theme,
                            cx,
                            |page, _, cx| page.close_delete(cx),
                        ))
                        .child(keyboard_button(
                            popover::btn_danger(&theme, "Delete").id("delete-widget-confirm"),
                            &theme,
                            cx,
                            |page, _, cx| page.confirm_delete(cx),
                        )),
                )
                .into_any_element();
            overlays.push(popover::modal("delete-widget-dialog", viewport, card));
        }
        overlays
    }
}

#[cfg(feature = "project-widgets-fixture")]
impl ProjectWidgetsPage {
    /// Show `snapshot` as a live collection without an engine, rendering
    /// each widget from `scripts` (by widget id); a widget without one shows
    /// its unreadable state.
    pub fn fixture_show(
        &mut self,
        target: WidgetsTarget,
        snapshot: ProjectWidgetsSnapshot,
        scripts: HashMap<String, String>,
        problem: Option<(&str, &str)>,
        cx: &mut Context<Self>,
    ) {
        self.visible = true;
        self.model.retarget(Some(target));
        self.watch_task = None;
        self.model.watched(self.model.generation, snapshot);
        for widget in self.model.widgets().to_vec() {
            let source = match scripts.get(&widget.id) {
                Some(script) => VisualSource::Inline {
                    script: script.clone(),
                    seed: widget.content.render_seed,
                },
                None => VisualSource::Widget {
                    widget_id: widget.id.clone(),
                    content_revision: widget.content_revision,
                    content: widget.content.clone(),
                },
            };
            let id = SharedString::from(format!("widget:{}", widget.id));
            let title = widget.title.clone();
            let frame = cx.new(|cx| VisualFrame::new(id, title, source, None, None, cx));
            self.insert_frame(&widget, frame, cx);
        }
        if let Some((id, problem)) = problem {
            self.model
                .problems
                .insert(id.to_owned(), problem.to_owned().into());
        }
        self.sync_items(cx);
        cx.notify();
    }
}

impl Render for ProjectWidgetsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.apply_pending_focus(window, cx);
        if std::mem::take(&mut self.menu_focus_pending) && self.menu.is_some() {
            window.focus(&self.menu_focus, cx);
        }
        if self.items.is_empty() {
            self.sync_items(cx);
        }
        let overlays = self.render_dialogs(window, cx);
        div()
            .id("project-widgets-page")
            .key_context("ProjectWidgets")
            .size_full()
            .relative()
            .on_key_down(cx.listener(|page, event: &gpui::KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" && page.on_escape(cx) {
                    cx.stop_propagation();
                }
            }))
            .child(
                list(self.list.clone(), cx.processor(Self::render_item))
                    .size_full()
                    .with_sizing_behavior(gpui::ListSizingBehavior::Auto),
            )
            .children(overlays)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zeron_proto::{PROJECT_WIDGET_RENDERER_LUAU_V1, ProjectWidgetSource};

    fn widget(id: &str, revision: u64) -> ProjectWidget {
        ProjectWidget {
            id: id.into(),
            space_id: "space-a".into(),
            owner_device_id: "device-a".into(),
            title: format!("Widget {id}"),
            revision,
            created_at: 0,
            updated_at: 0,
            last_successful_at: Some(0),
            content_revision: 1,
            content: ProjectWidgetContent {
                bundle_id: format!("bundle-{id}"),
                format_version: 1,
                renderer_format: PROJECT_WIDGET_RENDERER_LUAU_V1.into(),
                render_seed: 7,
                script_size: 1,
                script_sha256: "00".into(),
                assets: vec![],
            },
            source: Some(ProjectWidgetSource {
                source_chat_id: "chat".into(),
                source_visual_id: format!("v:{id}"),
                source_entry_id: None,
                source_title: None,
                source_device_id: "device-a".into(),
            }),
            refresh_recipe: None,
            refresh: None,
        }
    }

    fn snapshot(space: &str, revision: u64, widgets: Vec<ProjectWidget>) -> ProjectWidgetsSnapshot {
        ProjectWidgetsSnapshot {
            space_id: space.into(),
            owner_device_id: "device-a".into(),
            collection_revision: revision,
            widgets,
        }
    }

    fn target(space: &str) -> Option<WidgetsTarget> {
        Some(WidgetsTarget {
            space_id: space.into(),
            device_id: "device-a".into(),
        })
    }

    fn ids(model: &WidgetsModel) -> Vec<&str> {
        model.widgets().iter().map(|w| w.id.as_str()).collect()
    }

    #[test]
    fn loading_empty_unavailable_and_unsupported_stay_distinct() {
        let mut model = WidgetsModel::new();
        model.retarget(target("space-a"));
        // Waiting is never "no widgets".
        assert_eq!(model.collection, Collection::Loading);
        model.watch_lost(1, "unreachable".into());
        assert_eq!(
            model.collection,
            Collection::Unavailable("unreachable".into())
        );

        assert!(model.watched(1, snapshot("space-a", 1, vec![])));
        assert!(model.snapshot().is_some_and(|s| s.widgets.is_empty()));
        assert!(model.writable());

        // Losing the owner keeps what is shown and stops writes.
        assert!(model.watched(1, snapshot("space-a", 2, vec![widget("w1", 1)])));
        model.watch_lost(1, "gone".into());
        assert_eq!(ids(&model), ["w1"]);
        assert!(!model.writable());
        assert_eq!(
            model
                .begin("w1", MutationKind::Delete)
                .ok()
                .flatten()
                .map(|_| ()),
            None
        );

        model.unsupported(1);
        assert_eq!(model.collection, Collection::Unsupported);
    }

    #[test]
    fn snapshots_apply_deletions_and_ignore_other_spaces_and_late_replies() {
        let mut model = WidgetsModel::new();
        model.retarget(target("space-a"));
        assert!(model.watched(
            1,
            snapshot("space-a", 1, vec![widget("w1", 1), widget("w2", 1)])
        ));
        let (rename, _) = model
            .begin("w2", MutationKind::Rename("Revenue".into()))
            .unwrap()
            .unwrap();

        // Another clone's snapshot on the same generation is not this page's.
        assert!(!model.watched(1, snapshot("space-b", 9, vec![])));
        // A deletion elsewhere is observed.
        assert!(model.watched(1, snapshot("space-a", 2, vec![widget("w1", 1)])));
        assert_eq!(ids(&model), ["w1"]);

        // The user moves to the other space; replies for the first are dropped.
        assert!(model.retarget(target("space-b")));
        assert!(!model.watched(1, snapshot("space-a", 3, vec![widget("w1", 1)])));
        let mut renamed = widget("w2", 2);
        renamed.title = "Revenue".into();
        let outcome = model.finish(rename, Ok(Applied::Widget(renamed)));
        assert_eq!(outcome, MutationOutcome::default());
        assert_eq!(model.collection, Collection::Loading);
    }

    #[test]
    fn a_conflict_keeps_what_is_shown_and_a_delete_moves_focus_on() {
        let mut model = WidgetsModel::new();
        model.retarget(target("space-a"));
        let shown = snapshot(
            "space-a",
            4,
            vec![widget("w1", 1), widget("w2", 3), widget("w3", 1)],
        );
        assert!(model.watched(1, shown.clone()));

        let (rename, request) = model
            .begin("w2", MutationKind::Rename("  KPIs ".into()))
            .unwrap()
            .unwrap();
        let Request::Update(params) = request else {
            panic!("a rename updates the widget");
        };
        assert_eq!(
            (params.expected_revision, params.title.as_deref()),
            (3, Some("KPIs"))
        );
        // One change at a time.
        assert!(model.begin("w1", MutationKind::Delete).unwrap().is_none());
        let outcome = model.finish(
            rename,
            Err(RpcError::Capability(
                capability_errors::WIDGETS_CONFLICT.into(),
            )),
        );
        assert_eq!(outcome.focus.as_deref(), Some("w2"));
        assert_eq!(model.snapshot(), Some(&shown));
        assert_eq!(model.problems["w2"].as_ref(), CONFLICT_COPY);

        let (reorder, request) = model.begin("w3", MutationKind::Move(-1)).unwrap().unwrap();
        let Request::Reorder(params) = request else {
            panic!("a move reorders the space");
        };
        assert_eq!(params.widget_ids, ["w1", "w3", "w2"]);
        assert_eq!(params.expected_collection_revision, 4);
        model.finish(
            reorder,
            Err(RpcError::Capability(
                capability_errors::WIDGETS_CONFLICT.into(),
            )),
        );
        assert_eq!(ids(&model), ["w1", "w2", "w3"]);
        // The first widget can't move up.
        assert!(model.begin("w1", MutationKind::Move(-1)).unwrap().is_none());

        let (delete, _) = model.begin("w2", MutationKind::Delete).unwrap().unwrap();
        let outcome = model.finish(
            delete,
            Err(RpcError::Capability(
                capability_errors::WIDGETS_NOT_FOUND.into(),
            )),
        );
        assert_eq!(outcome.focus.as_deref(), Some("w3"));
        assert_eq!(ids(&model), ["w1", "w3"]);
    }

    #[test]
    fn a_refresh_shows_what_its_attempt_needs_and_never_steps_back() {
        use ProjectWidgetRefreshState as State;
        let attempt = |id: &str, state: State, started_at: i64| ProjectWidgetRefresh {
            refresh_id: id.into(),
            state,
            started_at,
            finished_at: None,
            execution_chat_id: Some("run".into()),
            error_code: None,
            error_message: None,
        };
        let mut w = widget("w1", 1);
        assert_eq!(refresh_control(&w, false), RefreshControl::Unsupported);
        // No recipe yet: Refresh asks for one first.
        assert_eq!(
            refresh_control(&w, true),
            RefreshControl::Start { configure: true }
        );
        w.refresh_recipe = Some(ProjectWidgetRecipe {
            instruction: "Recount".into(),
            harness: HarnessId::Codex,
            model: None,
            reasoning: None,
        });
        let run = Some("run".to_owned());
        for (state, control) in [
            (State::Succeeded, RefreshControl::Start { configure: false }),
            (
                State::Queued,
                RefreshControl::Running {
                    refresh_id: "r1".into(),
                    chat_id: run.clone(),
                },
            ),
            (
                State::AwaitingInput,
                RefreshControl::AwaitingInput {
                    refresh_id: "r1".into(),
                    chat_id: run.clone(),
                },
            ),
            (State::Cancelled, RefreshControl::Cancelled),
            (
                State::Interrupted,
                RefreshControl::Failed {
                    message: "The refresh didn't finish.".into(),
                    chat_id: run.clone(),
                },
            ),
        ] {
            w.refresh = Some(attempt("r1", state, 10));
            assert_eq!(refresh_control(&w, true), control, "{state:?}");
        }
        w.refresh = Some(ProjectWidgetRefresh {
            error_message: Some("The agent finished without sending a new version.".into()),
            ..attempt("r1", State::Failed, 10)
        });
        assert!(matches!(
            refresh_control(&w, true),
            RefreshControl::Failed { message, .. } if message.starts_with("The agent finished")
        ));

        // A late reply never replaces what the watch already shows.
        let mut model = WidgetsModel::new();
        model.retarget(target("space-a"));
        w.refresh = Some(attempt("r2", State::Succeeded, 20));
        model.watched(1, snapshot("space-a", 1, vec![w]));
        model.refresh_settled(1, "w1", attempt("r2", State::Running, 20));
        model.refresh_settled(1, "w1", attempt("r1", State::Cancelled, 10));
        model.refresh_settled(0, "w1", attempt("r3", State::Queued, 30));
        let shown = model.widget("w1").unwrap().refresh.clone().unwrap();
        assert_eq!(
            (shown.refresh_id.as_str(), shown.state),
            ("r2", State::Succeeded)
        );
        model.refresh_settled(1, "w1", attempt("r3", State::Queued, 30));
        assert_eq!(
            model
                .widget("w1")
                .unwrap()
                .refresh
                .as_ref()
                .unwrap()
                .refresh_id,
            "r3"
        );
        model.refresh_settled(1, "w1", attempt("r3", State::Cancelled, 30));
        assert_eq!(
            model.widget("w1").unwrap().refresh.as_ref().unwrap().state,
            State::Cancelled
        );
    }

    #[test]
    fn a_repository_group_opens_its_current_space_or_asks() {
        let space = |id: &str, device: &str| Space {
            id: id.into(),
            device_id: device.into(),
            path: format!("/src/{id}"),
            name: None,
            git_detected: true,
            git_checked_at: None,
            checkout_id: None,
            repository_id: Some("repo".into()),
            created_at: chrono::Utc::now(),
        };
        let mut state = AppState::new();
        state.apply_spaces(vec![
            space("clone-a", "device-a"),
            space("clone-b", "device-b"),
        ]);
        let Some(WidgetsEntry::Choose(choices)) = widgets_entry(&state, "clone-a", None) else {
            panic!("two clones are an explicit choice");
        };
        assert_eq!(choices.len(), 2);
        assert_ne!(choices[0].place, choices[1].place);
        assert_eq!(
            widgets_entry(&state, "clone-a", Some("clone-b")),
            Some(WidgetsEntry::Open(WidgetsTarget {
                space_id: "clone-b".into(),
                device_id: "device-b".into(),
            }))
        );
    }

    #[gpui::test]
    fn the_page_renders_and_focus_returns_after_dialogs(cx: &mut gpui::TestAppContext) {
        use gpui::AppContext as _;
        let dir = tempfile::tempdir().unwrap();
        cx.update(|cx| {
            crate::settings::init(Default::default(), dir.path(), cx);
            gpui_base::init(cx);
            cx.set_global(Theme::default());
        });
        let window = cx.add_window(|_, cx| {
            let state = cx.new(|_| AppState::new());
            ProjectWidgetsPage::new(state, cx)
        });
        let mut long = widget("w2", 1);
        long.title = "Weekly PostHog events and Google Analytics sessions ".repeat(4);
        window
            .update(cx, |page, _, cx| {
                page.open(target("space-a"), cx);
                // No engine here: the owner reads as unsupported until a
                // snapshot arrives.
                assert_eq!(page.model.collection, Collection::Unsupported);
                assert!(page.model.watched(
                    page.model.generation,
                    snapshot("space-a", 1, vec![widget("w1", 1), long, widget("w3", 1)]),
                ));
                page.synced(cx);
                assert_eq!(page.frames.len(), 3);
            })
            .unwrap();
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
            .unwrap();
        window
            .update(cx, |page, _, cx| {
                page.open_menu("w2", true, cx);
                page.activate_menu(0, cx);
                assert!(page.rename.is_some());
            })
            .unwrap();
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
            .unwrap();
        window
            .update(cx, |page, _, cx| {
                assert!(page.on_escape(cx));
                page.open_menu("w3", true, cx);
                page.activate_menu(3, cx);
                assert_eq!(page.delete_confirm.as_deref(), Some("w3"));
            })
            .unwrap();
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
            .unwrap();
        window
            .update(cx, |page, _, cx| page.close_delete(cx))
            .unwrap();
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
            .unwrap();
        window
            .update(cx, |page, window, cx| {
                assert!(page.trigger_focus["w3"].is_focused(window));
                // Leaving the route lets go of every renderer.
                page.hide(cx);
                assert!(page.frames.is_empty());
                assert!(page.watch_task.is_none());
            })
            .unwrap();
    }
}

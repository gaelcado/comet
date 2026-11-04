//! Project widgets: visuals kept in a project space on its owner engine
//! (`project-widgets-v1`).
//!
//! A widget belongs to one space, on the engine of the device that owns the
//! space, in that engine's profile. Clients reach it through the
//! `*ProjectWidget*` RPCs, relay-forwarded with `targetDeviceId`; the owner
//! validates the space and revisions itself. IDs are opaque strings.
//!
//! Concurrency is explicit: a widget change names the `revision` it was made
//! against and a reorder the `collectionRevision`; a stale one fails with
//! `widgets.conflict` and changes nothing. Every write carries an
//! `operationId`, so retrying after a lost reply returns the first answer
//! (`replayed`) instead of applying twice. Stable error codes are in
//! `zeron_rpc::capability_errors` (`WIDGETS_*`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{HarnessId, ReasoningLevel};

/// Reserved `assetName` that reads a widget's Luau script with
/// `ReadProjectWidgetAssetChunk`. Not a valid visual asset name, so it never
/// collides with one.
pub const PROJECT_WIDGET_SCRIPT_ASSET: &str = "@script";
/// The only renderer format in V1: a `show_visual` Luau script and its assets.
pub const PROJECT_WIDGET_RENDERER_LUAU_V1: &str = "luau-v1";
/// Live widgets one space may hold.
pub const PROJECT_WIDGETS_MAX_PER_SPACE: usize = 50;
/// Longest refresh instruction, in UTF-8 bytes.
pub const PROJECT_WIDGET_INSTRUCTION_MAX_BYTES: usize = 16 * 1024;

/// One file of a widget's content, as stored. Hashes are lowercase hex SHA-256.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWidgetAsset {
    pub name: String,
    pub size: u64,
    pub sha256: String,
}

/// A widget's immutable content. Each published content gets a new
/// `bundleId`; the files of a bundle never change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWidgetContent {
    pub bundle_id: String,
    pub format_version: u32,
    /// [`PROJECT_WIDGET_RENDERER_LUAU_V1`].
    pub renderer_format: String,
    /// The seed the script was checked with; render with exactly this seed
    /// (`zeron_visual::Visual::load(script, renderSeed)`) to show the same
    /// random data. A saved visual keeps its source's
    /// `zeron_visual::seed_for(sourceVisualId)`.
    pub render_seed: u64,
    pub script_size: u64,
    pub script_sha256: String,
    /// In name order.
    #[serde(default)]
    pub assets: Vec<ProjectWidgetAsset>,
    /// What the content showed when it was published, for miniatures.
    /// Absent until the owner has taken it (content published before
    /// previews existed); empty when the visual has nothing to summarize.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<ProjectWidgetPreview>,
}

/// Most values a preview series keeps; longer series are averaged down.
pub const PROJECT_WIDGET_PREVIEW_POINTS: usize = 48;
/// The scale of [`ProjectWidgetPreviewChart`] values: `0` is the chart's
/// lowest value, this its highest.
pub const PROJECT_WIDGET_PREVIEW_SCALE: u16 = 1000;

/// A summary of a widget's content as rendered at publication: its headline
/// figures, its first chart's shape, its first table's top rows, or its
/// opening text. Small, so it travels in every snapshot; never script or
/// asset bytes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWidgetPreview {
    /// At most three, in reading order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stats: Vec<ProjectWidgetPreviewStat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chart: Option<ProjectWidgetPreviewChart>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<ProjectWidgetPreviewTable>,
    /// The first text, when there is no figure, chart or table.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// The visual draws a canvas, a 3D scene or a shader, which a preview
    /// can't summarize.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub graphic: bool,
}

impl ProjectWidgetPreview {
    pub fn is_empty(&self) -> bool {
        self.stats.is_empty()
            && self.chart.is_none()
            && self.table.is_none()
            && self.text.is_none()
            && !self.graphic
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWidgetPreviewStat {
    pub value: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProjectWidgetPreviewChartKind {
    Bar,
    Line,
    Area,
    Scatter,
    #[serde(other)]
    Other,
}

/// A chart's shape: every series on one shared scale, so their heights
/// compare as they do in the full chart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWidgetPreviewChart {
    pub kind: ProjectWidgetPreviewChartKind,
    #[serde(default)]
    pub stacked: bool,
    /// At most three series of at most [`PROJECT_WIDGET_PREVIEW_POINTS`]
    /// values in `0..=`[`PROJECT_WIDGET_PREVIEW_SCALE`]; `None` is a gap.
    pub series: Vec<ProjectWidgetPreviewSeries>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWidgetPreviewSeries {
    pub name: String,
    pub values: Vec<Option<u16>>,
}

/// A table's header and top rows, at most three columns and four rows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWidgetPreviewTable {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// Where a saved visual came from. A link, not ownership: deleting the chat
/// leaves the widget intact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWidgetSource {
    pub source_chat_id: String,
    /// The chat's `MessagePart::Visual` id.
    pub source_visual_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_entry_id: Option<String>,
    /// The chat's title when the visual was saved, for display.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_title: Option<String>,
    pub source_device_id: String,
}

/// What a refresh asks an agent to do again. Never holds a credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWidgetRecipe {
    pub instruction: String,
    pub harness: HarnessId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ReasoningLevel>,
}

/// Where a refresh attempt stands. Stable wire names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProjectWidgetRefreshState {
    Queued,
    Running,
    AwaitingInput,
    Succeeded,
    Failed,
    Cancelled,
    Interrupted,
    Conflict,
    /// A state added by a newer engine.
    #[serde(other)]
    Other,
}

/// The latest refresh attempt of a widget. `startedAt` is when it was
/// last attempted, distinct from the widget's `lastSuccessfulAt`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWidgetRefresh {
    pub refresh_id: String,
    pub state: ProjectWidgetRefreshState,
    /// Epoch millis.
    pub started_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<i64>,
    /// The widget's dedicated refresh chat, kept archived ("Open run").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_chat_id: Option<String>,
    /// Why a finished attempt did not publish: `no_result`, `agent_error`,
    /// `timeout`, `cancelled`, `deleted`, `conflict`, `run_stopped`,
    /// `engine_restarted`, `chat_deleted`, `chat_busy`, `launch_failed`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    /// A short sentence for `errorCode`, written by the engine; never the
    /// agent's output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

/// A widget as its owner stores it. Times are epoch millis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWidget {
    pub id: String,
    pub space_id: String,
    pub owner_device_id: String,
    pub title: String,
    /// Bumped by every durable change of this widget (title, recipe,
    /// content, deletion); never by refresh progress or reordering.
    pub revision: u64,
    pub created_at: i64,
    pub updated_at: i64,
    /// When the current content was validated (creation, save or refresh).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_successful_at: Option<i64>,
    /// Bumped each time the content changes; names the bundle to read.
    pub content_revision: u64,
    pub content: ProjectWidgetContent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<ProjectWidgetSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_recipe: Option<ProjectWidgetRecipe>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh: Option<ProjectWidgetRefresh>,
}

/// `WatchProjectWidgets` item: the whole collection of one space, in order.
/// A deleted widget is simply absent from the next snapshot. Script bytes
/// never travel here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWidgetsSnapshot {
    pub space_id: String,
    pub owner_device_id: String,
    pub collection_revision: u64,
    #[serde(default)]
    pub widgets: Vec<ProjectWidget>,
}

/// `GetProjectWidget` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWidgetDetail {
    pub widget: ProjectWidget,
    /// The source chat no longer exists. The widget stays fully usable.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub source_missing: bool,
}

/// How a write was answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProjectWidgetWriteOutcome {
    Created,
    Updated,
    /// The same `operationId` (or `creationKey`) and payload were already
    /// applied; this is that first answer.
    Replayed,
    /// The visual is already saved in this space; nothing was created.
    Existing,
    /// A refresh run's content was validated and kept as the candidate of
    /// its attempt; the widget is unchanged until the run succeeds.
    Staged,
    /// An outcome added by a newer engine.
    #[serde(other)]
    Other,
}

/// Reply of `SaveProjectVisual`, `CreateProjectWidget` and
/// `UpdateProjectWidget`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWidgetWrite {
    pub outcome: ProjectWidgetWriteOutcome,
    pub widget: ProjectWidget,
}

/// `DeleteProjectWidget` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWidgetDeletion {
    pub widget_id: String,
    pub space_id: String,
    /// The tombstone's revision.
    pub revision: u64,
    pub collection_revision: u64,
    /// The widget was already deleted (by this operation's first attempt or
    /// another client).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub already_deleted: bool,
}

/// `ReadProjectWidgetAssetChunk` reply: chunk conventions of
/// `ReadAttachmentChunk` (base64 `data`, `nextOffset`, `done`), plus the
/// whole file's `size` and `sha256` to verify once assembled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWidgetAssetChunk {
    pub name: String,
    pub data: String,
    pub next_offset: u64,
    pub done: bool,
    pub size: u64,
    pub sha256: String,
}

// ── params ──────────────────────────────────────────────────────────────────

/// `WatchProjectWidgets` params. Stream; relay-forwardable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchProjectWidgetsParams {
    pub space_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_device_id: Option<String>,
}

/// `GetProjectWidget` params → [`ProjectWidgetDetail`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetProjectWidgetParams {
    pub widget_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_device_id: Option<String>,
}

/// `ReadProjectWidgetAssetChunk` params → [`ProjectWidgetAssetChunk`]. A
/// `contentRevision` other than the widget's current one is refused with
/// `widgets.conflict`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadProjectWidgetAssetChunkParams {
    pub widget_id: String,
    pub content_revision: u64,
    /// An asset name, or [`PROJECT_WIDGET_SCRIPT_ASSET`] for the script.
    pub asset_name: String,
    #[serde(default)]
    pub offset: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_device_id: Option<String>,
}

/// `SaveProjectVisual` params → [`ProjectWidgetWrite`]. The owner reads the
/// visual from the chat it hosts; no path is accepted from the client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveProjectVisualParams {
    pub space_id: String,
    pub source_chat_id: String,
    pub source_visual_id: String,
    /// Defaults to the visual's own title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub operation_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_device_id: Option<String>,
}

/// Full widget content sent by a client: a Luau script and its assets,
/// base64 by name, as `PublishVisual` takes them.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWidgetContentInput {
    pub script: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub assets: BTreeMap<String, String>,
    /// Keep the assets of this bundle that `assets` does not replace. Must
    /// name the widget's current bundle, or the update is a conflict. Absent:
    /// the content is exactly `assets`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_assets_from_bundle: Option<String>,
}

/// `CreateProjectWidget` params → [`ProjectWidgetWrite`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectWidgetParams {
    pub space_id: String,
    /// Stable within the space: the same key and payload return the first
    /// widget; the same key with another payload is `widgets.key_reused`.
    pub creation_key: String,
    pub operation_id: String,
    pub title: String,
    pub content: ProjectWidgetContentInput,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_recipe: Option<ProjectWidgetRecipe>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_device_id: Option<String>,
}

/// How an update changes the refresh recipe.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "kind", content = "recipe", rename_all = "camelCase")]
pub enum ProjectWidgetRecipePatch {
    #[default]
    Unchanged,
    Set(ProjectWidgetRecipe),
    Clear,
}

/// `UpdateProjectWidget` params → [`ProjectWidgetWrite`]. Omitted fields stay
/// as they are; a title-only change does not touch the content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProjectWidgetParams {
    pub widget_id: String,
    pub expected_revision: u64,
    pub operation_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default)]
    pub refresh_recipe: ProjectWidgetRecipePatch,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<ProjectWidgetContentInput>,
    /// Stage the content as a candidate of this refresh instead of
    /// publishing it. Refused with `widgets.refresh_inactive` unless the
    /// refresh is the widget's active one and `originChatId` is its run's chat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_id: Option<String>,
    /// The chat whose agent makes the call (the Zeron MCP's origin). A
    /// refresh run may only stage its own widget's content.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_chat_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_device_id: Option<String>,
}

/// `ReorderProjectWidgets` params → [`ProjectWidgetsSnapshot`]. `widgetIds`
/// must be exactly the live widgets of the space, each once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReorderProjectWidgetsParams {
    pub space_id: String,
    pub widget_ids: Vec<String>,
    pub expected_collection_revision: u64,
    pub operation_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_device_id: Option<String>,
}

/// `DeleteProjectWidget` params → [`ProjectWidgetDeletion`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteProjectWidgetParams {
    pub widget_id: String,
    pub expected_revision: u64,
    pub operation_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_device_id: Option<String>,
}

/// `RefreshProjectWidget` params → [`ProjectWidgetRefreshAdmission`]. Answers
/// once the attempt is durably queued, without waiting for the agent. The
/// same `requestId`, or any click while an attempt is active, returns that
/// attempt instead of starting another.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshProjectWidgetParams {
    pub widget_id: String,
    pub expected_revision: u64,
    pub request_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_device_id: Option<String>,
}

/// `CancelProjectWidgetRefresh` params → [`ProjectWidgetRefreshCancellation`].
/// The answer means no result of that attempt can be published any more;
/// stopping its agent follows. Cancelling again changes nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelProjectWidgetRefreshParams {
    pub widget_id: String,
    pub refresh_id: String,
    pub request_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_device_id: Option<String>,
}

/// `RefreshProjectWidget` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWidgetRefreshAdmission {
    pub widget_id: String,
    pub refresh: ProjectWidgetRefresh,
    /// An attempt for this `requestId`, or an active one, already existed;
    /// no new run was started.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub existing: bool,
}

/// How a cancellation was answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProjectWidgetCancelOutcome {
    Cancelled,
    AlreadyCancelled,
    /// The attempt had already published its result.
    AlreadyCompleted,
    /// The attempt had already ended without publishing.
    AlreadyFinished,
    #[serde(other)]
    Other,
}

/// `CancelProjectWidgetRefresh` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWidgetRefreshCancellation {
    pub widget_id: String,
    pub outcome: ProjectWidgetCancelOutcome,
    pub refresh: ProjectWidgetRefresh,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn wire_shapes_clients_rely_on() {
        // The script's reserved name can never be an asset's.
        assert!(!crate::valid_visual_asset_name(PROJECT_WIDGET_SCRIPT_ASSET));

        let update: UpdateProjectWidgetParams = serde_json::from_value(json!({
            "widgetId": "w", "expectedRevision": 3, "operationId": "op", "title": "KPIs"
        }))
        .unwrap();
        assert_eq!(update.refresh_recipe, ProjectWidgetRecipePatch::Unchanged);
        assert!(update.content.is_none());
        assert_eq!(
            serde_json::to_value(ProjectWidgetRecipePatch::Clear).unwrap(),
            json!({ "kind": "clear" })
        );
        assert_eq!(
            serde_json::from_value::<ProjectWidgetRefreshState>(json!("paused")).unwrap(),
            ProjectWidgetRefreshState::Other
        );
        assert_eq!(
            serde_json::to_value(ProjectWidgetRefreshState::AwaitingInput).unwrap(),
            "awaitingInput"
        );
        assert_eq!(
            serde_json::from_value::<ProjectWidgetWriteOutcome>(json!("merged")).unwrap(),
            ProjectWidgetWriteOutcome::Other
        );
        // A detail whose source still exists omits the flag.
        let content = ProjectWidgetContent {
            bundle_id: "b".into(),
            format_version: 1,
            renderer_format: PROJECT_WIDGET_RENDERER_LUAU_V1.into(),
            render_seed: u64::MAX,
            script_size: 1,
            script_sha256: "00".into(),
            assets: vec![],
            preview: None,
        };
        let widget = ProjectWidget {
            id: "w".into(),
            space_id: "s".into(),
            owner_device_id: "d".into(),
            title: "t".into(),
            revision: 1,
            created_at: 0,
            updated_at: 0,
            last_successful_at: None,
            content_revision: 1,
            content,
            source: None,
            refresh_recipe: None,
            refresh: None,
        };
        let wire = serde_json::to_value(ProjectWidgetDetail {
            widget,
            source_missing: false,
        })
        .unwrap();
        assert!(wire.get("sourceMissing").is_none());
        assert_eq!(wire["widget"]["content"]["renderSeed"], json!(u64::MAX));
        let back: ProjectWidgetDetail = serde_json::from_value(wire).unwrap();
        assert_eq!(back.widget.content.render_seed, u64::MAX);
    }
}

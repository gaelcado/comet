//! The project widget tools: `list_widgets`, `get_widget`, `create_widget`
//! and `update_widget`, over the engine's `*ProjectWidget*` RPCs.
//!
//! The engine that owns the project keeps the invariants (revisions,
//! replay by `operationId` and `creationKey`, content checks); these handlers
//! only resolve the project and device, read asset files from the agent's
//! working directory, and turn the engine's stable codes into errors that
//! start with the code. Admission happens in `Tools::invoke` like any tool.

use std::collections::BTreeMap;

use anyhow::{Context, anyhow, bail};
use base64::Engine as _;
use serde::Deserialize;
use serde_json::{Value, json};
use zeron_proto::{
    CreateProjectWidgetParams, GetProjectWidgetParams, PROJECT_WIDGET_SCRIPT_ASSET, ProjectWidget,
    ProjectWidgetContentInput, ProjectWidgetDetail, ProjectWidgetRecipe, ProjectWidgetRecipePatch,
    ProjectWidgetWrite, ProjectWidgetsSnapshot, ReadProjectWidgetAssetChunkParams,
    UpdateProjectWidgetParams, WatchProjectWidgetsParams,
};
use zeron_rpc::{RpcError, capability_errors as codes, methods};

use crate::tools::read_visual_assets;
use crate::zeron::Zeron;

/// Asset bytes `get_widget` returns inline, all assets together; the rest
/// are named in `omittedAssets`. Stated as "1 MB" in the tool description.
const INLINE_ASSET_BYTES: u64 = 1024 * 1024;

#[derive(Deserialize)]
pub(crate) struct ListArgs {
    project: Option<String>,
    device: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GetArgs {
    widget_id: String,
    device: Option<String>,
    #[serde(default)]
    include_content: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CreateArgs {
    project: String,
    device: Option<String>,
    creation_key: String,
    operation_id: String,
    title: String,
    script: String,
    #[serde(default)]
    assets: BTreeMap<String, String>,
    refresh_recipe: Option<ProjectWidgetRecipe>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UpdateArgs {
    widget_id: String,
    expected_revision: u64,
    operation_id: String,
    device: Option<String>,
    title: Option<String>,
    refresh_recipe: Option<ProjectWidgetRecipe>,
    #[serde(default)]
    clear_refresh_recipe: bool,
    script: Option<String>,
    #[serde(default)]
    assets: BTreeMap<String, String>,
    keep_existing_assets: Option<bool>,
    refresh_id: Option<String>,
}

pub(crate) async fn list(zeron: &Zeron, args: ListArgs) -> anyhow::Result<Value> {
    let project = match args.project {
        Some(project) => project,
        // Never an implicit "first project": only the origin chat's own.
        None => {
            let chat = zeron
                .origin()
                .chat_id
                .as_deref()
                .context("project is required outside a chat")?;
            zeron
                .resolve_chat(chat)
                .await?
                .space_id
                .context("this chat is not in a project; pass project")?
        }
    };
    let (space, device_id) = zeron
        .resolve_target(Some(&project), args.device.as_deref())
        .await?;
    let space = space.expect("project provided");
    let params = WatchProjectWidgetsParams {
        space_id: space.id.clone(),
        target_device_id: Some(device_id.clone()),
    };
    let snapshot = zeron
        .snapshot_rpc(methods::WATCH_PROJECT_WIDGETS, json!(params))
        .await
        .map_err(|e| widget_error(methods::WATCH_PROJECT_WIDGETS, e))?;
    let snapshot: ProjectWidgetsSnapshot =
        serde_json::from_value(snapshot).context("WatchProjectWidgets: unexpected shape")?;
    Ok(json!({
        "project": { "id": space.id, "name": space.display_name(), "path": space.path },
        "deviceId": device_id,
        "widgets": snapshot.widgets.iter().map(|w| json!({
            "id": w.id,
            "title": w.title,
            "revision": w.revision,
            "contentRevision": w.content_revision,
            "lastSuccessfulAt": w.last_successful_at,
            "updatedAt": w.updated_at,
            "hasSource": w.source.is_some(),
            "hasRefreshRecipe": w.refresh_recipe.is_some(),
        })).collect::<Vec<_>>(),
    }))
}

pub(crate) async fn get(zeron: &Zeron, args: GetArgs) -> anyhow::Result<Value> {
    let target = target_device(zeron, args.device.as_deref()).await?;
    let detail = detail(zeron, &args.widget_id, target.clone()).await?;
    let widget = &detail.widget;
    let mut out = json!({
        "widget": widget,
        "deviceId": widget.owner_device_id,
        "sourceMissing": detail.source_missing,
    });
    if !args.include_content {
        return Ok(out);
    }
    let script = read_file(zeron, widget, PROJECT_WIDGET_SCRIPT_ASSET, target.clone()).await?;
    let script = String::from_utf8(script).context("the widget's script is not UTF-8")?;
    let (mut assets, mut omitted, mut budget) = (serde_json::Map::new(), Vec::new(), 0);
    for asset in &widget.content.assets {
        if budget + asset.size > INLINE_ASSET_BYTES {
            omitted.push(json!({ "name": asset.name, "size": asset.size }));
            continue;
        }
        budget += asset.size;
        let bytes = read_file(zeron, widget, &asset.name, target.clone()).await?;
        assets.insert(
            asset.name.clone(),
            base64::engine::general_purpose::STANDARD
                .encode(bytes)
                .into(),
        );
    }
    out["script"] = script.into();
    out["assets"] = assets.into();
    if !omitted.is_empty() {
        out["omittedAssets"] = omitted.into();
    }
    Ok(out)
}

pub(crate) async fn create(zeron: &Zeron, args: CreateArgs) -> anyhow::Result<Value> {
    // Resolve the project before reading anything: no project, no effect.
    let (space, device_id) = zeron
        .resolve_target(Some(&args.project), args.device.as_deref())
        .await?;
    let space = space.expect("project provided");
    let assets = read_visual_assets(&args.assets).await?;
    let params = CreateProjectWidgetParams {
        space_id: space.id,
        creation_key: args.creation_key,
        operation_id: args.operation_id,
        title: args.title,
        content: ProjectWidgetContentInput {
            script: args.script,
            assets,
            keep_assets_from_bundle: None,
        },
        refresh_recipe: args.refresh_recipe,
        target_device_id: Some(device_id),
    };
    let write =
        call::<ProjectWidgetWrite>(zeron, methods::CREATE_PROJECT_WIDGET, json!(params)).await?;
    Ok(written(&write))
}

pub(crate) async fn update(zeron: &Zeron, args: UpdateArgs) -> anyhow::Result<Value> {
    if args.script.is_none() {
        anyhow::ensure!(
            args.assets.is_empty() && args.keep_existing_assets.is_none(),
            "assets and keepExistingAssets change the content, which needs the whole script: \
             send script too (get_widget with includeContent returns the current one)"
        );
        anyhow::ensure!(
            args.title.is_some() || args.refresh_recipe.is_some() || args.clear_refresh_recipe,
            "nothing to update: pass title, refreshRecipe, clearRefreshRecipe or script"
        );
    }
    anyhow::ensure!(
        !(args.refresh_recipe.is_some() && args.clear_refresh_recipe),
        "pass refreshRecipe or clearRefreshRecipe, not both"
    );
    let target = target_device(zeron, args.device.as_deref()).await?;
    let content = match args.script {
        None => None,
        Some(script) => {
            let current = detail(zeron, &args.widget_id, target.clone()).await?.widget;
            // `expectedRevision` fixes which bundle may be kept, so a retry
            // that reads the widget after its first attempt applied still
            // replays; at another revision the engine answers.
            let keep = match args.keep_existing_assets {
                Some(true) => Some(current.content.bundle_id.clone()),
                Some(false) => None,
                None => {
                    let assets = &current.content.assets;
                    if current.revision == args.expected_revision && !assets.is_empty() {
                        bail!(
                            "the widget has assets ({}); pass keepExistingAssets: true to keep \
                             those you do not send, or false to replace them with exactly assets",
                            assets
                                .iter()
                                .map(|a| a.name.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        );
                    }
                    None
                }
            };
            Some(ProjectWidgetContentInput {
                script,
                assets: read_visual_assets(&args.assets).await?,
                keep_assets_from_bundle: keep,
            })
        }
    };
    let refresh_recipe = match (args.refresh_recipe, args.clear_refresh_recipe) {
        (Some(recipe), _) => ProjectWidgetRecipePatch::Set(recipe),
        (None, true) => ProjectWidgetRecipePatch::Clear,
        (None, false) => ProjectWidgetRecipePatch::Unchanged,
    };
    let params = UpdateProjectWidgetParams {
        widget_id: args.widget_id,
        expected_revision: args.expected_revision,
        operation_id: args.operation_id,
        title: args.title,
        refresh_recipe,
        content,
        refresh_id: args.refresh_id,
        origin_chat_id: zeron.origin().chat_id.clone(),
        target_device_id: target,
    };
    let write =
        call::<ProjectWidgetWrite>(zeron, methods::UPDATE_PROJECT_WIDGET, json!(params)).await?;
    Ok(written(&write))
}

fn written(write: &ProjectWidgetWrite) -> Value {
    let w = &write.widget;
    json!({
        "outcome": write.outcome,
        "widgetId": w.id,
        "projectId": w.space_id,
        "deviceId": w.owner_device_id,
        "title": w.title,
        "revision": w.revision,
        "contentRevision": w.content_revision,
    })
}

/// `None` addresses the local engine.
async fn target_device(zeron: &Zeron, device: Option<&str>) -> anyhow::Result<Option<String>> {
    match device.map(str::trim).filter(|d| !d.is_empty()) {
        Some(key) => Ok(Some(zeron.resolve_device_id(Some(key)).await?)),
        None => Ok(None),
    }
}

async fn detail(
    zeron: &Zeron,
    widget_id: &str,
    target: Option<String>,
) -> anyhow::Result<ProjectWidgetDetail> {
    let params = GetProjectWidgetParams {
        widget_id: widget_id.to_owned(),
        target_device_id: target,
    };
    call(zeron, methods::GET_PROJECT_WIDGET, json!(params)).await
}

/// One whole file of the widget's current content, chunk by chunk.
async fn read_file(
    zeron: &Zeron,
    widget: &ProjectWidget,
    name: &str,
    target: Option<String>,
) -> anyhow::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    loop {
        let params = ReadProjectWidgetAssetChunkParams {
            widget_id: widget.id.clone(),
            content_revision: widget.content_revision,
            asset_name: name.to_owned(),
            offset: bytes.len() as u64,
            target_device_id: target.clone(),
        };
        let chunk: zeron_proto::ProjectWidgetAssetChunk = call(
            zeron,
            methods::READ_PROJECT_WIDGET_ASSET_CHUNK,
            json!(params),
        )
        .await?;
        bytes.extend(
            base64::engine::general_purpose::STANDARD
                .decode(&chunk.data)
                .context("ReadProjectWidgetAssetChunk: bad data")?,
        );
        anyhow::ensure!(
            bytes.len() as u64 <= chunk.size && chunk.next_offset == bytes.len() as u64,
            "ReadProjectWidgetAssetChunk: inconsistent chunk for {name}"
        );
        if chunk.done {
            return Ok(bytes);
        }
    }
}

async fn call<T: serde::de::DeserializeOwned>(
    zeron: &Zeron,
    method: &str,
    params: Value,
) -> anyhow::Result<T> {
    let value = zeron
        .call_rpc(method, params)
        .await
        .map_err(|e| widget_error(method, e))?;
    serde_json::from_value(value).with_context(|| format!("{method}: unexpected shape"))
}

/// An engine refusal as text that starts with its stable code, plus what to
/// do about it.
fn widget_error(method: &str, error: RpcError) -> anyhow::Error {
    let code = match error {
        RpcError::Capability(code) => code,
        // Keeps the script's error and line after the code.
        RpcError::Failed(message) if message.starts_with(codes::WIDGETS_INVALID_VISUAL) => {
            return anyhow!("{message}");
        }
        RpcError::BadParams(message) => return anyhow!("invalid arguments: {message}"),
        RpcError::UnknownMethod(_) => {
            return anyhow!(
                "the project's Zeron engine predates project widgets (project-widgets-v1); update it"
            );
        }
        other => return anyhow!("{method}: {other}"),
    };
    let hint = match code.as_str() {
        codes::WIDGETS_CONFLICT => {
            "the widget changed since that revision; call get_widget and retry against its \
             current revision with a new operationId"
        }
        codes::WIDGETS_KEY_REUSED => {
            "this operationId or creationKey was already used with other arguments; resend \
             the original arguments to get the first answer, or use a new id"
        }
        codes::WIDGETS_NOT_FOUND => "no such widget on that device (see list_widgets)",
        codes::WIDGETS_OWNER_UNAVAILABLE => {
            "the project's device is unreachable or no longer hosts this project"
        }
        codes::WIDGETS_REFRESH_INACTIVE => {
            "refreshId is not this widget's active refresh (it ended, was cancelled or \
             belongs to another chat); omit it outside a refresh run"
        }
        codes::WIDGETS_REFRESH_SCOPE => {
            "a widget refresh run may only stage its own widget's content: call update_widget \
             with that widget's id and the refreshId from the refresh instructions"
        }
        codes::WIDGETS_QUOTA_EXCEEDED => "this project already has the most widgets it can keep",
        codes::WIDGETS_STORE_UNAVAILABLE => "the widget store cannot be written on that device",
        codes::WIDGETS_WRITE_FAILED => "the engine could not save the widget; nothing changed",
        _ => return anyhow!("{method}: {code}"),
    };
    anyhow!("{code}: {hint}")
}

//! Project widgets: visuals kept in a project space by the engine that owns
//! the space (`zeron_proto::project_widgets`).
//!
//! [`ProjectWidgets`] owns the widgets of one engine profile:
//! `project-widgets.sqlite3` and the immutable bundles under
//! `project-widgets/assets/` in the profile store root. Every caller goes
//! through it; the invariants live here:
//!
//! - **Ownership.** A space is served only while it is live in the registry
//!   and owned by this device ([`WidgetSpaces`]); a `targetDeviceId` is
//!   routing, never authorization.
//! - **Compare-and-set.** A widget change applies only against its current
//!   `revision` (and never to a tombstone); a reorder only against the
//!   space's `collectionRevision` with exactly its live widgets.
//! - **Replay.** Each write records a receipt for its `operationId` with a
//!   fingerprint of its payload: the same operation returns its first answer,
//!   the same id with another payload is refused. A visual is saved once per
//!   space while its widget lives; a `creationKey` is never reused in a space.
//!   Tombstones and receipts are kept for the store's lifetime.
//! - **Immutable content.** Content is copied into a new bundle, validated
//!   and checked with the recorded seed before the transaction that points the
//!   widget at it. A failed write leaves the previous bundle and pointer; the
//!   orphan is collected later, after a grace period.
//!
//! SQLite and file work is blocking: call from `spawn_blocking`. The store
//! lock is a plain mutex never held across an `.await`.

mod bundles;
mod store;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::time::Duration;

use base64::Engine as _;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde_json::json;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use zeron_doc::MessagePart;
use zeron_proto::{
    CreateProjectWidgetParams, DeleteProjectWidgetParams, PROJECT_WIDGET_INSTRUCTION_MAX_BYTES,
    PROJECT_WIDGET_SCRIPT_ASSET, PROJECT_WIDGETS_MAX_PER_SPACE, ProjectWidget,
    ProjectWidgetAssetChunk, ProjectWidgetContentInput, ProjectWidgetDeletion, ProjectWidgetRecipe,
    ProjectWidgetRecipePatch, ProjectWidgetSource, ProjectWidgetWrite, ProjectWidgetWriteOutcome,
    ProjectWidgetsSnapshot, ReadProjectWidgetAssetChunkParams, ReorderProjectWidgetsParams,
    SaveProjectVisualParams, Space, UpdateProjectWidgetParams, VISUAL_MAX_ASSET_BYTES,
    VISUAL_MAX_ASSETS, VISUAL_MAX_TITLE_CHARS, valid_visual_asset_name,
};

use bundles::{BundleFile, Bundles, sha256};
use store::StoredWidget;

pub const PROJECT_WIDGETS_DB: &str = "project-widgets.sqlite3";
/// Bundles live in `<store_root>/project-widgets/assets/<bundleId>/`.
pub const PROJECT_WIDGETS_DIR: &str = "project-widgets";
/// An unreferenced bundle younger than this is never collected: it may be
/// published and waiting for its transaction.
const GC_GRACE: Duration = Duration::from_secs(10 * 60);
const MAX_KEY_BYTES: usize = 128;

const KIND_SAVE: &str = "save";
const KIND_CREATE: &str = "create";
const KIND_UPDATE: &str = "update";
const KIND_REORDER: &str = "reorder";
const KIND_DELETE: &str = "delete";

#[derive(Debug, thiserror::Error)]
pub enum WidgetsError {
    #[error("the widget changed elsewhere; reload it before retrying")]
    Conflict,
    #[error("this operation or creation key was already used with another payload")]
    KeyReused,
    #[error("no such widget")]
    NotFound,
    #[error("the visual to save no longer exists")]
    SourceNotFound,
    #[error("the project is not available on this device")]
    OwnerUnavailable,
    #[error("that refresh is not active")]
    RefreshInactive,
    #[error("{0}")]
    InvalidVisual(String),
    #[error("this project already has {PROJECT_WIDGETS_MAX_PER_SPACE} widgets")]
    QuotaExceeded,
    #[error("{0}")]
    BadRequest(String),
    #[error("the widget store is unavailable")]
    StoreUnavailable,
    #[error("could not save the widget: {0}")]
    Write(String),
}

/// Which spaces this engine may serve widgets for.
pub trait WidgetSpaces: Send + Sync + 'static {
    /// Live in the registry and owned by this device.
    fn owned(&self, space_id: &str) -> bool;
    /// Explicitly deleted (a tombstone, here or from a peer). An unknown
    /// space is not deleted.
    fn deleted(&self, space_id: &str) -> bool;
}

/// [`WidgetSpaces`] read from the workspace registry.
pub struct RegistrySpaces {
    workspace: crate::WorkspaceHost,
    device_id: String,
}

impl RegistrySpaces {
    pub fn new(workspace: crate::WorkspaceHost, device_id: &str) -> Self {
        Self {
            workspace,
            device_id: device_id.to_owned(),
        }
    }
}

impl WidgetSpaces for RegistrySpaces {
    fn owned(&self, space_id: &str) -> bool {
        self.workspace
            .space(space_id)
            .ok()
            .flatten()
            .is_some_and(|space| space.device_id == self.device_id)
    }

    fn deleted(&self, space_id: &str) -> bool {
        self.workspace.space_deleted(space_id)
    }
}

/// A chat visual resolved on this engine, ready to copy into a widget.
pub struct SavedVisualSource {
    pub source: ProjectWidgetSource,
    pub title: String,
    pub script: String,
    pub assets: BTreeMap<String, Vec<u8>>,
}

/// The project widgets of one engine profile. Cheap to clone.
#[derive(Clone)]
pub struct ProjectWidgets {
    inner: Arc<Inner>,
}

struct Inner {
    device_id: String,
    spaces: Arc<dyn WidgetSpaces>,
    /// `None` when the database could not be opened: every call fails with
    /// `StoreUnavailable` and nothing on disk is touched.
    db: Mutex<Option<Db>>,
    bundles: Bundles,
    /// Lock order: `db`, then `watchers`.
    watchers: Mutex<HashMap<String, watch::Sender<ProjectWidgetsSnapshot>>>,
    cancel: CancellationToken,
    #[cfg(test)]
    fail_next_commit: std::sync::atomic::AtomicBool,
}

struct Db {
    conn: Connection,
    /// False for a store written by a newer build.
    writable: bool,
}

impl ProjectWidgets {
    /// Open the profile's store. Never fails: an unusable store answers
    /// `StoreUnavailable`. Refresh attempts left unfinished by a previous
    /// run are marked interrupted here, before anything can resume them.
    pub fn open(profile_store_root: &Path, device_id: &str, spaces: Arc<dyn WidgetSpaces>) -> Self {
        let db = match store::open(&profile_store_root.join(PROJECT_WIDGETS_DB)) {
            Ok((conn, writable)) => {
                if writable {
                    match store::interrupt_unfinished_refreshes(&conn) {
                        Ok(0) => {}
                        Ok(count) => {
                            tracing::info!(count, "unfinished widget refreshes interrupted")
                        }
                        Err(err) => {
                            tracing::error!(error = %err, "widget refreshes not interrupted")
                        }
                    }
                }
                Some(Db { conn, writable })
            }
            Err(err) => {
                tracing::error!(error = %err, "project widgets store unavailable");
                None
            }
        };
        Self {
            inner: Arc::new(Inner {
                device_id: device_id.to_owned(),
                spaces,
                db: Mutex::new(db),
                bundles: Bundles::new(profile_store_root.join(PROJECT_WIDGETS_DIR).join("assets")),
                watchers: Mutex::new(HashMap::new()),
                cancel: CancellationToken::new(),
                #[cfg(test)]
                fail_next_commit: Default::default(),
            }),
        }
    }

    /// Delete the widgets of spaces deleted in the registry (by this device
    /// or a peer), now and after every registry change, and collect orphan
    /// bundles once. Ends with [`Self::shutdown`] or when the owner drops.
    pub fn start_reconciliation(&self, mut spaces: watch::Receiver<Vec<Space>>) {
        let weak = Arc::downgrade(&self.inner);
        let cancel = self.inner.cancel.clone();
        tokio::spawn(async move {
            let pass = |weak: &Weak<Inner>, collect: bool| {
                let inner = weak.upgrade()?;
                Some(tokio::task::spawn_blocking(move || {
                    let widgets = ProjectWidgets { inner };
                    if let Err(err) = widgets.reconcile_deleted_spaces() {
                        tracing::warn!(error = %err, "project widgets reconciliation failed");
                    }
                    if collect {
                        widgets.collect_garbage_older_than(GC_GRACE);
                    }
                }))
            };
            let Some(first) = pass(&weak, true) else {
                return;
            };
            let _ = first.await;
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    changed = spaces.changed() => {
                        if changed.is_err() {
                            break;
                        }
                        spaces.borrow_and_update();
                        let Some(next) = pass(&weak, false) else { break };
                        let _ = next.await;
                    }
                }
            }
        });
    }

    pub fn shutdown(&self) {
        self.inner.cancel.cancel();
    }

    /// The space's live widgets in order.
    pub fn snapshot(&self, space_id: &str) -> Result<ProjectWidgetsSnapshot, WidgetsError> {
        self.owned(space_id)?;
        self.read(|conn| self.snapshot_of(conn, space_id))
    }

    /// The space's snapshot now, then after every change to it.
    pub fn watch(
        &self,
        space_id: &str,
    ) -> Result<watch::Receiver<ProjectWidgetsSnapshot>, WidgetsError> {
        self.owned(space_id)?;
        self.read(|conn| {
            let mut watchers = lock(&self.inner.watchers);
            if let Some(sender) = watchers.get(space_id) {
                return Ok(sender.subscribe());
            }
            let sender = watch::Sender::new(self.snapshot_of(conn, space_id)?);
            let receiver = sender.subscribe();
            watchers.insert(space_id.to_owned(), sender);
            Ok(receiver)
        })
    }

    /// A live widget of a space this engine serves.
    pub fn get(&self, widget_id: &str) -> Result<ProjectWidget, WidgetsError> {
        let widget = self.read(|conn| live(store::load_widget(conn, widget_id)?))?;
        self.owned(&widget.space_id)?;
        Ok(widget)
    }

    /// One chunk of the current content's script ([`PROJECT_WIDGET_SCRIPT_ASSET`])
    /// or of a named asset. Another `contentRevision` is a conflict.
    pub fn read_asset_chunk(
        &self,
        p: &ReadProjectWidgetAssetChunkParams,
    ) -> Result<ProjectWidgetAssetChunk, WidgetsError> {
        let widget = self.get(&p.widget_id)?;
        if widget.content_revision != p.content_revision {
            return Err(WidgetsError::Conflict);
        }
        let content = &widget.content;
        let (file, size, sha) = if p.asset_name == PROJECT_WIDGET_SCRIPT_ASSET {
            (
                BundleFile::Script,
                content.script_size,
                &content.script_sha256,
            )
        } else {
            let asset = content
                .assets
                .iter()
                .find(|asset| asset.name == p.asset_name)
                .ok_or(WidgetsError::NotFound)?;
            (BundleFile::Asset(&asset.name), asset.size, &asset.sha256)
        };
        let (data, next_offset) =
            self.inner
                .bundles
                .read_chunk(&content.bundle_id, file, p.offset, size)?;
        Ok(ProjectWidgetAssetChunk {
            name: p.asset_name.clone(),
            data,
            next_offset,
            done: next_offset >= size,
            size,
            sha256: sha.clone(),
        })
    }

    /// Copy a chat visual into a new widget of `p.space_id`. `resolve` reads
    /// the visual on this engine and runs only when nothing is saved yet.
    pub fn save_visual(
        &self,
        p: &SaveProjectVisualParams,
        resolve: impl FnOnce() -> Result<SavedVisualSource, WidgetsError>,
    ) -> Result<ProjectWidgetWrite, WidgetsError> {
        check_key("operationId", &p.operation_id)?;
        let title = p.title.as_deref().map(clean_title).transpose()?;
        let fingerprint = fingerprint(&json!({
            "spaceId": p.space_id,
            "sourceChatId": p.source_chat_id,
            "sourceVisualId": p.source_visual_id,
            "title": title,
        }));
        // Replays and already-saved visuals answer without reading the chat.
        let settled = |conn: &mut Connection| -> Result<Option<ProjectWidgetWrite>, WidgetsError> {
            if let Some(response) = store::receipt(conn, &p.operation_id, KIND_SAVE, &fingerprint)?
            {
                return replayed(&response).map(Some);
            }
            let saved = conn
                .query_row(
                    &format!(
                        "SELECT {} FROM widgets WHERE space_id = ?1 AND source_chat_id = ?2
                         AND source_visual_id = ?3 AND deleted = 0",
                        store::WIDGET_COLUMNS
                    ),
                    params![p.space_id, p.source_chat_id, p.source_visual_id],
                    store::widget_from_row,
                )
                .optional()?;
            let Some(saved) = saved else {
                return Ok(None);
            };
            let write = ProjectWidgetWrite {
                outcome: ProjectWidgetWriteOutcome::Existing,
                widget: saved.widget,
            };
            // So a retry of this operation never recreates a widget deleted later.
            let tx = conn.transaction()?;
            store::record_receipt(&tx, &p.operation_id, KIND_SAVE, &fingerprint, &write)?;
            tx.commit()?;
            Ok(Some(write))
        };
        self.owned(&p.space_id)?;
        if let Some(done) = self.write(|conn| {
            let done = settled(conn)?;
            if done.is_none() {
                quota(conn, &p.space_id)?;
            }
            Ok(done)
        })? {
            return Ok(done);
        }
        let visual = resolve()?;
        let title = match title {
            Some(title) => title,
            None => clean_title(&visual.title)?,
        };
        let seed = zeron_visual::seed_for(&visual.source.source_visual_id);
        let content = self
            .inner
            .bundles
            .publish(&visual.script, &visual.assets, seed)?;
        let bundle_id = content.bundle_id.clone();
        let source = visual.source;
        let written = self.write(|conn| {
            if let Some(done) = settled(conn)? {
                return Ok(Err(done));
            }
            quota(conn, &p.space_id)?;
            let now = crate::now_ms();
            let widget = ProjectWidget {
                id: crate::new_id(),
                space_id: p.space_id.clone(),
                owner_device_id: self.inner.device_id.clone(),
                title,
                revision: 1,
                created_at: now,
                updated_at: now,
                last_successful_at: Some(now),
                content_revision: 1,
                content,
                source: Some(source),
                refresh_recipe: None,
                refresh: None,
            };
            let tx = conn.transaction()?;
            insert_widget(&tx, &widget, None)?;
            let write = ProjectWidgetWrite {
                outcome: ProjectWidgetWriteOutcome::Created,
                widget,
            };
            store::record_receipt(&tx, &p.operation_id, KIND_SAVE, &fingerprint, &write)?;
            self.commit(tx)?;
            self.publish(conn, &p.space_id);
            Ok(Ok(write))
        });
        self.settle_new_bundle(&bundle_id, written)
    }

    /// Create a widget from content sent by a client (an agent).
    pub fn create(
        &self,
        p: &CreateProjectWidgetParams,
    ) -> Result<ProjectWidgetWrite, WidgetsError> {
        self.write(|_| Ok(()))?;
        check_key("operationId", &p.operation_id)?;
        check_key("creationKey", &p.creation_key)?;
        let title = clean_title(&p.title)?;
        if let Some(recipe) = &p.refresh_recipe {
            check_recipe(recipe)?;
        }
        if p.content.keep_assets_from_bundle.is_some() {
            return Err(WidgetsError::BadRequest(
                "a new widget has no bundle to keep assets from".into(),
            ));
        }
        let assets = decode_assets(&p.content)?;
        let fingerprint = fingerprint(&json!({
            "spaceId": p.space_id,
            "creationKey": p.creation_key,
            "title": title,
            "content": content_digest(&p.content.script, &assets, None),
            "refreshRecipe": p.refresh_recipe,
        }));
        let settled = |conn: &Connection| -> Result<Option<ProjectWidgetWrite>, WidgetsError> {
            if let Some(response) =
                store::receipt(conn, &p.operation_id, KIND_CREATE, &fingerprint)?
            {
                return replayed(&response).map(Some);
            }
            let created = conn
                .query_row(
                    "SELECT creation_fingerprint, creation_operation_id FROM widgets
                     WHERE space_id = ?1 AND creation_key = ?2",
                    params![p.space_id, p.creation_key],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()?;
            match created {
                None => Ok(None),
                Some((stored, operation)) if stored == fingerprint => {
                    let response: String = conn.query_row(
                        "SELECT response FROM receipts WHERE operation_id = ?1",
                        params![operation],
                        |row| row.get(0),
                    )?;
                    replayed(&response).map(Some)
                }
                Some(_) => Err(WidgetsError::KeyReused),
            }
        };
        self.owned(&p.space_id)?;
        if let Some(done) = self.read(|conn| {
            let done = settled(conn)?;
            if done.is_none() {
                quota(conn, &p.space_id)?;
            }
            Ok(done)
        })? {
            return Ok(done);
        }
        let id = crate::new_id();
        let content =
            self.inner
                .bundles
                .publish(&p.content.script, &assets, zeron_visual::seed_for(&id))?;
        let bundle_id = content.bundle_id.clone();
        let written = self.write(|conn| {
            if let Some(done) = settled(conn)? {
                return Ok(Err(done));
            }
            quota(conn, &p.space_id)?;
            let now = crate::now_ms();
            let widget = ProjectWidget {
                id,
                space_id: p.space_id.clone(),
                owner_device_id: self.inner.device_id.clone(),
                title,
                revision: 1,
                created_at: now,
                updated_at: now,
                last_successful_at: Some(now),
                content_revision: 1,
                content,
                source: None,
                refresh_recipe: p.refresh_recipe.clone(),
                refresh: None,
            };
            let tx = conn.transaction()?;
            insert_widget(
                &tx,
                &widget,
                Some((&p.creation_key, &fingerprint, &p.operation_id)),
            )?;
            let write = ProjectWidgetWrite {
                outcome: ProjectWidgetWriteOutcome::Created,
                widget,
            };
            store::record_receipt(&tx, &p.operation_id, KIND_CREATE, &fingerprint, &write)?;
            self.commit(tx)?;
            self.publish(conn, &p.space_id);
            Ok(Ok(write))
        });
        self.settle_new_bundle(&bundle_id, written)
    }

    /// Change a widget's title, recipe or content against `expectedRevision`.
    pub fn update(
        &self,
        p: &UpdateProjectWidgetParams,
    ) -> Result<ProjectWidgetWrite, WidgetsError> {
        // Refuse a read-only store before reading or copying anything.
        self.write(|_| Ok(()))?;
        check_key("operationId", &p.operation_id)?;
        let title = p.title.as_deref().map(clean_title).transpose()?;
        if let ProjectWidgetRecipePatch::Set(recipe) = &p.refresh_recipe {
            check_recipe(recipe)?;
        }
        let assets = p.content.as_ref().map(decode_assets).transpose()?;
        let fingerprint = fingerprint(&json!({
            "widgetId": p.widget_id,
            "expectedRevision": p.expected_revision,
            "title": title,
            "refreshRecipe": p.refresh_recipe,
            // Only the bundle current at `expectedRevision` can be kept, so the
            // id adds nothing; leaving it out lets a client that re-reads the
            // widget to retry the same operation get its first answer.
            "content": p.content.as_ref().zip(assets.as_ref()).map(|(content, assets)| {
                let keep = content.keep_assets_from_bundle.as_ref().map(|_| "current");
                content_digest(&content.script, assets, keep)
            }),
            "refreshId": p.refresh_id,
        }));
        let current = self.read(|conn| {
            if let Some(response) =
                store::receipt(conn, &p.operation_id, KIND_UPDATE, &fingerprint)?
            {
                return replayed(&response).map(Err);
            }
            live(store::load_widget(conn, &p.widget_id)?).map(Ok)
        })?;
        let current = match current {
            Ok(widget) => widget,
            Err(replay) => return Ok(replay),
        };
        self.owned(&current.space_id)?;
        if current.revision != p.expected_revision {
            return Err(WidgetsError::Conflict);
        }
        if p.refresh_id.is_some() {
            // Refresh seam: a refresh run's update is staged as a candidate
            // of its active attempt, never published here. No attempt can be
            // active until refreshes are served.
            return Err(WidgetsError::RefreshInactive);
        }
        let new_content = match (&p.content, assets) {
            (Some(input), Some(mut assets)) => {
                if let Some(keep) = &input.keep_assets_from_bundle {
                    if *keep != current.content.bundle_id {
                        return Err(WidgetsError::Conflict);
                    }
                    for (name, bytes) in self.inner.bundles.read_assets(&current.content)? {
                        assets.entry(name).or_insert(bytes);
                    }
                }
                // The widget keeps its seed: the same random data across edits.
                Some(self.inner.bundles.publish(
                    &input.script,
                    &assets,
                    current.content.render_seed,
                )?)
            }
            _ => None,
        };
        let new_bundle = new_content.as_ref().map(|c| c.bundle_id.clone());
        let title = title.unwrap_or_else(|| current.title.clone());
        let recipe = match &p.refresh_recipe {
            ProjectWidgetRecipePatch::Unchanged => current.refresh_recipe.clone(),
            ProjectWidgetRecipePatch::Set(recipe) => Some(recipe.clone()),
            ProjectWidgetRecipePatch::Clear => None,
        };
        let changed =
            new_content.is_some() || title != current.title || recipe != current.refresh_recipe;
        let written = self.write(|conn| {
            if let Some(response) =
                store::receipt(conn, &p.operation_id, KIND_UPDATE, &fingerprint)?
            {
                return Ok(Err(replayed(&response)?));
            }
            let tx = conn.transaction()?;
            if changed {
                let now = crate::now_ms();
                let (content, content_revision, validated_at) = match &new_content {
                    Some(content) => (content, current.content_revision + 1, Some(now)),
                    None => (
                        &current.content,
                        current.content_revision,
                        current.last_successful_at,
                    ),
                };
                let updated = tx.execute(
                    "UPDATE widgets SET title = ?3, refresh_recipe = ?4, content = ?5,
                         bundle_id = ?6, content_revision = ?7, last_successful_at = ?8,
                         revision = revision + 1, updated_at = ?9
                     WHERE id = ?1 AND revision = ?2 AND deleted = 0",
                    params![
                        p.widget_id,
                        p.expected_revision as i64,
                        title,
                        recipe.as_ref().map(to_json).transpose()?,
                        to_json(content)?,
                        content.bundle_id,
                        content_revision as i64,
                        validated_at,
                        now,
                    ],
                )?;
                if updated == 0 {
                    return Err(match store::load_widget(&tx, &p.widget_id)? {
                        Some(stored) if !stored.deleted => WidgetsError::Conflict,
                        _ => WidgetsError::NotFound,
                    });
                }
            }
            let widget = live(store::load_widget(&tx, &p.widget_id)?)?;
            let write = ProjectWidgetWrite {
                outcome: ProjectWidgetWriteOutcome::Updated,
                widget,
            };
            store::record_receipt(&tx, &p.operation_id, KIND_UPDATE, &fingerprint, &write)?;
            self.commit(tx)?;
            self.publish(conn, &current.space_id);
            Ok(Ok(write))
        });
        match new_bundle {
            Some(bundle_id) => {
                let written = self.settle_new_bundle(&bundle_id, written);
                if written.is_ok() {
                    // The previous bundle is no longer referenced.
                    self.collect_garbage_older_than(GC_GRACE);
                }
                written
            }
            None => Ok(match written? {
                Ok(write) | Err(write) => write,
            }),
        }
    }

    /// Put the space's live widgets in the given order.
    pub fn reorder(
        &self,
        p: &ReorderProjectWidgetsParams,
    ) -> Result<ProjectWidgetsSnapshot, WidgetsError> {
        check_key("operationId", &p.operation_id)?;
        let fingerprint = fingerprint(&json!({
            "spaceId": p.space_id,
            "widgetIds": p.widget_ids,
            "expectedCollectionRevision": p.expected_collection_revision,
        }));
        self.owned(&p.space_id)?;
        self.write(|conn| {
            if let Some(response) = store::receipt(conn, &p.operation_id, KIND_REORDER, &fingerprint)? {
                return serde_json::from_str(&response)
                    .map_err(|err| WidgetsError::Write(err.to_string()));
            }
            if store::collection_revision(conn, &p.space_id)? != p.expected_collection_revision {
                return Err(WidgetsError::Conflict);
            }
            let live: HashSet<String> = store::live_widgets(conn, &p.space_id)?
                .into_iter()
                .map(|widget| widget.id)
                .collect();
            let requested: HashSet<&String> = p.widget_ids.iter().collect();
            if requested.len() != p.widget_ids.len()
                || requested.len() != live.len()
                || !requested.iter().all(|id| live.contains(*id))
            {
                return Err(WidgetsError::BadRequest(
                    "widgetIds must list every widget of the project exactly once".into(),
                ));
            }
            let tx = conn.transaction()?;
            tx.execute(
                "DELETE FROM collection_items WHERE space_id = ?1",
                params![p.space_id],
            )?;
            for (position, id) in p.widget_ids.iter().enumerate() {
                tx.execute(
                    "INSERT INTO collection_items (space_id, position, widget_id) VALUES (?1, ?2, ?3)",
                    params![p.space_id, position as i64, id],
                )?;
            }
            store::bump_collection(&tx, &p.space_id)?;
            let snapshot = self.snapshot_of(&tx, &p.space_id)?;
            store::record_receipt(&tx, &p.operation_id, KIND_REORDER, &fingerprint, &snapshot)?;
            self.commit(tx)?;
            self.publish(conn, &p.space_id);
            Ok(snapshot)
        })
    }

    /// Tombstone a widget against `expectedRevision`. Deleting a deleted
    /// widget succeeds with `alreadyDeleted`.
    pub fn delete(
        &self,
        p: &DeleteProjectWidgetParams,
    ) -> Result<ProjectWidgetDeletion, WidgetsError> {
        check_key("operationId", &p.operation_id)?;
        let fingerprint = fingerprint(&json!({
            "widgetId": p.widget_id,
            "expectedRevision": p.expected_revision,
        }));
        let deletion = self.write(|conn| {
            if let Some(response) =
                store::receipt(conn, &p.operation_id, KIND_DELETE, &fingerprint)?
            {
                return serde_json::from_str(&response)
                    .map_err(|err| WidgetsError::Write(err.to_string()));
            }
            let stored = store::load_widget(conn, &p.widget_id)?.ok_or(WidgetsError::NotFound)?;
            let space_id = stored.widget.space_id.clone();
            self.owned(&space_id)?;
            if stored.deleted {
                return Ok(ProjectWidgetDeletion {
                    widget_id: p.widget_id.clone(),
                    collection_revision: store::collection_revision(conn, &space_id)?,
                    space_id,
                    revision: stored.widget.revision,
                    already_deleted: true,
                });
            }
            if stored.widget.revision != p.expected_revision {
                return Err(WidgetsError::Conflict);
            }
            let tx = conn.transaction()?;
            let (revision, invalidated) = tombstone(&tx, &stored)?;
            let deletion = ProjectWidgetDeletion {
                widget_id: p.widget_id.clone(),
                collection_revision: store::bump_collection(&tx, &space_id)?,
                space_id: space_id.clone(),
                revision,
                already_deleted: false,
            };
            store::record_receipt(&tx, &p.operation_id, KIND_DELETE, &fingerprint, &deletion)?;
            self.commit(tx)?;
            self.publish(conn, &space_id);
            stop_refreshes(&invalidated);
            Ok(deletion)
        })?;
        self.collect_garbage_older_than(GC_GRACE);
        Ok(deletion)
    }

    /// Delete every widget of spaces the registry deleted. Returns how many.
    pub fn reconcile_deleted_spaces(&self) -> Result<usize, WidgetsError> {
        let spaces: Vec<String> = self.read(|conn| {
            let mut statement =
                conn.prepare("SELECT DISTINCT space_id FROM widgets WHERE deleted = 0")?;
            let spaces = statement
                .query_map([], |row| row.get(0))?
                .collect::<Result<_, _>>()?;
            Ok(spaces)
        })?;
        let mut removed = 0;
        for space_id in spaces {
            if !self.inner.spaces.deleted(&space_id) {
                continue;
            }
            let count = self.write(|conn| {
                let stored: Vec<StoredWidget> = {
                    let mut statement = conn.prepare(&format!(
                        "SELECT {} FROM widgets WHERE space_id = ?1 AND deleted = 0",
                        store::WIDGET_COLUMNS
                    ))?;
                    statement
                        .query_map(params![space_id], store::widget_from_row)?
                        .collect::<Result<_, _>>()?
                };
                let tx = conn.transaction()?;
                let mut invalidated = Vec::new();
                for widget in &stored {
                    invalidated.extend(tombstone(&tx, widget)?.1);
                }
                store::bump_collection(&tx, &space_id)?;
                self.commit(tx)?;
                self.publish(conn, &space_id);
                stop_refreshes(&invalidated);
                Ok(stored.len())
            })?;
            tracing::info!(space = %space_id, widgets = count, "deleted space's widgets removed");
            removed += count;
        }
        if removed > 0 {
            self.collect_garbage_older_than(GC_GRACE);
        }
        Ok(removed)
    }

    /// Remove bundles no live widget or active refresh references, once
    /// older than `grace`. Uploads and transcripts are never touched.
    fn collect_garbage_older_than(&self, grace: Duration) -> usize {
        let referenced = self.read(|conn| {
            let mut statement = conn.prepare(&format!(
                "SELECT bundle_id FROM widgets WHERE deleted = 0
                 UNION SELECT candidate_bundle_id FROM refresh_attempts
                 WHERE candidate_bundle_id IS NOT NULL AND state IN {}",
                store::ACTIVE_ATTEMPT_STATES
            ))?;
            let ids = statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<HashSet<_>, _>>()?;
            Ok(ids)
        });
        match referenced {
            // Never collect against a partial view of the references.
            Ok(referenced) => self.inner.bundles.collect(&referenced, grace),
            Err(_) => 0,
        }
    }

    // ── internals ───────────────────────────────────────────────────────────

    fn owned(&self, space_id: &str) -> Result<(), WidgetsError> {
        if self.inner.spaces.owned(space_id) {
            Ok(())
        } else {
            Err(WidgetsError::OwnerUnavailable)
        }
    }

    fn read<T>(
        &self,
        f: impl FnOnce(&Connection) -> Result<T, WidgetsError>,
    ) -> Result<T, WidgetsError> {
        let db = lock(&self.inner.db);
        let db = db.as_ref().ok_or(WidgetsError::StoreUnavailable)?;
        f(&db.conn)
    }

    fn write<T>(
        &self,
        f: impl FnOnce(&mut Connection) -> Result<T, WidgetsError>,
    ) -> Result<T, WidgetsError> {
        let mut db = lock(&self.inner.db);
        let db = db.as_mut().ok_or(WidgetsError::StoreUnavailable)?;
        if !db.writable {
            return Err(WidgetsError::StoreUnavailable);
        }
        f(&mut db.conn)
    }

    fn commit(&self, tx: Transaction<'_>) -> Result<(), WidgetsError> {
        #[cfg(test)]
        if self
            .inner
            .fail_next_commit
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            // Dropping the transaction rolls it back, like a failed commit.
            return Err(WidgetsError::Write("injected commit failure".into()));
        }
        tx.commit()?;
        Ok(())
    }

    /// A freshly published bundle the transaction did not adopt (a raced
    /// replay or a failure) is not referenced by anything: drop it now.
    fn settle_new_bundle(
        &self,
        bundle_id: &str,
        written: Result<Result<ProjectWidgetWrite, ProjectWidgetWrite>, WidgetsError>,
    ) -> Result<ProjectWidgetWrite, WidgetsError> {
        match written {
            Ok(Ok(write)) => Ok(write),
            Ok(Err(earlier)) => {
                self.inner.bundles.discard(bundle_id);
                Ok(earlier)
            }
            Err(err) => {
                self.inner.bundles.discard(bundle_id);
                Err(err)
            }
        }
    }

    fn snapshot_of(
        &self,
        conn: &Connection,
        space_id: &str,
    ) -> Result<ProjectWidgetsSnapshot, WidgetsError> {
        Ok(ProjectWidgetsSnapshot {
            space_id: space_id.to_owned(),
            owner_device_id: self.inner.device_id.clone(),
            collection_revision: store::collection_revision(conn, space_id)?,
            widgets: store::live_widgets(conn, space_id)?,
        })
    }

    /// Tell the space's watchers. Called under the store lock right after a
    /// commit, so snapshots are published in commit order.
    fn publish(&self, conn: &Connection, space_id: &str) {
        let mut watchers = lock(&self.inner.watchers);
        let Some(sender) = watchers.get(space_id) else {
            return;
        };
        if sender.receiver_count() == 0 {
            watchers.remove(space_id);
            return;
        }
        match self.snapshot_of(conn, space_id) {
            Ok(snapshot) => {
                sender.send_replace(snapshot);
            }
            Err(err) => tracing::warn!(error = %err, "project widgets snapshot unavailable"),
        }
    }
}

/// Find a `MessagePart::Visual` in a chat this engine hosts and read its
/// files from the profile's uploads (never a path sent by a client).
pub fn resolve_chat_visual(
    doc_host: &crate::DocHost,
    workspace: &crate::WorkspaceHost,
    uploads: &crate::Uploads,
    chat_id: &str,
    visual_id: &str,
) -> Result<SavedVisualSource, WidgetsError> {
    let chat = workspace
        .chat(chat_id)
        .ok()
        .flatten()
        .ok_or(WidgetsError::SourceNotFound)?;
    if chat.device_id != doc_host.device_id() {
        // Its files live on its host; save from there.
        return Err(WidgetsError::OwnerUnavailable);
    }
    let entries = doc_host
        .open(chat_id)
        .and_then(|handle| Ok(handle.doc().read_entries()?))
        .map_err(|_| WidgetsError::SourceNotFound)?;
    let (entry_id, path, title) = entries
        .iter()
        .find_map(|entry| {
            entry.parts.iter().find_map(|part| match part {
                MessagePart::Visual { id, path, title } if id == visual_id => {
                    Some((entry.id.clone(), path.clone(), title.clone()))
                }
                _ => None,
            })
        })
        .ok_or(WidgetsError::SourceNotFound)?;
    let stored = uploads.read_visual(&path).map_err(|err| {
        WidgetsError::InvalidVisual(format!("the visual cannot be copied: {err}"))
    })?;
    Ok(SavedVisualSource {
        source: ProjectWidgetSource {
            source_chat_id: chat_id.to_owned(),
            source_visual_id: visual_id.to_owned(),
            source_entry_id: Some(entry_id),
            source_title: chat.title,
            source_device_id: chat.device_id,
        },
        title,
        script: stored.script,
        assets: stored.assets,
    })
}

/// Refresh seam: stop the agent runs of refresh attempts a deletion made
/// unpublishable. The attempts are already invalidated in the store; until
/// refreshes are served none can exist.
fn stop_refreshes(refresh_ids: &[String]) {
    if !refresh_ids.is_empty() {
        tracing::info!(count = refresh_ids.len(), "widget refreshes invalidated");
    }
}

fn tombstone(
    tx: &Transaction<'_>,
    stored: &StoredWidget,
) -> Result<(u64, Vec<String>), WidgetsError> {
    let now = crate::now_ms();
    let id = &stored.widget.id;
    tx.execute(
        "UPDATE widgets SET deleted = 1, deleted_at = ?2, updated_at = ?2, revision = revision + 1
         WHERE id = ?1 AND deleted = 0",
        params![id, now],
    )?;
    tx.execute(
        "DELETE FROM collection_items WHERE widget_id = ?1",
        params![id],
    )?;
    let invalidated = store::invalidate_refreshes(tx, id, "cancelled")?;
    Ok((stored.widget.revision + 1, invalidated))
}

fn insert_widget(
    tx: &Transaction<'_>,
    widget: &ProjectWidget,
    creation: Option<(&str, &str, &str)>,
) -> Result<(), WidgetsError> {
    let (creation_key, creation_fingerprint, creation_operation) = match creation {
        Some((key, fingerprint, operation)) => (Some(key), Some(fingerprint), Some(operation)),
        None => (None, None, None),
    };
    tx.execute(
        "INSERT INTO widgets (id, space_id, owner_device_id, title, revision, created_at,
             updated_at, last_successful_at, content_revision, bundle_id, content, source,
             source_chat_id, source_visual_id, creation_key, creation_fingerprint,
             creation_operation_id, refresh_recipe)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
        params![
            widget.id,
            widget.space_id,
            widget.owner_device_id,
            widget.title,
            widget.revision as i64,
            widget.created_at,
            widget.updated_at,
            widget.last_successful_at,
            widget.content_revision as i64,
            widget.content.bundle_id,
            to_json(&widget.content)?,
            widget.source.as_ref().map(to_json).transpose()?,
            widget.source.as_ref().map(|s| s.source_chat_id.as_str()),
            widget.source.as_ref().map(|s| s.source_visual_id.as_str()),
            creation_key,
            creation_fingerprint,
            creation_operation,
            widget.refresh_recipe.as_ref().map(to_json).transpose()?,
        ],
    )?;
    tx.execute(
        "INSERT INTO collection_items (space_id, position, widget_id)
         SELECT ?1, COALESCE(MAX(position) + 1, 0), ?2 FROM collection_items WHERE space_id = ?1",
        params![widget.space_id, widget.id],
    )?;
    store::bump_collection(tx, &widget.space_id)?;
    Ok(())
}

fn quota(conn: &Connection, space_id: &str) -> Result<(), WidgetsError> {
    let live: i64 = conn.query_row(
        "SELECT COUNT(*) FROM widgets WHERE space_id = ?1 AND deleted = 0",
        params![space_id],
        |row| row.get(0),
    )?;
    if live as usize >= PROJECT_WIDGETS_MAX_PER_SPACE {
        return Err(WidgetsError::QuotaExceeded);
    }
    Ok(())
}

fn live(stored: Option<StoredWidget>) -> Result<ProjectWidget, WidgetsError> {
    match stored {
        Some(stored) if !stored.deleted => Ok(stored.widget),
        _ => Err(WidgetsError::NotFound),
    }
}

fn replayed(response: &str) -> Result<ProjectWidgetWrite, WidgetsError> {
    let mut write: ProjectWidgetWrite =
        serde_json::from_str(response).map_err(|err| WidgetsError::Write(err.to_string()))?;
    write.outcome = ProjectWidgetWriteOutcome::Replayed;
    Ok(write)
}

fn decode_assets(
    input: &ProjectWidgetContentInput,
) -> Result<BTreeMap<String, Vec<u8>>, WidgetsError> {
    if input.assets.len() > VISUAL_MAX_ASSETS {
        return Err(WidgetsError::InvalidVisual(format!(
            "a visual takes at most {VISUAL_MAX_ASSETS} assets"
        )));
    }
    let mut out = BTreeMap::new();
    for (name, data) in &input.assets {
        if !valid_visual_asset_name(name) {
            return Err(WidgetsError::InvalidVisual(format!(
                "asset name '{name}' must be a plain file name"
            )));
        }
        // Refuse before decoding what could only be too large.
        if data.len() > VISUAL_MAX_ASSET_BYTES.div_ceil(3) * 4 + 4 {
            return Err(WidgetsError::InvalidVisual(format!(
                "asset '{name}' is too large"
            )));
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|err| {
                WidgetsError::InvalidVisual(format!("asset '{name}' is not base64: {err}"))
            })?;
        out.insert(name.clone(), bytes);
    }
    Ok(out)
}

fn content_digest(
    script: &str,
    assets: &BTreeMap<String, Vec<u8>>,
    keep_from: Option<&str>,
) -> serde_json::Value {
    json!({
        "script": sha256(script.as_bytes()),
        "assets": assets.iter().map(|(name, bytes)| (name.clone(), sha256(bytes))).collect::<BTreeMap<_, _>>(),
        "keepAssetsFromBundle": keep_from,
    })
}

fn fingerprint(payload: &serde_json::Value) -> String {
    sha256(&serde_json::to_vec(payload).unwrap_or_default())
}

fn to_json(value: &impl serde::Serialize) -> Result<String, WidgetsError> {
    serde_json::to_string(value).map_err(|err| WidgetsError::Write(err.to_string()))
}

fn check_key(field: &str, value: &str) -> Result<(), WidgetsError> {
    if value.trim().is_empty() || value.len() > MAX_KEY_BYTES {
        return Err(WidgetsError::BadRequest(format!(
            "{field} must be 1 to {MAX_KEY_BYTES} bytes"
        )));
    }
    Ok(())
}

/// Trimmed and bounded like a published visual's title.
fn clean_title(title: &str) -> Result<String, WidgetsError> {
    let title: String = title.trim().chars().take(VISUAL_MAX_TITLE_CHARS).collect();
    if title.is_empty() {
        return Err(WidgetsError::BadRequest("title is empty".into()));
    }
    Ok(title)
}

fn check_recipe(recipe: &ProjectWidgetRecipe) -> Result<(), WidgetsError> {
    if recipe.instruction.trim().is_empty()
        || recipe.instruction.len() > PROJECT_WIDGET_INSTRUCTION_MAX_BYTES
    {
        return Err(WidgetsError::BadRequest(format!(
            "the refresh instruction must be 1 to {PROJECT_WIDGET_INSTRUCTION_MAX_BYTES} bytes"
        )));
    }
    Ok(())
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests;

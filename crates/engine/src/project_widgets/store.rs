//! `project-widgets.sqlite3`: widgets (tombstones kept), per-space order,
//! refresh attempts and operation receipts. Append-only migrations, each in
//! its own transaction; a file from a newer build is opened read-only.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use zeron_proto::{ProjectWidget, ProjectWidgetRecipe};

use super::WidgetsError;

/// Applied in order; `user_version` counts the applied ones. Never edit a
/// shipped entry, append a new one.
const MIGRATIONS: &[&str] = &[
    // 1: widgets, collections, refresh attempts, receipts.
    "CREATE TABLE widgets (
         id TEXT PRIMARY KEY,
         space_id TEXT NOT NULL,
         owner_device_id TEXT NOT NULL,
         title TEXT NOT NULL,
         revision INTEGER NOT NULL,
         created_at INTEGER NOT NULL,
         updated_at INTEGER NOT NULL,
         last_successful_at INTEGER,
         content_revision INTEGER NOT NULL,
         bundle_id TEXT NOT NULL,
         content TEXT NOT NULL,
         source TEXT,
         source_chat_id TEXT,
         source_visual_id TEXT,
         creation_key TEXT,
         creation_fingerprint TEXT,
         creation_operation_id TEXT,
         refresh_recipe TEXT,
         deleted INTEGER NOT NULL DEFAULT 0,
         deleted_at INTEGER
     );
     CREATE INDEX widgets_by_space ON widgets (space_id, deleted);
     -- A visual is saved at most once per space while that widget lives.
     CREATE UNIQUE INDEX widgets_live_source
         ON widgets (space_id, source_chat_id, source_visual_id)
         WHERE deleted = 0 AND source_visual_id IS NOT NULL;
     -- A creation key is never reused in its space, even after deletion.
     CREATE UNIQUE INDEX widgets_creation_key
         ON widgets (space_id, creation_key) WHERE creation_key IS NOT NULL;
     CREATE TABLE collections (
         space_id TEXT PRIMARY KEY,
         revision INTEGER NOT NULL
     );
     CREATE TABLE collection_items (
         space_id TEXT NOT NULL,
         position INTEGER NOT NULL,
         widget_id TEXT NOT NULL UNIQUE,
         PRIMARY KEY (space_id, position)
     );
     CREATE TABLE refresh_attempts (
         refresh_id TEXT PRIMARY KEY,
         widget_id TEXT NOT NULL,
         request_id TEXT NOT NULL,
         base_revision INTEGER NOT NULL,
         attempt_revision INTEGER NOT NULL DEFAULT 0,
         state TEXT NOT NULL,
         command_id TEXT,
         execution_chat_id TEXT,
         turn_id TEXT,
         candidate_bundle_id TEXT,
         error_code TEXT,
         error_message TEXT,
         created_at INTEGER NOT NULL,
         updated_at INTEGER NOT NULL,
         finished_at INTEGER
     );
     CREATE UNIQUE INDEX refresh_attempts_active ON refresh_attempts (widget_id)
         WHERE state IN ('queued', 'running', 'awaitingInput');
     CREATE UNIQUE INDEX refresh_attempts_request ON refresh_attempts (widget_id, request_id);
     CREATE TABLE receipts (
         operation_id TEXT PRIMARY KEY,
         kind TEXT NOT NULL,
         fingerprint TEXT NOT NULL,
         response TEXT NOT NULL,
         created_at INTEGER NOT NULL
     );",
];

/// Attempt states that may still publish.
pub(super) const ACTIVE_ATTEMPT_STATES: &str = "('queued', 'running', 'awaitingInput')";

pub(super) const WIDGET_COLUMNS: &str = "id, space_id, owner_device_id, title, revision, \
     created_at, updated_at, last_successful_at, content_revision, content, source, \
     refresh_recipe, deleted";

/// Open and migrate. `Ok((conn, false))` for a newer schema: readable, never written.
pub(super) fn open(path: &Path) -> rusqlite::Result<(Connection, bool)> {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut conn = Connection::open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    let known = MIGRATIONS.len() as i64;
    if version > known {
        tracing::error!(
            version,
            known,
            "project widgets store is from a newer build; serving it read-only"
        );
        return Ok((conn, false));
    }
    for (index, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", index as i64 + 1)?;
        tx.commit()?;
    }
    Ok((conn, true))
}

pub(super) struct StoredWidget {
    pub widget: ProjectWidget,
    pub deleted: bool,
}

pub(super) fn widget_from_row(row: &Row<'_>) -> rusqlite::Result<StoredWidget> {
    fn json<T: serde::de::DeserializeOwned>(index: usize, text: String) -> rusqlite::Result<T> {
        serde_json::from_str(&text).map_err(|err| {
            rusqlite::Error::FromSqlConversionFailure(
                index,
                rusqlite::types::Type::Text,
                err.into(),
            )
        })
    }
    let content = json(9, row.get(9)?)?;
    let source = row
        .get::<_, Option<String>>(10)?
        .map(|text| json(10, text))
        .transpose()?;
    // A recipe this build cannot read (a newer harness) is shown as none
    // rather than hiding the widget.
    let refresh_recipe = row
        .get::<_, Option<String>>(11)?
        .and_then(|text| serde_json::from_str::<ProjectWidgetRecipe>(&text).ok());
    Ok(StoredWidget {
        widget: ProjectWidget {
            id: row.get(0)?,
            space_id: row.get(1)?,
            owner_device_id: row.get(2)?,
            title: row.get(3)?,
            revision: row.get::<_, i64>(4)? as u64,
            created_at: row.get(5)?,
            updated_at: row.get(6)?,
            last_successful_at: row.get(7)?,
            content_revision: row.get::<_, i64>(8)? as u64,
            content,
            source,
            refresh_recipe,
            // Refresh state arrives with refresh support.
            refresh: None,
        },
        deleted: row.get::<_, i64>(12)? != 0,
    })
}

pub(super) fn load_widget(
    conn: &Connection,
    widget_id: &str,
) -> Result<Option<StoredWidget>, WidgetsError> {
    Ok(conn
        .query_row(
            &format!("SELECT {WIDGET_COLUMNS} FROM widgets WHERE id = ?1"),
            params![widget_id],
            widget_from_row,
        )
        .optional()?)
}

/// The space's live widgets in their collection order.
pub(super) fn live_widgets(
    conn: &Connection,
    space_id: &str,
) -> Result<Vec<ProjectWidget>, WidgetsError> {
    let columns = WIDGET_COLUMNS
        .split(", ")
        .map(|column| format!("w.{column}"))
        .collect::<Vec<_>>()
        .join(", ");
    let mut statement = conn.prepare(&format!(
        "SELECT {columns} FROM collection_items c JOIN widgets w ON w.id = c.widget_id
         WHERE c.space_id = ?1 AND w.deleted = 0 ORDER BY c.position"
    ))?;
    let rows = statement.query_map(params![space_id], widget_from_row)?;
    Ok(rows
        .map(|row| row.map(|stored| stored.widget))
        .collect::<Result<_, _>>()?)
}

pub(super) fn collection_revision(conn: &Connection, space_id: &str) -> Result<u64, WidgetsError> {
    Ok(conn
        .query_row(
            "SELECT revision FROM collections WHERE space_id = ?1",
            params![space_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .unwrap_or(0) as u64)
}

pub(super) fn bump_collection(tx: &Transaction<'_>, space_id: &str) -> Result<u64, WidgetsError> {
    Ok(tx.query_row(
        "INSERT INTO collections (space_id, revision) VALUES (?1, 1)
         ON CONFLICT (space_id) DO UPDATE SET revision = revision + 1
         RETURNING revision",
        params![space_id],
        |row| row.get::<_, i64>(0),
    )? as u64)
}

/// A stored answer for `operation_id`. `Ok(None)`: not used yet. Used for
/// another kind or payload: `KeyReused`.
pub(super) fn receipt(
    conn: &Connection,
    operation_id: &str,
    kind: &str,
    fingerprint: &str,
) -> Result<Option<String>, WidgetsError> {
    let stored = conn
        .query_row(
            "SELECT kind, fingerprint, response FROM receipts WHERE operation_id = ?1",
            params![operation_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?;
    match stored {
        None => Ok(None),
        Some((k, f, response)) if k == kind && f == fingerprint => Ok(Some(response)),
        Some(_) => Err(WidgetsError::KeyReused),
    }
}

pub(super) fn record_receipt(
    tx: &Transaction<'_>,
    operation_id: &str,
    kind: &str,
    fingerprint: &str,
    response: &impl serde::Serialize,
) -> Result<(), WidgetsError> {
    let response =
        serde_json::to_string(response).map_err(|err| WidgetsError::Write(err.to_string()))?;
    tx.execute(
        "INSERT INTO receipts (operation_id, kind, fingerprint, response, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![operation_id, kind, fingerprint, response, crate::now_ms()],
    )?;
    Ok(())
}

/// Make the widget's unfinished refresh attempts unable to publish. Returns
/// their ids so their runs can be stopped.
pub(super) fn invalidate_refreshes(
    tx: &Transaction<'_>,
    widget_id: &str,
    state: &str,
) -> Result<Vec<String>, WidgetsError> {
    let now = crate::now_ms();
    let mut statement = tx.prepare(&format!(
        "UPDATE refresh_attempts SET state = ?2, updated_at = ?3, finished_at = ?3
         WHERE widget_id = ?1 AND state IN {ACTIVE_ATTEMPT_STATES} RETURNING refresh_id"
    ))?;
    let ids = statement
        .query_map(params![widget_id, state, now], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(ids)
}

/// Engine restart: no attempt of the previous run may still publish.
pub(super) fn interrupt_unfinished_refreshes(conn: &Connection) -> Result<usize, WidgetsError> {
    let now = crate::now_ms();
    Ok(conn.execute(
        &format!(
            "UPDATE refresh_attempts SET state = 'interrupted', updated_at = ?1, finished_at = ?1
             WHERE state IN {ACTIVE_ATTEMPT_STATES}"
        ),
        params![now],
    )?)
}

impl From<rusqlite::Error> for WidgetsError {
    fn from(err: rusqlite::Error) -> Self {
        WidgetsError::Write(err.to_string())
    }
}

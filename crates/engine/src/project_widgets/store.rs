//! `project-widgets.sqlite3`: widgets (tombstones kept), per-space order,
//! refresh attempts and operation receipts. Append-only migrations, each in
//! its own transaction; a file from a newer build is opened read-only.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use zeron_proto::{
    ProjectWidget, ProjectWidgetRecipe, ProjectWidgetRefresh, ProjectWidgetRefreshState,
};

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
    // 2: refresh runs. Each widget's dedicated refresh chat, and the content
    // manifest of an attempt's candidate bundle.
    "ALTER TABLE widgets ADD COLUMN refresh_chat_id TEXT;
     ALTER TABLE refresh_attempts ADD COLUMN candidate_content TEXT;",
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
            // Filled from `refresh_attempts` by the loaders below.
            refresh: None,
        },
        deleted: row.get::<_, i64>(12)? != 0,
    })
}

pub(super) fn load_widget(
    conn: &Connection,
    widget_id: &str,
) -> Result<Option<StoredWidget>, WidgetsError> {
    let stored = conn
        .query_row(
            &format!("SELECT {WIDGET_COLUMNS} FROM widgets WHERE id = ?1"),
            params![widget_id],
            widget_from_row,
        )
        .optional()?;
    stored
        .map(|mut stored| {
            stored.widget.refresh = latest_attempt(conn, widget_id)?.map(|a| a.refresh());
            Ok(stored)
        })
        .transpose()
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
    let mut widgets: Vec<ProjectWidget> = rows
        .map(|row| row.map(|stored| stored.widget))
        .collect::<Result<_, _>>()?;
    for widget in &mut widgets {
        widget.refresh = latest_attempt(conn, &widget.id)?.map(|a| a.refresh());
    }
    Ok(widgets)
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

/// Make the widget's unfinished refresh attempts unable to publish (a
/// deletion). Returns their ids so their runs can be stopped.
pub(super) fn invalidate_refreshes(
    tx: &Transaction<'_>,
    widget_id: &str,
) -> Result<Vec<String>, WidgetsError> {
    let now = crate::now_ms();
    let mut statement = tx.prepare(&format!(
        "UPDATE refresh_attempts SET state = 'cancelled', error_code = 'deleted',
             error_message = ?2, updated_at = ?3, finished_at = ?3
         WHERE widget_id = ?1 AND state IN {ACTIVE_ATTEMPT_STATES} RETURNING refresh_id"
    ))?;
    let ids = statement
        .query_map(params![widget_id, error_message("deleted"), now], |row| {
            row.get(0)
        })?
        .collect::<Result<_, _>>()?;
    Ok(ids)
}

/// Engine restart: no attempt of the previous run may still publish.
/// Returns the `(executionChatId, commandId)` of each, to withdraw a command
/// that never ran.
pub(super) fn interrupt_unfinished_refreshes(
    conn: &Connection,
) -> Result<Vec<(String, String)>, WidgetsError> {
    let now = crate::now_ms();
    let mut statement = conn.prepare(&format!(
        "UPDATE refresh_attempts SET state = 'interrupted', error_code = 'engine_restarted',
             error_message = ?2, updated_at = ?1, finished_at = ?1
         WHERE state IN {ACTIVE_ATTEMPT_STATES}
         RETURNING execution_chat_id, command_id"
    ))?;
    let rows = statement
        .query_map(params![now, error_message("engine_restarted")], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows
        .into_iter()
        .filter_map(|(chat, command)| chat.zip(command))
        .collect())
}

/// One refresh attempt as stored.
#[derive(Debug, Clone)]
pub(super) struct Attempt {
    pub refresh_id: String,
    pub widget_id: String,
    pub base_revision: u64,
    pub state: ProjectWidgetRefreshState,
    pub command_id: String,
    pub execution_chat_id: String,
    pub candidate_content: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub created_at: i64,
    pub finished_at: Option<i64>,
}

impl Attempt {
    pub fn active(&self) -> bool {
        is_active(self.state)
    }

    pub fn refresh(&self) -> ProjectWidgetRefresh {
        ProjectWidgetRefresh {
            refresh_id: self.refresh_id.clone(),
            state: self.state,
            started_at: self.created_at,
            finished_at: self.finished_at,
            execution_chat_id: Some(self.execution_chat_id.clone()),
            error_code: self.error_code.clone(),
            error_message: self.error_message.clone(),
        }
    }
}

pub(super) fn is_active(state: ProjectWidgetRefreshState) -> bool {
    use ProjectWidgetRefreshState::*;
    matches!(state, Queued | Running | AwaitingInput)
}

pub(super) fn state_name(state: ProjectWidgetRefreshState) -> &'static str {
    use ProjectWidgetRefreshState::*;
    match state {
        Queued => "queued",
        Running => "running",
        AwaitingInput => "awaitingInput",
        Succeeded => "succeeded",
        Failed => "failed",
        Cancelled => "cancelled",
        Interrupted => "interrupted",
        Conflict => "conflict",
        Other => "other",
    }
}

/// The sentence shown for a stored error code; never the agent's own text.
pub(super) fn error_message(code: &str) -> &'static str {
    match code {
        "no_result" => "The agent finished without sending a new version of the widget.",
        "agent_error" => "The agent stopped with an error. Open the run to see what happened.",
        "timeout" => "The refresh took more than 15 minutes and was stopped.",
        "cancelled" => "Refresh cancelled.",
        "deleted" => "The widget was deleted.",
        "conflict" => {
            "The widget changed during the refresh. Refresh again to use the latest version."
        }
        "run_stopped" => "The run was stopped before it finished.",
        "engine_restarted" => "Zeron restarted during the refresh.",
        "chat_deleted" => "The refresh chat was deleted.",
        "chat_busy" => "The refresh chat is busy with another message.",
        _ => "The agent run could not be started.",
    }
}

const ATTEMPT_COLUMNS: &str = "refresh_id, widget_id, base_revision, state, command_id, \
     execution_chat_id, candidate_content, error_code, error_message, created_at, finished_at";

fn attempt_from_row(row: &Row<'_>) -> rusqlite::Result<Attempt> {
    let state: String = row.get(3)?;
    Ok(Attempt {
        refresh_id: row.get(0)?,
        widget_id: row.get(1)?,
        base_revision: row.get::<_, i64>(2)? as u64,
        state: serde_json::from_value(serde_json::Value::String(state))
            .unwrap_or(ProjectWidgetRefreshState::Other),
        command_id: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
        execution_chat_id: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
        candidate_content: row.get(6)?,
        error_code: row.get(7)?,
        error_message: row.get(8)?,
        created_at: row.get(9)?,
        finished_at: row.get(10)?,
    })
}

fn attempt_where(
    conn: &Connection,
    condition: &str,
    values: impl rusqlite::Params,
) -> Result<Option<Attempt>, WidgetsError> {
    Ok(conn
        .query_row(
            &format!(
                "SELECT {ATTEMPT_COLUMNS} FROM refresh_attempts WHERE {condition}
                 ORDER BY created_at DESC, rowid DESC LIMIT 1"
            ),
            values,
            attempt_from_row,
        )
        .optional()?)
}

pub(super) fn load_attempt(
    conn: &Connection,
    refresh_id: &str,
) -> Result<Option<Attempt>, WidgetsError> {
    attempt_where(conn, "refresh_id = ?1", params![refresh_id])
}

pub(super) fn latest_attempt(
    conn: &Connection,
    widget_id: &str,
) -> Result<Option<Attempt>, WidgetsError> {
    attempt_where(conn, "widget_id = ?1", params![widget_id])
}

/// The attempt a refresh click joins: the one made for `request_id`, else
/// the widget's active one.
pub(super) fn joinable_attempt(
    conn: &Connection,
    widget_id: &str,
    request_id: &str,
) -> Result<Option<Attempt>, WidgetsError> {
    match attempt_where(
        conn,
        "widget_id = ?1 AND request_id = ?2",
        params![widget_id, request_id],
    )? {
        Some(attempt) => Ok(Some(attempt)),
        None => attempt_where(
            conn,
            &format!("widget_id = ?1 AND state IN {ACTIVE_ATTEMPT_STATES}"),
            params![widget_id],
        ),
    }
}

/// The active attempt whose run executes in `chat_id`, if any.
pub(super) fn active_attempt_in_chat(
    conn: &Connection,
    chat_id: &str,
) -> Result<Option<Attempt>, WidgetsError> {
    attempt_where(
        conn,
        &format!("execution_chat_id = ?1 AND state IN {ACTIVE_ATTEMPT_STATES}"),
        params![chat_id],
    )
}

/// Move an active attempt to `state` (a terminal one stamps `finished_at`
/// and the error's sentence). `false` when it was no longer active, or not
/// in `from` when given.
pub(super) fn set_attempt_state(
    tx: &Transaction<'_>,
    refresh_id: &str,
    from: Option<ProjectWidgetRefreshState>,
    state: ProjectWidgetRefreshState,
    error_code: Option<&str>,
) -> Result<bool, WidgetsError> {
    let now = crate::now_ms();
    let finished = (!is_active(state)).then_some(now);
    let from = match from {
        Some(from) => format!("('{}')", state_name(from)),
        None => ACTIVE_ATTEMPT_STATES.to_owned(),
    };
    Ok(tx.execute(
        &format!(
            "UPDATE refresh_attempts SET state = ?2, error_code = ?3, error_message = ?4,
                 updated_at = ?5, finished_at = ?6
             WHERE refresh_id = ?1 AND state IN {from}"
        ),
        params![
            refresh_id,
            state_name(state),
            error_code,
            error_code.map(error_message),
            now,
            finished,
        ],
    )? > 0)
}

/// Every chat a widget refresh ever ran in.
pub(super) fn refresh_chats(conn: &Connection) -> Result<Vec<String>, WidgetsError> {
    let mut statement = conn.prepare(
        "SELECT refresh_chat_id FROM widgets WHERE refresh_chat_id IS NOT NULL
         UNION SELECT execution_chat_id FROM refresh_attempts
         WHERE execution_chat_id IS NOT NULL",
    )?;
    let chats = statement
        .query_map([], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(chats)
}

impl From<rusqlite::Error> for WidgetsError {
    fn from(err: rusqlite::Error) -> Self {
        WidgetsError::Write(err.to_string())
    }
}

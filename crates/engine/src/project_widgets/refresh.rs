//! Widget refresh: an agent run in the widget's own chat proposes new content,
//! published only if that run's turn completes successfully.
//!
//! - **Admission** is one transaction: one active attempt per widget, and the
//!   same `requestId` (or any click while one is active) joins it. The attempt
//!   is stored `queued` with its run's chat and command ids preallocated
//!   before anything starts, so a crash anywhere afterwards can never launch
//!   it twice: the next boot marks it `interrupted` and withdraws a command
//!   that did not run ([`ProjectWidgets::withdraw_interrupted_refreshes`]).
//! - **The run** is an ordinary chat in the widget's space, created on first
//!   use and kept archived. Its Run command is queued under the attempt's
//!   command id, which is also its user message id, so its turn end is
//!   watched before it can drain ([`crate::SessionsEngine::watch_turn_end`]).
//!   Attempts of one widget take turns in its chat ([`RefreshState::lane`]),
//!   so stopping one never stops the next.
//! - **Candidates.** The agent calls `update_widget` with the `refreshId`; the
//!   content is validated into a bundle and kept as the attempt's candidate.
//!   A later valid one replaces it; an invalid one changes nothing.
//! - **Publication** happens when the turn completes, in one transaction that
//!   requires the attempt to be active and the widget still at its base
//!   revision. Any other end, cancellation, deletion, the 15-minute limit or
//!   a restart leaves the content and `lastSuccessfulAt` as they were.
//!
//! Diagnostics name ids, phases and error codes only.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use rusqlite::params;
use tokio_util::sync::CancellationToken;
use zeron_doc::{SessionCommandPayload, SessionCommandStatus};
use zeron_proto::{
    CancelProjectWidgetRefreshParams, ChatConfig, DoneStatus, ProjectWidget,
    ProjectWidgetCancelOutcome, ProjectWidgetRefreshAdmission, ProjectWidgetRefreshCancellation,
    ProjectWidgetRefreshState as State, ProjectWidgetWrite, ProjectWidgetWriteOutcome,
    RefreshProjectWidgetParams, RunRequest, SandboxLevel, SessionStatus, UpdateProjectWidgetParams,
};

use super::store::{self, Attempt};
use super::{GC_GRACE, KIND_UPDATE, ProjectWidgets, WidgetsError, check_key, live, lock};

/// Longest a refresh may take, queued and awaiting input included.
const REFRESH_TIMEOUT: Duration = Duration::from_secs(15 * 60);
/// Longest a refresh waits for its chat to finish another turn.
const IDLE_WAIT: Duration = Duration::from_secs(60);
/// After a stop request, how long the run is followed to its end.
const STOP_SETTLE: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(250);
/// The current script quoted in the run's prompt, at most.
const PROMPT_SCRIPT_BYTES: usize = 24 * 1024;
/// Zeron MCP tools a refresh run needs.
const RUN_TOOLS: [&str; 2] = ["get_widget", "update_widget"];

/// What runs refreshes, attached at engine assembly.
#[derive(Clone)]
pub struct RefreshRuntime {
    pub sessions: crate::SessionsEngine,
    pub doc_host: crate::DocHost,
    pub workspace: crate::WorkspaceHost,
    pub registry: Arc<crate::HarnessRegistry>,
    pub policy: crate::McpToolPolicy,
}

#[derive(Default)]
pub(super) struct RefreshState {
    runtime: OnceLock<RefreshRuntime>,
    /// Refresh id → its run's stop signal (cancel, delete).
    stops: Mutex<HashMap<String, CancellationToken>>,
    /// Widget id → the lane its attempts take turns in.
    lanes: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    /// `(chat, command)` of attempts interrupted when the store opened.
    interrupted: Mutex<Vec<(String, String)>>,
}

impl RefreshState {
    pub(super) fn interrupted_at_open(&self, runs: Vec<(String, String)>) {
        lock(&self.interrupted).extend(runs);
    }

    fn lane(&self, widget_id: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut lanes = lock(&self.lanes);
        lanes.retain(|_, lane| Arc::strong_count(lane) > 1);
        lanes.entry(widget_id.to_owned()).or_default().clone()
    }
}

fn unavailable(reason: impl Into<String>) -> WidgetsError {
    WidgetsError::RefreshUnavailable(reason.into())
}

/// How a monitored run woke up.
enum Wake {
    Shutdown,
    Stop,
    Ended(Option<DoneStatus>),
    Timeout,
    Tick,
}

impl ProjectWidgets {
    /// Serve refreshes with these engine parts (once, at assembly).
    pub fn attach_refresh(&self, runtime: RefreshRuntime) {
        let _ = self.inner.refresh.runtime.set(runtime);
    }

    /// Every chat a widget refresh ran in: boot recovery must not resume them.
    pub fn refresh_chat_ids(&self) -> HashSet<String> {
        self.read(store::refresh_chats)
            .map(|chats| chats.into_iter().collect())
            .unwrap_or_default()
    }

    /// Keep the commands of attempts interrupted at open from ever running.
    /// Call at assembly, before chat docs open and drain.
    pub fn withdraw_interrupted_refreshes(&self, doc_host: &crate::DocHost) {
        let runs = std::mem::take(&mut *lock(&self.inner.refresh.interrupted));
        for (chat_id, command_id) in runs {
            match doc_host.withdraw_command(&chat_id, &command_id, "Zeron restarted first") {
                Ok(true) => {
                    tracing::info!(command = %command_id, "interrupted widget refresh withdrawn")
                }
                Ok(false) => {}
                Err(err) => {
                    tracing::warn!(command = %command_id, error = %err, "widget refresh not withdrawn")
                }
            }
        }
    }

    /// Admit a refresh of a widget: preflight, then a `queued` attempt, then
    /// its run in the background. Returns before the agent starts.
    pub fn refresh(
        &self,
        p: &RefreshProjectWidgetParams,
    ) -> Result<ProjectWidgetRefreshAdmission, WidgetsError> {
        check_key("requestId", &p.request_id)?;
        let widget = self.get(&p.widget_id)?;
        let joinable =
            |conn: &rusqlite::Connection| store::joinable_attempt(conn, &widget.id, &p.request_id);
        if let Some(attempt) = self.read(joinable)? {
            return Ok(admission(&widget.id, &attempt, true));
        }
        if widget.revision != p.expected_revision {
            return Err(WidgetsError::Conflict);
        }
        let runtime = self.preflight(&widget)?;
        let refresh_id = crate::new_id();
        let (attempt, existing) = self.write(|conn| {
            if let Some(attempt) = joinable(conn)? {
                return Ok((attempt, true));
            }
            let stored = live(store::load_widget(conn, &widget.id)?)?;
            if stored.revision != p.expected_revision {
                return Err(WidgetsError::Conflict);
            }
            // The widget's chat, unless it was deleted: then a new one.
            let chat_id = conn
                .query_row(
                    "SELECT refresh_chat_id FROM widgets WHERE id = ?1",
                    params![widget.id],
                    |row| row.get::<_, Option<String>>(0),
                )?
                .filter(|chat| runtime.workspace.chat(chat).ok().flatten().is_some())
                .unwrap_or_else(crate::new_id);
            let now = crate::now_ms();
            let tx = conn.transaction()?;
            tx.execute(
                "UPDATE widgets SET refresh_chat_id = ?2 WHERE id = ?1",
                params![widget.id, chat_id],
            )?;
            tx.execute(
                "INSERT INTO refresh_attempts (refresh_id, widget_id, request_id, base_revision,
                     state, command_id, execution_chat_id, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, 'queued', ?5, ?6, ?7, ?7)",
                params![
                    refresh_id,
                    widget.id,
                    p.request_id,
                    stored.revision as i64,
                    crate::new_id(),
                    chat_id,
                    now
                ],
            )?;
            self.commit(tx)?;
            self.publish(conn, &widget.space_id);
            let attempt = store::load_attempt(conn, &refresh_id)?.ok_or(WidgetsError::NotFound)?;
            Ok((attempt, false))
        })?;
        if !existing {
            tracing::info!(widget = %widget.id, refresh = %attempt.refresh_id, "widget refresh queued");
            let stop = CancellationToken::new();
            lock(&self.inner.refresh.stops).insert(attempt.refresh_id.clone(), stop.clone());
            match tokio::runtime::Handle::try_current() {
                Ok(handle) => {
                    handle.spawn(self.clone().run_attempt(runtime, attempt.clone(), stop));
                }
                Err(_) => {
                    self.finish(&attempt.refresh_id, State::Failed, "launch_failed");
                }
            }
        }
        Ok(admission(&widget.id, &attempt, existing))
    }

    /// Make an attempt unpublishable, then stop its run. Cancelling an
    /// ended attempt changes nothing and says how it ended.
    pub fn cancel_refresh(
        &self,
        p: &CancelProjectWidgetRefreshParams,
    ) -> Result<ProjectWidgetRefreshCancellation, WidgetsError> {
        check_key("requestId", &p.request_id)?;
        let (outcome, attempt) = self.write(|conn| {
            let attempt = store::load_attempt(conn, &p.refresh_id)?
                .filter(|attempt| attempt.widget_id == p.widget_id)
                .ok_or(WidgetsError::NotFound)?;
            let space_id = store::load_widget(conn, &p.widget_id)?
                .ok_or(WidgetsError::NotFound)?
                .widget
                .space_id;
            self.owned(&space_id)?;
            let outcome = match attempt.state {
                _ if attempt.active() => {
                    let tx = conn.transaction()?;
                    store::set_attempt_state(
                        &tx,
                        &attempt.refresh_id,
                        None,
                        State::Cancelled,
                        Some("cancelled"),
                    )?;
                    self.commit(tx)?;
                    self.publish(conn, &space_id);
                    ProjectWidgetCancelOutcome::Cancelled
                }
                State::Succeeded => ProjectWidgetCancelOutcome::AlreadyCompleted,
                State::Cancelled => ProjectWidgetCancelOutcome::AlreadyCancelled,
                _ => ProjectWidgetCancelOutcome::AlreadyFinished,
            };
            let attempt =
                store::load_attempt(conn, &p.refresh_id)?.ok_or(WidgetsError::NotFound)?;
            Ok((outcome, attempt))
        })?;
        if outcome == ProjectWidgetCancelOutcome::Cancelled {
            tracing::info!(refresh = %attempt.refresh_id, "widget refresh cancelled");
            self.stop_refreshes(std::slice::from_ref(&attempt.refresh_id));
        }
        Ok(ProjectWidgetRefreshCancellation {
            widget_id: p.widget_id.clone(),
            outcome,
            refresh: attempt.refresh(),
        })
    }

    /// Stop the runs of attempts already made unpublishable in the store.
    pub(super) fn stop_refreshes(&self, refresh_ids: &[String]) {
        let stops = lock(&self.inner.refresh.stops);
        for id in refresh_ids {
            if let Some(stop) = stops.get(id) {
                stop.cancel();
            }
        }
    }

    /// Keep a refresh run's `update_widget` as its attempt's candidate.
    pub(super) fn stage_candidate(
        &self,
        p: &UpdateProjectWidgetParams,
        refresh_id: &str,
        current: &ProjectWidget,
        assets: Option<BTreeMap<String, Vec<u8>>>,
        fingerprint: &str,
    ) -> Result<ProjectWidgetWrite, WidgetsError> {
        let attempt = self
            .read(|conn| store::load_attempt(conn, refresh_id))?
            .filter(|attempt| {
                attempt.active()
                    && p.origin_chat_id.as_deref() == Some(attempt.execution_chat_id.as_str())
            })
            .ok_or(WidgetsError::RefreshInactive)?;
        if attempt.widget_id != current.id {
            return Err(WidgetsError::RefreshScope);
        }
        if p.expected_revision != attempt.base_revision {
            return Err(WidgetsError::Conflict);
        }
        let (Some(input), Some(mut assets)) = (&p.content, assets) else {
            return Err(WidgetsError::BadRequest(
                "a refresh run sends the widget's whole script".into(),
            ));
        };
        if p.title.is_some() || p.refresh_recipe != Default::default() {
            return Err(WidgetsError::BadRequest(
                "a refresh run changes the content only, not the title or recipe".into(),
            ));
        }
        if let Some(keep) = &input.keep_assets_from_bundle {
            if *keep != current.content.bundle_id {
                return Err(WidgetsError::Conflict);
            }
            for (name, bytes) in self.inner.bundles.read_assets(&current.content)? {
                assets.entry(name).or_insert(bytes);
            }
        }
        let content =
            self.inner
                .bundles
                .publish(&input.script, &assets, current.content.render_seed)?;
        let bundle_id = content.bundle_id.clone();
        let written = self.write(|conn| {
            if let Some(response) = store::receipt(conn, &p.operation_id, KIND_UPDATE, fingerprint)?
            {
                return Ok(Err(super::replayed(&response)?));
            }
            let tx = conn.transaction()?;
            let staged = tx.execute(
                &format!(
                    "UPDATE refresh_attempts SET candidate_bundle_id = ?2, candidate_content = ?3,
                         attempt_revision = attempt_revision + 1, updated_at = ?4
                     WHERE refresh_id = ?1 AND state IN {}",
                    store::ACTIVE_ATTEMPT_STATES
                ),
                params![
                    refresh_id,
                    bundle_id,
                    super::to_json(&content)?,
                    crate::now_ms()
                ],
            )?;
            if staged == 0 {
                return Err(WidgetsError::RefreshInactive);
            }
            let write = ProjectWidgetWrite {
                outcome: ProjectWidgetWriteOutcome::Staged,
                widget: live(store::load_widget(&tx, &current.id)?)?,
            };
            store::record_receipt(&tx, &p.operation_id, KIND_UPDATE, fingerprint, &write)?;
            self.commit(tx)?;
            Ok(Ok(write))
        });
        let written = self.settle_new_bundle(&bundle_id, written);
        if written.is_ok() {
            tracing::info!(refresh = %refresh_id, "widget refresh candidate staged");
        }
        written
    }

    // ── the run ─────────────────────────────────────────────────────────────

    fn preflight(&self, widget: &ProjectWidget) -> Result<RefreshRuntime, WidgetsError> {
        let runtime = self
            .inner
            .refresh
            .runtime
            .get()
            .cloned()
            .ok_or_else(|| unavailable("this engine does not run widget refreshes"))?;
        let recipe = widget
            .refresh_recipe
            .as_ref()
            .ok_or_else(|| unavailable("the widget has no refresh recipe"))?;
        super::check_recipe(recipe)?;
        let harness = runtime
            .registry
            .descriptors()
            .into_iter()
            .find(|descriptor| descriptor.id == recipe.harness);
        let runnable = harness.as_ref().is_some_and(|descriptor| {
            descriptor.installed
                && (descriptor.enabled == Some(true)
                    || descriptor.id == zeron_proto::HarnessId::Mock)
        });
        if !runnable {
            let name = harness.map_or_else(|| format!("{:?}", recipe.harness), |d| d.name);
            return Err(unavailable(format!(
                "{name} is not available on this device"
            )));
        }
        if !runtime.sessions.serves_zeron_mcp() {
            return Err(unavailable(
                "this engine does not give its agents the Zeron MCP",
            ));
        }
        for tool in RUN_TOOLS {
            if !runtime.policy.admit(tool, true).admitted {
                return Err(unavailable(format!(
                    "{tool} is switched off in Settings → MCP"
                )));
            }
        }
        Ok(runtime)
    }

    async fn run_attempt(self, runtime: RefreshRuntime, attempt: Attempt, stop: CancellationToken) {
        let refresh_id = attempt.refresh_id.clone();
        let started = tokio::time::Instant::now();
        self.drive(&runtime, attempt, &stop).await;
        lock(&self.inner.refresh.stops).remove(&refresh_id);
        tracing::info!(
            refresh = %refresh_id,
            elapsed_ms = started.elapsed().as_millis() as u64,
            "widget refresh run ended"
        );
    }

    async fn drive(&self, runtime: &RefreshRuntime, attempt: Attempt, stop: &CancellationToken) {
        let shutdown = self.inner.cancel.clone();
        let deadline = tokio::time::Instant::now() + REFRESH_TIMEOUT;
        let (refresh_id, chat_id, command_id) = (
            attempt.refresh_id.as_str(),
            attempt.execution_chat_id.as_str(),
            attempt.command_id.as_str(),
        );
        // One attempt of a widget at a time in its chat.
        let lane = self.inner.refresh.lane(&attempt.widget_id);
        let _lane = tokio::select! {
            guard = lane.lock_owned() => guard,
            _ = stop.cancelled() => return,
            _ = shutdown.cancelled() => return,
            _ = tokio::time::sleep_until(deadline) => {
                self.finish_blocking(refresh_id, State::Failed, "timeout").await;
                return;
            }
        };
        let this = self.clone();
        let id = attempt.widget_id.clone();
        let refresh = refresh_id.to_owned();
        let loaded = tokio::task::spawn_blocking(move || {
            this.read(|conn| {
                let widget = live(store::load_widget(conn, &id)?)?;
                let active = store::load_attempt(conn, &refresh)?.is_some_and(|a| a.active());
                Ok((widget, active))
            })
        })
        .await;
        let widget = match loaded {
            Ok(Ok((widget, true))) => widget,
            _ => return,
        };
        let this = self.clone();
        let rt = runtime.clone();
        let run = attempt.clone();
        let prepared =
            tokio::task::spawn_blocking(move || this.prepare_run(&rt, &widget, &run)).await;
        let request = match prepared {
            Ok(Ok(request)) => request,
            Ok(Err(err)) => {
                tracing::warn!(refresh = %refresh_id, error = %err, "widget refresh launch failed");
                self.finish_blocking(refresh_id, State::Failed, "launch_failed")
                    .await;
                return;
            }
            Err(_) => return,
        };
        // Never interrupt another turn in the chat (a user typing in it).
        let idle_by = tokio::time::Instant::now() + IDLE_WAIT;
        while runtime.sessions.turn_in_flight(chat_id) {
            if tokio::time::Instant::now() >= idle_by {
                self.finish_blocking(refresh_id, State::Failed, "chat_busy")
                    .await;
                return;
            }
            tokio::select! {
                _ = tokio::time::sleep(POLL) => {}
                _ = stop.cancelled() => return,
                _ = shutdown.cancelled() => return,
            }
        }
        // Cancelled while preparing: nothing was sent. A stop from here on
        // withdraws the command or stops its turn.
        if stop.is_cancelled() || shutdown.is_cancelled() {
            return;
        }
        // Watch before queueing: the turn may end before we look again.
        let mut turn = runtime.sessions.watch_turn_end(command_id);
        if let Err(err) = runtime
            .doc_host
            .queue_internal_run(chat_id, command_id, request)
        {
            tracing::warn!(refresh = %refresh_id, error = %err, "widget refresh launch failed");
            self.finish_blocking(refresh_id, State::Failed, "launch_failed")
                .await;
            return;
        }
        self.transition_blocking(refresh_id, State::Queued, State::Running)
            .await;
        let mut state = State::Running;
        let mut tick = tokio::time::interval(POLL);
        loop {
            let wake = tokio::select! {
                biased;
                _ = shutdown.cancelled() => Wake::Shutdown,
                _ = stop.cancelled() => Wake::Stop,
                end = turn.wait() => Wake::Ended(end.map(|end| end.status)),
                _ = tokio::time::sleep_until(deadline) => Wake::Timeout,
                _ = tick.tick() => Wake::Tick,
            };
            match wake {
                // The next boot marks the attempt interrupted.
                Wake::Shutdown | Wake::Ended(None) => return,
                Wake::Stop => {
                    self.stop_run(runtime, chat_id, command_id, &mut turn).await;
                    return;
                }
                Wake::Timeout => {
                    if self
                        .finish_blocking(refresh_id, State::Failed, "timeout")
                        .await
                    {
                        self.stop_run(runtime, chat_id, command_id, &mut turn).await;
                    }
                    return;
                }
                Wake::Ended(Some(status)) => {
                    let this = self.clone();
                    let refresh = refresh_id.to_owned();
                    let _ =
                        tokio::task::spawn_blocking(move || this.settle(&refresh, status)).await;
                    return;
                }
                Wake::Tick => {
                    if !matches!(runtime.workspace.chat(chat_id), Ok(Some(_))) {
                        self.finish_blocking(refresh_id, State::Interrupted, "chat_deleted")
                            .await;
                        return;
                    }
                    if command_refused(runtime, chat_id, command_id) {
                        self.finish_blocking(refresh_id, State::Failed, "launch_failed")
                            .await;
                        return;
                    }
                    let awaiting = runtime
                        .sessions
                        .session_status(chat_id)
                        .is_some_and(|s| s.status == SessionStatus::AwaitingInput);
                    let next = if awaiting {
                        State::AwaitingInput
                    } else {
                        State::Running
                    };
                    if next != state && self.transition_blocking(refresh_id, state, next).await {
                        state = next;
                    }
                }
            }
        }
    }

    /// The attempt can no longer publish; keep its command from starting,
    /// or stop the turn it started, and follow it to its end (bounded).
    async fn stop_run(
        &self,
        runtime: &RefreshRuntime,
        chat_id: &str,
        command_id: &str,
        turn: &mut crate::sessions::TurnEndWatch,
    ) {
        let doc_host = runtime.doc_host.clone();
        let (chat, command) = (chat_id.to_owned(), command_id.to_owned());
        let withdrawn = tokio::task::spawn_blocking(move || {
            doc_host.withdraw_command(&chat, &command, "the widget refresh ended first")
        })
        .await;
        if matches!(withdrawn, Ok(Ok(true))) {
            return;
        }
        let until = tokio::time::Instant::now() + STOP_SETTLE;
        while turn.ended().is_none() && tokio::time::Instant::now() < until {
            // Its own turn: attempts of a widget never overlap in its chat.
            if runtime.sessions.turn_in_flight(chat_id)
                && let Err(err) = runtime.sessions.stop_turn(chat_id).await
            {
                tracing::warn!(error = %err, "widget refresh run not stopped");
            }
            tokio::select! {
                _ = turn.wait() => return,
                _ = tokio::time::sleep(POLL) => {}
                _ = self.inner.cancel.cancelled() => return,
            }
        }
    }

    /// Create or reuse the widget's chat, archived, and the Run to send it.
    fn prepare_run(
        &self,
        runtime: &RefreshRuntime,
        widget: &ProjectWidget,
        attempt: &Attempt,
    ) -> Result<RunRequest, WidgetsError> {
        let chat_id = attempt.execution_chat_id.as_str();
        let failed = |err: crate::EngineError| WidgetsError::Write(err.to_string());
        let recipe = widget
            .refresh_recipe
            .clone()
            .ok_or_else(|| unavailable("the widget has no refresh recipe"))?;
        let space = runtime
            .workspace
            .space(&widget.space_id)
            .map_err(failed)?
            .ok_or(WidgetsError::OwnerUnavailable)?;
        let config = ChatConfig {
            harness: recipe.harness,
            model: recipe.model.clone(),
            reasoning: recipe.reasoning,
            model_options: Default::default(),
            sandbox: SandboxLevel::WorkspaceWrite,
        };
        let workspace = &runtime.workspace;
        match workspace.chat(chat_id).map_err(failed)? {
            None => {
                workspace
                    .create_chat(chat_id, Some(&space.id), None, Some(config), None)
                    .map_err(failed)?;
                // A title of its own also keeps the auto-titler out of it.
                let title: String = format!("Refresh · {}", widget.title)
                    .chars()
                    .take(120)
                    .collect();
                workspace.rename_chat(chat_id, &title).map_err(failed)?;
            }
            Some(chat) if chat.config.as_ref() != Some(&config) => {
                workspace
                    .set_chat_config(chat_id, &config)
                    .map_err(failed)?;
            }
            Some(_) => {}
        }
        // Out of the chat list; "Open run" reaches it.
        if workspace
            .chat(chat_id)
            .map_err(failed)?
            .is_some_and(|chat| !chat.archived)
        {
            workspace.set_chat_archived(chat_id, true).map_err(failed)?;
        }
        let script = self.inner.bundles.read_script(&widget.content)?;
        Ok(RunRequest {
            prompt: run_prompt(widget, attempt, &recipe.instruction, &script),
            harness: Some(recipe.harness),
            model: recipe.model,
            reasoning: recipe.reasoning,
            model_options: Default::default(),
            cwd: space.path,
            sandbox: SandboxLevel::WorkspaceWrite,
            auto_approve: false,
            resume: None,
            attachments: Vec::new(),
            worktree: None,
            mcp: None,
        })
    }

    /// The turn ended: publish the candidate, or record why not.
    fn settle(&self, refresh_id: &str, status: DoneStatus) {
        let (state, code) = match status {
            DoneStatus::Completed => {
                if let Err(err) = self.publish_candidate(refresh_id) {
                    tracing::error!(refresh = %refresh_id, error = %err, "widget refresh not published");
                    self.finish(refresh_id, State::Failed, "launch_failed");
                }
                return;
            }
            DoneStatus::Errored => (State::Failed, "agent_error"),
            DoneStatus::Interrupted => (State::Interrupted, "run_stopped"),
        };
        self.finish(refresh_id, state, code);
    }

    fn publish_candidate(&self, refresh_id: &str) -> Result<(), WidgetsError> {
        let outcome = self.write(|conn| {
            let Some(attempt) = store::load_attempt(conn, refresh_id)?.filter(|a| a.active())
            else {
                return Ok(None); // cancelled or deleted meanwhile
            };
            let stored =
                store::load_widget(conn, &attempt.widget_id)?.ok_or(WidgetsError::NotFound)?;
            let space_id = stored.widget.space_id.clone();
            let tx = conn.transaction()?;
            let (state, code) = match &attempt.candidate_content {
                None => (State::Failed, Some("no_result")),
                Some(_) if stored.deleted => (State::Cancelled, Some("deleted")),
                Some(_) if stored.widget.revision != attempt.base_revision => {
                    (State::Conflict, Some("conflict"))
                }
                Some(content) => {
                    let content: zeron_proto::ProjectWidgetContent = serde_json::from_str(content)
                        .map_err(|err| WidgetsError::Write(err.to_string()))?;
                    let now = crate::now_ms();
                    let published = tx.execute(
                        "UPDATE widgets SET content = ?3, bundle_id = ?4,
                             content_revision = content_revision + 1, last_successful_at = ?5,
                             revision = revision + 1, updated_at = ?5
                         WHERE id = ?1 AND revision = ?2 AND deleted = 0",
                        params![
                            attempt.widget_id,
                            attempt.base_revision as i64,
                            super::to_json(&content)?,
                            content.bundle_id,
                            now
                        ],
                    )?;
                    if published == 1 {
                        (State::Succeeded, None)
                    } else {
                        (State::Conflict, Some("conflict"))
                    }
                }
            };
            store::set_attempt_state(&tx, refresh_id, None, state, code)?;
            self.commit(tx)?;
            self.publish(conn, &space_id);
            Ok(Some((state, code)))
        })?;
        if let Some((state, code)) = outcome {
            tracing::info!(refresh = %refresh_id, ?state, code, "widget refresh finished");
            if state == State::Succeeded {
                // The previous bundle is no longer referenced.
                self.collect_garbage_older_than(GC_GRACE);
            }
        }
        Ok(())
    }

    /// End an active attempt. `false` when it had already ended.
    fn finish(&self, refresh_id: &str, state: State, code: &str) -> bool {
        self.change_state(refresh_id, None, state, Some(code))
    }

    fn change_state(
        &self,
        refresh_id: &str,
        from: Option<State>,
        state: State,
        code: Option<&str>,
    ) -> bool {
        let changed = self.write(|conn| {
            let Some(attempt) = store::load_attempt(conn, refresh_id)? else {
                return Ok(false);
            };
            let tx = conn.transaction()?;
            if !store::set_attempt_state(&tx, refresh_id, from, state, code)? {
                return Ok(false);
            }
            self.commit(tx)?;
            if let Some(stored) = store::load_widget(conn, &attempt.widget_id)? {
                self.publish(conn, &stored.widget.space_id);
            }
            Ok(true)
        });
        match changed {
            Ok(changed) => {
                if changed && !store::is_active(state) {
                    tracing::info!(refresh = %refresh_id, ?state, code, "widget refresh finished");
                }
                changed
            }
            Err(err) => {
                tracing::error!(refresh = %refresh_id, error = %err, "widget refresh state not saved");
                false
            }
        }
    }

    async fn finish_blocking(&self, refresh_id: &str, state: State, code: &'static str) -> bool {
        let (this, id) = (self.clone(), refresh_id.to_owned());
        tokio::task::spawn_blocking(move || this.finish(&id, state, code))
            .await
            .unwrap_or(false)
    }

    async fn transition_blocking(&self, refresh_id: &str, from: State, to: State) -> bool {
        let (this, id) = (self.clone(), refresh_id.to_owned());
        tokio::task::spawn_blocking(move || this.change_state(&id, Some(from), to, None))
            .await
            .unwrap_or(false)
    }
}

/// The run's command was settled without running (rejected or expired).
fn command_refused(runtime: &RefreshRuntime, chat_id: &str, command_id: &str) -> bool {
    let Ok(handle) = runtime.doc_host.open(chat_id) else {
        return false;
    };
    handle.doc().read_commands().is_ok_and(|commands| {
        commands.iter().any(|c| {
            c.id == command_id
                && matches!(c.payload, SessionCommandPayload::Run { .. })
                && matches!(
                    c.status,
                    SessionCommandStatus::Rejected
                        | SessionCommandStatus::Expired
                        | SessionCommandStatus::Superseded
                )
        })
    })
}

fn admission(widget_id: &str, attempt: &Attempt, existing: bool) -> ProjectWidgetRefreshAdmission {
    ProjectWidgetRefreshAdmission {
        widget_id: widget_id.to_owned(),
        refresh: attempt.refresh(),
        existing,
    }
}

/// What the run is asked: the recipe, the ids its `update_widget` needs and
/// the current script for reference (bounded; asset names only).
fn run_prompt(
    widget: &ProjectWidget,
    attempt: &Attempt,
    instruction: &str,
    script: &str,
) -> String {
    let assets = if widget.content.assets.is_empty() {
        "none".to_owned()
    } else {
        widget
            .content
            .assets
            .iter()
            .map(|asset| asset.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut quoted: String = script.to_owned();
    let cut = quoted.len() > PROMPT_SCRIPT_BYTES;
    if cut {
        let mut end = PROMPT_SCRIPT_BYTES;
        while !quoted.is_char_boundary(end) {
            end -= 1;
        }
        quoted.truncate(end);
    }
    format!(
        "Refresh the project widget \"{title}\".\n\n{instruction}\n\n\
         When the new version is ready, call the Zeron MCP tool update_widget with \
         widgetId \"{id}\", expectedRevision {revision}, refreshId \"{refresh_id}\" \
         and a new operationId, sending the whole new Luau script (the show_visual \
         language) and its assets. Current assets: {assets}; pass keepExistingAssets: true \
         to keep those you do not send. The widget changes only when this turn completes \
         successfully. Do not change its title or recipe, and do not write to other \
         widgets.\n\nCurrent script{note}:\n```luau\n{quoted}\n```",
        title = widget.title,
        id = widget.id,
        revision = attempt.base_revision,
        refresh_id = attempt.refresh_id,
        note = if cut {
            " (truncated; get_widget with includeContent returns all of it)"
        } else {
            ""
        },
    )
}

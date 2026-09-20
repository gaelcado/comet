//! Native, read-only PR inspection with explicit browser escape routes.
use crate::{
    settings::{self, PullRequestDestination, SavePolicy, widgets},
    state::AppState,
    theme::Theme,
};
use gpui::{
    Action, AnyElement, App, Context, Entity, IntoElement, Render, SharedString, Subscription,
    Task, Window, div, prelude::*, px,
};
#[cfg(test)]
use std::time::Duration;
use std::{cell::RefCell, rc::Rc, sync::Arc, time::Instant};
use zeron_proto::{ChangeRequestDetail, ChangeRequestListItem};
use zeron_rpc::methods;

#[derive(Clone, PartialEq, Action)]
#[action(namespace = shell, no_json)]
pub struct OpenPullRequest(pub String, pub Option<String>);

#[derive(Clone, PartialEq, Action)]
#[action(namespace = shell, no_json)]
pub struct ClosePullRequest;

pub fn open(url: &str, window: &mut Window, cx: &mut App) {
    open_on_device(url, None, window, cx);
}

pub fn open_on_device(url: &str, device: Option<String>, window: &mut Window, cx: &mut App) {
    if settings::current(cx).pull_request_destination == PullRequestDestination::External {
        cx.open_url(url);
    } else {
        window.dispatch_action(Box::new(OpenPullRequest(url.to_owned(), device)), cx);
    }
}

/// The same persistent preference is exposed in Settings and beside the board.
pub fn destination_setting(theme: &Theme, cx: &App) -> AnyElement {
    let current = settings::current(cx).pull_request_destination;
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(widgets::row_title(theme, "Open pull requests in"))
        .child(div().flex().flex_wrap().gap(px(4.0)).children(
            PullRequestDestination::ALL.into_iter().map(|destination| {
                let selected = destination == current;
                widgets::ghost_action(theme)
                    .id(SharedString::from(format!(
                        "pr-destination-{destination:?}"
                    )))
                    .role(gpui::Role::Button)
                    .aria_label(destination.label())
                    .aria_selected(selected)
                    .tab_index(0)
                    .border_1()
                    .border_color(gpui::transparent_black())
                    .focus_visible(|style| style.border_color(theme.accent))
                    .when(selected, |el| {
                        el.bg(theme.glass_hover()).text_color(theme.text)
                    })
                    .cursor_pointer()
                    .on_click(move |_, _, cx| {
                        settings::update(SavePolicy::Immediate, cx, |settings| {
                            settings.pull_request_destination = destination
                        });
                        cx.refresh_windows();
                    })
                    .child(destination.label())
            }),
        ))
        .child(
            div()
                .text_size(px(11.0))
                .text_color(theme.text_muted)
                .child("Applies to PR badges throughout Zeron."),
        )
        .into_any_element()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Summary,
    Code,
    Activity,
    Checks,
}

#[derive(Clone)]
struct CodeRow {
    text: SharedString,
    old: String,
    new: String,
    kind: crate::changes::LineKind,
}

fn code_rows(patch: &str) -> (Vec<CodeRow>, Vec<(String, usize)>) {
    use crate::changes::LineKind;
    let mut rows = Vec::new();
    let mut files = Vec::new();
    for file in crate::changes::parse_patch(patch) {
        files.push((file.path.clone(), rows.len()));
        rows.push(CodeRow {
            text: file.path.clone().into(),
            old: String::new(),
            new: String::new(),
            kind: LineKind::Meta,
        });
        for notice in crate::changes::file_notices(&file) {
            rows.push(CodeRow {
                text: notice.into(),
                old: String::new(),
                new: String::new(),
                kind: LineKind::Meta,
            });
        }
        for hunk in file.hunks {
            rows.push(CodeRow {
                text: hunk.header.into(),
                old: String::new(),
                new: String::new(),
                kind: LineKind::Meta,
            });
            rows.extend(hunk.lines.into_iter().map(|line| CodeRow {
                text: line.text.into(),
                old: line.old_no.map(|n| n.to_string()).unwrap_or_default(),
                new: line.new_no.map(|n| n.to_string()).unwrap_or_default(),
                kind: line.kind,
            }));
        }
    }
    (rows, files)
}

fn code_content_width(rows: &[CodeRow]) -> f32 {
    rows.iter()
        .map(|row| crate::changes::visual_columns(&row.text))
        .max()
        .unwrap_or(0) as f32
        * 7.0
        + 128.0
}

#[derive(Clone)]
struct ParsedDiff {
    patch: Arc<String>,
    rows: Arc<Vec<CodeRow>>,
    files: Arc<Vec<(String, usize)>>,
    width: f32,
}

impl ParsedDiff {
    fn new(patch: String) -> Self {
        let (rows, files) = code_rows(&patch);
        let width = code_content_width(&rows);
        Self {
            patch: Arc::new(patch),
            rows: Arc::new(rows),
            files: Arc::new(files),
            width,
        }
    }
}

#[derive(Clone)]
struct DetailSnapshot {
    detail: ChangeRequestDetail,
    body: crate::markdown::BlockTree,
    activity: Vec<crate::markdown::BlockTree>,
    fetched: Instant,
    diff: Option<ParsedDiff>,
}

/// Window/profile scoped, bounded cache. Device remains part of the identity.
#[derive(Default)]
pub(crate) struct PullRequestCache {
    entries: Vec<((Option<String>, String), DetailSnapshot)>,
}

impl PullRequestCache {
    fn get(&mut self, target: &Option<String>, url: &str) -> Option<DetailSnapshot> {
        let index = self
            .entries
            .iter()
            .position(|((device, entry), _)| device == target && entry == url)?;
        let entry = self.entries.remove(index);
        let snapshot = entry.1.clone();
        self.entries.push(entry);
        Some(snapshot)
    }

    fn put(&mut self, target: Option<String>, url: String, snapshot: DetailSnapshot) {
        self.entries
            .retain(|((device, entry), _)| device != &target || entry != &url);
        self.entries.push(((target, url), snapshot));
        if self.entries.len() > 12 {
            self.entries.remove(0);
        }
    }
}

pub struct PullRequestDetailPage {
    state: Entity<AppState>,
    pub url: String,
    target: Option<String>,
    detail: Option<ChangeRequestDetail>,
    body: Option<crate::markdown::BlockTree>,
    activity_bodies: Vec<crate::markdown::BlockTree>,
    cache: Rc<RefCell<PullRequestCache>>,
    preview: Option<ChangeRequestListItem>,
    error: Option<String>,
    loading: bool,
    fetched: Option<Instant>,
    task: Option<Task<()>>,
    diff_task: Option<Task<()>>,
    copy_reset: Option<Task<()>>,
    copied_link: bool,
    diff: Option<Arc<String>>,
    code_rows: Arc<Vec<CodeRow>>,
    code_files: Arc<Vec<(String, usize)>>,
    code_width: f32,
    code_horizontal: gpui::ScrollHandle,
    code_scroll: gpui::UniformListScrollHandle,
    diff_error: Option<String>,
    tab: Tab,
    files_expanded: bool,
    scroll: widgets::PageScroll,
}

impl PullRequestDetailPage {
    pub(crate) fn new(
        state: Entity<AppState>,
        url: String,
        target: Option<String>,
        cache: Rc<RefCell<PullRequestCache>>,
        preview: Option<ChangeRequestListItem>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut page = Self {
            state,
            url,
            target,
            detail: None,
            body: None,
            activity_bodies: Vec::new(),
            cache,
            preview,
            error: None,
            loading: false,
            fetched: None,
            task: None,
            diff_task: None,
            copy_reset: None,
            copied_link: false,
            diff: None,
            code_rows: Default::default(),
            code_files: Default::default(),
            code_width: 128.0,
            code_horizontal: gpui::ScrollHandle::new(),
            code_scroll: gpui::UniformListScrollHandle::new(),
            diff_error: None,
            tab: Tab::Summary,
            files_expanded: false,
            scroll: widgets::PageScroll::default(),
        };
        let cached = page.cache.borrow_mut().get(&page.target, &page.url);
        if let Some(snapshot) = cached {
            page.fetched = Some(snapshot.fetched);
            page.detail = Some(snapshot.detail);
            page.body = Some(snapshot.body);
            page.activity_bodies = snapshot.activity;
            if let Some(diff) = snapshot.diff {
                page.install_diff(diff);
            }
        }
        if page.detail.is_none() {
            page.load(false, cx);
        }
        page
    }

    pub(crate) fn titlebar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let url = self.url.clone();
        let copy_url = self.url.clone();
        let identity = self
            .detail
            .as_ref()
            .map(|detail| format!("#{} · {}", detail.number, detail.title))
            .or_else(|| {
                self.preview
                    .as_ref()
                    .map(|item| format!("#{} · {}", item.number, item.title))
            })
            .unwrap_or_else(|| "Pull request".into());
        div()
            .w_full()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(4.0))
            .child(
                action("pr-back", "Back to pull requests", &theme)
                    .flex_1()
                    .min_w_0()
                    .w_auto()
                    .justify_start()
                    .px(px(4.0))
                    .aria_label(format!("Back to pull requests from {identity}"))
                    .child(
                        div()
                            .id("pr-back-title")
                            .debug_selector(|| "pr-back-title".into())
                            .min_w_0()
                            .truncate()
                            .text_size(px(12.0))
                            .child(identity),
                    )
                    .on_click(|_, window, cx| {
                        cx.stop_propagation();
                        window.dispatch_action(Box::new(ClosePullRequest), cx)
                    }),
            )
            .child(
                action("pr-detail-refresh", "Refresh pull request", &theme)
                    .when(self.loading, |el| el.opacity(0.4))
                    .on_click(cx.listener(|page, _, _, cx| {
                        cx.stop_propagation();
                        if !page.loading {
                            page.diff = None;
                            let cached = page.cache.borrow_mut().get(&page.target, &page.url);
                            if let Some(mut snapshot) = cached {
                                snapshot.diff = None;
                                snapshot.fetched = Instant::now();
                                page.cache.borrow_mut().put(
                                    page.target.clone(),
                                    page.url.clone(),
                                    snapshot,
                                );
                            }
                            page.load(true, cx);
                            if page.tab == Tab::Code {
                                page.load_diff(true, cx);
                            }
                        }
                    })),
            )
            .child(
                action(
                    "pr-copy-url",
                    if self.copied_link {
                        "Link copied"
                    } else {
                        "Copy link"
                    },
                    &theme,
                )
                .on_click(cx.listener(move |page, _, _, cx| {
                    cx.stop_propagation();
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(copy_url.clone()));
                    page.copied_link = true;
                    page.copy_reset = Some(cx.spawn(async move |page, cx| {
                        cx.background_executor()
                            .timer(std::time::Duration::from_secs(2))
                            .await;
                        let _ = page.update(cx, |page, cx| {
                            page.copied_link = false;
                            cx.notify();
                        });
                    }));
                    cx.notify();
                })),
            )
            .child(
                action("pr-external", "Open in default browser", &theme).on_click(
                    move |_, _, cx| {
                        cx.stop_propagation();
                        cx.open_url(&url);
                    },
                ),
            )
            .into_any_element()
    }

    fn params(&self, refresh: bool) -> serde_json::Value {
        let mut params = serde_json::json!({"url": self.url, "refresh": refresh});
        if let Some(target) = &self.target {
            params["targetDeviceId"] = target.clone().into();
        }
        params
    }

    fn load(&mut self, refresh: bool, cx: &mut Context<Self>) {
        let Some(engine) = self.state.read(cx).engine().cloned() else {
            self.error = Some("Connect to your device to load this pull request.".into());
            return;
        };
        self.loading = true;
        self.error = None;
        let params = self.params(refresh);
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = engine.client().call(methods::GET_CHANGE_REQUEST, params).await
                .map_err(|error| format!("Could not load this PR: {error}. Check GitHub CLI authentication and update the selected device if needed."))
                .and_then(|value| serde_json::from_value::<ChangeRequestDetail>(value).map_err(|error| error.to_string()));
            // Large descriptions and review threads must not stall the UI thread.
            let result = cx.background_executor().spawn(async move {
                result.map(|detail| DetailSnapshot {
                    body: super::pull_request_media::parse_description(&detail.body),
                    activity: detail.comments.iter().chain(&detail.reviews)
                        .map(|comment| super::pull_request_media::parse_description(&comment.body)).collect(),
                    detail, fetched: Instant::now(), diff: None,
                })
            }).await;
            let _ = this.update(cx, |page, cx| {
                page.loading = false;
                match result {
                    Ok(mut snapshot) => {
                        page.fetched = Some(snapshot.fetched);
                        snapshot.diff = page.diff_snapshot();
                        page.body = Some(snapshot.body.clone());
                        page.activity_bodies = snapshot.activity.clone();
                        page.detail = Some(snapshot.detail.clone());
                        page.cache.borrow_mut().put(page.target.clone(), page.url.clone(), snapshot);
                    }
                    Err(error) => page.error = Some(error),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn load_diff(&mut self, refresh: bool, cx: &mut Context<Self>) {
        let Some(engine) = self.state.read(cx).engine().cloned() else {
            self.diff_error = Some("Connect to your device to load the diff.".into());
            return;
        };
        self.diff_error = None;
        let params = self.params(refresh);
        self.diff_task = Some(cx.spawn(async move |this, cx| {
            let result = engine
                .client()
                .call(methods::GET_CHANGE_REQUEST_DIFF, params)
                .await
                .map_err(|error| {
                    format!(
                        "Could not load the diff: {error}. Open GitHub in your browser for large diffs."
                    )
                })
                .and_then(|value| {
                    serde_json::from_value::<String>(value)
                        .map_err(|_| "The device returned an invalid diff.".to_owned())
                });
            let result = cx
                .background_executor()
                .spawn(async move { result.map(ParsedDiff::new) })
                .await;
            let _ = this.update(cx, |page, cx| {
                page.diff_task = None;
                match result {
                    Ok(diff) => {
                        let cached = page.cache.borrow_mut().get(&page.target, &page.url);
                        if let Some(mut snapshot) = cached {
                            snapshot.diff = Some(diff.clone());
                            page.cache.borrow_mut().put(
                                page.target.clone(),
                                page.url.clone(),
                                snapshot,
                            );
                        }
                        page.install_diff(diff);
                    }
                    Err(error) => page.diff_error = Some(error),
                }
                cx.notify();
            });
        }));
    }

    fn diff_snapshot(&self) -> Option<ParsedDiff> {
        Some(ParsedDiff {
            patch: self.diff.clone()?,
            rows: self.code_rows.clone(),
            files: self.code_files.clone(),
            width: self.code_width,
        })
    }

    fn install_diff(&mut self, diff: ParsedDiff) {
        self.diff = Some(diff.patch);
        self.code_rows = diff.rows;
        self.code_files = diff.files;
        self.code_width = diff.width;
    }

    fn navigation(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let radius = 18.0;
        let border = if theme.is_frost() {
            match theme.appearance {
                crate::theme::Appearance::Dark => gpui::hsla(210.0 / 360.0, 0.18, 0.78, 0.09),
                crate::theme::Appearance::Light => gpui::hsla(210.0 / 360.0, 0.18, 0.32, 0.10),
            }
        } else {
            theme.border
        };
        let tabs = div()
            .id("pr-detail-nav")
            .debug_selector(|| "pr-detail-nav".into())
            .p(px(6.0))
            .rounded(px(radius))
            .border_1()
            .border_color(border)
            .when(theme.is_frost(), |el| el.bg(theme.composer_sidebar_tint()))
            .when(!theme.is_frost(), |el| {
                el.bg(theme.input_glass_bg()).shadow_lg()
            })
            .flex()
            .items_center()
            .gap(px(4.0))
            .children(
                [
                    (
                        Tab::Summary,
                        "Summary",
                        "pr-summary",
                        crate::icons::DOCUMENT,
                    ),
                    (Tab::Code, "Code", "pr-code", crate::icons::FILE_CODE),
                    (
                        Tab::Activity,
                        "Activity",
                        "pr-activity",
                        crate::icons::CHAT_ROUND_LINE,
                    ),
                    (Tab::Checks, "Checks", "pr-checks", crate::icons::CHECKLIST),
                ]
                .into_iter()
                .map(|(tab, label, id, glyph)| {
                    crate::surface_chrome::tab(id, tab == self.tab, theme)
                        .debug_selector(move || id.into())
                        .aria_label(label)
                        .h(px(44.0))
                        .rounded(px(12.0))
                        .px(px(2.0))
                        .flex_1()
                        .flex_shrink(1.0)
                        .min_w_0()
                        .flex_col()
                        .gap(px(3.0))
                        .justify_center()
                        .child(crate::icons::icon(glyph).size(px(16.0)).flex_none())
                        .child(
                            div()
                                .min_w_0()
                                .max_w_full()
                                .truncate()
                                .text_size(px(11.0))
                                .child(label),
                        )
                        .on_click(cx.listener(move |page, _, _, cx| page.select_tab(tab, cx)))
                }),
            );
        div()
            .absolute()
            .bottom(px(16.0))
            .left(px(12.0))
            .right(px(12.0))
            .max_w(px(430.0))
            .mx_auto()
            .child(crate::frost::frosted(radius, crate::frost::MENU_BLUR, tabs))
            .into_any_element()
    }

    fn select_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        self.tab = tab;
        self.scroll.scroll.set_offset(gpui::Point::default());
        if tab == Tab::Code && self.diff.is_none() && self.diff_task.is_none() {
            self.load_diff(false, cx);
        }
        cx.notify();
    }
}

fn rich_text(
    body: &crate::markdown::BlockTree,
    key: String,
    url: &str,
    theme: &Theme,
    window: &mut Window,
) -> AnyElement {
    let options = crate::markdown::render::RenderOptions {
        tasks: None,
        media: Some(super::pull_request_media::media(url)),
        row_key: key.into(),
        veil: None,
        cache: None,
        now: Instant::now(),
        link: None,
        workspace_root: None,
        code: None,
        copy: Some(crate::markdown::render::CopyUi {
            handler: Rc::new(|_, code, _, cx| {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(code.to_string()))
            }),
            copied_ix: None,
        }),
    };
    div()
        .debug_selector(|| "pr-rich-text".into())
        .child(crate::markdown::render::render_tree(
            body,
            &options,
            theme,
            window,
            &|_| None,
        ))
        .into_any_element()
}

fn activity_time(raw: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(raw)
        .map(|time| {
            crate::pull_requests::relative_time(
                time.with_timezone(&chrono::Utc),
                chrono::Utc::now(),
            )
        })
        .unwrap_or_else(|_| raw.to_owned())
}

struct PrActionTooltip(&'static str);

impl Render for PrActionTooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx);
        div()
            .px(px(8.0))
            .py(px(6.0))
            .rounded(px(5.0))
            .bg(theme.surface_raised)
            .text_color(theme.text)
            .shadow_md()
            .text_size(crate::typography::ui_rems(11.0))
            .child(self.0)
    }
}

fn action(id: &'static str, label: &'static str, theme: &Theme) -> gpui::Stateful<gpui::Div> {
    let icon_only = matches!(
        id,
        "pr-back" | "pr-external" | "pr-copy-url" | "pr-detail-refresh"
    );
    let glyph = match id {
        "pr-back" => Some(crate::icons::ALT_ARROW_LEFT),
        "pr-copy-url" if label == "Link copied" => Some(crate::icons::CHECK),
        "pr-copy-url" | "pr-copy-patch" => Some(crate::icons::COPY),
        "pr-detail-refresh" | "pr-retry-diff" => Some(crate::icons::REFRESH),
        "pr-external" => Some(crate::icons::ARROW_UP_RIGHT),
        "pr-files" => Some(crate::icons::FOLDER_WITH_FILES),
        "pr-summary" => Some(crate::icons::DOCUMENT),
        "pr-code" => Some(crate::icons::FILE_CODE),
        "pr-activity" => Some(crate::icons::CHAT_ROUND_LINE),
        _ => None,
    };
    widgets::ghost_action(theme)
        .id(id)
        .debug_selector(move || id.to_owned())
        .role(gpui::Role::Button)
        .aria_label(label)
        .tab_index(0)
        .border_1()
        .border_color(gpui::transparent_black())
        .focus_visible(|style| style.border_color(theme.accent))
        .cursor_pointer()
        .hover(|style| style.bg(theme.glass_hover()))
        .when(icon_only, |el| {
            el.size(px(24.0))
                .flex_none()
                .rounded(px(6.0))
                .occlude()
                .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                    window.prevent_default()
                })
                .px_0()
                .py_0()
                .justify_center()
                .tooltip(move |_, cx| cx.new(|_| PrActionTooltip(label)).into())
        })
        .children(glyph.map(|glyph| {
            crate::icons::icon(glyph)
                .size(px(14.0))
                .text_color(theme.text_muted)
        }))
        .when(!icon_only, |el| el.child(label))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StatusTone {
    Neutral,
    Positive,
    Warning,
    Negative,
    Merged,
}

fn status_style(raw: &str) -> (String, &'static str, StatusTone) {
    use crate::icons;
    match raw.to_ascii_uppercase().replace(' ', "_").as_str() {
        "OPEN" => ("Open".into(), icons::PULL_REQUEST, StatusTone::Positive),
        "MERGED" => ("Merged".into(), icons::PULL_REQUEST, StatusTone::Merged),
        "CLOSED" => ("Closed".into(), icons::CLOSE_CIRCLE, StatusTone::Negative),
        "DRAFT" => ("Draft".into(), icons::DOCUMENT, StatusTone::Neutral),
        "APPROVED" => ("Approved".into(), icons::CHECK, StatusTone::Positive),
        "SUCCESS" => ("Passed".into(), icons::CHECK, StatusTone::Positive),
        "FAILURE" | "ERROR" | "TIMED_OUT" | "ACTION_REQUIRED" => {
            (humanize(raw), icons::CLOSE_CIRCLE, StatusTone::Negative)
        }
        "CHANGES_REQUESTED" => (
            "Changes requested".into(),
            icons::DANGER_TRIANGLE,
            StatusTone::Warning,
        ),
        "REVIEW_REQUIRED" => (
            "Awaiting review".into(),
            icons::CLOCK_CIRCLE,
            StatusTone::Warning,
        ),
        "IN_PROGRESS" | "PENDING" | "QUEUED" | "WAITING" => {
            (humanize(raw), icons::CLOCK_CIRCLE, StatusTone::Warning)
        }
        _ => (humanize(raw), icons::DOCUMENT, StatusTone::Neutral),
    }
}

fn humanize(raw: &str) -> String {
    let text = raw.replace('_', " ").to_lowercase();
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_else(|| "Not reported".into())
}

fn status_chip(raw: &str, theme: &Theme) -> AnyElement {
    let (label, glyph, tone) = status_style(raw);
    let color = match tone {
        StatusTone::Neutral => theme.text_muted,
        StatusTone::Positive => theme.success,
        StatusTone::Warning => theme.warning,
        StatusTone::Negative => theme.danger,
        StatusTone::Merged => theme.code_text,
    };
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(6.0))
        .px(px(8.0))
        .py(px(3.0))
        .rounded(px(6.0))
        .bg(color.opacity(0.08))
        .text_color(theme.text)
        .text_size(crate::typography::ui_rems(12.0))
        .child(crate::icons::icon(glyph).size(px(14.0)).text_color(color))
        .child(label)
        .into_any_element()
}

fn section_heading(label: &str, glyph: &'static str, theme: &Theme) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(
            crate::icons::icon(glyph)
                .size(px(16.0))
                .text_color(theme.text_muted),
        )
        .child(widgets::row_title(theme, label))
        .into_any_element()
}

fn field(label: &str, value: String, theme: &Theme) -> AnyElement {
    let content = if matches!(label, "Status" | "Review") {
        status_chip(&value, theme)
    } else if label == "Changes" {
        div()
            .flex()
            .flex_wrap()
            .gap(px(6.0))
            .children(value.split_whitespace().map(|word| {
                div()
                    .text_color(if word.starts_with('+') {
                        theme.success
                    } else if word.starts_with('−') {
                        theme.danger
                    } else {
                        theme.text_muted
                    })
                    .child(word.to_owned())
            }))
            .into_any_element()
    } else {
        div()
            .font_family(theme.font_mono.clone())
            .text_size(crate::typography::ui_rems(12.0))
            .child(value)
            .into_any_element()
    };
    div()
        .flex()
        .items_start()
        .gap(px(12.0))
        .py(px(7.0))
        .child(
            div()
                .w(px(94.0))
                .flex_none()
                .flex()
                .items_center()
                .gap(px(8.0))
                .text_color(theme.text_muted)
                .child(SharedString::from(label.to_owned())),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .child(content),
        )
        .into_any_element()
}

impl crate::popover::ScrollRailHost for PullRequestDetailPage {
    fn rail_bar(&mut self) -> &mut crate::popover::MenuScrollbarState {
        self.scroll.rail_bar()
    }
    fn rail_scroll(&self) -> Option<gpui::ScrollHandle> {
        self.scroll.rail_scroll()
    }
}

impl Render for PullRequestDetailPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let content = {
            let mut column = widgets::page_column()
                .pt(px(32.0))
                .pb(px(96.0))
                .text_size(crate::typography::ui_rems(13.0))
                .text_color(theme.text);
            if let Some(error) = &self.error {
                column = column.child(widgets::error_strip(&theme, error.clone()));
            }
            if let Some(detail) = &self.detail {
                let repository = self
                    .url
                    .split('/')
                    .skip(3)
                    .take(2)
                    .collect::<Vec<_>>()
                    .join("/");
                column = column
                    .child(
                        div()
                            .mb(px(12.0))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                crate::icons::icon(crate::icons::FOLDER_WITH_FILES)
                                    .size(px(14.0))
                                    .text_color(theme.text_muted),
                            )
                            .child(
                                div()
                                    .text_size(crate::typography::ui_rems(12.0))
                                    .text_color(theme.text_muted)
                                    .child(repository),
                            ),
                    )
                    .child(
                        div()
                            .text_size(crate::typography::ui_rems(22.0))
                            .line_height(px(30.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(detail.title.clone()),
                    )
                    .child(
                        div()
                            .mt(px(8.0))
                            .text_color(theme.text_muted)
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(super::pull_request_media::avatar(
                                &detail.author.login,
                                "pr-author".into(),
                                20.0,
                                &theme,
                            ))
                            .child(if detail.author.login.is_empty() {
                                "Deleted account".to_owned()
                            } else {
                                detail.author.login.clone()
                            })
                            .child(format!("· #{}", detail.number)),
                    )
                    .child(
                        div()
                            .mt(px(24.0))
                            .child(field(
                                "Branch",
                                format!("{} → {}", detail.head_ref_name, detail.base_ref_name),
                                &theme,
                            ))
                            .child(field(
                                "Status",
                                if detail.is_draft {
                                    "Draft".into()
                                } else {
                                    detail.state.clone()
                                },
                                &theme,
                            ))
                            .child(field(
                                "Review",
                                if detail.review_decision.is_empty() {
                                    "No review decision".into()
                                } else {
                                    detail.review_decision.replace('_', " ").to_lowercase()
                                },
                                &theme,
                            ))
                            .child(field(
                                "Changes",
                                format!(
                                    "{} files · +{} −{}",
                                    detail.files.len(),
                                    detail.additions,
                                    detail.deletions
                                ),
                                &theme,
                            )),
                    );
                if let Some(fetched) = self.fetched {
                    let age = if fetched.elapsed().as_secs() < 60 {
                        "just now".into()
                    } else {
                        format!("{}m ago", fetched.elapsed().as_secs() / 60)
                    };
                    column = column.child(
                        div()
                            .mt(px(12.0))
                            .text_size(px(11.0))
                            .text_color(theme.text_muted)
                            .child(format!("Loaded {age} · Refresh to check for changes")),
                    );
                }
                column = column.child(div().h(px(24.0)));
                match self.tab {
                    Tab::Summary => {
                        column = column.child(section_heading(
                            "Description",
                            crate::icons::DOCUMENT,
                            &theme,
                        ));
                        if detail.body.is_empty() {
                            column = column.child(
                                div()
                                    .mt(px(12.0))
                                    .text_color(theme.text_muted)
                                    .child("No description provided."),
                            );
                        } else if let Some(body) = &self.body {
                            column = column.child(div().mt(px(12.0)).child(rich_text(
                                body,
                                "pr-description".into(),
                                &self.url,
                                &theme,
                                window,
                            )));
                        }
                    }
                    Tab::Checks => {
                        column = column.child(div().child(section_heading(
                            "Checks",
                            crate::icons::CHECKLIST,
                            &theme,
                        )));
                        if detail.status_check_rollup.is_empty() {
                            column = column.child(
                                div()
                                    .mt(px(12.0))
                                    .text_color(theme.text_muted)
                                    .child("No checks reported."),
                            );
                        }
                        for (index, check) in detail.status_check_rollup.iter().enumerate() {
                            let name = if check.name.is_empty() {
                                &check.context
                            } else {
                                &check.name
                            };
                            let status = [&check.conclusion, &check.state, &check.status]
                                .into_iter()
                                .find(|v| !v.is_empty())
                                .map(|v| v.replace('_', " ").to_lowercase())
                                .unwrap_or_else(|| "Pending".into());
                            let link = if check.details_url.is_empty() {
                                &check.target_url
                            } else {
                                &check.details_url
                            }
                            .clone();
                            column = column.child(
                                div()
                                    .id(SharedString::from(format!("pr-check-{index}")))
                                    .py(px(10.0))
                                    .flex()
                                    .flex_wrap()
                                    .gap(px(12.0))
                                    .child(div().flex_1().min_w_0().child(name.clone()))
                                    .child(status_chip(&status, &theme))
                                    .when(!link.is_empty(), |el| {
                                        el.cursor_pointer()
                                            .role(gpui::Role::Link)
                                            .tab_index(0)
                                            .aria_label(format!("Open {name}"))
                                            .rounded(px(6.0))
                                            .focus_visible(|style| style.bg(theme.glass_hover()))
                                            .hover(|style| style.bg(theme.glass_hover()))
                                            .on_click(move |_, _, cx| cx.open_url(&link))
                                    }),
                            );
                        }
                    }
                    Tab::Code => {
                        if let Some(error) = &self.diff_error {
                            column = column
                                .child(widgets::error_strip(&theme, error.clone()))
                                .child(action("pr-retry-diff", "Retry diff", &theme).on_click(
                                    cx.listener(|page, _, _, cx| page.load_diff(true, cx)),
                                ));
                        } else if let Some(diff) = &self.diff {
                            let patch = diff.clone();
                            column = column.child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(
                                        action("pr-files", "Changed files", &theme)
                                            .aria_expanded(self.files_expanded)
                                            .child(self.code_files.len().to_string())
                                            .child(
                                                crate::icons::icon(if self.files_expanded {
                                                    crate::icons::ALT_ARROW_UP
                                                } else {
                                                    crate::icons::ALT_ARROW_DOWN
                                                })
                                                .size(px(12.0))
                                                .text_color(theme.text_muted),
                                            )
                                            .on_click(cx.listener(|page, _, _, cx| {
                                                page.files_expanded = !page.files_expanded;
                                                cx.notify();
                                            })),
                                    )
                                    .child(div().flex_1())
                                    .child(action("pr-copy-patch", "Copy diff", &theme).on_click(
                                        move |_, _, cx| {
                                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                                patch.as_ref().clone(),
                                            ));
                                        },
                                    )),
                            );
                            if self.files_expanded {
                                let mut file_list = div()
                                    .id("pr-file-list")
                                    .max_h(px(160.0))
                                    .overflow_y_scroll()
                                    .mt(px(8.0));
                                for (index, (path, offset)) in self.code_files.iter().enumerate() {
                                    let offset = *offset;
                                    file_list = file_list.child(
                                        widgets::ghost_action(&theme)
                                            .id(SharedString::from(format!("pr-file-{index}")))
                                            .role(gpui::Role::Button)
                                            .aria_label(format!("Jump to {path}"))
                                            .focus_visible(|style| style.bg(theme.glass_hover()))
                                            .hover(|style| style.bg(theme.glass_hover()))
                                            .tab_index(0)
                                            .on_click(cx.listener(move |page, _, _, cx| {
                                                page.code_scroll.scroll_to_item(
                                                    offset,
                                                    gpui::ScrollStrategy::Top,
                                                );
                                                page.files_expanded = false;
                                                cx.notify();
                                            }))
                                            .child(
                                                crate::icons::icon(crate::icons::FILE_CODE)
                                                    .size(px(14.0))
                                                    .text_color(theme.text_muted),
                                            )
                                            .child(div().min_w_0().truncate().child(path.clone())),
                                    );
                                }
                                column = column.child(file_list);
                            }
                            let rows = self.code_rows.clone();
                            let code_width = (self.code_width - 128.0) / 7.0
                                * crate::changes::diff_text_size(&theme)
                                * 0.7
                                + 144.0;
                            let colors = theme.clone();
                            let code_scroll = self.code_scroll.0.borrow().base_handle.clone();
                            column =
                                column.child(
                                    div()
                                        .id("pr-code-viewport")
                                        .debug_selector(|| "pr-code-viewport".into())
                                        .mt(px(16.0))
                                        .h(px(440.0))
                                        .overflow_x_scroll()
                                        .track_scroll(&self.code_horizontal)
                                        .child(
                                            crate::edge_fade::edge_faded(
                                                16.0,
                                                true,
                                                true,
                                                gpui::uniform_list(
                                                    "pr-code-lines",
                                                    rows.len(),
                                                    move |range, _, _| {
                                                        range
                                                    .map(|index| {
                                                        let row = &rows[index];
                                                        if row.kind == crate::changes::LineKind::Meta {
                                                            div().w_full().h(px(crate::changes::diff_line_height(&colors)))
                                                                .px(px(12.0)).bg(colors.glass_hover())
                                                                .font_family(colors.font_mono.clone())
                                                                .text_size(px(crate::changes::diff_text_size(&colors)))
                                                                .text_color(colors.text_muted).child(row.text.clone()).into_any_element()
                                                        } else {
                                                            crate::changes::readonly_diff_line(&crate::changes::DiffLine {
                                                                kind: row.kind, old_no: row.old.parse().ok(), new_no: row.new.parse().ok(),
                                                                text: row.text.to_string(),
                                                            }, &colors)
                                                        }
                                                    })
                                                    .collect::<Vec<_>>()
                                                    },
                                                )
                                                .w(px(code_width))
                                                .min_w_full()
                                                .h_full()
                                                .track_scroll(&self.code_scroll),
                                            )
                                            .fade_overflow_y(&code_scroll),
                                        ),
                                );
                        } else {
                            column = column.child(div().mt(px(20.0)).child("Loading diff…"));
                        }
                    }
                    Tab::Activity => {
                        let mut activity: Vec<_> = detail
                            .comments
                            .iter()
                            .chain(detail.reviews.iter())
                            .enumerate()
                            .collect();
                        activity.sort_by_key(|(_, comment)| {
                            if comment.created_at.is_empty() {
                                &comment.submitted_at
                            } else {
                                &comment.created_at
                            }
                        });
                        if activity.is_empty() {
                            column = column.child(
                                div()
                                    .text_color(theme.text_muted)
                                    .child("No comments or reviews yet."),
                            );
                        }
                        for (index, comment) in activity {
                            column = column.child(
                                div()
                                    .mb(px(24.0))
                                    .flex()
                                    .flex_col()
                                    .gap(px(8.0))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_wrap()
                                            .items_center()
                                            .gap(px(8.0))
                                            .child(super::pull_request_media::avatar(
                                                &comment.author.login,
                                                format!("pr-actor-{index}").into(),
                                                24.0,
                                                &theme,
                                            ))
                                            .child(
                                                div().font_weight(gpui::FontWeight::MEDIUM).child(
                                                    if comment.author.login.is_empty() {
                                                        "Deleted account".to_owned()
                                                    } else {
                                                        comment.author.login.clone()
                                                    },
                                                ),
                                            )
                                            .when(!comment.state.is_empty(), |el| {
                                                el.child(status_chip(&comment.state, &theme))
                                            }),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .text_color(theme.text_muted)
                                            .child(activity_time(
                                                if comment.created_at.is_empty() {
                                                    &comment.submitted_at
                                                } else {
                                                    &comment.created_at
                                                },
                                            )),
                                    )
                                    .children(self.activity_bodies.get(index).map(|body| {
                                        rich_text(
                                            body,
                                            format!("pr-activity-{index}"),
                                            &self.url,
                                            &theme,
                                            window,
                                        )
                                    })),
                            );
                        }
                        column =
                            column.child(div().text_color(theme.text_muted).child(
                                "Open GitHub in your browser to comment, review, or merge.",
                            ));
                    }
                }
            } else if self.loading {
                if let Some(preview) = &self.preview {
                    column = column
                        .child(
                            div()
                                .text_color(theme.text_muted)
                                .child(preview.repository.clone()),
                        )
                        .child(
                            div()
                                .mt(px(12.0))
                                .text_size(crate::typography::ui_rems(22.0))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .child(preview.title.clone()),
                        );
                }
                column = column.child(
                    div()
                        .py(px(32.0))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(crate::loaders::mini_glyph_spinner(
                            "pr-detail-loading",
                            1.75,
                            theme.glyph,
                            cx.entity_id(),
                            cx,
                        ))
                        .child("Loading pull request…"),
                );
            }
            let scroll = self.scroll.scroll.clone();
            let rail = crate::popover::rail(self, "pr-detail-scrollbar", &theme, cx);
            div()
                .id("pr-detail-scroll-host")
                .flex_1()
                .min_h_0()
                .relative()
                .on_hover(cx.listener(|page, hovered: &bool, _, cx| {
                    if page.scroll.set_list_hovered(*hovered) {
                        cx.notify();
                    }
                }))
                .child(
                    crate::edge_fade::edge_faded(
                        24.0,
                        true,
                        true,
                        div()
                            .id("pr-detail-scroll")
                            .debug_selector(|| "pr-detail-scroll".into())
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&scroll)
                            .child(column),
                    )
                    .fade_overflow_y(&scroll),
                )
                .children(rail)
                .into_any_element()
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .pt(px(Theme::TITLEBAR_HEIGHT))
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .child(content)
                    .child(self.navigation(&theme, cx)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(title: &str) -> DetailSnapshot {
        DetailSnapshot {
            detail: ChangeRequestDetail {
                title: title.into(),
                ..Default::default()
            },
            body: crate::markdown::parse_full("**Rich description**"),
            activity: vec![crate::markdown::parse_full("### Review\n\n- **important**")],
            fetched: Instant::now(),
            diff: None,
        }
    }

    #[test]
    fn pull_request_cache_is_bounded_and_separates_devices() {
        let mut cache = PullRequestCache::default();
        cache.put(None, "same".into(), snapshot("Local"));
        cache.put(Some("remote".into()), "same".into(), snapshot("Remote"));
        assert_eq!(cache.get(&None, "same").unwrap().detail.title, "Local");
        assert_eq!(
            cache
                .get(&Some("remote".into()), "same")
                .unwrap()
                .detail
                .title,
            "Remote"
        );
        for index in 0..12 {
            cache.put(None, format!("url-{index}"), snapshot("PR"));
        }
        assert_eq!(cache.entries.len(), 12);
        assert!(cache.get(&None, "same").is_none());
        cache.get(&None, "url-0");
        cache.put(None, "new".into(), snapshot("New"));
        assert!(cache.get(&None, "url-0").is_some());
        assert!(cache.get(&None, "url-1").is_none());
    }

    #[gpui::test]
    fn pull_request_reopening_uses_cached_content_without_a_network_request(
        cx: &mut gpui::TestAppContext,
    ) {
        use gpui::AppContext;
        cx.update(|cx| cx.set_global(Theme::default()));
        let (_, cx) = cx.add_window_view(|window, cx| {
            let state = cx.new(|_| AppState::new());
            let cache = Rc::new(RefCell::new(PullRequestCache::default()));
            let url = "https://github.com/a/b/pull/1";
            let diff = ParsedDiff::new(
                "diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1 +1 @@\n-old\n+new\n".into(),
            );
            let rows = diff.rows.clone();
            let mut cached = snapshot("Already loaded");
            cached.diff = Some(diff);
            cached.fetched = Instant::now() - Duration::from_secs(3600);
            cache.borrow_mut().put(None, url.into(), cached);
            let page = PullRequestDetailPage::new(state, url.into(), None, cache, None, window, cx);
            // No engine exists in this fixture. Attempting a fetch would set an error.
            assert!(!page.loading && page.error.is_none());
            assert_eq!(page.detail.as_ref().unwrap().title, "Already loaded");
            assert_eq!(page.activity_bodies.len(), 1);
            assert!(
                Arc::ptr_eq(&page.code_rows, &rows),
                "cached diff rows are reused without parsing or copying"
            );
            page
        });
        cx.run_until_parked();
    }

    #[test]
    fn status_cues_distinguish_passed_pending_failed_and_skipped() {
        assert_eq!(status_style("SUCCESS").2, StatusTone::Positive);
        assert_eq!(status_style("IN_PROGRESS").2, StatusTone::Warning);
        assert_eq!(status_style("FAILURE").2, StatusTone::Negative);
        assert_eq!(status_style("SKIPPED").2, StatusTone::Neutral);
        assert_eq!(status_style("CANCELLED").2, StatusTone::Neutral);
        assert_eq!(status_style("MERGED").2, StatusTone::Merged);
        assert_eq!(status_style("CHANGES_REQUESTED").0, "Changes requested");
        assert_eq!(status_style("REVIEW_REQUIRED").0, "Awaiting review");
    }

    #[test]
    fn pull_request_code_rows_preserve_sides_and_file_jump_offsets() {
        let patch = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1,2 +1,2 @@\n context\n-old\n+new\n";
        let (rows, files) = code_rows(patch);
        assert_eq!(files, vec![("a.rs".into(), 0)]);
        let added = rows
            .iter()
            .find(|row| row.kind == crate::changes::LineKind::Add)
            .unwrap();
        assert_eq!(added.old, "");
        assert_eq!(added.new, "2");
        let removed = rows
            .iter()
            .find(|row| row.kind == crate::changes::LineKind::Del)
            .unwrap();
        assert_eq!(removed.old, "2");
        assert_eq!(removed.new, "");
    }

    struct DetailHost {
        page: Entity<PullRequestDetailPage>,
        _subscription: Subscription,
        returned: bool,
    }

    impl DetailHost {
        fn new(window: &mut Window, cx: &mut Context<Self>, loaded: bool) -> Self {
            let state = cx.new(|_| AppState::new());
            let page = cx.new(|cx| {
                let mut page = PullRequestDetailPage::new(
                    state, "https://github.com/a/b/pull/1".into(),
                    Some("remote-device".into()),
                    Rc::new(RefCell::new(PullRequestCache::default())), None, window, cx,
                );
                if loaded {
                    page.error = None;
                    page.detail = Some(ChangeRequestDetail {
                        title: "Inspect a pull request with a very long title that must leave room for all actions".into(),
                        number: 1, body: "A description".into(), ..Default::default()
                    });
                    page.body = Some(crate::markdown::parse_full("A description"));
                    let review = "### Review notes\n\n**Strong** text and [a link](https://github.com).\n\n```rust\nlet answer = 42;\n```";
                    page.detail.as_mut().unwrap().reviews.push(zeron_proto::ChangeRequestComment {
                        body: review.into(), ..Default::default()
                    });
                    page.activity_bodies = vec![super::super::pull_request_media::parse_description(review)];
                    let patch = format!("diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-old\n+{}\n", "long_expression_".repeat(30));
                    let (rows, files) = code_rows(&patch);
                    page.code_width = code_content_width(&rows);
                    page.code_rows = Arc::new(rows);
                    page.code_files = Arc::new(files);
                    page.diff = Some(Arc::new(patch));
                }
                page
            });
            let subscription = cx.observe(&page, |_, _, cx| cx.notify());
            Self {
                page,
                _subscription: subscription,
                returned: false,
            }
        }
    }

    impl Render for DetailHost {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let header = self.page.update(cx, |page, cx| page.titlebar(cx));
            div()
                .size_full()
                .relative()
                .on_action(cx.listener(|host, _: &ClosePullRequest, _, cx| {
                    host.returned = true;
                    cx.notify();
                }))
                .child(self.page.clone())
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .w_full()
                        .h(px(Theme::TITLEBAR_HEIGHT))
                        .flex()
                        .items_center()
                        .child(header),
                )
        }
    }

    #[gpui::test]
    fn pull_request_detail_actions_fit_and_tabs_switch(cx: &mut gpui::TestAppContext) {
        use gpui::AppContext;
        cx.update(|cx| cx.set_global(Theme::default()));
        let (host, cx) = cx.add_window_view(|window, cx| DetailHost::new(window, cx, true));
        let page = host.read_with(cx, |host, _| host.page.clone());
        page.read_with(cx, |page, _| {
            assert_eq!(page.params(false)["targetDeviceId"], "remote-device")
        });
        for width in [240.0, 320.0, 600.0, 900.0] {
            cx.simulate_resize(gpui::size(px(width), px(800.0)));
            cx.run_until_parked();
            let mut previous_right = px(0.0);
            for selector in ["pr-back", "pr-detail-refresh", "pr-copy-url", "pr-external"] {
                let bounds = cx.debug_bounds(selector).unwrap();
                assert!(
                    bounds.left() >= previous_right && bounds.right() <= px(width),
                    "{selector}: {bounds:?}"
                );
                assert!(bounds.top() >= px(0.0) && bounds.bottom() <= px(Theme::TITLEBAR_HEIGHT));
                previous_right = bounds.right();
            }
            assert!(cx.debug_bounds("pr-immersive").is_none());
            assert!(cx.debug_bounds("pr-browser").is_none());
            let nav = cx.debug_bounds("pr-detail-nav").unwrap();
            assert_eq!(nav.bottom(), px(784.0));
            assert!(nav.top() > px(700.0));
            for selector in ["pr-summary", "pr-code", "pr-checks", "pr-activity"] {
                let bounds = cx.debug_bounds(selector).unwrap();
                assert!(
                    bounds.left() >= px(0.0) && bounds.right() <= px(width),
                    "{selector}: {bounds:?}"
                );
            }
        }
        let checks = cx.debug_bounds("pr-checks").unwrap();
        cx.simulate_mouse_down(
            checks.center(),
            gpui::MouseButton::Left,
            gpui::Modifiers::default(),
        );
        cx.simulate_mouse_up(
            checks.center(),
            gpui::MouseButton::Left,
            gpui::Modifiers::default(),
        );
        cx.run_until_parked();
        page.read_with(cx, |page, _| assert!(page.tab == Tab::Checks));
        let code = cx.debug_bounds("pr-code").unwrap();
        cx.simulate_mouse_down(
            code.center(),
            gpui::MouseButton::Left,
            gpui::Modifiers::default(),
        );
        cx.simulate_mouse_up(
            code.center(),
            gpui::MouseButton::Left,
            gpui::Modifiers::default(),
        );
        cx.run_until_parked();
        page.read_with(cx, |page, _| assert!(page.tab == Tab::Code));
        assert!(cx.debug_bounds("pr-copy-patch").is_some());
        let copy = cx.debug_bounds("pr-copy-url").unwrap();
        cx.simulate_mouse_down(
            copy.center(),
            gpui::MouseButton::Left,
            gpui::Modifiers::default(),
        );
        cx.simulate_mouse_up(
            copy.center(),
            gpui::MouseButton::Left,
            gpui::Modifiers::default(),
        );
        cx.update(|_, cx| {
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().unwrap(),
                "https://github.com/a/b/pull/1"
            )
        });
        page.read_with(cx, |page, _| {
            assert!(page.code_horizontal.max_offset().x > px(0.0))
        });
        let files = cx.debug_bounds("pr-files").unwrap();
        cx.simulate_mouse_down(
            files.center(),
            gpui::MouseButton::Left,
            gpui::Modifiers::default(),
        );
        cx.simulate_mouse_up(
            files.center(),
            gpui::MouseButton::Left,
            gpui::Modifiers::default(),
        );
        page.read_with(cx, |page, _| assert!(page.files_expanded));
        let activity = cx.debug_bounds("pr-activity").unwrap();
        cx.simulate_mouse_down(
            activity.center(),
            gpui::MouseButton::Left,
            gpui::Modifiers::default(),
        );
        cx.simulate_mouse_up(
            activity.center(),
            gpui::MouseButton::Left,
            gpui::Modifiers::default(),
        );
        cx.run_until_parked();
        page.read_with(cx, |page, _| assert!(page.tab == Tab::Activity));
        assert!(cx.debug_bounds("pr-rich-text").is_some());
        cx.simulate_resize(gpui::size(px(900.0), px(400.0)));
        cx.run_until_parked();
        let nav = cx.debug_bounds("pr-detail-nav").unwrap();
        page.update(cx, |page, cx| {
            page.scroll
                .scroll
                .set_offset(gpui::point(px(0.0), px(-150.0)));
            cx.notify();
        });
        cx.run_until_parked();
        assert_eq!(
            cx.debug_bounds("pr-detail-nav").unwrap(),
            nav,
            "navigation must not move with content"
        );
        page.read_with(cx, |page, _| {
            assert!(page.scroll.scroll.offset().y < px(0.0))
        });
        let title = cx.debug_bounds("pr-back-title").unwrap();
        let back = cx.debug_bounds("pr-back").unwrap();
        assert!(title.size.width > px(100.0) && back.contains(&title.center()));
        cx.simulate_mouse_down(
            title.center(),
            gpui::MouseButton::Left,
            gpui::Modifiers::default(),
        );
        cx.simulate_mouse_up(
            title.center(),
            gpui::MouseButton::Left,
            gpui::Modifiers::default(),
        );
        host.read_with(cx, |host, _| {
            assert!(host.returned, "the title is part of the back action")
        });
    }

    #[gpui::test]
    fn pull_request_error_keeps_retry_and_browser_actions(cx: &mut gpui::TestAppContext) {
        use gpui::AppContext;
        cx.update(|cx| cx.set_global(Theme::default()));
        let (_, cx) = cx.add_window_view(|window, cx| DetailHost::new(window, cx, false));
        cx.simulate_resize(gpui::size(px(320.0), px(800.0)));
        cx.run_until_parked();
        for selector in ["pr-detail-refresh", "pr-external"] {
            assert!(cx.debug_bounds(selector).is_some(), "{selector}");
        }
    }
}

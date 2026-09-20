//! Native, read-only PR inspection with explicit browser escape routes.
use crate::{
    browser::{BrowserContext, BrowserSurface},
    settings::{self, PullRequestDestination, SavePolicy, widgets},
    state::AppState,
    theme::Theme,
};
use gpui::{
    Action, AnyElement, App, Context, Entity, IntoElement, Render, SharedString, Subscription,
    Task, Window, div, prelude::*, px,
};
use zeron_proto::ChangeRequestDetail;
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
                    .when(selected, |el| el.bg(theme.selection).text_color(theme.text))
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

pub struct PullRequestDetailPage {
    state: Entity<AppState>,
    pub url: String,
    target: Option<String>,
    detail: Option<ChangeRequestDetail>,
    body: Option<crate::markdown::BlockTree>,
    error: Option<String>,
    loading: bool,
    task: Option<Task<()>>,
    diff_task: Option<Task<()>>,
    copy_reset: Option<Task<()>>,
    copied_link: bool,
    diff: Option<String>,
    code_rows: std::rc::Rc<Vec<CodeRow>>,
    code_files: Vec<(String, usize)>,
    code_scroll: gpui::UniformListScrollHandle,
    diff_error: Option<String>,
    tab: Tab,
    scroll: widgets::PageScroll,
    browser_context: BrowserContext,
    browser: Option<Entity<BrowserSurface>>,
    browser_events: Option<Subscription>,
}

impl PullRequestDetailPage {
    pub fn new(
        state: Entity<AppState>,
        url: String,
        target: Option<String>,
        browser_context: BrowserContext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut page = Self {
            state,
            url,
            target,
            detail: None,
            body: None,
            error: None,
            loading: false,
            task: None,
            diff_task: None,
            copy_reset: None,
            copied_link: false,
            diff: None,
            code_rows: Default::default(),
            code_files: Vec::new(),
            code_scroll: gpui::UniformListScrollHandle::new(),
            diff_error: None,
            tab: Tab::Summary,
            scroll: widgets::PageScroll::default(),
            browser_context,
            browser: None,
            browser_events: None,
        };
        if settings::current(cx).pull_request_destination == PullRequestDestination::Browser {
            page.show_browser(window, cx);
        } else {
            page.load(cx);
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
            .unwrap_or_else(|| "Pull request".into());
        div()
            .w_full()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(4.0))
            .child(
                action("pr-back", "Back to board", &theme).on_click(|_, window, cx| {
                    cx.stop_propagation();
                    window.dispatch_action(Box::new(ClosePullRequest), cx)
                }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .px(px(8.0))
                    .overflow_hidden()
                    .text_size(crate::typography::ui_rems(12.0))
                    .text_color(theme.text_muted)
                    .text_ellipsis()
                    .child(identity),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    .p(px(2.0))
                    .rounded(px(8.0))
                    .bg(theme.glass_hover())
                    .child(
                        action("pr-native", "PR view", &theme)
                            .aria_selected(self.browser.is_none())
                            .when(self.browser.is_none(), |el| el.bg(theme.surface_raised))
                            .on_click(cx.listener(|page, _, _, cx| {
                                cx.stop_propagation();
                                page.browser = None;
                                page.browser_events = None;
                                if page.detail.is_none() {
                                    page.load(cx);
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        action("pr-browser", "Zeron browser", &theme)
                            .aria_selected(self.browser.is_some())
                            .when(self.browser.is_some(), |el| el.bg(theme.surface_raised))
                            .on_click(cx.listener(|page, _, window, cx| {
                                cx.stop_propagation();
                                page.show_browser(window, cx);
                            })),
                    ),
            )
            .child(
                action("pr-detail-refresh", "Refresh pull request", &theme)
                    .when(self.loading && self.browser.is_none(), |el| el.opacity(0.4))
                    .on_click(cx.listener(|page, _, _, cx| {
                        cx.stop_propagation();
                        if let Some(browser) = &page.browser {
                            browser.update(cx, |browser, cx| browser.reload(cx));
                        } else if !page.loading {
                            page.diff = None;
                            page.load(cx);
                            if page.tab == Tab::Code {
                                page.load_diff(cx);
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

    fn params(&self) -> serde_json::Value {
        let mut params = serde_json::json!({"url": self.url});
        if let Some(target) = &self.target {
            params["targetDeviceId"] = target.clone().into();
        }
        params
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        let Some(engine) = self.state.read(cx).engine().cloned() else {
            self.error = Some("Connect to your device to load this pull request.".into());
            return;
        };
        self.loading = true;
        self.error = None;
        let params = self.params();
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = engine.client().call(methods::GET_CHANGE_REQUEST, params).await
                .map_err(|error| format!("Could not load this PR: {error}. Check GitHub CLI authentication and update the selected device if needed."))
                .and_then(|value| serde_json::from_value::<ChangeRequestDetail>(value).map_err(|error| error.to_string()));
            let _ = this.update(cx, |page, cx| {
                page.loading = false;
                match result {
                    Ok(detail) => { page.body = Some(super::pull_request_media::parse_description(&detail.body)); page.detail = Some(detail); }
                    Err(error) => page.error = Some(error),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn load_diff(&mut self, cx: &mut Context<Self>) {
        let Some(engine) = self.state.read(cx).engine().cloned() else {
            self.diff_error = Some("Connect to your device to load the diff.".into());
            return;
        };
        self.diff_error = None;
        let params = self.params();
        self.diff_task = Some(cx.spawn(async move |this, cx| {
            let result = engine.client().call(methods::GET_CHANGE_REQUEST_DIFF, params).await;
            let _ = this.update(cx, |page, cx| {
                page.diff_task = None;
                match result {
                    Ok(value) => match serde_json::from_value::<String>(value) {
                        Ok(diff) => {
                            let (rows, files) = code_rows(&diff);
                            page.code_rows = std::rc::Rc::new(rows);
                            page.code_files = files;
                            page.diff = Some(diff);
                        },
                        Err(_) => page.diff_error = Some("The device returned an invalid diff.".into()),
                    },
                    Err(error) => page.diff_error = Some(format!("Could not load the diff: {error}. Open the browser view for large diffs.")),
                }
                cx.notify();
            });
        }));
    }

    fn show_browser(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let browser =
            cx.new(|cx| BrowserSurface::new(self.browser_context.clone(), false, window, cx));
        browser.update(cx, |browser, cx| browser.navigate(&self.url, window, cx));
        self.browser_events =
            Some(
                cx.subscribe_in(&browser, window, |page, browser, event, window, cx| {
                    match event {
                        crate::browser::BrowserEvent::NewTab(Some(url)) => {
                            browser.update(cx, |browser, cx| browser.navigate(url, window, cx))
                        }
                        crate::browser::BrowserEvent::Close => {
                            page.browser = None;
                            if page.detail.is_none() {
                                page.load(cx);
                            }
                        }
                        _ => {}
                    }
                    cx.notify();
                }),
            );
        self.browser = Some(browser);
        cx.notify();
    }

    fn select_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        self.tab = tab;
        self.scroll.scroll.set_offset(gpui::Point::default());
        if tab == Tab::Code && self.diff.is_none() && self.diff_task.is_none() {
            self.load_diff(cx);
        }
        cx.notify();
    }
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
        "pr-back"
            | "pr-external"
            | "pr-native"
            | "pr-browser"
            | "pr-copy-url"
            | "pr-detail-refresh"
    );
    let glyph = match id {
        "pr-back" => Some(crate::icons::ALT_ARROW_LEFT),
        "pr-copy-url" if label == "Link copied" => Some(crate::icons::CHECK),
        "pr-copy-url" | "pr-copy-patch" => Some(crate::icons::COPY),
        "pr-detail-refresh" | "pr-retry-diff" => Some(crate::icons::REFRESH),
        "pr-external" => Some(crate::icons::ARROW_UP_RIGHT),
        "pr-browser" => Some(crate::icons::GLOBAL),
        "pr-native" => Some(crate::icons::PULL_REQUEST),
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
        .child(div().flex_1().min_w_0().child(content))
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
        let content = if let Some(browser) = &self.browser {
            div()
                .flex_1()
                .min_h_0()
                .child(browser.clone())
                .into_any_element()
        } else {
            let mut column = widgets::page_column()
                .pt(px(32.0))
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
                            .child(format!("#{} · {}", detail.number, detail.author.login)),
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
                    )
                    .child(
                        div()
                            .mt(px(24.0))
                            .mb(px(20.0))
                            .flex()
                            .flex_wrap()
                            .gap(px(8.0))
                            .children(
                                [
                                    (Tab::Summary, "Summary", "pr-summary"),
                                    (Tab::Code, "Code", "pr-code"),
                                    (Tab::Activity, "Activity", "pr-activity"),
                                ]
                                .into_iter()
                                .map(|(tab, label, id)| {
                                    action(id, label, &theme)
                                        .aria_selected(tab == self.tab)
                                        .when(tab == self.tab, |el| el.bg(theme.selection))
                                        .on_click(cx.listener(move |page, _, _, cx| {
                                            page.select_tab(tab, cx)
                                        }))
                                }),
                            ),
                    );
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
                            let opts = crate::markdown::render::RenderOptions {
                                tasks: None,
                                media: Some(super::pull_request_media::media(&self.url)),
                                row_key: "pr-description".into(),
                                veil: None,
                                cache: None,
                                now: std::time::Instant::now(),
                                copy: None,
                                link: None,
                                workspace_root: None,
                                code: None,
                            };
                            column = column.child(div().mt(px(12.0)).child(
                                crate::markdown::render::render_tree(
                                    body,
                                    &opts,
                                    &theme,
                                    window,
                                    &|_| None,
                                ),
                            ));
                        }
                        column = column.child(div().mt(px(32.0)).child(section_heading(
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
                                            .focus_visible(|style| style.bg(theme.selection))
                                            .hover(|style| style.bg(theme.selection.opacity(0.5)))
                                            .on_click(move |_, _, cx| cx.open_url(&link))
                                    }),
                            );
                        }
                    }
                    Tab::Code => {
                        if let Some(error) = &self.diff_error {
                            column = column
                                .child(widgets::error_strip(&theme, error.clone()))
                                .child(
                                    action("pr-retry-diff", "Retry diff", &theme)
                                        .on_click(cx.listener(|page, _, _, cx| page.load_diff(cx))),
                                );
                        } else if let Some(diff) = &self.diff {
                            let patch = diff.clone();
                            column = column.child(
                                action("pr-copy-patch", "Copy diff", &theme).on_click(
                                    move |_, _, cx| {
                                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                            patch.clone(),
                                        ))
                                    },
                                ),
                            );
                            for (index, (path, offset)) in self.code_files.iter().enumerate() {
                                let offset = *offset;
                                column = column.child(
                                    widgets::ghost_action(&theme)
                                        .id(SharedString::from(format!("pr-file-{index}")))
                                        .role(gpui::Role::Button)
                                        .aria_label(format!("Jump to {path}"))
                                        .focus_visible(|style| style.bg(theme.selection))
                                        .hover(|style| style.bg(theme.selection.opacity(0.5)))
                                        .tab_index(0)
                                        .on_click(cx.listener(move |page, _, _, cx| {
                                            page.code_scroll
                                                .scroll_to_item(offset, gpui::ScrollStrategy::Top);
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
                            let rows = self.code_rows.clone();
                            let colors = theme.clone();
                            let code_scroll = self.code_scroll.0.borrow().base_handle.clone();
                            column = column.child(
                                div().mt(px(16.0)).h(px(440.0)).child(
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
                                                        let (wash, color, mark) = match row.kind {
                                                            crate::changes::LineKind::Add => (
                                                                colors.success.opacity(0.07),
                                                                colors.success,
                                                                "+",
                                                            ),
                                                            crate::changes::LineKind::Del => (
                                                                colors.danger.opacity(0.07),
                                                                colors.danger,
                                                                "−",
                                                            ),
                                                            crate::changes::LineKind::Meta => (
                                                                colors.selection,
                                                                colors.text_muted,
                                                                "",
                                                            ),
                                                            _ => (
                                                                gpui::transparent_black(),
                                                                colors.text,
                                                                "",
                                                            ),
                                                        };
                                                        div()
                                                            .h(px(22.0))
                                                            .flex()
                                                            .items_center()
                                                            .font_family(colors.font_mono.clone())
                                                            .text_size(px(11.0))
                                                            .bg(wash)
                                                            .text_color(color)
                                                            .child(
                                                                div()
                                                                    .w(px(42.0))
                                                                    .flex_none()
                                                                    .text_right()
                                                                    .text_color(colors.text_muted)
                                                                    .child(row.old.clone()),
                                                            )
                                                            .child(
                                                                div()
                                                                    .w(px(42.0))
                                                                    .flex_none()
                                                                    .text_right()
                                                                    .text_color(colors.text_muted)
                                                                    .child(row.new.clone()),
                                                            )
                                                            .child(
                                                                div()
                                                                    .w(px(24.0))
                                                                    .flex_none()
                                                                    .text_center()
                                                                    .child(mark),
                                                            )
                                                            .child(
                                                                div()
                                                                    .flex_1()
                                                                    .min_w_0()
                                                                    .id(SharedString::from(
                                                                        format!("pr-line-{index}"),
                                                                    ))
                                                                    .overflow_x_scroll()
                                                                    .whitespace_nowrap()
                                                                    .child(row.text.clone()),
                                                            )
                                                            .into_any_element()
                                                    })
                                                    .collect::<Vec<_>>()
                                            },
                                        )
                                        .size_full()
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
                            .collect();
                        activity.sort_by_key(|comment| {
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
                        for comment in activity {
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
                                            .child(
                                                crate::icons::icon(crate::icons::CHAT_ROUND_LINE)
                                                    .size(px(14.0))
                                                    .text_color(theme.text_muted),
                                            )
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
                                            .child(if comment.created_at.is_empty() {
                                                comment.submitted_at.clone()
                                            } else {
                                                comment.created_at.clone()
                                            }),
                                    )
                                    .child(comment.body.clone()),
                            );
                        }
                        column = column.child(
                            div()
                                .text_color(theme.text_muted)
                                .child("Use the browser view to comment, review, or merge."),
                        );
                    }
                }
            } else if self.loading {
                column = column.child(div().py(px(32.0)).child("Loading pull request…"));
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
            .child(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }

    impl DetailHost {
        fn new(window: &mut Window, cx: &mut Context<Self>, loaded: bool) -> Self {
            let state = cx.new(|_| AppState::new());
            let page = cx.new(|cx| {
                let mut page = PullRequestDetailPage::new(
                    state, "https://github.com/a/b/pull/1".into(),
                    Some("remote-device".into()), BrowserContext::default(), window, cx,
                );
                if loaded {
                    page.error = None;
                    page.detail = Some(ChangeRequestDetail {
                        title: "Inspect a pull request with a very long title that must leave room for all actions".into(),
                        number: 1, body: "A description".into(), ..Default::default()
                    });
                    page.body = Some(crate::markdown::parse_full("A description"));
                    page.diff = Some(String::new());
                }
                page
            });
            let subscription = cx.observe(&page, |_, _, cx| cx.notify());
            Self {
                page,
                _subscription: subscription,
            }
        }
    }

    impl Render for DetailHost {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let header = self.page.update(cx, |page, cx| page.titlebar(cx));
            div().size_full().relative().child(self.page.clone()).child(
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
            assert_eq!(page.params()["targetDeviceId"], "remote-device")
        });
        for width in [240.0, 320.0, 600.0, 900.0] {
            cx.simulate_resize(gpui::size(px(width), px(800.0)));
            cx.run_until_parked();
            let mut previous_right = px(0.0);
            for selector in [
                "pr-back",
                "pr-native",
                "pr-browser",
                "pr-detail-refresh",
                "pr-copy-url",
                "pr-external",
            ] {
                let bounds = cx.debug_bounds(selector).unwrap();
                assert!(
                    bounds.left() >= previous_right && bounds.right() <= px(width),
                    "{selector}: {bounds:?}"
                );
                assert!(bounds.top() >= px(0.0) && bounds.bottom() <= px(Theme::TITLEBAR_HEIGHT));
                previous_right = bounds.right();
            }
            assert!(cx.debug_bounds("pr-immersive").is_none());
        }
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
    }

    #[gpui::test]
    fn pull_request_error_keeps_retry_and_browser_actions(cx: &mut gpui::TestAppContext) {
        use gpui::AppContext;
        cx.update(|cx| cx.set_global(Theme::default()));
        let (_, cx) = cx.add_window_view(|window, cx| DetailHost::new(window, cx, false));
        cx.simulate_resize(gpui::size(px(320.0), px(800.0)));
        cx.run_until_parked();
        for selector in ["pr-detail-refresh", "pr-browser", "pr-external"] {
            assert!(cx.debug_bounds(selector).is_some(), "{selector}");
        }
    }
}

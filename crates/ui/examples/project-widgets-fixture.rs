//! Offscreen visual evidence for project widgets: the stacked cards on the
//! new-chat screen (stacked and spread), a widget's sheet, the refresh setup
//! dialog, and "Save as widget" under a chat visual (open, choosing a
//! project, saved), in dark and light.
//!
//!   cargo run -p zeron-ui --example project-widgets-fixture \
//!     --features project-widgets-fixture -- <output-dir>
use std::collections::HashMap;
use std::{path::Path, path::PathBuf, time::Duration};

use gpui::{
    AnyElement, AppContext, AsyncApp, Bounds, Context, Entity, IntoElement, ParentElement, Render,
    Styled, Window, WindowBounds, WindowOptions, div, px, size,
};
use zeron_proto::{
    HarnessId, PROJECT_WIDGET_RENDERER_LUAU_V1, ProjectWidget, ProjectWidgetContent,
    ProjectWidgetPreview, ProjectWidgetPreviewChart, ProjectWidgetPreviewChartKind,
    ProjectWidgetPreviewSeries, ProjectWidgetPreviewStat, ProjectWidgetPreviewTable,
    ProjectWidgetRefresh, ProjectWidgetRefreshState, ProjectWidgetSource, ProjectWidgetsSnapshot,
};
use zeron_ui::project_widgets::{ProjectWidgetsPage, SaveVisual, WidgetsTarget};
use zeron_ui::widget_shelf::WidgetShelf;
use zeron_ui::*;

const ANALYTICS: &str = r#"
local days = {}
local events, sessions = {}, {}
for d = 1, 30 do
  days[d] = tostring(d)
  events[d] = 1200 + math.floor(400 * math.sin(d / 4)) + d * 12
  sessions[d] = 800 + math.floor(250 * math.cos(d / 5)) + d * 8
end
return ui.column {
  gap = 12,
  ui.row {
    gap = 24,
    ui.stat { label = "PostHog events", value = "41,920", delta = "+8.2%" },
    ui.stat { label = "GA sessions", value = "27,310", delta = "+3.1%" },
  },
  ui.chart {
    type = "line",
    x = days,
    series = {
      { name = "Events", values = events },
      { name = "Sessions", values = sessions },
    },
  },
}
"#;

fn series(f: impl Fn(f32) -> f32) -> Vec<Option<u16>> {
    let raw: Vec<f32> = (0..30).map(|d| f(d as f32)).collect();
    let (lo, hi) = raw
        .iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
    raw.iter()
        .map(|v| Some(((v - lo) / (hi - lo) * 1000.0) as u16))
        .collect()
}

fn stat(value: &str, label: &str, delta: Option<&str>) -> ProjectWidgetPreviewStat {
    ProjectWidgetPreviewStat {
        value: value.into(),
        label: label.into(),
        delta: delta.map(Into::into),
    }
}

fn widget(id: &str, title: &str, preview: Option<ProjectWidgetPreview>) -> ProjectWidget {
    let now = chrono::Utc::now().timestamp_millis();
    ProjectWidget {
        id: id.into(),
        space_id: "space-site".into(),
        owner_device_id: "device-studio".into(),
        title: title.into(),
        revision: 1,
        created_at: now - 86_400_000,
        updated_at: now - 7_200_000,
        last_successful_at: Some(now - 7_200_000),
        content_revision: 1,
        content: ProjectWidgetContent {
            bundle_id: format!("bundle-{id}"),
            format_version: 1,
            renderer_format: PROJECT_WIDGET_RENDERER_LUAU_V1.into(),
            render_seed: 42,
            script_size: 1,
            script_sha256: "00".into(),
            assets: vec![],
            preview,
        },
        source: Some(ProjectWidgetSource {
            source_chat_id: format!("chat-{id}"),
            source_visual_id: format!("v:{id}"),
            source_entry_id: None,
            source_title: None,
            source_device_id: "device-studio".into(),
        }),
        refresh_recipe: None,
        refresh: None,
    }
}

fn refresh(state: ProjectWidgetRefreshState, at: i64) -> ProjectWidgetRefresh {
    ProjectWidgetRefresh {
        refresh_id: "r1".into(),
        state,
        started_at: at,
        finished_at: None,
        execution_chat_id: None,
        error_code: None,
        error_message: None,
    }
}

fn widgets() -> Vec<ProjectWidget> {
    let traffic = ProjectWidgetPreview {
        stats: vec![stat("41,920", "PostHog events", Some("+8.2%"))],
        chart: Some(ProjectWidgetPreviewChart {
            kind: ProjectWidgetPreviewChartKind::Area,
            stacked: false,
            series: vec![
                ProjectWidgetPreviewSeries {
                    name: "Events".into(),
                    values: series(|d| 1200.0 + 400.0 * (d / 4.0).sin() + d * 12.0),
                },
                ProjectWidgetPreviewSeries {
                    name: "Sessions".into(),
                    values: series(|d| 800.0 + 250.0 * (d / 5.0).cos() + d * 8.0),
                },
            ],
        }),
        ..Default::default()
    };
    let signups = ProjectWidgetPreview {
        chart: Some(ProjectWidgetPreviewChart {
            kind: ProjectWidgetPreviewChartKind::Bar,
            stacked: false,
            series: vec![ProjectWidgetPreviewSeries {
                name: "Signups".into(),
                values: [320, 540, 410, 880, 760, 1000, 690, 450, 610, 830, 720, 940]
                    .into_iter()
                    .map(Some)
                    .collect(),
            }],
        }),
        ..Default::default()
    };
    let revenue = ProjectWidgetPreview {
        stats: vec![
            stat("$18.4k", "MRR", Some("+4.1%")),
            stat("2.3%", "Churn", Some("-0.4 pt")),
        ],
        ..Default::default()
    };
    let pages = ProjectWidgetPreview {
        table: Some(ProjectWidgetPreviewTable {
            columns: vec!["Page".into(), "Views".into()],
            rows: vec![
                vec!["/pricing".into(), "8,214".into()],
                vec!["/docs/getting-started".into(), "6,902".into()],
                vec!["/blog/launch".into(), "3,118".into()],
            ],
        }),
        ..Default::default()
    };
    let mut running = widget("revenue", "Revenue health", Some(revenue));
    running.refresh = Some(refresh(
        ProjectWidgetRefreshState::Running,
        running.updated_at,
    ));
    let mut failed = widget("pages", "Top pages this week", Some(pages));
    failed.refresh = Some(refresh(
        ProjectWidgetRefreshState::Failed,
        failed.updated_at,
    ));
    vec![
        widget("traffic", "Traffic · last 30 days", Some(traffic)),
        widget("signups", "Signups by month", Some(signups)),
        running,
        failed,
        widget(
            "globe",
            "Visitors globe",
            Some(ProjectWidgetPreview {
                graphic: true,
                ..Default::default()
            }),
        ),
        widget(
            "notes",
            "Release notes",
            Some(ProjectWidgetPreview {
                text: Some(
                    "Shipped project widgets, refresh runs and the new MCP settings page. \
                     Next: scheduled refreshes."
                        .into(),
                ),
                ..Default::default()
            }),
        ),
        widget("old", "Older widget", None),
    ]
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    ShelfStacked,
    ShelfSpread,
    ShelfNarrow,
    ShelfLanding,
    SheetMorph,
    Sheet,
    Recipe,
    SaveOpen,
    SaveChoose,
    Saved,
}

struct Stage {
    mode: Mode,
    state: Entity<state::AppState>,
    shelf: Entity<WidgetShelf>,
    page: Entity<ProjectWidgetsPage>,
    save: Entity<SaveVisual>,
    visual: Entity<visuals::VisualFrame>,
    motion: zeron_ui::widget_sheet::SheetMotion,
}

impl Render for Stage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = theme::Theme::of(cx).clone();
        let viewport = window.viewport_size();
        let body: AnyElement = match self.mode {
            Mode::ShelfStacked | Mode::ShelfSpread | Mode::ShelfNarrow | Mode::ShelfLanding => {
                div()
                    .pt(px(28.0))
                    .flex()
                    .justify_center()
                    .child(self.shelf.clone())
                    .into_any_element()
            }
            Mode::Sheet | Mode::Recipe | Mode::SheetMorph => {
                let morph = self.mode == Mode::SheetMorph;
                let sheet = self.motion.render(
                    viewport,
                    project_widgets::sheet_width(viewport),
                    project_widgets::sheet_max_height(viewport),
                    &theme,
                    !morph,
                    self.page.clone().into_any_element(),
                    |frame| frame,
                );
                let mut base = div().size_full();
                if morph {
                    base = base
                        .pt(px(28.0))
                        .flex()
                        .justify_center()
                        .child(self.shelf.clone());
                }
                base.child(sheet).into_any_element()
            }
            Mode::SaveOpen | Mode::SaveChoose | Mode::Saved => div()
                .p(px(40.0))
                .max_w(px(760.0))
                .child(self.visual.clone())
                .child(self.save.clone())
                .into_any_element(),
        };
        div()
            .size_full()
            .bg(theme.bg)
            .text_color(theme.text)
            .child(body)
    }
}

fn capture(
    window: gpui::AnyWindowHandle,
    cx: &mut AsyncApp,
    output: &Path,
    name: &str,
) -> anyhow::Result<()> {
    window.update(cx, |_, window, cx| {
        window.draw(cx).clear();
        window.render_to_image()?.save(output.join(name))?;
        Ok(())
    })?
}

fn target() -> WidgetsTarget {
    WidgetsTarget {
        space_id: "space-site".into(),
        device_id: "device-studio".into(),
    }
}

fn main() -> anyhow::Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    let _guard = runtime.enter();
    let output = PathBuf::from(std::env::args().nth(1).expect("output directory"));
    std::fs::create_dir_all(&output)?;
    let temp = tempfile::tempdir()?;
    let data = temp.path().to_path_buf();
    gpui_platform::application()
        .with_assets(icons::Assets)
        .run(move |cx| {
            gpui_tokio::init(cx);
            gpui_base::init(cx);
            let prefs = settings::UiSettings::default();
            settings::init(prefs.clone(), data.clone(), cx);
            let fonts = typography::register_fonts(cx);
            typography::init(
                prefs.ui_font_family.clone(),
                prefs.ui_font_size,
                prefs.terminal_font_family.clone(),
                prefs.terminal_font_size,
                prefs.code_font_family.clone(),
                prefs.code_font_size,
                fonts,
                cx,
            );
            theme_library::init(data, cx);
            appearance::init(
                appearance::AppearanceMode::Dark,
                prefs.theme_selection,
                prefs.accent,
                prefs.surface,
                cx,
            );
            composer::init(cx, prefs.composer_send_behavior);
            let state = cx.new(|_| state::AppState::new());
            let window = cx
                .open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                            gpui::point(px(-12000.0), px(-12000.0)),
                            size(px(1200.0), px(860.0)),
                        ))),
                        focus: false,
                        show: true,
                        ..Default::default()
                    },
                    |_, cx| {
                        cx.new(|cx| {
                            let shelf = cx.new(|cx| WidgetShelf::new(state.clone(), cx));
                            let page = cx.new(|cx| {
                                let mut page = ProjectWidgetsPage::new(state.clone(), cx);
                                page.fixture_show(
                                    target(),
                                    ProjectWidgetsSnapshot {
                                        space_id: "space-site".into(),
                                        owner_device_id: "device-studio".into(),
                                        collection_revision: 1,
                                        widgets: vec![widgets().remove(0)],
                                    },
                                    HashMap::from([("traffic".to_owned(), ANALYTICS.to_owned())]),
                                    None,
                                    cx,
                                );
                                page.fixture_sheet("traffic", cx);
                                page
                            });
                            let save = cx.new(|cx| {
                                SaveVisual::new(
                                    state.clone(),
                                    "chat-1".into(),
                                    "v:1".into(),
                                    "Traffic · last 30 days".into(),
                                    cx,
                                )
                            });
                            let visual = cx.new(|cx| {
                                visuals::VisualFrame::new(
                                    "fixture-visual".into(),
                                    "Traffic".into(),
                                    visuals::VisualSource::Inline {
                                        script: ANALYTICS.into(),
                                        seed: 42,
                                    },
                                    None,
                                    None,
                                    cx,
                                )
                            });
                            Stage {
                                motion: zeron_ui::widget_sheet::SheetMotion::new(None),
                                mode: Mode::ShelfStacked,
                                state: state.clone(),
                                shelf,
                                page,
                                save,
                                visual,
                            }
                        })
                    },
                )
                .unwrap();
            cx.spawn(async move |cx| {
                let result: anyhow::Result<()> = async {
                    for (appearance, theme_label) in [
                        (appearance::AppearanceMode::Dark, "dark"),
                        (appearance::AppearanceMode::Light, "light"),
                    ] {
                        for (mode, label) in [
                            (Mode::ShelfStacked, "shelf-stacked"),
                            (Mode::ShelfSpread, "shelf-spread"),
                            (Mode::ShelfNarrow, "shelf-narrow"),
                            (Mode::ShelfLanding, "shelf-landing"),
                            (Mode::Sheet, "sheet"),
                            (Mode::SheetMorph, "sheet-morph"),
                            (Mode::Recipe, "recipe"),
                            (Mode::SaveOpen, "save-open"),
                            (Mode::SaveChoose, "save-choose"),
                            (Mode::Saved, "saved"),
                        ] {
                            window.update(cx, |stage, window, cx| {
                                appearance::set_mode(appearance, cx);
                                let width = if mode == Mode::ShelfNarrow {
                                    720.0
                                } else {
                                    1200.0
                                };
                                window.resize(size(px(width), px(860.0)));
                                stage.mode = mode;
                                stage.motion = zeron_ui::widget_sheet::SheetMotion::new(None);
                                stage.shelf.update(cx, |shelf, cx| {
                                    shelf.set_available_width(width);
                                    shelf.fixture_show(
                                        target(),
                                        widgets(),
                                        !matches!(mode, Mode::ShelfStacked | Mode::SheetMorph),
                                        cx,
                                    );
                                    if mode == Mode::ShelfSpread {
                                        shelf.fixture_hover(1, cx);
                                    }
                                    if mode == Mode::ShelfLanding {
                                        // A refresh lands: last month's bars give way.
                                        let before =
                                            widgets()[1].content.preview.clone().map(|mut p| {
                                                if let Some(chart) = p.chart.as_mut() {
                                                    for v in chart.series[0].values.iter_mut() {
                                                        *v = v.map(|v| 1000 - v);
                                                    }
                                                }
                                                p
                                            });
                                        shelf.fixture_changed("signups", before, 0.45);
                                    }
                                });
                                stage
                                    .page
                                    .update(cx, |page, cx| page.fixture_close_recipe(cx));
                                if mode == Mode::Recipe {
                                    stage.page.update(cx, |page, cx| {
                                        page.fixture_open_recipe(
                                            "traffic",
                                            vec![
                                                (HarnessId::ClaudeCode, "Claude Code".into()),
                                                (HarnessId::Codex, "Codex".into()),
                                                (HarnessId::Opencode, "opencode".into()),
                                            ],
                                            cx,
                                        )
                                    });
                                }
                                let state = stage.state.clone();
                                stage.save = cx.new(|cx| {
                                    let mut save = SaveVisual::new(
                                        state,
                                        "chat-1".into(),
                                        "v:1".into(),
                                        "Traffic · last 30 days".into(),
                                        cx,
                                    );
                                    let site = ("marketing-site", "Studio · ~/code/marketing-site");
                                    match mode {
                                        Mode::SaveChoose => save.fixture_show(
                                            None,
                                            &[site, ("analytics", "Studio · ~/code/analytics")],
                                            true,
                                            false,
                                            cx,
                                        ),
                                        _ => save.fixture_show(
                                            Some(site),
                                            &[],
                                            mode == Mode::SaveOpen,
                                            mode == Mode::Saved,
                                            cx,
                                        ),
                                    }
                                    save
                                });
                                cx.notify();
                            })?;
                            cx.background_executor()
                                .timer(Duration::from_millis(700))
                                .await;
                            if mode == Mode::SheetMorph {
                                // Grow out of the front card, 40% of the way.
                                window.update(cx, |stage, _, cx| {
                                    let origin = stage.shelf.read(cx).card_bounds("traffic");
                                    stage.motion =
                                        zeron_ui::widget_sheet::SheetMotion::fixture_frozen(
                                            origin, 0.4,
                                        );
                                    stage.shelf.update(cx, |shelf, cx| {
                                        shelf.set_lifted(Some("traffic".into()), cx)
                                    });
                                    cx.notify();
                                })?;
                                cx.background_executor()
                                    .timer(Duration::from_millis(300))
                                    .await;
                            }
                            capture(
                                window.into(),
                                cx,
                                &output,
                                &format!("{label}-{theme_label}.png"),
                            )?;
                        }
                    }
                    Ok(())
                }
                .await;
                if let Err(error) = result {
                    eprintln!("project widgets fixture failed: {error:#}");
                }
                cx.update(|cx| cx.quit());
            })
            .detach();
        });
    Ok(())
}

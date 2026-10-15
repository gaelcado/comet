//! Offscreen visual evidence for a project's Widgets page: an analytics
//! widget, a long title, an unreadable widget and a conflict, in dark and
//! light at a wide and a narrow width.
//!
//!   cargo run -p zeron-ui --example project-widgets-fixture \
//!     --features project-widgets-fixture -- <output-dir>
use std::collections::HashMap;
use std::{path::Path, path::PathBuf, time::Duration};

use gpui::{AppContext, AsyncApp, Bounds, WindowBounds, WindowOptions, px, size};
use zeron_proto::{
    PROJECT_WIDGET_RENDERER_LUAU_V1, ProjectWidget, ProjectWidgetContent, ProjectWidgetSource,
    ProjectWidgetsSnapshot,
};
use zeron_ui::project_widgets::WidgetsTarget;
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

const TABLE: &str = r#"
return ui.table {
  columns = { "Page", "Views", "Bounce" },
  rows = {
    { "/pricing", "8,214", "31%" },
    { "/docs/getting-started", "6,902", "24%" },
    { "/blog/launch", "3,118", "47%" },
  },
}
"#;

fn widget(id: &str, title: &str, revision: u64) -> ProjectWidget {
    ProjectWidget {
        id: id.into(),
        space_id: "space-site".into(),
        owner_device_id: "device-studio".into(),
        title: title.into(),
        revision,
        created_at: 1_760_000_000_000,
        updated_at: 1_760_500_000_000,
        last_successful_at: Some(1_760_500_000_000),
        content_revision: 1,
        content: ProjectWidgetContent {
            bundle_id: format!("bundle-{id}"),
            format_version: 1,
            renderer_format: PROJECT_WIDGET_RENDERER_LUAU_V1.into(),
            render_seed: 42,
            script_size: 1,
            script_sha256: "00".into(),
            assets: vec![],
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
                            size(px(1100.0), px(1300.0)),
                        ))),
                        focus: false,
                        show: true,
                        ..Default::default()
                    },
                    |_, cx| {
                        cx.new(|cx| {
                            let mut page = project_widgets::ProjectWidgetsPage::new(state, cx);
                            let long = "Weekly PostHog events and Google Analytics sessions \
                                for the marketing site, compared with the previous period";
                            let snapshot = ProjectWidgetsSnapshot {
                                space_id: "space-site".into(),
                                owner_device_id: "device-studio".into(),
                                collection_revision: 3,
                                widgets: vec![
                                    widget("analytics", "Traffic · last 30 days", 2),
                                    widget("pages", long, 1),
                                    widget("broken", "Signups by channel", 4),
                                ],
                            };
                            let scripts = HashMap::from([
                                ("analytics".to_owned(), ANALYTICS.to_owned()),
                                ("pages".to_owned(), TABLE.to_owned()),
                            ]);
                            page.fixture_show(
                                WidgetsTarget {
                                    space_id: "space-site".into(),
                                    device_id: "device-studio".into(),
                                },
                                snapshot,
                                scripts,
                                Some((
                                    "pages",
                                    "This widget changed elsewhere. Reload it before retrying.",
                                )),
                                cx,
                            );
                            page
                        })
                    },
                )
                .unwrap();
            cx.spawn(async move |cx| {
                let result: anyhow::Result<()> = async {
                    for (mode, width, label) in [
                        (appearance::AppearanceMode::Dark, 1100.0, "dark-wide"),
                        (appearance::AppearanceMode::Dark, 480.0, "dark-narrow"),
                        (appearance::AppearanceMode::Light, 1100.0, "light-wide"),
                        (appearance::AppearanceMode::Light, 480.0, "light-narrow"),
                    ] {
                        window.update(cx, |_, window, cx| {
                            appearance::set_mode(mode, cx);
                            window.resize(size(px(width), px(1300.0)));
                        })?;
                        cx.background_executor()
                            .timer(Duration::from_millis(400))
                            .await;
                        capture(window.into(), cx, &output, &format!("widgets-{label}.png"))?;
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

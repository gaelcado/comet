//! Offscreen visual evidence for Settings → MCP → Zeron: a long catalog with
//! a long description, an unsaved edit, the original and an expanded input
//! schema, in dark and light at a wide and a narrow width.
//!
//!   cargo run -p zeron-ui --example mcp-settings-fixture \
//!     --features mcp-settings-fixture -- <output-dir>
use gpui::{AppContext, AsyncApp, Bounds, WindowBounds, WindowOptions, px, size};
use serde_json::json;
use std::{path::Path, path::PathBuf, time::Duration};
use zeron_proto::{McpCatalogSnapshot, McpToolAvailability, McpToolDescriptor, McpToolState};
use zeron_ui::*;

fn catalog() -> McpCatalogSnapshot {
    let long = "Publish a native visual into the chat from a Luau script. Keep tables \
        compact, label every axis, and prefer theme colors. "
        .repeat(12);
    let mut tools: Vec<McpToolState> = (0..20)
        .map(|ix| McpToolState {
            descriptor: McpToolDescriptor {
                name: format!("example_tool_{ix}"),
                description: format!("Example tool number {ix}, described in one line."),
                input_schema: json!({ "type": "object", "properties": {} }),
                availability: McpToolAvailability::Always,
            },
            enabled: ix % 4 != 1,
            description_override: None,
        })
        .collect();
    tools.insert(
        1,
        McpToolState {
            descriptor: McpToolDescriptor {
                name: "show_visual".into(),
                description: long.clone(),
                input_schema: json!({
                    "type": "object",
                    "required": ["script"],
                    "properties": {
                        "script": { "type": "string", "description": long },
                        "title": { "type": "string", "maxLength": 200 },
                        "assets": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "required": ["name", "path"],
                                "properties": {
                                    "name": { "type": "string" },
                                    "path": { "type": "string" }
                                }
                            }
                        }
                    }
                }),
                availability: McpToolAvailability::ChatOnly,
            },
            enabled: true,
            description_override: Some("Show a native chart or table in the chat.".into()),
        },
    );
    McpCatalogSnapshot {
        catalog_version: 2,
        policy_revision: 4,
        tools,
        policy_unreadable: false,
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
                            size(px(900.0), px(1100.0)),
                        ))),
                        focus: false,
                        show: true,
                        ..Default::default()
                    },
                    |_, cx| {
                        cx.new(|cx| {
                            let mut page = settings::mcp::McpSettingsPage::new(state, cx);
                            page.fixture_show(
                                catalog(),
                                "show_visual",
                                Some("Show a native chart, table or diagram in the chat."),
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
                        (appearance::AppearanceMode::Dark, 900.0, "dark-wide"),
                        (appearance::AppearanceMode::Dark, 520.0, "dark-narrow"),
                        (appearance::AppearanceMode::Light, 900.0, "light-wide"),
                        (appearance::AppearanceMode::Light, 520.0, "light-narrow"),
                    ] {
                        window.update(cx, |_, window, cx| {
                            appearance::set_mode(mode, cx);
                            window.resize(size(px(width), px(1100.0)));
                        })?;
                        cx.background_executor()
                            .timer(Duration::from_millis(400))
                            .await;
                        capture(window.into(), cx, &output, &format!("mcp-{label}.png"))?;
                    }
                    Ok(())
                }
                .await;
                if let Err(error) = result {
                    eprintln!("mcp settings fixture failed: {error:#}");
                }
                cx.update(|cx| cx.quit());
            })
            .detach();
        });
    Ok(())
}

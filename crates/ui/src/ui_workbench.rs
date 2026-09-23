//! Native design workbench. Every specimen here is a production GPUI builder
//! or an embedded production asset; unmounted application composites are
//! reported as coverage gaps instead of being redrawn as lookalikes.

use std::path::PathBuf;

use gpui::{
    AssetSource as _, Context, Entity, Focusable as _, Hsla, IntoElement, Render, ScrollHandle,
    SharedString, TitlebarOptions, Window, WindowBounds, WindowOptions, div, prelude::*, px, size,
    svg,
};
use serde::Deserialize;
use zeron_proto::{ChangeRequestState, ChangeRequestSummary};
use zeron_theme::{AccentPreset, AccentSelection, ThemeRegistry, ThemeSelection};

use crate::{
    appearance, change_requests, changes, composer::ComposerInput, files, icons, markdown, popover,
    settings, shell, surface_chrome, theme, theme_library, typography,
};
use theme::{Appearance, Theme};

const GUIDE: &str = include_str!("../../../apps/ui-workbench/metrics.json");
const COMPONENTS: &str = include_str!("../../../apps/ui-workbench/components.json");
const MOTIONS: &str = include_str!("../../../apps/icon-lab/motions.json");

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Overview,
    Foundations,
    Components,
    Icons,
    Coverage,
}

impl Page {
    const ALL: [Self; 5] = [
        Self::Overview,
        Self::Foundations,
        Self::Components,
        Self::Icons,
        Self::Coverage,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Foundations => "Foundations",
            Self::Components => "Components",
            Self::Icons => "Icons & motion",
            Self::Coverage => "Coverage",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Overview => icons::GRID,
            Self::Foundations => icons::TUNING,
            Self::Components => icons::CHECKBOX,
            Self::Icons => icons::STAR,
            Self::Coverage => icons::LIST,
        }
    }
}

#[derive(Deserialize)]
struct MotionRecord {
    title: String,
    #[serde(rename = "fromIcon")]
    from_icon: String,
    #[serde(rename = "toIcon")]
    to_icon: String,
}

struct Morph {
    title: String,
    from: &'static str,
    to: &'static str,
}

struct Workbench {
    page: Page,
    scroll: ScrollHandle,
    appearance: Appearance,
    theme_selection: ThemeSelection,
    accent: AccentSelection,
    switch_on: bool,
    checkbox_on: bool,
    search: Entity<ComposerInput>,
    field: Entity<ComposerInput>,
    icons: Vec<String>,
    morphs: Vec<Morph>,
    morphed: Vec<bool>,
}

impl Workbench {
    fn new(cx: &mut Context<Self>) -> Self {
        let assets = icons::Assets;
        let mut paths: Vec<String> = assets
            .list("custom-icons/")
            .expect("embedded icon list")
            .into_iter()
            .map(|path| path.to_string())
            .filter(|path| !path.starts_with("custom-icons/small/"))
            .collect();
        paths.sort();
        assert_eq!(
            paths.len(),
            135,
            "workbench must show the complete icon family"
        );

        let records: Vec<MotionRecord> =
            serde_json::from_str(MOTIONS).expect("reviewed icon motions");
        let resolve = |name: &str| -> &'static str {
            icons::MORPH_PATHS
                .iter()
                .copied()
                .find(|path| {
                    path.strip_prefix("custom-icons/")
                        .and_then(|path| path.strip_suffix(".svg"))
                        == Some(name)
                })
                .unwrap_or_else(|| panic!("motion endpoint missing from native assets: {name}"))
        };
        let morphs: Vec<Morph> = records
            .into_iter()
            .map(|record| Morph {
                title: record.title,
                from: resolve(&record.from_icon),
                to: resolve(&record.to_icon),
            })
            .collect();
        assert_eq!(morphs.len(), 36, "workbench must show every native morph");
        let morphed = vec![false; morphs.len()];

        let (appearance, accent) = {
            let current = Theme::of(cx);
            (current.appearance, current.accent_selection)
        };
        let search = cx.new(|cx| {
            ComposerInput::new("Search files", cx)
                .with_single_line()
                .with_accessibility_role(gpui::Role::SearchInput)
                .with_text_metrics(11.0, 16.0)
        });
        let field = cx.new(|cx| ComposerInput::new("Device name", cx));
        Self {
            page: Page::Overview,
            scroll: ScrollHandle::new(),
            appearance,
            theme_selection: appearance::themes(cx),
            accent,
            switch_on: false,
            checkbox_on: false,
            search,
            field,
            icons: paths,
            morphs,
            morphed,
        }
    }

    fn install_theme(&self, cx: &mut Context<Self>) {
        let variant_appearance = match self.appearance {
            Appearance::Dark => zeron_theme::Appearance::Dark,
            Appearance::Light => zeron_theme::Appearance::Light,
        };
        Theme::install_selection(
            self.appearance,
            self.theme_selection.variant_id(variant_appearance),
            self.accent,
            appearance::surface(cx),
            cx,
        );
        appearance::reapply_window_background(cx);
        cx.refresh_windows();
    }

    fn navigation(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::Div {
        let mut nav = div()
            .w(px(218.0))
            .h_full()
            .flex_none()
            .bg(t.surface)
            .border_r_1()
            .border_color(t.border)
            .px(px(14.0))
            .pt(px(30.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .px(px(12.0))
                    .pb(px(30.0))
                    .text_size(px(19.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child("zeron / workbench"),
            );
        for page in Page::ALL {
            nav = nav.child(
                div()
                    .id(SharedString::from(format!(
                        "workbench-nav-{}",
                        page.label()
                    )))
                    .h(px(38.0))
                    .px(px(11.0))
                    .rounded(px(7.0))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .bg(if self.page == page {
                        t.element_active
                    } else {
                        t.surface
                    })
                    .text_color(if self.page == page {
                        t.text
                    } else {
                        t.text_muted
                    })
                    .cursor_pointer()
                    .hover(|s| s.bg(t.element_hover))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.page = page;
                        this.scroll = ScrollHandle::new();
                        cx.notify();
                    }))
                    .child(icons::icon(page.icon()).size(px(16.0)))
                    .child(page.label()),
            );
        }
        nav.child(
            div()
                .mt_auto()
                .px(px(12.0))
                .pb(px(18.0))
                .text_size(px(11.0))
                .text_color(t.text_faint)
                .child("Native GPUI · production assets"),
        )
    }

    fn topbar(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::Div {
        let next = if self.appearance == Appearance::Dark {
            Appearance::Light
        } else {
            Appearance::Dark
        };
        let variant_appearance = match self.appearance {
            Appearance::Dark => zeron_theme::Appearance::Dark,
            Appearance::Light => zeron_theme::Appearance::Light,
        };
        let variants: Vec<(String, String)> = ThemeRegistry::active()
            .variants_for(variant_appearance)
            .map(|variant| (variant.id.clone(), variant.name.clone()))
            .collect();
        let selected = self.theme_selection.variant_id(variant_appearance);
        let index = variants
            .iter()
            .position(|(id, _)| id == selected)
            .unwrap_or(0);
        let variant_label = format!("Theme: {}", variants[index].1);
        let next_variant = variants[(index + 1) % variants.len()].0.clone();
        let current_accent = self.accent;
        let accent_name = self.accent.label();
        div()
            .h(px(54.0))
            .w_full()
            .flex_none()
            .px(px(26.0))
            .flex()
            .items_center()
            .justify_between()
            .border_b_1()
            .border_color(t.border)
            .bg(t.surface)
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(t.text_muted)
                    .child(format!("DESIGN SYSTEM  /  {}", self.page.label())),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(
                        popover::btn_ghost(t, &variant_label, "workbench-variant")
                            .id("workbench-variant-button")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.theme_selection
                                    .set_variant(variant_appearance, next_variant.clone());
                                this.install_theme(cx);
                            })),
                    )
                    .child(
                        popover::btn_ghost(t, accent_name, "workbench-accent")
                            .id("workbench-accent-button")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let choices = AccentPreset::ALL;
                                let index = match current_accent {
                                    AccentSelection::Preset(value) => {
                                        choices.iter().position(|item| *item == value).unwrap_or(0)
                                    }
                                    AccentSelection::ThemeDefault => choices.len() - 1,
                                };
                                this.accent =
                                    AccentSelection::Preset(choices[(index + 1) % choices.len()]);
                                this.install_theme(cx);
                            })),
                    )
                    .child(
                        popover::btn_ghost(
                            t,
                            if next == Appearance::Light {
                                "Light"
                            } else {
                                "Dark"
                            },
                            "workbench-appearance",
                        )
                        .id("workbench-appearance-button")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.appearance = next;
                            this.install_theme(cx);
                        })),
                    ),
            )
    }

    fn heading(t: &Theme, eyebrow: &str, title: &str, copy: &str) -> gpui::Div {
        div()
            .flex()
            .flex_col()
            .gap(px(11.0))
            .pb(px(27.0))
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(t.accent)
                    .child(eyebrow.to_string()),
            )
            .child(
                div()
                    .text_size(px(36.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child(title.to_string()),
            )
            .child(
                div()
                    .max_w(px(680.0))
                    .text_size(px(13.0))
                    .text_color(t.text_muted)
                    .child(copy.to_string()),
            )
    }

    fn overview(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::Div {
        let catalog: serde_json::Value =
            serde_json::from_str(COMPONENTS).expect("component inventory");
        let entries = catalog.as_array().expect("component inventory array");
        let count = entries.len();
        let mounted = entries
            .iter()
            .filter(|entry| entry["workbench"] == "mounted")
            .count();
        let mut root = div()
            .flex()
            .flex_col()
            .child(Self::heading(t, "A LIVING NATIVE REFERENCE", "The shape of Zeron.", "This window is painted by Zeron's GPUI renderer. Component cards call the same builders as production; the icon board loads the same embedded assets and transition engine."))
            .child(
                div()
                    .p(px(24.0))
                    .rounded(px(13.0))
                    .border_1()
                    .border_color(t.border)
                    .bg(t.surface_card)
                    .flex()
                    .gap(px(26.0))
                    .child(Self::stat(t, "135", "native glyphs"))
                    .child(Self::stat(t, "36", "native morphs"))
                    .child(Self::stat(t, "38", "theme roles"))
                    .child(Self::stat(t, &format!("{mounted}/{count}"), "catalog entries mounted")),
            )
            .child(
                div()
                    .mt(px(30.0))
                    .text_size(px(17.0))
                    .child("Mounted production primitives"),
            );
        root = root.child(self.components(t, cx));
        root
    }

    fn stat(t: &Theme, value: &str, label: &str) -> gpui::Div {
        div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(5.0))
            .child(
                div()
                    .text_size(px(27.0))
                    .text_color(t.text)
                    .child(value.to_string()),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(t.text_faint)
                    .child(label.to_string()),
            )
    }

    fn specimen(t: &Theme, title: &str, source: &str, content: impl IntoElement) -> gpui::Div {
        div()
            .w(px(330.0))
            .min_h(px(170.0))
            .rounded(px(11.0))
            .border_1()
            .border_color(t.border)
            .bg(t.surface_card)
            .overflow_hidden()
            .flex()
            .flex_col()
            .child(
                div()
                    .min_h(px(108.0))
                    .p(px(19.0))
                    .flex()
                    .items_center()
                    .child(content),
            )
            .child(
                div()
                    .border_t_1()
                    .border_color(t.border)
                    .px(px(17.0))
                    .py(px(12.0))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(div().text_size(px(12.0)).child(title.to_string()))
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(t.text_faint)
                            .child(source.to_string()),
                    ),
            )
    }

    fn components(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::Div {
        let toggle = self.switch_on;
        let checkbox_on = self.checkbox_on;
        let checkbox_entity = cx.entity().downgrade();
        let search_focus = self.search.focus_handle(cx);
        let mut grid = div().flex().flex_wrap().gap(px(12.0));
        grid = grid.child(Self::specimen(
            t,
            "Dialog actions",
            "popover::btn_primary / btn_ghost / btn_danger",
            div()
                .flex()
                .flex_wrap()
                .gap(px(7.0))
                .child(popover::btn_primary(t, "Continue"))
                .child(popover::btn_ghost(t, "Cancel", "workbench-ghost"))
                .child(popover::btn_danger(t, "Delete")),
        ));
        grid = grid.child(Self::specimen(
            t,
            "Settings badges",
            "settings::widgets::badge / badge_active",
            div()
                .flex()
                .gap(px(8.0))
                .child(settings::widgets::badge(t, "Default"))
                .child(settings::widgets::badge_active(t, "Active")),
        ));
        grid = grid.child(Self::specimen(
            t,
            "Toggle switch",
            "settings::widgets::toggle_switch",
            div()
                .id("workbench-toggle")
                .cursor_pointer()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.switch_on = !this.switch_on;
                    cx.notify();
                }))
                .flex()
                .items_center()
                .gap(px(12.0))
                .child(settings::widgets::toggle_switch(t, toggle))
                .child(if toggle { "On" } else { "Off" }),
        ));
        grid = grid.child(Self::specimen(
            t,
            "Task checkbox",
            "markdown::render::task_checkbox",
            div().flex().items_center().gap(px(12.0)).child(
                markdown::render::task_checkbox(
                    "workbench-task-checkbox".into(),
                    checkbox_on,
                    true,
                    "Toggle task".into(),
                    t,
                )
                .on_change(move |_, _, _, cx| {
                    checkbox_entity
                        .update(cx, |this, cx| {
                            this.checkbox_on = !this.checkbox_on;
                            cx.notify();
                        })
                        .ok();
                }),
            ),
        ));
        grid = grid.child(Self::specimen(
            t,
            "File search",
            "surface_chrome::input / composer::ComposerInput",
            surface_chrome::toolbar(t).child(
                surface_chrome::input()
                    .id("workbench-file-search")
                    .overflow_hidden()
                    .cursor_text()
                    .hover(|style| style.bg(crate::theme::ink(0.055)))
                    .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                        window.focus(&search_focus, cx);
                        cx.stop_propagation();
                    })
                    .child(
                        icons::icon(icons::MAGNIFER)
                            .size(px(12.0))
                            .flex_none()
                            .text_color(t.text_faint),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .overflow_hidden()
                            .child(self.search.clone()),
                    ),
            ),
        ));
        grid = grid.child(Self::specimen(
            t,
            "Dialog text field",
            "popover::dialog_field / composer::ComposerInput",
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(settings::widgets::field_label(t, "Device name"))
                .child(popover::dialog_field(self.field.clone().into_any_element())),
        ));
        grid = grid.child(Self::specimen(
            t,
            "Settings row",
            "settings::widgets::section_card / card_row / row_tile / row_title",
            settings::widgets::section_card(t).w_full().child(
                settings::widgets::card_row(t, true)
                    .child(settings::widgets::row_tile(t, icons::BELL))
                    .child(settings::widgets::row_title(t, "Notifications")),
            ),
        ));
        grid = grid.child(Self::specimen(
            t,
            "Inline error",
            "settings::widgets::error_strip",
            settings::widgets::error_strip(t, "Could not connect to this device."),
        ));
        grid = grid.child(Self::specimen(
            t,
            "Inline warning",
            "settings::widgets::warning_strip",
            settings::widgets::warning_strip(t, "A new version is available."),
        ));
        grid = grid.child(Self::specimen(
            t,
            "Popover menu",
            "popover::popover_card / menu_heading / menu_row",
            popover::popover_card(t)
                .child(popover::menu_heading(t, "Actions"))
                .child(
                    popover::menu_row(t, false, "workbench-menu-row")
                        .id("workbench-menu-row")
                        .child("New session"),
                ),
        ));
        grid = grid.child(Self::specimen(
            t,
            "Dialog surface",
            "popover::dialog_card / dialog_title / dialog_body",
            popover::dialog_card(t)
                .child(popover::dialog_title(t, "Create a space"))
                .child(popover::dialog_body(t, "Choose a name for your workspace.")),
        ));
        grid = grid.child(Self::specimen(
            t,
            "Surface toolbar",
            "surface_chrome::toolbar / input",
            surface_chrome::toolbar(t).child(
                surface_chrome::input()
                    .child(icons::icon(icons::SEARCH).size(px(16.0)))
                    .child("Search files"),
            ),
        ));
        grid = grid.child(Self::specimen(
            t,
            "Page heading",
            "settings::widgets::page_header / page_subtitle",
            div()
                .flex()
                .flex_col()
                .child(settings::widgets::page_header(t, "Appearance", Some(7)))
                .child(settings::widgets::page_subtitle(
                    t,
                    "Choose a theme and accent.",
                )),
        ));
        grid = grid.child(Self::specimen(
            t,
            "Icon control",
            "icons::icon (native SVG renderer)",
            div()
                .flex()
                .gap(px(15.0))
                .child(icons::icon(icons::SETTINGS).size(px(16.0)))
                .child(icons::icon(icons::PULL_REQUEST).size(px(24.0)))
                .child(icons::icon(icons::PANEL_LEFT_OPEN).size(px(32.0))),
        ));
        grid = grid.child(Self::specimen(
            t,
            "Toolbar icon buttons",
            "files::toolbar_button / surface_chrome::ICON_SIZE",
            div()
                .flex()
                .gap(px(surface_chrome::CONTROL_GAP))
                .child(
                    files::toolbar_button("workbench-icon-search", "Search").child(
                        icons::icon(icons::MAGNIFER)
                            .size(px(surface_chrome::ICON_SIZE))
                            .text_color(t.text_muted),
                    ),
                )
                .child(
                    files::toolbar_button("workbench-icon-refresh", "Refresh").child(
                        icons::icon(icons::REFRESH)
                            .size(px(surface_chrome::ICON_SIZE))
                            .text_color(t.text_muted),
                    ),
                ),
        ));
        let summary = |state| ChangeRequestSummary {
            provider: "github".into(),
            number: 418,
            title: "Refine the native workbench".into(),
            url: "https://github.com/zeronsh/zeron/pull/418".into(),
            state,
            base_ref: "main".into(),
            head_ref: "design/ui-workbench".into(),
        };
        grid = grid.child(Self::specimen(
            t,
            "Pull request states",
            "change_requests::pull_request_badge_preview",
            div()
                .flex()
                .flex_wrap()
                .gap(px(8.0))
                .child(change_requests::pull_request_badge_preview(
                    "workbench-pr-open".into(),
                    summary(ChangeRequestState::Open),
                    change_requests::ChangeRequestBadgeSurface::Composer,
                    t,
                ))
                .child(change_requests::pull_request_badge_preview(
                    "workbench-pr-merged".into(),
                    summary(ChangeRequestState::Merged),
                    change_requests::ChangeRequestBadgeSurface::Composer,
                    t,
                ))
                .child(change_requests::pull_request_badge_preview(
                    "workbench-pr-closed".into(),
                    summary(ChangeRequestState::Closed),
                    change_requests::ChangeRequestBadgeSurface::Composer,
                    t,
                )),
        ));
        let patch = "diff --git a/src/main.rs b/src/main.rs\nindex 1100000..2200000 100644\n--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1,3 +1,3 @@\n fn main() {\n-    let gap = 12;\n+    let gap = 8;\n }\n";
        let file = changes::parse_patch(patch)
            .into_iter()
            .next()
            .expect("workbench diff fixture");
        grid = grid.child(Self::specimen(
            t,
            "Diff hunk",
            "changes::parse_patch / render_file_body_with_syntax",
            changes::render_file_body_with_syntax(&file, None, t),
        ));
        grid = grid.child(Self::specimen(
            t,
            "Empty sessions",
            "shell::empty_sessions_message",
            shell::empty_sessions_message(t),
        ));
        grid
    }

    fn foundations(&self, t: &Theme) -> gpui::Div {
        let mut content = div().flex().flex_col().gap(px(22.0))
            .child(Self::heading(t, "LIVE THEME", "Foundations", "These swatches read the effective GPUI Theme, including installed custom variants. Typography uses the app's registered font faces."))
            .child(div().text_size(px(11.0)).text_color(t.text_muted)
                .child(format!("Variant {} · family {} · {}", t.variant_id, t.family_id, self.accent.label())));

        for (group, roles) in color_groups(t) {
            let mut swatches = div().flex().flex_wrap().gap(px(8.0));
            for (name, value) in roles {
                swatches = swatches.child(
                    div()
                        .w(px(148.0))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(t.border)
                        .overflow_hidden()
                        .child(div().h(px(48.0)).bg(value))
                        .child(
                            div()
                                .px(px(10.0))
                                .py(px(8.0))
                                .text_size(px(11.0))
                                .child(name),
                        ),
                );
            }
            content = content.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .child(div().text_size(px(16.0)).child(group))
                    .child(swatches),
            );
        }

        content = content.child(
            div()
                .mt(px(14.0))
                .flex()
                .flex_col()
                .gap(px(9.0))
                .child(div().text_size(px(17.0)).child("Typography"))
                .child(
                    div()
                        .font_family(t.font_sans.clone())
                        .text_size(px(28.0))
                        .child("The shape of Zeron."),
                )
                .child(
                    div()
                        .font_family(t.font_sans_fixed.clone())
                        .text_size(px(14.0))
                        .child("Interface chrome · Geist"),
                )
                .child(
                    div()
                        .font_family(t.font_mono.clone())
                        .text_size(px(13.0))
                        .child("const idea = true; // code"),
                )
                .child(
                    div()
                        .text_size(px(10.0))
                        .text_color(t.text_faint)
                        .child(format!(
                            "UI {} · code {} · terminal {}",
                            t.font_sans, t.font_mono, t.font_terminal
                        )),
                ),
        );

        let guide: serde_json::Value = serde_json::from_str(GUIDE).expect("source-derived metrics");
        if let Some(groups) = guide["metrics"].as_object() {
            content = content.child(
                div()
                    .mt(px(14.0))
                    .text_size(px(17.0))
                    .child("Native layout metrics"),
            );
            for (group, rows) in groups {
                let mut line = div().flex().flex_wrap().gap(px(7.0));
                if let Some(rows) = rows.as_array() {
                    for row in rows {
                        let name = row["name"].as_str().unwrap_or("metric");
                        let value = row["value"].as_f64().unwrap_or(0.0);
                        line = line.child(
                            div()
                                .px(px(10.0))
                                .py(px(8.0))
                                .rounded(px(7.0))
                                .border_1()
                                .border_color(t.border)
                                .bg(t.surface_card)
                                .text_size(px(11.0))
                                .child(format!("{name}  {value} px")),
                        );
                    }
                }
                content = content.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(7.0))
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(t.text_muted)
                                .child(group.clone()),
                        )
                        .child(line),
                );
            }
        }
        if let Some(specs) = guide["motion"].as_array() {
            content = content.child(
                div()
                    .mt(px(14.0))
                    .text_size(px(17.0))
                    .child("Motion timing"),
            );
            let mut timing = div().flex().flex_wrap().gap(px(7.0));
            for spec in specs {
                let name = spec["name"].as_str().unwrap_or("motion");
                let duration = spec["duration"].as_u64().unwrap_or(0);
                let curve = spec["curve"].as_str().unwrap_or("");
                timing = timing.child(
                    div()
                        .w(px(178.0))
                        .px(px(10.0))
                        .py(px(9.0))
                        .rounded(px(7.0))
                        .border_1()
                        .border_color(t.border)
                        .bg(t.surface_card)
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(
                            div()
                                .text_size(px(11.0))
                                .child(name.replace('_', " ").to_lowercase()),
                        )
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(t.text_muted)
                                .child(format!("{duration} ms · {curve}")),
                        ),
                );
            }
            content = content.child(timing);
        }
        content
    }

    fn icons(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::Div {
        let mut content = div().flex().flex_col().gap(px(17.0))
            .child(Self::heading(t, "EMBEDDED PRODUCTION ASSETS", "Icons & motion", "The grid uses Zeron's GPUI SVG renderer and embedded icon assets. Click any transition to run the same element-local morph used in production."))
            .child(div().text_size(px(17.0)).child("135 glyphs · 24px / optical 16px"));
        let mut grid = div().flex().flex_wrap().gap(px(7.0));
        for path in &self.icons {
            let name = path
                .strip_prefix("custom-icons/")
                .unwrap_or(path)
                .trim_end_matches(".svg");
            let small = format!("custom-icons/small/{name}.svg");
            grid = grid.child(
                div()
                    .w(px(126.0))
                    .h(px(107.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(t.border)
                    .bg(t.surface_card)
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(13.0))
                            .child(svg().path(path.clone()).size(px(24.0)).text_color(t.text))
                            .child(svg().path(small).size(px(16.0)).text_color(t.text)),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(t.text_muted)
                            .child(name.to_string()),
                    ),
            );
        }
        content = content.child(grid).child(
            div()
                .mt(px(23.0))
                .text_size(px(17.0))
                .child("36 reversible morphs"),
        );
        let mut motion_grid = div().flex().flex_wrap().gap(px(8.0));
        for (index, morph) in self.morphs.iter().enumerate() {
            let path = if self.morphed[index] {
                morph.to
            } else {
                morph.from
            };
            motion_grid = motion_grid.child(
                div()
                    .id(SharedString::from(format!("workbench-motion-tile-{index}")))
                    .w(px(190.0))
                    .h(px(117.0))
                    .rounded(px(9.0))
                    .border_1()
                    .border_color(t.border)
                    .bg(t.surface_card)
                    .cursor_pointer()
                    .hover(|s| s.bg(t.element_hover))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(9.0))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.morphed[index] = !this.morphed[index];
                        cx.notify();
                    }))
                    .child(
                        icons::icon(path)
                            .morph(SharedString::from(format!("workbench-morph-{index}")))
                            .size(px(34.0)),
                    )
                    .child(div().text_size(px(11.0)).child(morph.title.clone())),
            );
        }
        content.child(motion_grid)
    }

    fn coverage(&self, t: &Theme) -> gpui::Div {
        let catalog: serde_json::Value =
            serde_json::from_str(COMPONENTS).expect("component inventory");
        let entries = catalog.as_array().expect("component inventory array");
        let mounted_count = entries
            .iter()
            .filter(|entry| entry["workbench"] == "mounted")
            .count();
        let mut content = div().flex().flex_col().gap(px(8.0))
            .child(Self::heading(t, "HONEST COVERAGE", "What is actually mounted?", "A native card appears only when it calls the production builder. Data-bound application composites need shared fixture adapters before the workbench can claim 1:1 coverage."))
            .child(div().pb(px(13.0)).text_size(px(13.0)).text_color(t.text_muted)
                .child(format!("{} mounted / {} catalog entries · {} need adapters", mounted_count, entries.len(), entries.len() - mounted_count)));
        for entry in entries {
            let name = entry["name"].as_str().unwrap_or("");
            let source = entry["source"].as_str().unwrap_or("");
            let mounted = entry["workbench"] == "mounted";
            content = content.child(
                div()
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(t.border)
                    .bg(t.surface_card)
                    .px(px(14.0))
                    .py(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        icons::icon(if mounted { icons::CHECK } else { icons::INFO })
                            .size(px(16.0))
                            .text_color(if mounted { t.success } else { t.text_faint }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(div().text_size(px(12.0)).child(name.to_string()))
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(t.text_faint)
                                    .child(source.to_string()),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(if mounted { t.success } else { t.text_muted })
                            .child(if mounted {
                                "PRODUCTION BUILDER"
                            } else {
                                "NEEDS NATIVE FIXTURE"
                            }),
                    ),
            );
        }
        content
    }
}

impl Render for Workbench {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = Theme::of(cx).clone();
        let nav = self.navigation(&t, cx);
        let top = self.topbar(&t, cx);
        let page = match self.page {
            Page::Overview => self.overview(&t, cx),
            Page::Foundations => self.foundations(&t),
            Page::Components => div().flex().flex_col()
                .child(Self::heading(&t, "PRODUCTION BUILDERS", "Components", "Every mounted card calls a component function from crates/ui. Composed views that still require a live engine are not imitated here."))
                .child(self.components(&t, cx)),
            Page::Icons => self.icons(&t, cx),
            Page::Coverage => self.coverage(&t),
        };
        div()
            .size_full()
            .flex()
            .bg(t.bg)
            .text_color(t.text)
            .font_family(t.font_sans.clone())
            .child(nav)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(top)
                    .child(
                        div()
                            .id("workbench-scroll")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .track_scroll(&self.scroll)
                            .child(
                                div()
                                    .w_full()
                                    .max_w(px(1300.0))
                                    .mx_auto()
                                    .px(px(35.0))
                                    .py(px(32.0))
                                    .child(page),
                            ),
                    ),
            )
    }
}

fn color_groups(t: &Theme) -> Vec<(&'static str, Vec<(&'static str, Hsla)>)> {
    vec![
        (
            "Surfaces",
            vec![
                ("bg", t.bg),
                ("surface", t.surface),
                ("surface_raised", t.surface_raised),
                ("surface_card", t.surface_card),
                ("surface_dialog", t.surface_dialog),
                ("surface_overlay", t.surface_overlay),
                ("input_bg", t.input_bg),
                ("band", t.band),
            ],
        ),
        (
            "Text",
            vec![
                ("text", t.text),
                ("text_muted", t.text_muted),
                ("text_faint", t.text_faint),
                ("text_dim", t.text_dim),
                ("solid", t.solid),
                ("on_solid", t.on_solid),
            ],
        ),
        (
            "Structure",
            vec![
                ("border", t.border),
                ("border_strong", t.border_strong),
                ("element_hover", t.element_hover),
                ("element_active", t.element_active),
                ("surface_raised_hover", t.surface_raised_hover),
            ],
        ),
        (
            "Accent",
            vec![
                ("accent", t.accent),
                ("accent_strong", t.accent_strong),
                ("accent_wash", t.accent_wash),
                ("on_accent", t.on_accent),
                ("selection", t.selection),
                ("caret", t.caret),
            ],
        ),
        (
            "Feedback",
            vec![
                ("danger", t.danger),
                ("danger_muted", t.danger_muted),
                ("danger_strong", t.danger_strong),
                ("warning", t.warning),
                ("warning_muted", t.warning_muted),
                ("success", t.success),
                ("success_muted", t.success_muted),
                ("busy", t.busy),
            ],
        ),
        (
            "Code and diff",
            vec![
                ("code_text", t.code_text),
                ("code_wash", t.code_wash),
                ("diff_add", t.diff_add),
                ("diff_del", t.diff_del),
                ("diff_hunk_bg", t.diff_hunk_bg),
            ],
        ),
    ]
}

/// Open a self-contained native window without booting a workspace or engine.
pub fn run(data_dir: PathBuf) {
    let app = gpui_platform::application().with_assets(icons::Assets);
    app.run(move |cx| {
        gpui_base::init(cx);
        let ui_settings = settings::UiSettings::load(&data_dir);
        settings::init(ui_settings.clone(), data_dir.clone(), cx);
        let availability = typography::register_fonts(cx);
        typography::init(
            ui_settings.ui_font_family.clone(),
            ui_settings.ui_font_size,
            ui_settings.terminal_font_family.clone(),
            ui_settings.terminal_font_size,
            ui_settings.code_font_family.clone(),
            ui_settings.code_font_size,
            availability,
            cx,
        );
        theme_library::init(data_dir, cx);
        appearance::init(
            ui_settings.appearance,
            ui_settings.theme_selection,
            ui_settings.accent,
            ui_settings.surface,
            cx,
        );
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(gpui::Bounds::centered(
                    None,
                    size(px(1300.0), px(900.0)),
                    cx,
                ))),
                window_min_size: Some(size(px(900.0), px(620.0))),
                titlebar: Some(TitlebarOptions {
                    title: Some("Zeron UI Workbench".into()),
                    appears_transparent: false,
                    traffic_light_position: None,
                }),
                window_background: Theme::of(cx).window_background_appearance(),
                app_id: Some("zeron-ui-workbench".into()),
                ..Default::default()
            },
            |window, cx| {
                window.set_rem_size(px(typography::font_size(cx).pixels()));
                cx.new(Workbench::new)
            },
        )
        .expect("open native UI workbench");
        cx.activate(true);
    });
}

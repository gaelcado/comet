//! Native design workbench. Every specimen here is a production GPUI builder
//! or an embedded production asset; unmounted patterns and missing shared
//! builders are reported instead of being redrawn as lookalikes.

use std::path::PathBuf;

use gpui::{
    AssetSource as _, Context, Entity, Focusable as _, Hsla, IntoElement, Render, ScrollHandle,
    SharedString, Subscription, TitlebarOptions, Window, WindowBounds, WindowOptions, div,
    prelude::*, px, size, svg,
};
use serde::Deserialize;
use zeron_proto::{ChangeRequestState, ChangeRequestSummary};
use zeron_theme::{AccentPreset, AccentSelection, ThemeRegistry, ThemeSelection};

use crate::{
    appearance, badges, change_requests, changes,
    composer::{ComposerInput, ComposerInputEvent},
    experimental_icons, files, icons, loaders, markdown, motion, notice, popover, settings, shell,
    surface_chrome, theme, theme_library, typography,
};
use theme::{Appearance, Theme};

const GUIDE: &str = include_str!("../../../apps/ui-workbench/metrics.json");
const COMPONENTS: &str = include_str!("../../../apps/ui-workbench/components.json");
const MOTIONS: &str = include_str!("../../../apps/icon-lab/motions.json");
const ICON_CATALOG: &str = include_str!("../../../apps/icon-lab/catalog.json");
const AUDIT: &str = include_str!("../../../apps/ui-workbench/audit.json");
const CONSOLIDATION: &str = include_str!("../../../apps/ui-workbench/consolidation.json");

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Overview,
    Foundations,
    Components,
    CurrentIcons,
    Icons,
    Coverage,
}

impl Page {
    const ALL: [Self; 6] = [
        Self::Overview,
        Self::Foundations,
        Self::Components,
        Self::CurrentIcons,
        Self::Icons,
        Self::Coverage,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Foundations => "Foundations",
            Self::Components => "Components",
            Self::CurrentIcons => "Current icons",
            Self::Icons => "Icon atelier",
            Self::Coverage => "Inventory & gaps",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Overview => icons::WIDGET,
            Self::Foundations => icons::TUNING,
            Self::Components => icons::CHECKLIST,
            Self::CurrentIcons => icons::STAR,
            Self::Icons => icons::MAGIC_STICK_3,
            Self::Coverage => icons::LIST,
        }
    }

    fn is_experiment(self) -> bool {
        self == Self::Icons
    }
}

#[derive(Deserialize)]
struct MotionRecord {
    title: String,
    #[serde(rename = "fromIcon")]
    from_icon: String,
    #[serde(rename = "toIcon")]
    to_icon: String,
    context: String,
    duration: u32,
    mode: String,
}

struct Morph {
    title: String,
    from: &'static str,
    to: &'static str,
    context: String,
    duration: u32,
    mode: String,
}

#[derive(Deserialize)]
struct IconRecord {
    name: String,
    category: String,
    note: String,
    status: String,
}

struct Workbench {
    page: Page,
    scroll: ScrollHandle,
    appearance: Appearance,
    theme_selection: ThemeSelection,
    accent: AccentSelection,
    switch_on: bool,
    checkbox_on: bool,
    option_compact: bool,
    select_state: settings::widgets::SelectState,
    select_choice: usize,
    action_pressed: bool,
    split_on: bool,
    search: Entity<ComposerInput>,
    field: Entity<ComposerInput>,
    icon_search: Entity<ComposerInput>,
    _icon_search_sub: Subscription,
    icons: Vec<IconRecord>,
    icon_category: Option<String>,
    selected_icon: Option<String>,
    morphs: Vec<Morph>,
    morphed: Vec<bool>,
    show_all_morphs: bool,
    show_modules: bool,
    show_all_catalog: bool,
    selected_decision: Option<String>,
}

impl Workbench {
    fn new(cx: &mut Context<Self>) -> Self {
        let assets = experimental_icons::Assets;
        let paths: Vec<String> = assets
            .list("custom-icons/")
            .expect("embedded icon list")
            .into_iter()
            .map(|path| path.to_string())
            .filter(|path| !path.starts_with("custom-icons/small/"))
            .collect();
        assert_eq!(
            paths.len(),
            135,
            "workbench must show the complete icon family"
        );

        let icons: Vec<IconRecord> =
            serde_json::from_str(ICON_CATALOG).expect("reviewed icon catalog");
        assert_eq!(
            icons.len(),
            paths.len(),
            "icon catalog and native assets differ"
        );
        for icon in &icons {
            let path = format!("custom-icons/{}.svg", icon.name);
            assert!(
                paths.contains(&path),
                "catalog icon missing from native assets: {path}"
            );
        }

        let records: Vec<MotionRecord> =
            serde_json::from_str(MOTIONS).expect("reviewed icon motions");
        let resolve = |name: &str| -> &'static str {
            experimental_icons::MORPH_PATHS
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
                context: record.context,
                duration: record.duration,
                mode: record.mode,
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
        let icon_search = cx.new(|cx| {
            ComposerInput::new("Search icons", cx)
                .with_single_line()
                .with_accessibility_role(gpui::Role::SearchInput)
                .with_text_metrics(12.0, 18.0)
        });
        let icon_search_sub = cx.subscribe(&icon_search, |_, _, event, cx| {
            if matches!(event, ComposerInputEvent::Edited) {
                cx.notify();
            }
        });
        let selected_icon = icons.first().map(|icon| icon.name.clone());
        Self {
            page: Page::Overview,
            scroll: ScrollHandle::new(),
            appearance,
            theme_selection: appearance::themes(cx),
            accent,
            switch_on: false,
            checkbox_on: false,
            option_compact: true,
            select_state: settings::widgets::SelectState::default(),
            select_choice: 0,
            action_pressed: false,
            split_on: false,
            search,
            field,
            icon_search,
            _icon_search_sub: icon_search_sub,
            icons,
            icon_category: None,
            selected_icon,
            morphs,
            morphed,
            show_all_morphs: false,
            show_modules: false,
            show_all_catalog: false,
            selected_decision: None,
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

    fn navigation(&self, t: &Theme, window: &mut Window, cx: &mut Context<Self>) -> gpui::Div {
        let mut nav = div()
            .w(px(238.0))
            .h_full()
            .flex_none()
            .bg(t.surface)
            .border_r_1()
            .border_color(t.border)
            .flex()
            .flex_col()
            .pt(px(Theme::TITLEBAR_HEIGHT));
        nav = nav.child(
            div()
                .px(px(24.0))
                .pt(px(14.0))
                .pb(px(29.0))
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(typography::ui_rems(18.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .child("Design system"),
                )
                .child(
                    div()
                        .text_size(typography::ui_rems(12.0))
                        .text_color(t.text_muted)
                        .child("Native workbench"),
                ),
        );
        for (experiment, label) in [(false, "Current app"), (true, "Target studies")] {
            let mut group = div()
                .px(px(Theme::SPACE_SM))
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(settings::widgets::section_label(t, label).pb(px(8.0)));
            for page in Page::ALL
                .into_iter()
                .filter(|page| page.is_experiment() == experiment)
            {
                let selected = self.page == page;
                let key = format!("workbench-nav-{}", page.label());
                let hover_key = format!("{key}-hover");
                let selection_t = settings::widgets::tab_selection_t(
                    window,
                    format!("{key}-selection"),
                    selected,
                    motion::reduced_motion(cx),
                );
                group = group.child(
                    settings::widgets::section_tab(t, selected, selection_t, key, hover_key)
                        .role(gpui::Role::Tab)
                        .aria_label(page.label())
                        .aria_selected(selected)
                        .tab_index(0)
                        .focus_visible(|tab| tab.border_2().border_color(t.accent))
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.page = page;
                            this.scroll = ScrollHandle::new();
                            cx.notify();
                        }))
                        .child(
                            icons::icon(page.icon())
                                .size(px(16.0))
                                .text_color(if selected { t.text } else { t.text_muted }),
                        )
                        .child(page.label()),
                );
            }
            nav = nav.child(group.when(experiment, |group| group.mt(px(30.0))));
        }
        nav.child(
            div()
                .mt_auto()
                .px(px(24.0))
                .pb(px(24.0))
                .flex()
                .flex_col()
                .gap(px(5.0))
                .text_size(typography::ui_rems(11.0))
                .text_color(t.text_faint)
                .child("Current uses production builders.")
                .child("Target is an isolated proposal."),
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
        let appearance_label = if next == Appearance::Light {
            "Light"
        } else {
            "Dark"
        };
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
                    .text_size(typography::ui_rems(11.0))
                    .text_color(t.text_muted)
                    .child(format!(
                        "{} / {}",
                        if self.page.is_experiment() {
                            "TARGET STUDY"
                        } else {
                            "CURRENT APP"
                        },
                        self.page.label()
                    )),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(
                        settings::widgets::text_action(
                            t,
                            settings::widgets::ActionTone::Outlined,
                            variant_label.clone(),
                        )
                        .id("workbench-variant-button")
                        .role(gpui::Role::Button)
                        .aria_label(format!("Change {variant_label}"))
                        .tab_index(0)
                        .focus_visible(|button| button.border_2().border_color(t.accent))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.theme_selection
                                .set_variant(variant_appearance, next_variant.clone());
                            this.install_theme(cx);
                        })),
                    )
                    .child(
                        settings::widgets::text_action(
                            t,
                            settings::widgets::ActionTone::Outlined,
                            accent_name,
                        )
                        .id("workbench-accent-button")
                        .role(gpui::Role::Button)
                        .aria_label(format!("Change accent: {accent_name}"))
                        .tab_index(0)
                        .focus_visible(|button| button.border_2().border_color(t.accent))
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
                        settings::widgets::text_action(
                            t,
                            settings::widgets::ActionTone::Outlined,
                            appearance_label,
                        )
                        .id("workbench-appearance-button")
                        .role(gpui::Role::Button)
                        .aria_label(format!("Switch to {appearance_label} appearance"))
                        .tab_index(0)
                        .focus_visible(|button| button.border_2().border_color(t.accent))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.appearance = next;
                            this.install_theme(cx);
                        })),
                    ),
            )
    }

    fn heading(t: &Theme, _eyebrow: &str, title: &str, copy: &str) -> gpui::Div {
        div()
            .flex()
            .flex_col()
            .pb(px(18.0))
            .child(settings::widgets::page_header(t, title, None))
            .child(settings::widgets::page_subtitle(t, copy.to_string()))
    }

    fn overview(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::Div {
        let catalog: serde_json::Value =
            serde_json::from_str(COMPONENTS).expect("component inventory");
        let entries = catalog.as_array().expect("component inventory array");
        let mounted = entries
            .iter()
            .filter(|entry| entry["workbench"] == "mounted")
            .count();
        let audit: serde_json::Value = serde_json::from_str(AUDIT).expect("source audit");
        let source_files = audit["sourceFiles"].as_u64().unwrap_or(0);
        let mut current = settings::widgets::section_card(t).mt(px(0.0));
        for (index, page, title, detail) in [
            (
                0,
                Page::Foundations,
                "Foundations",
                "Live theme roles, typography, spacing and motion",
            ),
            (
                1,
                Page::Components,
                "Native components",
                "Production builders and interactive states",
            ),
            (
                2,
                Page::CurrentIcons,
                "Current icons",
                "Every control glyph shipped in the desktop app",
            ),
            (
                3,
                Page::Coverage,
                "Inventory & gaps",
                "Families, fixtures and consolidation scope",
            ),
        ] {
            current = current.child(
                settings::widgets::card_row(t, index == 0)
                    .id(SharedString::from(format!("workbench-destination-{index}")))
                    .role(gpui::Role::Button)
                    .aria_label(title)
                    .tab_index(0)
                    .focus_visible(|row| row.border_2().border_color(t.accent))
                    .cursor_pointer()
                    .hover(|row| row.bg(t.element_hover))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.page = page;
                        this.scroll = ScrollHandle::new();
                        cx.notify();
                    }))
                    .child(settings::widgets::row_tile(t, page.icon()))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(3.0))
                            .child(settings::widgets::row_title(t, title))
                            .child(
                                div()
                                    .text_size(typography::ui_rems(12.0))
                                    .text_color(t.text_muted)
                                    .child(detail),
                            ),
                    )
                    .child(
                        icons::icon(icons::ALT_ARROW_RIGHT)
                            .size(px(16.0))
                            .text_color(t.text_faint),
                    ),
            );
        }
        let target = settings::widgets::section_card(t).mt(px(0.0)).child(
            settings::widgets::card_row(t, true)
                .id("workbench-destination-icons")
                .role(gpui::Role::Button)
                .aria_label("Open icon atelier")
                .tab_index(0)
                .focus_visible(|row| row.border_2().border_color(t.accent))
                .cursor_pointer()
                .hover(|row| row.bg(t.element_hover))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.page = Page::Icons;
                    this.scroll = ScrollHandle::new();
                    cx.notify();
                }))
                .child(settings::widgets::row_tile(t, icons::STAR))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(3.0))
                        .child(settings::widgets::row_title(t, "Icon atelier"))
                        .child(
                            div()
                                .text_size(typography::ui_rems(12.0))
                                .text_color(t.text_muted)
                                .child("135 proposed glyphs and 36 reversible transitions"),
                        ),
                )
                .child(
                    div()
                        .text_size(typography::ui_rems(11.0))
                        .text_color(t.accent)
                        .child("EXPERIMENT"),
                ),
        );
        let status = settings::widgets::section_card(t)
            .mt(px(0.0))
            .child(
                settings::widgets::card_row(t, true)
                    .child(settings::widgets::row_title(t, "Native specimens"))
                    .child(
                        div()
                            .ml_auto()
                            .text_color(t.text_muted)
                            .child(format!("{mounted} / {} mounted", entries.len())),
                    ),
            )
            .child(
                settings::widgets::card_row(t, false)
                    .child(settings::widgets::row_title(t, "Desktop source scan"))
                    .child(
                        div()
                            .ml_auto()
                            .text_color(t.text_muted)
                            .child(format!("{source_files} Rust modules")),
                    ),
            );
        div()
            .flex()
            .flex_col()
            .child(Self::heading(
                t,
                "CURRENT APP",
                "Design system",
                "A native reference for Zeron's shipped interface. Specimens call production GPUI builders; the inventory names missing fixtures and patterns that need consolidation.",
            ))
            .child(settings::widgets::section(t, "Current app", current))
            .child(settings::widgets::section(t, "Target studies", target))
            .child(settings::widgets::section(t, "Coverage", status))
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
            .rounded(px(12.0))
            .bg(settings::widgets::block_fill(t))
            .flex()
            .flex_col()
            .child(
                div()
                    .min_h(px(112.0))
                    .p(px(20.0))
                    .flex()
                    .items_center()
                    .child(content),
            )
            .child(
                div()
                    .border_t_1()
                    .border_color(settings::widgets::row_divider(t))
                    .px(px(20.0))
                    .py(px(12.0))
                    .flex()
                    .flex_col()
                    .gap(px(3.0))
                    .child(settings::widgets::row_title(t, title))
                    .child(
                        div()
                            .text_size(typography::ui_rems(11.0))
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
        let mut actions = div().flex().flex_wrap().gap(px(12.0));
        let mut inputs = div().flex().flex_wrap().gap(px(12.0));
        let mut feedback = div().flex().flex_wrap().gap(px(12.0));
        let mut navigation = div().flex().flex_wrap().gap(px(12.0));
        let mut collections = div().flex().flex_wrap().gap(px(12.0));
        let mut composed = div().flex().flex_wrap().gap(px(12.0));
        actions = actions.child(Self::specimen(
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
        let mut settings_actions = div().flex().flex_wrap().gap(px(8.0));
        for (tone, label) in [
            (settings::widgets::ActionTone::Quiet, "Quiet"),
            (settings::widgets::ActionTone::Outlined, "Outlined"),
            (settings::widgets::ActionTone::Filled, "Filled"),
            (settings::widgets::ActionTone::Solid, "Solid"),
        ] {
            settings_actions = settings_actions.child(
                settings::widgets::action_button(t, tone)
                    .id(SharedString::from(format!("workbench-action-{label}")))
                    .role(gpui::Role::Button)
                    .aria_label(label)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.action_pressed = !this.action_pressed;
                        cx.notify();
                    }))
                    .child(label),
            );
        }
        actions = actions.child(Self::specimen(
            t,
            "Settings actions",
            "settings::widgets::action_button · four tones",
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(settings_actions)
                .child(
                    div()
                        .text_size(typography::ui_rems(11.0))
                        .text_color(t.text_muted)
                        .child(if self.action_pressed {
                            "Activated"
                        } else {
                            "Choose a tone"
                        }),
                ),
        ));
        actions = actions.child(Self::specimen(
            t,
            "Diff layout toggle",
            "changes::Changes::header_toggle · unified / split",
            div()
                .flex()
                .items_center()
                .gap(px(10.0))
                .child(
                    changes::Changes::header_toggle(
                        "workbench-diff-layout",
                        if self.split_on {
                            icons::SPLIT_COLUMNS
                        } else {
                            icons::SPLIT_COLUMNS
                        },
                        self.split_on,
                        t,
                    )
                    .role(gpui::Role::Button)
                    .aria_label("Toggle diff layout")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.split_on = !this.split_on;
                        cx.notify();
                    })),
                )
                .child(if self.split_on { "Split" } else { "Unified" }),
        ));
        feedback = feedback.child(Self::specimen(
            t,
            "Settings badges",
            "settings::widgets::badge / badge_active",
            div()
                .flex()
                .gap(px(8.0))
                .child(settings::widgets::badge(t, "Default"))
                .child(settings::widgets::badge_active(t, "Active")),
        ));
        inputs = inputs.child(Self::specimen(
            t,
            "Toggle switch",
            "settings::widgets::toggle_switch",
            div()
                .id("workbench-toggle")
                .role(gpui::Role::Button)
                .aria_label("Toggle switch")
                .cursor_pointer()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.switch_on = !this.switch_on;
                    cx.notify();
                }))
                .flex()
                .items_center()
                .gap(px(12.0))
                .child(settings::widgets::toggle_switch(
                    t,
                    toggle,
                    "workbench-switch",
                ))
                .child(if toggle { "On" } else { "Off" }),
        ));
        inputs = inputs.child(Self::specimen(
            t,
            "Settings select",
            "settings::widgets::select · pointer and keyboard",
            settings::widgets::select(
                "workbench-settings-select",
                "Density",
                t,
                |this: &mut Workbench| &mut this.select_state,
            )
            .options(
                [
                    settings::widgets::SelectOption::new("Comfortable"),
                    settings::widgets::SelectOption::new("Compact"),
                    settings::widgets::SelectOption::new("Dense"),
                ],
                self.select_choice,
            )
            .width(220.0)
            .on_select(|this, choice, _, cx| {
                this.select_choice = choice;
                cx.notify();
            })
            .render(&self.select_state, cx),
        ));
        inputs = inputs.child(Self::specimen(
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
        inputs = inputs.child(Self::specimen(
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
        inputs = inputs.child(Self::specimen(
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
        collections = collections.child(Self::specimen(
            t,
            "Settings row",
            "settings::widgets::section_card / card_row / row_tile / row_title",
            settings::widgets::section_card(t).w_full().child(
                settings::widgets::card_row(t, true)
                    .child(settings::widgets::row_tile(t, icons::BELL))
                    .child(settings::widgets::row_title(t, "Notifications")),
            ),
        ));
        feedback = feedback.child(Self::specimen(
            t,
            "Inline error",
            "settings::widgets::error_strip",
            settings::widgets::error_strip(t, "Could not connect to this device."),
        ));
        feedback = feedback.child(Self::specimen(
            t,
            "Inline warning",
            "settings::widgets::warning_strip",
            settings::widgets::warning_strip(t, "A new version is available."),
        ));
        navigation = navigation.child(Self::specimen(
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
        composed = composed.child(Self::specimen(
            t,
            "Dialog surface",
            "popover::dialog_card / dialog_title / dialog_body",
            popover::dialog_card(t)
                .child(popover::dialog_title(t, "Create a space"))
                .child(popover::dialog_body(t, "Choose a name for your workspace.")),
        ));
        actions = actions.child(Self::specimen(
            t,
            "Surface toolbar",
            "surface_chrome::toolbar / input",
            surface_chrome::toolbar(t).child(
                surface_chrome::input()
                    .child(
                        icons::icon(icons::MAGNIFER)
                            .size(px(16.0))
                            .text_color(t.text_muted),
                    )
                    .child("Search files"),
            ),
        ));
        navigation = navigation.child(Self::specimen(
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
        actions = actions.child(Self::specimen(
            t,
            "Icon control",
            "icons::icon (native SVG renderer)",
            div()
                .flex()
                .gap(px(15.0))
                .child(
                    icons::icon(icons::SETTINGS)
                        .size(px(16.0))
                        .text_color(t.text),
                )
                .child(
                    icons::icon(icons::PULL_REQUEST)
                        .size(px(24.0))
                        .text_color(t.text),
                )
                .child(
                    icons::icon(icons::SIDEBAR_MINIMALISTIC_LEFT)
                        .size(px(32.0))
                        .text_color(t.text),
                ),
        ));
        actions = actions.child(Self::specimen(
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
        feedback = feedback.child(Self::specimen(
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
        collections = collections.child(Self::specimen(
            t,
            "Diff hunk",
            "changes::parse_patch / render_file_body_with_syntax",
            changes::render_file_body_with_syntax(&file, None, t),
        ));
        navigation = navigation.child(Self::specimen(
            t,
            "Empty sessions",
            "shell::empty_sessions_message",
            shell::empty_sessions_message(t),
        ));
        feedback = feedback.child(Self::specimen(
            t,
            "Failure notices",
            "notice::notice_chip · composer / transcript variants",
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(7.0))
                .child(notice::notice_chip(
                    t,
                    false,
                    "Run failed",
                    "The process exited with code 1.",
                    notice::NoticeChipIcon::Plain,
                ))
                .child(notice::notice_chip(
                    t,
                    true,
                    "Retry available",
                    "The connection was interrupted.",
                    notice::NoticeChipIcon::Tile,
                )),
        ));
        feedback = feedback.child(Self::specimen(
            t,
            "Loading glyph",
            "loaders::gradient_spinner",
            div()
                .flex()
                .items_center()
                .gap(px(12.0))
                .child(loaders::gradient_spinner(
                    "workbench-loading",
                    t,
                    5.0,
                    cx.entity_id(),
                    cx,
                ))
                .child("Working"),
        ));
        feedback = feedback.child(Self::specimen(
            t,
            "Upload progress",
            "loaders::upload_progress_ring",
            div()
                .size(px(46.0))
                .rounded(px(9.0))
                .bg(t.text_muted.opacity(0.6))
                .flex()
                .items_center()
                .justify_center()
                .child(loaders::upload_progress_ring(64, 32.0)),
        ));
        let context_badge = badges::MessageBadge {
            icon: icons::INFO_CIRCLE,
            label: "Review context".into(),
            details: vec![badges::BadgeDetail {
                location: "src/main.rs:42".into(),
                tag: Some("R".into()),
                body: "Check this branch before merging.".into(),
            }],
        };
        collections = collections.child(Self::specimen(
            t,
            "Message context badge",
            "badges::render · hover for source detail",
            badges::render("workbench-context-badge", &context_badge, t),
        ));
        inputs = inputs.child(Self::specimen(
            t,
            "Visual option cards",
            "settings::widgets::option_card_row / option_card",
            settings::widgets::option_card_row()
                .child(
                    settings::widgets::option_card(
                        t,
                        icons::WIDGET,
                        "Compact",
                        self.option_compact,
                        if self.option_compact { 1.0 } else { 0.0 },
                        div()
                            .w_full()
                            .h_full()
                            .rounded(px(settings::widgets::OPTION_CARD_RADIUS))
                            .bg(t.surface)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                icons::icon(icons::WIDGET)
                                    .size(px(22.0))
                                    .text_color(t.accent),
                            )
                            .into_any_element(),
                    )
                    .id("workbench-option-compact")
                    .role(gpui::Role::Button)
                    .aria_label("Compact layout")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.option_compact = true;
                        cx.notify();
                    })),
                )
                .child(
                    settings::widgets::option_card(
                        t,
                        icons::LIST,
                        "List",
                        !self.option_compact,
                        if self.option_compact { 0.0 } else { 1.0 },
                        div()
                            .w_full()
                            .h_full()
                            .rounded(px(settings::widgets::OPTION_CARD_RADIUS))
                            .bg(t.surface)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                icons::icon(icons::LIST)
                                    .size(px(22.0))
                                    .text_color(t.text_muted),
                            )
                            .into_any_element(),
                    )
                    .id("workbench-option-list")
                    .role(gpui::Role::Button)
                    .aria_label("List layout")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.option_compact = false;
                        cx.notify();
                    })),
                ),
        ));
        feedback = feedback.child(Self::specimen(
            t,
            "Zeron mark loader",
            "loaders::zeron_mark_loader",
            loaders::zeron_mark_loader("workbench-mark", t, 68.0, cx.entity_id(), cx),
        ));
        feedback = feedback.child(Self::specimen(
            t,
            "Skeleton states",
            "popover::skeleton_rows / skeleton_menu_rows",
            div()
                .w_full()
                .flex()
                .gap(px(13.0))
                .child(div().flex_1().child(popover::skeleton_rows(
                    "workbench-skeleton-list",
                    t,
                    3,
                    cx.entity_id(),
                    cx,
                )))
                .child(div().flex_1().child(popover::skeleton_menu_rows(
                    "workbench-skeleton-menu",
                    t,
                    3,
                    cx.entity_id(),
                    cx,
                ))),
        ));
        feedback = feedback.child(Self::specimen(
            t,
            "Popover error",
            "popover::error_row",
            popover::error_row(t, "Unable to load options."),
        ));
        navigation = navigation.child(Self::specimen(
            t,
            "Command palette shell",
            "popover::palette_card / palette_search_icon / key_hint_pair",
            popover::palette_card(t, px(270.0), 12.0)
                .child(
                    div()
                        .p(px(11.0))
                        .flex()
                        .items_center()
                        .gap(px(9.0))
                        .child(popover::palette_search_icon(t))
                        .child("Search commands"),
                )
                .child(div().border_t_1().border_color(t.border).p(px(8.0)).child(
                    popover::key_hint_pair(t, icons::ARROW_UP, icons::ARROW_DOWN, "Navigate"),
                )),
        ));
        let mut content = div()
            .flex()
            .flex_col()
            .child(Self::heading(
                t,
                "PRODUCTION BUILDERS",
                "Native components",
                "Interact with production GPUI builders. Each specimen names its source; app-bound views and unresolved shared patterns appear in Inventory & gaps.",
            ));
        for (label, count, specimens) in [
            ("Actions", 6, actions),
            ("Inputs", 6, inputs),
            ("Feedback", 10, feedback),
            ("Navigation", 4, navigation),
            ("Collections", 3, collections),
            ("Composed", 1, composed),
        ] {
            content = content.child(settings::widgets::section(
                t,
                format!("{label} · {count} specimens"),
                specimens,
            ));
        }
        content
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

    fn current_icons(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::Div {
        let mut paths: Vec<String> = icons::Assets
            .list("icons/")
            .expect("current embedded icon list")
            .into_iter()
            .map(|path| path.to_string())
            .collect();
        paths.sort();
        let query = self.icon_search.read(cx).text().trim().to_lowercase();
        let visible: Vec<_> = paths
            .iter()
            .filter(|path| path.to_lowercase().contains(&query))
            .collect();
        let mut grid = div().flex().flex_wrap().gap(px(8.0));
        for path in &visible {
            let name = path
                .strip_prefix("icons/")
                .and_then(|name| name.strip_suffix(".svg"))
                .unwrap_or(path);
            grid = grid.child(
                div()
                    .w(px(124.0))
                    .min_h(px(98.0))
                    .p(px(10.0))
                    .rounded(px(9.0))
                    .bg(settings::widgets::block_fill(t))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(11.0))
                    .child(
                        svg()
                            .path(SharedString::from((*path).clone()))
                            .size(px(24.0))
                            .text_color(t.text),
                    )
                    .child(
                        div()
                            .max_w_full()
                            .text_size(typography::ui_rems(10.0))
                            .text_color(t.text_muted)
                            .child(name.to_string()),
                    ),
            );
        }
        div()
            .flex()
            .flex_col()
            .child(Self::heading(
                t,
                "CURRENT APP",
                "Current icons",
                "These are the control and identity SVGs loaded by production icons::Assets. They remain the app's current glyphs while the custom set is reviewed in the separate Icon atelier.",
            ))
            .child(
                div()
                    .mt(px(12.0))
                    .max_w(px(330.0))
                    .child(popover::dialog_field(self.icon_search.clone().into_any_element())),
            )
            .child(settings::widgets::section_label(
                t,
                format!("Control assets · {} of {}", visible.len(), paths.len()),
            ).mt(px(30.0)).mb(px(10.0)))
            .child(grid)
            .when(visible.is_empty(), |page| {
                page.child(
                    div()
                        .py(px(20.0))
                        .text_color(t.text_muted)
                        .child("No current icons match this search."),
                )
            })
    }

    fn icons(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::Div {
        let query = self.icon_search.read(cx).text().trim().to_lowercase();
        let mut content = div()
            .flex()
            .flex_col()
            .gap(px(32.0))
            .child(Self::heading(
                t,
                "TARGET STUDY",
                "Icon atelier",
                "A proposed icon language, separate from the current app. Browse 135 custom SVGs and try 36 reversible transitions. These assets are mounted only in this workbench.",
            ))
            .child(
                div()
                    .px(px(18.0))
                    .py(px(15.0))
                    .rounded(px(12.0))
                    .bg(t.accent_wash)
                    .flex()
                    .flex_col()
                    .gap(px(5.0))
                    .child(settings::widgets::row_title(t, "Experimental proposal"))
                    .child(
                        div()
                            .text_size(typography::ui_rems(12.0))
                            .text_color(t.text_muted)
                            .child("The Current app pages render today's production icons. This page isolates candidate glyphs, optical masters and motion for review."),
                    ),
            );

        let mut comparison = settings::widgets::section_card(t).mt(px(0.0));
        for (index, label, current, proposed) in [
            (0, "Settings", icons::SETTINGS, experimental_icons::SETTINGS),
            (1, "Folder", icons::FOLDER, experimental_icons::FOLDER),
            (
                2,
                "Pull request",
                icons::PULL_REQUEST,
                experimental_icons::PULL_REQUEST,
            ),
            (3, "Notification", icons::BELL, experimental_icons::BELL),
            (4, "Light", icons::SUN, experimental_icons::SUN),
            (5, "Dark", icons::MOON, experimental_icons::MOON),
        ] {
            comparison = comparison.child(
                settings::widgets::card_row(t, index == 0)
                    .child(
                        div()
                            .w(px(140.0))
                            .text_size(typography::ui_rems(12.0))
                            .child(label),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(icons::icon(current).size(px(20.0)).text_color(t.text))
                            .child(
                                div()
                                    .text_size(typography::ui_rems(11.0))
                                    .text_color(t.text_faint)
                                    .child("Current"),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                experimental_icons::icon(proposed)
                                    .size(px(20.0))
                                    .text_color(t.text),
                            )
                            .child(
                                div()
                                    .text_size(typography::ui_rems(11.0))
                                    .text_color(t.text_faint)
                                    .child("Target"),
                            ),
                    ),
            );
        }
        content = content.child(settings::widgets::section(
            t,
            "Current → target",
            comparison,
        ));

        let shown_morphs = if self.show_all_morphs {
            self.morphs.len()
        } else {
            8
        };
        let mut motion_grid = div().flex().flex_wrap().gap(px(10.0));
        for (index, morph) in self.morphs.iter().enumerate().take(shown_morphs) {
            let path = if self.morphed[index] {
                morph.to
            } else {
                morph.from
            };
            motion_grid = motion_grid.child(
                div()
                    .id(SharedString::from(format!("workbench-motion-tile-{index}")))
                    .w(px(220.0))
                    .min_h(px(132.0))
                    .rounded(px(9.0))
                    .border_1()
                    .border_color(t.border)
                    .bg(t.surface_card)
                    .p(px(15.0))
                    .role(gpui::Role::Button)
                    .aria_label(morph.title.clone())
                    .cursor_pointer()
                    .hover(|s| s.bg(t.element_hover))
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.morphed[index] = !this.morphed[index];
                        cx.notify();
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.0))
                            .child(
                                experimental_icons::icon(morph.from)
                                    .size(px(20.0))
                                    .text_color(t.text_muted),
                            )
                            .child(
                                experimental_icons::icon(path)
                                    .morph(SharedString::from(format!("workbench-morph-{index}")))
                                    .size(px(36.0))
                                    .text_color(t.text),
                            )
                            .child(
                                experimental_icons::icon(morph.to)
                                    .size(px(20.0))
                                    .text_color(t.text_muted),
                            ),
                    )
                    .child(div().text_size(px(12.0)).child(morph.title.clone()))
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(t.text_muted)
                            .child(morph.context.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(9.0))
                            .text_color(t.text_faint)
                            .child(format!("{} ms · {}", morph.duration, morph.mode)),
                    ),
            );
        }
        content = content.child(
            div()
                .flex()
                .flex_col()
                .gap(px(13.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(div().text_size(px(18.0)).child("State transitions"))
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(t.text_muted)
                                .child("36 reversible pairs · click to play"),
                        ),
                )
                .child(motion_grid)
                .child(
                    div()
                        .id("workbench-more-motion")
                        .h(px(30.0))
                        .px(px(11.0))
                        .rounded(px(6.0))
                        .border_1()
                        .border_color(t.border)
                        .role(gpui::Role::Button)
                        .aria_label("Show more or fewer icon transitions")
                        .cursor_pointer()
                        .hover(|s| s.bg(t.element_hover))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.show_all_morphs = !this.show_all_morphs;
                            cx.notify();
                        }))
                        .flex()
                        .items_center()
                        .child(if self.show_all_morphs {
                            "Show featured transitions"
                        } else {
                            "Show all 36 transitions"
                        }),
                ),
        );

        let categories = [
            "Navigation",
            "Workspace",
            "Files",
            "Composer",
            "Git & changes",
            "Agents & tools",
            "Settings",
            "Status",
        ];
        let visible_count = self
            .icons
            .iter()
            .filter(|icon| {
                self.icon_category
                    .as_deref()
                    .is_none_or(|category| icon.category == category)
                    && (query.is_empty()
                        || icon.name.contains(&query)
                        || icon.note.to_lowercase().contains(&query))
            })
            .count();
        let mut filters = div().flex().flex_wrap().gap(px(7.0));
        for category in std::iter::once(None).chain(categories.iter().map(|name| Some(*name))) {
            let selected = self.icon_category.as_deref() == category;
            let label = category.unwrap_or("All");
            let target = category.map(str::to_string);
            filters = filters.child(
                div()
                    .id(SharedString::from(format!("workbench-category-{label}")))
                    .h(px(29.0))
                    .px(px(10.0))
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(if selected { t.accent } else { t.border })
                    .bg(if selected {
                        t.element_active
                    } else {
                        t.surface_card
                    })
                    .role(gpui::Role::Button)
                    .aria_label(format!("Filter icons: {label}"))
                    .cursor_pointer()
                    .hover(|s| s.bg(t.element_hover))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.icon_category = target.clone();
                        cx.notify();
                    }))
                    .flex()
                    .items_center()
                    .text_size(px(11.0))
                    .child(label.to_string()),
            );
        }
        let mut library =
            div()
                .flex()
                .flex_col()
                .gap(px(17.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(16.0))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(5.0))
                                .child(div().text_size(px(18.0)).child("Glyph library"))
                                .child(div().text_size(px(11.0)).text_color(t.text_muted).child(
                                    format!("{visible_count} of 135 · 24px and optical 16px"),
                                )),
                        )
                        .child(
                            div()
                                .w(px(260.0))
                                .flex_none()
                                .child(popover::search_input_frame(
                                    t,
                                    self.icon_search.clone().into_any_element(),
                                )),
                        ),
                )
                .child(filters);
        if let Some(selected) = self
            .selected_icon
            .as_deref()
            .and_then(|name| self.icons.iter().find(|icon| icon.name == name))
        {
            let regular = format!("custom-icons/{}.svg", selected.name);
            let optical = format!("custom-icons/small/{}.svg", selected.name);
            library = library.child(
                div()
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(t.border)
                    .bg(t.surface_raised)
                    .p(px(20.0))
                    .flex()
                    .items_center()
                    .gap(px(22.0))
                    .child(
                        div()
                            .size(px(74.0))
                            .rounded(px(9.0))
                            .bg(t.surface_card)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(svg().path(regular).size(px(42.0)).text_color(t.text)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(5.0))
                            .child(div().text_size(px(16.0)).child(selected.name.clone()))
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(t.text_muted)
                                    .child(format!("{} · {}", selected.category, selected.status)),
                            )
                            .child(
                                div()
                                    .max_w(px(590.0))
                                    .text_size(px(11.0))
                                    .text_color(t.text_muted)
                                    .child(selected.note.clone()),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(svg().path(optical).size(px(16.0)).text_color(t.text))
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(t.text_faint)
                                    .child("optical 16"),
                            ),
                    ),
            );
        }
        for category in categories {
            if self
                .icon_category
                .as_deref()
                .is_some_and(|selected| selected != category)
            {
                continue;
            }
            let visible: Vec<&IconRecord> = self
                .icons
                .iter()
                .filter(|icon| {
                    icon.category == category
                        && (query.is_empty()
                            || icon.name.contains(&query)
                            || icon.note.to_lowercase().contains(&query))
                })
                .collect();
            if visible.is_empty() {
                continue;
            }
            let mut grid = div().flex().flex_wrap().gap(px(8.0));
            for icon in &visible {
                let regular = format!("custom-icons/{}.svg", icon.name);
                let optical = format!("custom-icons/small/{}.svg", icon.name);
                let selected = self.selected_icon.as_deref() == Some(icon.name.as_str());
                let name = icon.name.clone();
                grid = grid.child(
                    div()
                        .id(SharedString::from(format!("workbench-icon-{name}")))
                        .w(px(142.0))
                        .min_h(px(99.0))
                        .p(px(11.0))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(if selected { t.accent } else { t.border })
                        .bg(if selected {
                            t.element_active
                        } else {
                            t.surface_card
                        })
                        .role(gpui::Role::Button)
                        .aria_label(format!("Inspect icon: {name}"))
                        .cursor_pointer()
                        .hover(|s| s.bg(t.element_hover))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.selected_icon = Some(name.clone());
                            cx.notify();
                        }))
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap(px(10.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(11.0))
                                .child(svg().path(regular).size(px(24.0)).text_color(t.text))
                                .child(svg().path(optical).size(px(16.0)).text_color(t.text_muted)),
                        )
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(t.text_muted)
                                .child(icon.name.clone()),
                        ),
                );
            }
            library = library.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(div().text_size(px(13.0)).child(category.to_string()))
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(t.text_faint)
                                    .child(format!("{}", visible.len())),
                            ),
                    )
                    .child(grid),
            );
        }
        if visible_count == 0 {
            library = library.child(
                div()
                    .py(px(25.0))
                    .text_size(px(12.0))
                    .text_color(t.text_muted)
                    .child("No glyphs match this search. Try a name, action or another category."),
            );
        }
        content.child(library)
    }

    fn coverage(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::Div {
        let catalog: serde_json::Value =
            serde_json::from_str(COMPONENTS).expect("component inventory");
        let entries = catalog.as_array().expect("component inventory array");
        let audit: serde_json::Value = serde_json::from_str(AUDIT).expect("source audit");
        let mounted_count = entries
            .iter()
            .filter(|entry| entry["workbench"] == "mounted")
            .count();
        let files = audit["sourceFiles"].as_u64().unwrap_or(0);
        let cataloged_files = audit["catalogedSourceFiles"].as_u64().unwrap_or(0);
        let support_modules = audit["supportModules"].as_u64().unwrap_or(0);
        let unreviewed = audit["unreviewedSourceFiles"].as_u64().unwrap_or(0);
        let fingerprint = audit["sourceFingerprint"].as_str().unwrap_or("");
        let mut content = div()
            .flex()
            .flex_col()
            .gap(px(32.0))
            .child(Self::heading(
                t,
                "SOURCE-BACKED INVENTORY",
                "Inventory & gaps",
                "Every desktop UI module is classified as a native pattern or a support module. The gallery mounts exact production builders; data-bound views remain named here until they have a faithful fixture.",
            ))
            .child(
                div()
                    .p(px(20.0))
                    .rounded(px(10.0))
                    .bg(t.surface_card)
                    .border_1()
                    .border_color(t.border)
                    .flex()
                    .gap(px(18.0))
                    .child(Self::stat(t, &files.to_string(), "Rust source files scanned"))
                    .child(Self::stat(t, &entries.len().to_string(), "pattern entries"))
                    .child(Self::stat(t, &mounted_count.to_string(), "live references"))
                    .child(Self::stat(
                        t,
                        &(entries.len() - mounted_count).to_string(),
                        "unmounted patterns",
                    )),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(t.text_faint)
                    .child(format!("Source fingerprint {fingerprint} · excludes this workbench from production counts")),
            )
            .child(
                div()
                    .p(px(17.0))
                    .rounded(px(9.0))
                    .border_1()
                    .bg(settings::widgets::block_fill(t))
                    .flex()
                    .flex_col()
                    .gap(px(7.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(t.text)
                            .child(format!("{files}/{files} source modules classified")),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(t.text_muted)
                            .child(format!("{cataloged_files} pattern modules · {support_modules} documented support modules · {unreviewed} unreviewed")),
                    ),
            );

        let decisions: serde_json::Value =
            serde_json::from_str(CONSOLIDATION).expect("consolidation scope");
        let decisions = decisions.as_array().expect("consolidation decisions");
        let mut decision_rows = settings::widgets::section_card(t).mt(px(0.0));
        for (index, decision) in decisions.iter().enumerate() {
            let family = decision["family"].as_str().unwrap_or("");
            let priority = decision["priority"].as_str().unwrap_or("");
            let current = decision["current"].as_str().unwrap_or("");
            let target = decision["target"].as_str().unwrap_or("");
            let id = decision["id"].as_str().unwrap_or("").to_string();
            let expanded = self.selected_decision.as_deref() == Some(id.as_str());
            let source_count = decision["sources"].as_array().map_or(0, Vec::len);
            let source_paths = div().flex().flex_col().gap(px(4.0)).children(
                decision["sources"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|source| source.as_str())
                    .map(|source| {
                        div()
                            .text_size(typography::ui_rems(10.0))
                            .text_color(t.text_faint)
                            .child(source.to_string())
                    }),
            );
            decision_rows = decision_rows.child(
                settings::widgets::card_row(t, index == 0)
                    .id(SharedString::from(format!("workbench-decision-{id}")))
                    .role(gpui::Role::Button)
                    .aria_label(format!("Inspect {family} source locations"))
                    .aria_expanded(expanded)
                    .tab_index(0)
                    .focus_visible(|row| row.border_2().border_color(t.accent))
                    .cursor_pointer()
                    .hover(|row| row.bg(t.element_hover))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.selected_decision =
                            if this.selected_decision.as_deref() == Some(id.as_str()) {
                                None
                            } else {
                                Some(id.clone())
                            };
                        cx.notify();
                    }))
                    .flex_col()
                    .items_start()
                    .gap(px(7.0))
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(12.0))
                            .child(settings::widgets::row_title(t, family))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(
                                        div()
                                            .text_size(typography::ui_rems(10.0))
                                            .text_color(if priority == "High" {
                                                t.warning
                                            } else {
                                                t.text_muted
                                            })
                                            .child(priority.to_string()),
                                    )
                                    .child(
                                        icons::icon(if expanded {
                                            icons::ALT_ARROW_UP
                                        } else {
                                            icons::ALT_ARROW_DOWN
                                        })
                                        .size(px(14.0))
                                        .text_color(t.text_muted),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .text_size(typography::ui_rems(12.0))
                            .text_color(t.text_muted)
                            .child(current.to_string()),
                    )
                    .child(
                        div()
                            .text_size(typography::ui_rems(12.0))
                            .text_color(t.text)
                            .child(format!("Target · {target}")),
                    )
                    .child(
                        div()
                            .text_size(typography::ui_rems(10.0))
                            .text_color(t.text_faint)
                            .child(format!("{source_count} source locations · open")),
                    )
                    .when(expanded, |row| row.child(source_paths)),
            );
        }
        content = content.child(settings::widgets::section(
            t,
            format!("Consolidation scope · {} open", decisions.len()),
            decision_rows,
        ));

        let mut signal_grid = div().flex().flex_wrap().gap(px(10.0));
        for signal in audit["signals"].as_array().expect("audit signals") {
            let name = signal["name"].as_str().unwrap_or("");
            let count = signal["count"].as_u64().unwrap_or(0);
            let note = signal["note"].as_str().unwrap_or("");
            let values = signal["values"]
                .as_array()
                .into_iter()
                .flatten()
                .take(4)
                .map(|value| {
                    format!(
                        "{} × {}",
                        value["value"].as_str().unwrap_or(""),
                        value["count"].as_u64().unwrap_or(0)
                    )
                })
                .collect::<Vec<_>>()
                .join("  ·  ");
            let examples = signal["locations"]
                .as_array()
                .into_iter()
                .flatten()
                .take(2)
                .filter_map(|location| location.as_str())
                .collect::<Vec<_>>()
                .join("  ·  ");
            signal_grid = signal_grid.child(
                div()
                    .w_full()
                    .min_h(px(170.0))
                    .p(px(17.0))
                    .rounded(px(9.0))
                    .border_1()
                    .border_color(t.border)
                    .bg(t.surface_card)
                    .flex()
                    .flex_col()
                    .gap(px(7.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(div().text_size(px(13.0)).child(name.to_string()))
                            .child(
                                div()
                                    .text_size(px(19.0))
                                    .text_color(t.accent)
                                    .child(count.to_string()),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(t.text_muted)
                            .child(note.to_string()),
                    )
                    .when(!values.is_empty(), |card| {
                        card.child(
                            div()
                                .text_size(px(10.0))
                                .text_color(t.text)
                                .child(values.clone()),
                        )
                    })
                    .child(
                        div()
                            .mt_auto()
                            .text_size(px(9.0))
                            .text_color(t.text_faint)
                            .child(examples),
                    ),
            );
        }
        content = content.child(
            div()
                .flex()
                .flex_col()
                .gap(px(12.0))
                .child(settings::widgets::section_label(t, "Source signals"))
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(t.text_muted)
                        .child("These source-wide signals identify where shared tokens, accessible control treatment, or explicit exceptions deserve review."),
                )
                .child(signal_grid),
        );

        let mut family_grid = div().flex().flex_wrap().gap(px(10.0));
        for family in audit["families"].as_array().expect("audit families") {
            let mut body = div()
                .w_full()
                .p(px(17.0))
                .rounded(px(9.0))
                .border_1()
                .border_color(t.border)
                .bg(t.surface_card)
                .flex()
                .flex_col()
                .gap(px(9.0))
                .child(
                    div()
                        .pb(px(5.0))
                        .text_size(px(13.0))
                        .child(family["name"].as_str().unwrap_or("").to_string()),
                );
            for builder in family["builders"].as_array().expect("family builders") {
                let symbol = builder["symbol"].as_str().unwrap_or("");
                let references = builder["references"].as_u64().unwrap_or(0);
                body = body.child(
                    div()
                        .flex()
                        .justify_between()
                        .gap(px(10.0))
                        .text_size(px(11.0))
                        .child(symbol.to_string())
                        .child(
                            div()
                                .text_color(if references == 0 {
                                    t.warning
                                } else {
                                    t.text_muted
                                })
                                .child(format!("{references} refs")),
                        ),
                );
            }
            family_grid = family_grid.child(body);
        }
        content = content.child(
            div()
                .flex()
                .flex_col()
                .gap(px(12.0))
                .child(settings::widgets::section_label(t, "Builder families"))
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(t.text_muted)
                        .child("Different native recipes appear in these semantic families. A zero-reference builder or an overlapping family is a consolidation question, not a verdict."),
                )
                .child(family_grid),
        );

        let mut catalog_section = div()
            .flex()
            .flex_col()
            .gap(px(15.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(settings::widgets::section_label(t, "Pattern inventory"))
                    .child(
                        div()
                            .id("workbench-catalog-filter")
                            .h(px(30.0))
                            .px(px(11.0))
                            .rounded(px(6.0))
                            .border_1()
                            .border_color(t.border)
                            .role(gpui::Role::Button)
                            .aria_label("Toggle catalog filter")
                            .cursor_pointer()
                            .hover(|style| style.bg(t.element_hover))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_all_catalog = !this.show_all_catalog;
                                cx.notify();
                            }))
                            .flex()
                            .items_center()
                            .text_size(px(11.0))
                            .child(if self.show_all_catalog {
                                "Show unmounted patterns"
                            } else {
                                "Show every pattern"
                            }),
                    ),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(t.text_muted)
                    .child(format!(
                        "{} catalog entries mounted · {} remain unmounted",
                        mounted_count,
                        entries.len() - mounted_count
                    )),
            );
        for group in [
            "Foundations",
            "Actions",
            "Inputs",
            "Feedback",
            "Navigation",
            "Collections",
            "Composed",
        ] {
            let mut rows = div().flex().flex_wrap().gap(px(8.0));
            let mut count = 0;
            for entry in entries {
                let mounted = entry["workbench"] == "mounted";
                let needs_consolidation = entry["workbench"] == "needs-consolidation";
                if entry["group"] != group || (!self.show_all_catalog && mounted) {
                    continue;
                }
                count += 1;
                let name = entry["name"].as_str().unwrap_or("");
                let source = entry["source"].as_str().unwrap_or("");
                let description = entry["description"].as_str().unwrap_or("");
                rows = rows.child(
                    div()
                        .w_full()
                        .min_h(px(115.0))
                        .p(px(14.0))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(t.border)
                        .bg(t.surface_card)
                        .flex()
                        .flex_col()
                        .gap(px(5.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(div().text_size(px(12.0)).child(name.to_string()))
                                .child(
                                    div()
                                        .text_size(px(9.0))
                                        .text_color(if mounted { t.success } else { t.warning })
                                        .child(if mounted {
                                            "MOUNTED"
                                        } else if needs_consolidation {
                                            "CONSOLIDATE"
                                        } else {
                                            "FIXTURE NEEDED"
                                        }),
                                ),
                        )
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(t.text_muted)
                                .child(description.to_string()),
                        )
                        .child(
                            div()
                                .mt_auto()
                                .text_size(px(9.0))
                                .text_color(t.text_faint)
                                .child(source.to_string()),
                        ),
                );
            }
            if count > 0 {
                catalog_section = catalog_section.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(9.0))
                        .child(
                            div()
                                .text_size(px(12.0))
                                .child(format!("{group} · {count}")),
                        )
                        .child(rows),
                );
            }
        }
        content = content.child(catalog_section);

        let mut modules = div().flex().flex_col().gap(px(9.0)).child(
            div()
                .id("workbench-audit-modules")
                .h(px(34.0))
                .px(px(12.0))
                .rounded(px(7.0))
                .border_1()
                .border_color(t.border)
                .role(gpui::Role::Button)
                .aria_label("Toggle source module map")
                .cursor_pointer()
                .hover(|style| style.bg(t.element_hover))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.show_modules = !this.show_modules;
                    cx.notify();
                }))
                .flex()
                .items_center()
                .child(if self.show_modules {
                    format!("Hide {files} scanned modules")
                } else {
                    format!("Show all {files} scanned modules")
                }),
        );
        if self.show_modules {
            for module in audit["modules"].as_array().expect("audited modules") {
                let cataloged = module["catalogEntries"].as_u64().unwrap_or(0) > 0;
                let role = module["role"].as_str().unwrap_or("unreviewed");
                let names = |field: &str| {
                    let values = module[field].as_array().expect("audited symbols");
                    let mut labels = values
                        .iter()
                        .take(6)
                        .filter_map(|value| value.as_str())
                        .collect::<Vec<_>>()
                        .join(", ");
                    if values.len() > 6 {
                        labels.push_str(&format!(" +{} more", values.len() - 6));
                    }
                    labels
                };
                let detail = [
                    module["scopeReason"].as_str().map(str::to_string),
                    (!module["exports"].as_array().expect("exports").is_empty())
                        .then(|| format!("Functions: {}", names("exports"))),
                    (!module["views"].as_array().expect("views").is_empty())
                        .then(|| format!("Views: {}", names("views"))),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · ");
                modules = modules.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .py(px(8.0))
                        .border_b_1()
                        .border_color(t.border)
                        .text_size(px(10.0))
                        .child(
                            div()
                                .flex()
                                .justify_between()
                                .gap(px(15.0))
                                .child(module["path"].as_str().unwrap_or("").to_string())
                                .child(
                                    div()
                                        .text_color(if cataloged {
                                            t.success
                                        } else {
                                            t.text_faint
                                        })
                                        .child(format!(
                                            "{} · {} public functions · {} render impls",
                                            role.to_uppercase(),
                                            module["publicFunctions"].as_u64().unwrap_or(0),
                                            module["renderImpls"].as_u64().unwrap_or(0)
                                        )),
                                ),
                        )
                        .when(!detail.is_empty(), |row| {
                            row.child(
                                div()
                                    .text_size(px(9.0))
                                    .text_color(t.text_faint)
                                    .child(detail),
                            )
                        }),
                );
            }
        }
        content.child(modules)
    }
}

impl Render for Workbench {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = Theme::of(cx).clone();
        let nav = self.navigation(&t, window, cx);
        let top = self.topbar(&t, cx);
        let page = match self.page {
            Page::Overview => self.overview(&t, cx),
            Page::Foundations => self.foundations(&t),
            Page::Components => self.components(&t, cx),
            Page::CurrentIcons => self.current_icons(&t, cx),
            Page::Icons => self.icons(&t, cx),
            Page::Coverage => self.coverage(&t, cx),
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
                            .child(settings::widgets::page_column().pt(px(32.0)).child(page)),
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
    let app = gpui_platform::application().with_assets(experimental_icons::Assets);
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

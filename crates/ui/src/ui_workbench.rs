//! Native design workbench. Every specimen here is a production GPUI builder
//! or an embedded production asset; unmounted patterns and missing shared
//! builders are reported instead of being redrawn as lookalikes.
//!
//! The workbench is a contributor tool: every page filters from one field,
//! every builder, token, icon and source location can be copied, and pages
//! open directly from the command line (`zeron ui-workbench components`).

use std::{collections::HashMap, path::PathBuf, time::Duration};

use gpui::{
    AnyElement, AssetSource as _, BoxShadow, ClipboardItem, Context, Entity, FocusHandle,
    Focusable as _, Hsla, IntoElement, KeyDownEvent, Render, Rgba, ScrollHandle, SharedString,
    Subscription, Task, TitlebarOptions, Window, WindowBounds, WindowOptions, div, point,
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
use settings::widgets as w;
use theme::{Appearance, Theme};

const METRICS: &str = include_str!("../../../apps/ui-workbench/metrics.json");
const COMPONENTS: &str = include_str!("../../../apps/ui-workbench/components.json");
const MOTIONS: &str = include_str!("../../../apps/icon-lab/motions.json");
const ICON_CATALOG: &str = include_str!("../../../apps/icon-lab/catalog.json");
const AUDIT: &str = include_str!("../../../apps/ui-workbench/audit.json");
const CONSOLIDATION: &str = include_str!("../../../apps/ui-workbench/consolidation.json");
/// Constant names are read from the source that declares them, so the copied
/// identifier is always the one a contributor can paste into production code.
const CURRENT_ICON_SOURCE: &str = include_str!("icons.rs");
const PROPOSED_ICON_SOURCE: &str = include_str!("experimental_icons/generated.rs");

const NAV_WIDTH: f32 = 232.0;
const TOPBAR_HEIGHT: f32 = 52.0;
/// Wider than the Settings column: specimens and glyphs need a gallery measure.
const PAGE_MAX_WIDTH: f32 = 1120.0;
const PAGE_PAD_X: f32 = 40.0;
const GRID_GAP: f32 = Theme::SPACE_LG;
const SECTION_GAP: f32 = 40.0;
const CARD_RADIUS: f32 = 12.0;
/// Concentric with [`CARD_RADIUS`] across the 8px inset of tiles in a card.
const INSET_RADIUS: f32 = 4.0;
const WELL_RADIUS: f32 = 8.0;
const COPIED_FOR: Duration = Duration::from_millis(1600);
const FEATURED_MORPHS: usize = 8;

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

    /// Command-line name: `zeron ui-workbench <slug>`.
    fn slug(self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::Foundations => "foundations",
            Self::Components => "components",
            Self::CurrentIcons => "icons",
            Self::Icons => "atelier",
            Self::Coverage => "inventory",
        }
    }

    fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|page| page.slug() == slug)
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

    fn summary(self) -> &'static str {
        match self {
            Self::Overview => "Start here: coverage, pages and the contributor workflow",
            Self::Foundations => "Live theme roles, contrast, type scale, spacing and motion",
            Self::Components => "Production builders mounted with interactive state",
            Self::CurrentIcons => "Every control glyph shipped in the desktop app",
            Self::Icons => "Proposed glyphs, optical masters and reversible transitions",
            Self::Coverage => "Classified modules, fixture gaps and consolidation scope",
        }
    }

    fn filter_placeholder(self) -> &'static str {
        match self {
            Self::Overview => "Filter pages and commands",
            Self::Foundations => "Filter tokens and metrics",
            Self::Components => "Filter by name or builder",
            Self::CurrentIcons => "Filter by constant or file",
            Self::Icons => "Filter glyphs, notes or transitions",
            Self::Coverage => "Filter patterns, sources and modules",
        }
    }

    fn is_experiment(self) -> bool {
        self == Self::Icons
    }

    fn shortcut(self) -> String {
        let index = Self::ALL.iter().position(|page| *page == self).unwrap_or(0) + 1;
        shortcut_label(&index.to_string())
    }
}

fn shortcut_label(key: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("⌘{key}")
    } else {
        format!("Ctrl+{key}")
    }
}

/// Launch options from the command line.
#[derive(Default)]
pub struct Options {
    /// Page slug to open first; see [`page_slugs`].
    pub page: Option<String>,
    /// `light` or `dark`; overrides the saved appearance for this window only.
    pub appearance: Option<String>,
}

/// Valid `ui-workbench` page names, for command-line help and errors.
pub fn page_slugs() -> Vec<&'static str> {
    Page::ALL.into_iter().map(Page::slug).collect()
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
    #[serde(default)]
    legacy: Vec<String>,
    #[serde(default)]
    references: Vec<String>,
}

#[derive(Deserialize)]
struct ComponentEntry {
    name: String,
    group: String,
    description: String,
    source: String,
    workbench: String,
}

#[derive(Deserialize)]
struct Decision {
    id: String,
    priority: String,
    family: String,
    current: String,
    target: String,
    sources: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PatternFilter {
    All,
    NeedsFixture,
    Consolidate,
    Mounted,
}

impl PatternFilter {
    fn admits(self, status: &str) -> bool {
        match self {
            Self::All => true,
            Self::NeedsFixture => status == "needs-fixture",
            Self::Consolidate => status == "needs-consolidation",
            Self::Mounted => status == "mounted",
        }
    }
}

const ICON_CATEGORIES: [&str; 8] = [
    "Navigation",
    "Workspace",
    "Files",
    "Composer",
    "Git & changes",
    "Agents & tools",
    "Settings",
    "Status",
];

const PATTERN_GROUPS: [&str; 7] = [
    "Foundations",
    "Actions",
    "Inputs",
    "Feedback",
    "Navigation",
    "Collections",
    "Composed",
];

/// One mounted production builder. The table drives filtering, counts and
/// ordering; [`Workbench::specimen_stage`] mounts the live element.
#[derive(Clone, Copy)]
struct SpecimenDef {
    id: SpecimenId,
    group: &'static str,
    title: &'static str,
    /// The builder to call; copied verbatim by the card's copy action.
    symbol: &'static str,
    note: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SpecimenId {
    DialogActions,
    SettingsActions,
    DiffToggle,
    SurfaceToolbar,
    IconControl,
    ToolbarIconButtons,
    ToggleSwitch,
    SettingsSelect,
    TaskCheckbox,
    FileSearch,
    DialogField,
    OptionCards,
    Badges,
    ErrorStrip,
    WarningStrip,
    PullRequestStates,
    FailureNotices,
    LoadingGlyph,
    UploadProgress,
    MarkLoader,
    Skeletons,
    PopoverError,
    PopoverMenu,
    PageHeading,
    EmptySessions,
    CommandPalette,
    SettingsRow,
    DiffHunk,
    ContextBadge,
    DialogSurface,
}

const SPECIMEN_GROUPS: [&str; 6] = [
    "Actions",
    "Inputs",
    "Feedback",
    "Navigation",
    "Collections",
    "Composed",
];

const SPECIMENS: &[SpecimenDef] = &[
    SpecimenDef {
        id: SpecimenId::DialogActions,
        group: "Actions",
        title: "Dialog actions",
        symbol: "popover::btn_primary",
        note: "with btn_ghost and btn_danger",
    },
    SpecimenDef {
        id: SpecimenId::SettingsActions,
        group: "Actions",
        title: "Settings actions",
        symbol: "settings::widgets::action_button",
        note: "Quiet, Outlined, Filled and Solid tones",
    },
    SpecimenDef {
        id: SpecimenId::DiffToggle,
        group: "Actions",
        title: "Diff layout toggle",
        symbol: "changes::Changes::header_toggle",
        note: "unified and split states",
    },
    SpecimenDef {
        id: SpecimenId::SurfaceToolbar,
        group: "Actions",
        title: "Surface toolbar",
        symbol: "surface_chrome::toolbar",
        note: "with surface_chrome::input",
    },
    SpecimenDef {
        id: SpecimenId::IconControl,
        group: "Actions",
        title: "Icon control",
        symbol: "icons::icon",
        note: "native SVG renderer at 16, 24 and 32",
    },
    SpecimenDef {
        id: SpecimenId::ToolbarIconButtons,
        group: "Actions",
        title: "Toolbar icon buttons",
        symbol: "files::toolbar_button",
        note: "sized by surface_chrome::ICON_SIZE",
    },
    SpecimenDef {
        id: SpecimenId::ToggleSwitch,
        group: "Inputs",
        title: "Toggle switch",
        symbol: "settings::widgets::toggle_switch",
        note: "caller owns activation",
    },
    SpecimenDef {
        id: SpecimenId::SettingsSelect,
        group: "Inputs",
        title: "Settings select",
        symbol: "settings::widgets::select",
        note: "pointer and keyboard",
    },
    SpecimenDef {
        id: SpecimenId::TaskCheckbox,
        group: "Inputs",
        title: "Task checkbox",
        symbol: "markdown::render::task_checkbox",
        note: "",
    },
    SpecimenDef {
        id: SpecimenId::FileSearch,
        group: "Inputs",
        title: "File search",
        symbol: "surface_chrome::input",
        note: "hosting composer::ComposerInput",
    },
    SpecimenDef {
        id: SpecimenId::DialogField,
        group: "Inputs",
        title: "Dialog text field",
        symbol: "popover::dialog_field",
        note: "hosting composer::ComposerInput",
    },
    SpecimenDef {
        id: SpecimenId::OptionCards,
        group: "Inputs",
        title: "Visual option cards",
        symbol: "settings::widgets::option_card",
        note: "in option_card_row",
    },
    SpecimenDef {
        id: SpecimenId::Badges,
        group: "Feedback",
        title: "Settings badges",
        symbol: "settings::widgets::badge",
        note: "with badge_active",
    },
    SpecimenDef {
        id: SpecimenId::ErrorStrip,
        group: "Feedback",
        title: "Inline error",
        symbol: "settings::widgets::error_strip",
        note: "",
    },
    SpecimenDef {
        id: SpecimenId::WarningStrip,
        group: "Feedback",
        title: "Inline warning",
        symbol: "settings::widgets::warning_strip",
        note: "",
    },
    SpecimenDef {
        id: SpecimenId::PullRequestStates,
        group: "Feedback",
        title: "Pull request states",
        symbol: "change_requests::pull_request_badge_preview",
        note: "open, merged and closed",
    },
    SpecimenDef {
        id: SpecimenId::FailureNotices,
        group: "Feedback",
        title: "Failure notices",
        symbol: "notice::notice_chip",
        note: "composer and transcript variants",
    },
    SpecimenDef {
        id: SpecimenId::LoadingGlyph,
        group: "Feedback",
        title: "Loading glyph",
        symbol: "loaders::gradient_spinner",
        note: "",
    },
    SpecimenDef {
        id: SpecimenId::UploadProgress,
        group: "Feedback",
        title: "Upload progress",
        symbol: "loaders::upload_progress_ring",
        note: "",
    },
    SpecimenDef {
        id: SpecimenId::MarkLoader,
        group: "Feedback",
        title: "Zeron mark loader",
        symbol: "loaders::zeron_mark_loader",
        note: "",
    },
    SpecimenDef {
        id: SpecimenId::Skeletons,
        group: "Feedback",
        title: "Skeleton states",
        symbol: "popover::skeleton_rows",
        note: "with skeleton_menu_rows",
    },
    SpecimenDef {
        id: SpecimenId::PopoverError,
        group: "Feedback",
        title: "Popover error",
        symbol: "popover::error_row",
        note: "",
    },
    SpecimenDef {
        id: SpecimenId::PopoverMenu,
        group: "Navigation",
        title: "Popover menu",
        symbol: "popover::popover_card",
        note: "with menu_heading and menu_row",
    },
    SpecimenDef {
        id: SpecimenId::PageHeading,
        group: "Navigation",
        title: "Page heading",
        symbol: "settings::widgets::page_header",
        note: "with page_subtitle",
    },
    SpecimenDef {
        id: SpecimenId::EmptySessions,
        group: "Navigation",
        title: "Empty sessions",
        symbol: "shell::empty_sessions_message",
        note: "",
    },
    SpecimenDef {
        id: SpecimenId::CommandPalette,
        group: "Navigation",
        title: "Command palette shell",
        symbol: "popover::palette_card",
        note: "with palette_search_icon and key_hint_pair",
    },
    SpecimenDef {
        id: SpecimenId::SettingsRow,
        group: "Collections",
        title: "Settings row",
        symbol: "settings::widgets::card_row",
        note: "in section_card, with row_tile and row_title",
    },
    SpecimenDef {
        id: SpecimenId::DiffHunk,
        group: "Collections",
        title: "Diff hunk",
        symbol: "changes::render_file_body_with_syntax",
        note: "from changes::parse_patch",
    },
    SpecimenDef {
        id: SpecimenId::ContextBadge,
        group: "Collections",
        title: "Message context badge",
        symbol: "badges::render",
        note: "hover for source detail",
    },
    SpecimenDef {
        id: SpecimenId::DialogSurface,
        group: "Composed",
        title: "Dialog surface",
        symbol: "popover::dialog_card",
        note: "with dialog_title and dialog_body",
    },
];

/// A block of a page. Sections are direct children of the scroll container so
/// the jump bar can scroll one to the top.
struct Section {
    title: SharedString,
    count: Option<String>,
    note: Option<SharedString>,
    trailing: Option<AnyElement>,
    body: AnyElement,
}

impl Section {
    fn new(title: impl Into<SharedString>, body: impl IntoElement) -> Self {
        Self {
            title: title.into(),
            count: None,
            note: None,
            trailing: None,
            body: body.into_any_element(),
        }
    }

    fn count(mut self, count: impl ToString) -> Self {
        self.count = Some(count.to_string());
        self
    }

    fn note(mut self, note: impl Into<SharedString>) -> Self {
        self.note = Some(note.into());
        self
    }

    fn trailing(mut self, trailing: impl IntoElement) -> Self {
        self.trailing = Some(trailing.into_any_element());
        self
    }
}

struct PageContent {
    lede: SharedString,
    banner: Option<AnyElement>,
    sections: Vec<Section>,
}

struct Workbench {
    page: Page,
    scroll: ScrollHandle,
    focus: FocusHandle,
    content_width: f32,
    appearance: Appearance,
    theme_selection: ThemeSelection,
    accent: AccentSelection,
    theme_select: w::SelectState,
    accent_select: w::SelectState,
    filter: Entity<ComposerInput>,
    _filter_sub: Subscription,
    copied: Option<SharedString>,
    _copied_reset: Option<Task<()>>,
    // Specimen state.
    switch_on: bool,
    checkbox_on: bool,
    option_compact: bool,
    select_state: w::SelectState,
    select_choice: usize,
    action_pressed: bool,
    split_on: bool,
    search: Entity<ComposerInput>,
    field: Entity<ComposerInput>,
    // Icons.
    current_icons: Vec<(String, String)>,
    proposed_names: HashMap<String, Vec<String>>,
    icon_size: f32,
    icons: Vec<IconRecord>,
    icon_category: Option<&'static str>,
    selected_icon: Option<String>,
    morphs: Vec<Morph>,
    morphed: Vec<bool>,
    show_all_morphs: bool,
    // Inventory.
    components: Vec<ComponentEntry>,
    audit: serde_json::Value,
    decisions: Vec<Decision>,
    metrics: serde_json::Value,
    pattern_filter: PatternFilter,
    show_modules: bool,
    selected_decision: Option<String>,
}

impl Workbench {
    fn new(page: Page, cx: &mut Context<Self>) -> Self {
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
        let filter = cx.new(|cx| {
            ComposerInput::new(page.filter_placeholder(), cx)
                .with_single_line()
                .with_accessibility_role(gpui::Role::SearchInput)
                .with_text_metrics(12.5, 18.0)
        });
        let filter_sub = cx.subscribe(&filter, |this, _, event, cx| {
            if matches!(event, ComposerInputEvent::Edited) {
                this.scroll.set_offset(point(px(0.0), px(0.0)));
                cx.notify();
            }
        });
        let search = cx.new(|cx| {
            ComposerInput::new("Search files", cx)
                .with_single_line()
                .with_accessibility_role(gpui::Role::SearchInput)
                .with_text_metrics(11.0, 16.0)
        });
        let field = cx.new(|cx| ComposerInput::new("Device name", cx));
        let selected_icon = icons.first().map(|icon| icon.name.clone());
        Self {
            page,
            scroll: ScrollHandle::new(),
            focus: cx.focus_handle(),
            content_width: 900.0,
            appearance,
            theme_selection: appearance::themes(cx),
            accent,
            theme_select: w::SelectState::default(),
            accent_select: w::SelectState::default(),
            filter,
            _filter_sub: filter_sub,
            copied: None,
            _copied_reset: None,
            switch_on: false,
            checkbox_on: false,
            option_compact: true,
            select_state: w::SelectState::default(),
            select_choice: 0,
            action_pressed: false,
            split_on: false,
            search,
            field,
            current_icons: current_icon_names(),
            proposed_names: proposed_icon_names(),
            icon_size: 20.0,
            icons,
            icon_category: None,
            selected_icon,
            morphs,
            morphed,
            show_all_morphs: false,
            components: serde_json::from_str(COMPONENTS).expect("component inventory"),
            audit: serde_json::from_str(AUDIT).expect("source audit"),
            decisions: serde_json::from_str(CONSOLIDATION).expect("consolidation scope"),
            metrics: serde_json::from_str(METRICS).expect("source-derived metrics"),
            pattern_filter: PatternFilter::All,
            show_modules: false,
            selected_decision: None,
        }
    }

    // ── State ────────────────────────────────────────────────────────────

    fn go(&mut self, page: Page, window: &mut Window, cx: &mut Context<Self>) {
        if self.page != page {
            self.page = page;
            self.scroll = ScrollHandle::new();
            self.filter.update(cx, |filter, cx| {
                filter.set_text("", cx);
                filter.set_placeholder(page.filter_placeholder(), cx);
            });
            window.set_window_title(&window_title(page));
        }
        cx.notify();
    }

    fn query(&self, cx: &Context<Self>) -> String {
        self.filter.read(cx).text().trim().to_lowercase()
    }

    fn clear_filter(&mut self, cx: &mut Context<Self>) {
        self.filter.update(cx, |filter, cx| filter.set_text("", cx));
        cx.notify();
    }

    fn copy(&mut self, value: SharedString, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(value.to_string()));
        self.copied = Some(value);
        self._copied_reset = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(COPIED_FOR).await;
            this.update(cx, |this, cx| {
                this.copied = None;
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    fn is_copied(&self, value: &str) -> bool {
        self.copied.as_deref() == Some(value)
    }

    fn variant_appearance(&self) -> zeron_theme::Appearance {
        match self.appearance {
            Appearance::Dark => zeron_theme::Appearance::Dark,
            Appearance::Light => zeron_theme::Appearance::Light,
        }
    }

    fn install_theme(&self, cx: &mut Context<Self>) {
        Theme::install_selection(
            self.appearance,
            self.theme_selection.variant_id(self.variant_appearance()),
            self.accent,
            appearance::surface(cx),
            cx,
        );
        appearance::reapply_window_background(cx);
        cx.refresh_windows();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let modifiers = &keystroke.modifiers;
        let secondary_only =
            modifiers.secondary() && !modifiers.shift && !modifiers.alt && !modifiers.function;
        if secondary_only {
            if let Some(page) = keystroke
                .key
                .parse::<usize>()
                .ok()
                .and_then(|digit| digit.checked_sub(1))
                .and_then(|index| Page::ALL.get(index).copied())
            {
                self.go(page, window, cx);
                cx.stop_propagation();
                return;
            }
            if keystroke.key == "f" {
                window.focus(&self.filter.focus_handle(cx), cx);
                cx.stop_propagation();
                return;
            }
        }
        if keystroke.key == "escape" && !self.query(cx).is_empty() {
            self.clear_filter(cx);
            cx.stop_propagation();
        }
    }

    // ── Shared pieces ────────────────────────────────────────────────────

    /// Columns for a grid of cards at least `min` wide in the current page.
    fn columns(&self, min: f32) -> u16 {
        (((self.content_width + GRID_GAP) / (min + GRID_GAP)).floor() as u16).max(1)
    }

    fn grid(&self, min: f32) -> gpui::Div {
        div()
            .w_full()
            .grid()
            .grid_cols(self.columns(min))
            .gap(px(GRID_GAP))
    }

    /// Small copy control showing `label` in the code face. Activating it
    /// copies `value` and swaps the glyph for a check while it is on the
    /// clipboard.
    fn copy_chip(
        &self,
        t: &Theme,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        value: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let value: SharedString = value.into();
        let copied = self.is_copied(&value);
        let tooltip = format!("Copy {value}");
        focusable(
            t,
            div()
                .id(id.into())
                .max_w_full()
                .min_w_0()
                .h(px(24.0))
                .px(px(6.0))
                .rounded(px(6.0))
                .flex()
                .flex_none()
                .items_center()
                .gap(px(6.0))
                .cursor_pointer()
                .text_color(if copied { t.success } else { t.text_muted })
                .hover(|s| s.bg(t.glass_hover()).text_color(t.text))
                .role(gpui::Role::Button)
                .aria_label(tooltip.clone())
                .tooltip(w::text_tooltip(if copied {
                    "Copied".to_string()
                } else {
                    tooltip
                }))
                .on_click(cx.listener({
                    let value = value.clone();
                    move |this, _, _, cx| this.copy(value.clone(), cx)
                })),
        )
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .font_family(t.font_mono.clone())
                .text_size(typography::ui_rems(11.5))
                .child(label.into()),
        )
        .child(
            icons::icon(if copied { icons::CHECK } else { icons::COPY })
                .flex_none()
                .size(px(12.0)),
        )
    }

    /// A quiet segmented control; `pick` receives the chosen value.
    fn segmented<T: Copy + PartialEq + 'static>(
        &self,
        t: &Theme,
        id: &str,
        label: &str,
        options: Vec<(T, SharedString)>,
        selected: T,
        pick: fn(&mut Self, T, &mut Window, &mut Context<Self>),
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let mut track = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(2.0))
            .p(px(2.0))
            .rounded(px(8.0))
            .bg(t.wash(0.05))
            .id(SharedString::from(id.to_string()))
            .role(gpui::Role::RadioGroup)
            .aria_label(label.to_string());
        for (index, (value, text)) in options.into_iter().enumerate() {
            let on = value == selected;
            track = track.child(
                focusable(
                    t,
                    div()
                        .id(SharedString::from(format!("{id}-{index}")))
                        .h(px(26.0))
                        .px(px(10.0))
                        .rounded(px(6.0))
                        .flex()
                        .items_center()
                        .cursor_pointer()
                        .text_size(typography::ui_rems(12.0))
                        .text_color(if on { t.text } else { t.text_muted })
                        .when(on, |chip| {
                            chip.bg(theme::card_selected_bg())
                                .shadow(theme::card_selected_shadows())
                                .font_weight(gpui::FontWeight::MEDIUM)
                        })
                        .when(!on, |chip| chip.hover(|s| s.text_color(t.text)))
                        .role(gpui::Role::RadioButton)
                        .aria_label(text.clone())
                        .aria_selected(on)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            pick(this, value, window, cx);
                            cx.notify();
                        })),
                )
                .child(text),
            );
        }
        track
    }

    /// Empty state for a filter with no matches, with a way out.
    fn no_matches(&self, t: &Theme, query: &str, cx: &mut Context<Self>) -> gpui::Div {
        panel(t)
            .p(px(24.0))
            .flex()
            .flex_col()
            .items_start()
            .gap(px(12.0))
            .child(w::row_title(t, format!("Nothing matches “{query}”")))
            .child(body_text(
                t,
                "The filter checks names, builders, source paths and notes on this page.",
            ))
            .child(focusable(
                t,
                w::text_action(t, w::ActionTone::Outlined, "Clear filter")
                    .id("workbench-clear-empty")
                    .role(gpui::Role::Button)
                    .on_click(cx.listener(|this, _, _, cx| this.clear_filter(cx))),
            ))
    }

    // ── Chrome ───────────────────────────────────────────────────────────

    fn page_count(&self, page: Page) -> Option<usize> {
        match page {
            Page::Overview | Page::Foundations => None,
            Page::Components => Some(SPECIMENS.len()),
            Page::CurrentIcons => Some(self.current_icons.len()),
            Page::Icons => Some(self.icons.len()),
            Page::Coverage => Some(self.components.len()),
        }
    }

    fn navigation(
        &self,
        t: &Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let mut nav = div()
            .w(px(NAV_WIDTH))
            .h_full()
            .flex_none()
            .bg(t.surface)
            .border_r_1()
            .border_color(t.border)
            .flex()
            .flex_col()
            .id("workbench-navigation")
            .role(gpui::Role::Navigation)
            .aria_label("Workbench pages")
            .child(
                div()
                    .h(px(TOPBAR_HEIGHT))
                    .px(px(20.0))
                    .flex()
                    .flex_col()
                    .justify_center()
                    .child(
                        div()
                            .text_size(typography::ui_rems(14.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child("Zeron design system"),
                    )
                    .child(caption(t, "Native UI workbench")),
            );
        for (experiment, label) in [(false, "Current app"), (true, "Target studies")] {
            let mut group = div()
                .px(px(Theme::SPACE_SM))
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .px(px(Theme::SPACE_SM))
                        .pb(px(6.0))
                        .text_size(typography::ui_rems(11.0))
                        .text_color(t.text_faint)
                        .child(label),
                );
            for page in Page::ALL
                .into_iter()
                .filter(|page| page.is_experiment() == experiment)
            {
                let selected = self.page == page;
                let key = format!("workbench-nav-{}", page.slug());
                let hover_key = format!("{key}-hover");
                let selection_t = w::tab_selection_t(
                    window,
                    format!("{key}-selection"),
                    selected,
                    motion::reduced_motion(cx),
                );
                group = group.child(
                    focusable(
                        t,
                        w::section_tab(t, selected, selection_t, key, hover_key)
                            .role(gpui::Role::Tab)
                            .aria_label(page.label())
                            .aria_selected(selected)
                            .cursor_pointer()
                            .on_click(
                                cx.listener(move |this, _, window, cx| this.go(page, window, cx)),
                            ),
                    )
                    .child(
                        icons::icon(page.icon())
                            .flex_none()
                            .size(px(16.0))
                            .text_color(if selected { t.text } else { t.text_muted }),
                    )
                    .child(div().flex_1().min_w_0().child(page.label()))
                    .when_some(self.page_count(page), |row, count| {
                        row.child(
                            div()
                                .text_size(typography::ui_rems(11.0))
                                .text_color(t.text_faint)
                                .child(count.to_string()),
                        )
                    }),
                );
            }
            nav = nav.child(group.mt(px(if experiment { 24.0 } else { 12.0 })));
        }
        let fingerprint = self.audit["sourceFingerprint"].as_str().unwrap_or("");
        nav.child(
            div()
                .mt_auto()
                .px(px(20.0))
                .pb(px(20.0))
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(shortcut_row(t, &shortcut_label("1–6"), "Switch page"))
                .child(shortcut_row(t, &shortcut_label("F"), "Filter this page"))
                .child(shortcut_row(t, "Esc", "Clear the filter"))
                .child(
                    div()
                        .pt(px(10.0))
                        .text_size(typography::ui_rems(11.0))
                        .text_color(t.text_faint)
                        .child(format!("Audit {fingerprint}")),
                ),
        )
    }

    fn topbar(&self, t: &Theme, query: &str, cx: &mut Context<Self>) -> gpui::Div {
        let variant_appearance = self.variant_appearance();
        let variants: Vec<(String, String)> = ThemeRegistry::active()
            .variants_for(variant_appearance)
            .map(|variant| (variant.id.clone(), variant.name.clone()))
            .collect();
        let selected_variant = self.theme_selection.variant_id(variant_appearance);
        let variant_index = variants
            .iter()
            .position(|(id, _)| id == selected_variant)
            .unwrap_or(0);
        let variant_ids: Vec<String> = variants.iter().map(|(id, _)| id.clone()).collect();
        let theme_select = w::select(
            "workbench-theme-select",
            "Theme",
            t,
            |this: &mut Workbench| &mut this.theme_select,
        )
        .options(
            variants
                .iter()
                .map(|(_, name)| w::SelectOption::new(name.clone())),
            variant_index,
        )
        .width(168.0)
        .on_select(move |this, choice, _, cx| {
            if let Some(id) = variant_ids.get(choice) {
                let appearance = this.variant_appearance();
                this.theme_selection.set_variant(appearance, id.clone());
                this.install_theme(cx);
            }
        })
        .render(&self.theme_select, cx);

        let accents: Vec<AccentSelection> = std::iter::once(AccentSelection::ThemeDefault)
            .chain(AccentPreset::ALL.into_iter().map(AccentSelection::Preset))
            .collect();
        let accent_index = accents
            .iter()
            .position(|accent| *accent == self.accent)
            .unwrap_or(0);
        let theme_accent = t.accent;
        let accent_options = accents.iter().map(|accent| {
            let color: Hsla = match accent {
                AccentSelection::ThemeDefault => theme_accent,
                AccentSelection::Preset(preset) => {
                    let c = preset.color(variant_appearance);
                    Rgba {
                        r: c.r as f32 / 255.0,
                        g: c.g as f32 / 255.0,
                        b: c.b as f32 / 255.0,
                        a: 1.0,
                    }
                    .into()
                }
            };
            w::SelectOption::new(accent.label()).leading(move || {
                div()
                    .size(px(10.0))
                    .rounded_full()
                    .bg(color)
                    .into_any_element()
            })
        });
        let accent_select = w::select(
            "workbench-accent-select",
            "Accent",
            t,
            |this: &mut Workbench| &mut this.accent_select,
        )
        .options(accent_options, accent_index)
        .width(148.0)
        .on_select(move |this, choice, _, cx| {
            if let Some(accent) = accents.get(choice) {
                this.accent = *accent;
                this.install_theme(cx);
            }
        })
        .render(&self.accent_select, cx);

        let appearance_switch = self.segmented(
            t,
            "workbench-appearance",
            "Appearance",
            vec![
                (Appearance::Dark, "Dark".into()),
                (Appearance::Light, "Light".into()),
            ],
            self.appearance,
            |this, appearance, _, cx| {
                if this.appearance != appearance {
                    this.appearance = appearance;
                    this.install_theme(cx);
                }
            },
            cx,
        );

        let filter_focus = self.filter.focus_handle(cx);
        let filter = div()
            .id("workbench-filter")
            .flex_1()
            .min_w(px(160.0))
            .max_w(px(320.0))
            .h(px(32.0))
            .pl(px(10.0))
            .pr(px(4.0))
            .rounded(px(8.0))
            .bg(t.input_glass_bg())
            .border_1()
            .border_color(t.border)
            .flex()
            .items_center()
            .gap(px(8.0))
            .cursor_text()
            .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                window.focus(&filter_focus, cx);
                cx.stop_propagation();
            })
            .child(
                icons::icon(icons::MAGNIFER)
                    .flex_none()
                    .size(px(14.0))
                    .text_color(t.text_faint),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .child(self.filter.clone()),
            )
            .map(|field| {
                if query.is_empty() {
                    field.child(key_hint(t, &shortcut_label("F")))
                } else {
                    field.child(focusable(
                        t,
                        div()
                            .id("workbench-filter-clear")
                            .size(px(24.0))
                            .flex_none()
                            .rounded(px(6.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .hover(|s| s.bg(t.glass_hover()))
                            .role(gpui::Role::Button)
                            .aria_label("Clear filter")
                            .tooltip(w::text_tooltip("Clear filter (Esc)"))
                            .on_click(cx.listener(|this, _, _, cx| this.clear_filter(cx)))
                            .child(
                                icons::icon(icons::CLOSE)
                                    .size(px(12.0))
                                    .text_color(t.text_muted),
                            ),
                    ))
                }
            });

        div()
            .h(px(TOPBAR_HEIGHT))
            .w_full()
            .flex_none()
            .px(px(20.0))
            .flex()
            .items_center()
            .gap(px(12.0))
            .border_b_1()
            .border_color(t.border)
            .bg(t.surface)
            .child(filter)
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(8.0))
                    .child(theme_select)
                    .child(accent_select)
                    .child(appearance_switch),
            )
    }

    /// Header, jump bar and sections of a page, each a direct child of the
    /// scroll container.
    fn page_blocks(
        &self,
        t: &Theme,
        content: PageContent,
        query: &str,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let page = self.page;
        let mut header = div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .pt(px(32.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_size(typography::ui_rems(12.0))
                    .text_color(t.text_muted)
                    .child(if page.is_experiment() {
                        "Target study"
                    } else {
                        "Current app"
                    })
                    .when(page.is_experiment(), |line| {
                        line.child(
                            div()
                                .px(px(6.0))
                                .rounded(px(4.0))
                                .bg(t.accent_wash)
                                .text_color(t.accent)
                                .text_size(typography::ui_rems(11.0))
                                .child("Not shipped"),
                        )
                    }),
            )
            .child(
                div()
                    .text_size(typography::ui_rems(24.0))
                    .line_height(typography::ui_rems(30.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child(page.label()),
            )
            .child(
                div()
                    .max_w(px(680.0))
                    .text_size(typography::ui_rems(13.0))
                    .line_height(typography::ui_rems(19.0))
                    .text_color(t.text_muted)
                    .child(content.lede),
            );
        if let Some(banner) = content.banner {
            header = header.child(div().pt(px(8.0)).child(banner));
        }
        let mut blocks = vec![header.into_any_element()];

        let titles: Vec<SharedString> = content
            .sections
            .iter()
            .map(|section| section.title.clone())
            .collect();
        let jump = if titles.len() >= 3 {
            let mut bar = div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(6.0))
                .pt(px(20.0))
                .id("workbench-jump")
                .role(gpui::Role::Navigation)
                .aria_label("Sections on this page")
                .child(
                    div()
                        .pr(px(4.0))
                        .text_size(typography::ui_rems(12.0))
                        .text_color(t.text_faint)
                        .child("Jump to"),
                );
            for (index, title) in titles.iter().enumerate() {
                bar = bar.child(
                    focusable(
                        t,
                        div()
                            .id(SharedString::from(format!("workbench-jump-{index}")))
                            .h(px(26.0))
                            .px(px(10.0))
                            .rounded(px(13.0))
                            .flex()
                            .items_center()
                            .bg(t.wash(0.05))
                            .text_size(typography::ui_rems(12.0))
                            .text_color(t.text_muted)
                            .cursor_pointer()
                            .hover(|s| s.bg(t.glass_hover()).text_color(t.text))
                            .role(gpui::Role::Link)
                            .aria_label(format!("Jump to {title}"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.scroll.scroll_to_top_of_item(index + 2);
                                cx.notify();
                            })),
                    )
                    .child(title.clone()),
                );
            }
            bar.into_any_element()
        } else {
            div().into_any_element()
        };
        blocks.push(jump);

        if content.sections.is_empty() && !query.is_empty() {
            blocks.push(
                div()
                    .pt(px(SECTION_GAP))
                    .child(self.no_matches(t, query, cx))
                    .into_any_element(),
            );
        }
        for section in content.sections {
            let mut heading = div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(16.0))
                .min_h(px(30.0))
                .child(
                    div()
                        .flex()
                        .items_baseline()
                        .gap(px(8.0))
                        .child(
                            div()
                                .text_size(typography::ui_rems(16.0))
                                .line_height(typography::ui_rems(22.0))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .child(section.title),
                        )
                        .when_some(section.count, |row, count| {
                            row.child(
                                div()
                                    .text_size(typography::ui_rems(13.0))
                                    .text_color(t.text_faint)
                                    .child(count),
                            )
                        }),
                );
            if let Some(trailing) = section.trailing {
                heading = heading.child(trailing);
            }
            blocks.push(
                div()
                    .pt(px(SECTION_GAP))
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .child(heading)
                    .when_some(section.note, |block, note| {
                        block.child(div().max_w(px(680.0)).child(body_text(t, note)))
                    })
                    .child(div().pt(px(4.0)).child(section.body))
                    .into_any_element(),
            );
        }
        blocks.push(div().h(px(64.0)).into_any_element());
        blocks
    }

    // ── Overview ─────────────────────────────────────────────────────────

    fn overview(&self, t: &Theme, query: &str, cx: &mut Context<Self>) -> PageContent {
        let mounted = self
            .components
            .iter()
            .filter(|entry| entry.workbench == "mounted")
            .count();
        let files = self.audit["sourceFiles"].as_u64().unwrap_or(0);
        let unreviewed = self.audit["unreviewedSourceFiles"].as_u64().unwrap_or(0);
        let mut sections = Vec::new();

        if query.is_empty() {
            let mut stats = self.grid(200.0);
            for (index, (page, value, label, detail)) in [
                (
                    Page::Components,
                    SPECIMENS.len().to_string(),
                    "Live specimens",
                    format!("{mounted} of {} patterns mounted", self.components.len()),
                ),
                (
                    Page::Coverage,
                    format!("{}", files - unreviewed),
                    "Modules classified",
                    format!("of {files} scanned · {unreviewed} unreviewed"),
                ),
                (
                    Page::CurrentIcons,
                    self.current_icons.len().to_string(),
                    "Current icons",
                    "Production control glyphs".to_string(),
                ),
                (
                    Page::Icons,
                    self.icons.len().to_string(),
                    "Proposed glyphs",
                    format!("{} reversible transitions", self.morphs.len()),
                ),
            ]
            .into_iter()
            .enumerate()
            {
                stats = stats.child(
                    focusable(
                        t,
                        panel(t)
                            .id(SharedString::from(format!("workbench-stat-{index}")))
                            .p(px(16.0))
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .cursor_pointer()
                            .hover(|s| s.bg(t.wash(0.07)))
                            .role(gpui::Role::Link)
                            .aria_label(format!("{label}: {value}. Open {}", page.label()))
                            .on_click(
                                cx.listener(move |this, _, window, cx| this.go(page, window, cx)),
                            ),
                    )
                    .child(
                        div()
                            .text_size(typography::ui_rems(26.0))
                            .line_height(typography::ui_rems(32.0))
                            .child(value),
                    )
                    .child(w::row_title(t, label))
                    .child(caption(t, detail)),
                );
            }
            sections.push(Section::new("At a glance", stats));
        }

        let mut pages = w::section_card(t).mt(px(0.0));
        let mut shown = 0;
        for page in Page::ALL.into_iter().filter(|page| *page != Page::Overview) {
            if !matches(query, &[page.label(), page.summary(), page.slug()]) {
                continue;
            }
            pages = pages.child(
                focusable(
                    t,
                    w::card_row(t, shown == 0)
                        .id(SharedString::from(format!(
                            "workbench-destination-{}",
                            page.slug()
                        )))
                        .flex_nowrap()
                        .cursor_pointer()
                        .hover(|row| row.bg(t.wash(0.03)))
                        .role(gpui::Role::Link)
                        .aria_label(format!("Open {}", page.label()))
                        .on_click(
                            cx.listener(move |this, _, window, cx| this.go(page, window, cx)),
                        ),
                )
                .child(w::row_tile(t, page.icon()))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(w::row_title(t, page.label()))
                                .when(page.is_experiment(), |row| {
                                    row.child(
                                        div()
                                            .text_size(typography::ui_rems(11.0))
                                            .text_color(t.accent)
                                            .child("Target study"),
                                    )
                                }),
                        )
                        .child(body_text(t, page.summary())),
                )
                .child(key_hint(t, &page.shortcut()))
                .child(
                    icons::icon(icons::ALT_ARROW_RIGHT)
                        .flex_none()
                        .size(px(16.0))
                        .text_color(t.text_faint),
                ),
            );
            shown += 1;
        }
        if shown > 0 {
            sections.push(Section::new("Pages", pages));
        }

        let steps: [(&str, &str, &str); 5] = [
            (
                "Open a page directly",
                "Page names: overview, foundations, components, icons, atelier, inventory. Add --appearance light or dark to check both.",
                "scripts/run-macos-dev.sh ui-workbench components --appearance light",
            ),
            (
                "Mount a production builder",
                "Add a SpecimenDef and its stage in crates/ui/src/ui_workbench.rs, then mark the pattern \"mounted\" in components.json. Call the real builder; never redraw a lookalike.",
                "crates/ui/src/ui_workbench.rs",
            ),
            (
                "Classify a new UI module",
                "The audit fails until every desktop UI module is a pattern in components.json or a support module in support.json.",
                "python3 apps/ui-workbench/audit.py --check",
            ),
            (
                "Refresh foundations metrics",
                "Regenerates metrics.json from the layout and motion constants in production source. Drop --check to write.",
                "python3 apps/ui-workbench/extract.py --check",
            ),
            (
                "Run without the bundle",
                "Any platform. The engine does not start and no workspace opens; saved appearance is read but never written.",
                "cargo run -p zeron --features ui-workbench -- ui-workbench",
            ),
        ];
        let mut workflow = w::section_card(t).mt(px(0.0));
        let mut shown = 0;
        for (index, (title, detail, command)) in steps.into_iter().enumerate() {
            if !matches(query, &[title, detail, command]) {
                continue;
            }
            workflow = workflow.child(
                w::card_row(t, shown == 0)
                    .flex_col()
                    .items_start()
                    .gap(px(6.0))
                    .child(w::row_title(t, title))
                    .child(body_text(t, detail))
                    .child(div().max_w_full().rounded(px(6.0)).bg(t.wash(0.04)).child(
                        self.copy_chip(
                            t,
                            format!("workbench-command-{index}"),
                            command,
                            command,
                            cx,
                        ),
                    )),
            );
            shown += 1;
        }
        if shown > 0 {
            sections.push(
                Section::new("Contributor workflow", workflow)
                    .note("Everything here reads production source. Copy a command, run it from the repository root."),
            );
        }

        PageContent {
            lede: "A native reference for Zeron's shipped interface. Specimens call production GPUI builders, the inventory names what still needs a fixture or consolidation, and the atelier isolates proposals from the app.".into(),
            banner: None,
            sections,
        }
    }

    // ── Foundations ──────────────────────────────────────────────────────

    fn foundations(&self, t: &Theme, query: &str, cx: &mut Context<Self>) -> PageContent {
        let mut sections = Vec::new();
        let groups = color_groups(t);
        let mut colors = div().flex().flex_col().gap(px(24.0));
        let mut color_count = 0;
        for (group, roles) in groups {
            let roles: Vec<_> = roles
                .into_iter()
                .filter(|(name, _)| matches(query, &[name, group]))
                .collect();
            if roles.is_empty() {
                continue;
            }
            let text_group = group == "Text";
            let mut grid = self.grid(168.0);
            for (name, value) in &roles {
                let token = format!("theme.{name}");
                let copied = self.is_copied(&token);
                let (hex, alpha) = hex(*value);
                let ratio = contrast(*value, t.bg);
                let detail = if text_group {
                    format!(
                        "{hex} · {ratio:.1}:1 {}",
                        if ratio >= 4.5 {
                            "AA"
                        } else if ratio >= 3.0 {
                            "AA large"
                        } else {
                            "below AA"
                        }
                    )
                } else if alpha < 1.0 {
                    format!("{hex} · {:.0}%", alpha * 100.0)
                } else {
                    hex
                };
                grid = grid.child(
                    focusable(
                        t,
                        panel(t)
                            .id(SharedString::from(format!("workbench-token-{name}")))
                            .p(px(8.0))
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .cursor_pointer()
                            .hover(|s| s.bg(t.wash(0.07)))
                            .role(gpui::Role::Button)
                            .aria_label(format!("Copy {token}"))
                            .tooltip(w::text_tooltip(format!("Copy {token}")))
                            .on_click(cx.listener({
                                let token = SharedString::from(token.clone());
                                move |this, _, _, cx| this.copy(token.clone(), cx)
                            })),
                    )
                    .child(
                        div()
                            .h(px(48.0))
                            .rounded(px(INSET_RADIUS))
                            .bg(t.bg)
                            .shadow(outline_ring(t))
                            .child(div().size_full().rounded(px(INSET_RADIUS)).bg(*value).when(
                                text_group,
                                |swatch| {
                                    swatch
                                        .bg(t.bg)
                                        .flex()
                                        .items_center()
                                        .px(px(10.0))
                                        .text_size(typography::ui_rems(15.0))
                                        .text_color(*value)
                                        .child("Aa · The shape of Zeron")
                                },
                            )),
                    )
                    .child(
                        div()
                            .px(px(4.0))
                            .pb(px(2.0))
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .font_family(t.font_mono.clone())
                                    .text_size(typography::ui_rems(12.0))
                                    .text_color(if copied { t.success } else { t.text })
                                    .child(name.to_string())
                                    .when(copied, |row| {
                                        row.child(icons::icon(icons::CHECK).size(px(12.0)))
                                    }),
                            )
                            .child(caption(t, detail)),
                    ),
                );
            }
            color_count += roles.len();
            colors = colors.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .child(subheading(t, group, roles.len()))
                    .when(text_group, |block| {
                        block.child(caption(
                            t,
                            "Contrast is measured against theme.bg. Body text needs 4.5:1; large or bold text needs 3:1.",
                        ))
                    })
                    .child(grid),
            );
        }
        if color_count > 0 {
            sections.push(
                Section::new("Color roles", colors)
                    .count(color_count)
                    .note(format!(
                        "The effective theme: variant {} in family {}, accent {}. Change them from the top bar; select a swatch to copy its token.",
                        t.variant_id, t.family_id, self.accent.label()
                    )),
            );
        }

        let type_roles: [(&str, &str, f32, f32, bool, bool, &str); 8] = [
            (
                "Page title",
                "settings::widgets::page_header",
                20.0,
                26.0,
                true,
                false,
                "Appearance",
            ),
            (
                "Page subtitle",
                "settings::widgets::page_subtitle",
                13.0,
                17.0,
                false,
                true,
                "Choose how Zeron looks on this device.",
            ),
            (
                "Section label",
                "settings::widgets::section_label",
                13.0,
                17.0,
                false,
                true,
                "Startup",
            ),
            (
                "Row title",
                "settings::widgets::row_title",
                w::ROW_TITLE_SIZE,
                17.0,
                true,
                false,
                "Notifications",
            ),
            (
                "Row description",
                "settings::widgets::meta_line",
                w::ROW_DESCRIPTION_SIZE,
                16.0,
                false,
                true,
                "Shown when an agent needs your attention",
            ),
            (
                "Action",
                "settings::widgets::action_button",
                12.5,
                16.0,
                false,
                false,
                "Save changes",
            ),
            (
                "Badge",
                "settings::widgets::badge",
                10.5,
                14.0,
                false,
                true,
                "Default",
            ),
            (
                "Tooltip",
                "settings::widgets::TextTooltip",
                11.0,
                15.0,
                false,
                true,
                "Copies the path to the clipboard",
            ),
        ];
        let mut type_card = w::section_card(t).mt(px(0.0));
        let mut shown = 0;
        for (index, (role, builder, size, line, medium, muted, sample)) in
            type_roles.into_iter().enumerate()
        {
            if !matches(query, &[role, builder]) {
                continue;
            }
            type_card = type_card.child(
                w::card_row(t, shown == 0)
                    .flex_nowrap()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(typography::ui_rems(size))
                            .line_height(typography::ui_rems(line))
                            .when(medium, |s| s.font_weight(gpui::FontWeight::MEDIUM))
                            .text_color(if muted { t.text_muted } else { t.text })
                            .child(sample),
                    )
                    .child(
                        div()
                            .w(px(260.0))
                            .flex_none()
                            .flex()
                            .flex_col()
                            .items_end()
                            .gap(px(2.0))
                            .child(caption(
                                t,
                                format!(
                                    "{role} · {size} / {line}{}",
                                    if medium { " · medium" } else { "" }
                                ),
                            ))
                            .child(self.copy_chip(
                                t,
                                format!("workbench-type-{index}"),
                                builder,
                                builder,
                                cx,
                            )),
                    ),
            );
            shown += 1;
        }
        let faces = [
            ("Interface", t.font_sans.clone(), "font_sans"),
            (
                "Fixed-width chrome",
                t.font_sans_fixed.clone(),
                "font_sans_fixed",
            ),
            ("Code", t.font_mono.clone(), "font_mono"),
            ("Terminal", t.font_terminal.clone(), "font_terminal"),
        ];
        for (label, family, token) in faces {
            if !matches(query, &[label, token, &family]) {
                continue;
            }
            type_card = type_card.child(
                w::card_row(t, shown == 0)
                    .flex_nowrap()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_family(family.clone())
                            .text_size(typography::ui_rems(15.0))
                            .child(format!("{label} — {family}")),
                    )
                    .child(self.copy_chip(
                        t,
                        format!("workbench-face-{token}"),
                        format!("theme.{token}"),
                        format!("theme.{token}"),
                        cx,
                    )),
            );
            shown += 1;
        }
        if shown > 0 {
            sections.push(
                Section::new("Typography", type_card)
                    .count(shown)
                    .note("Roles rendered at their production size and weight through typography::ui_rems, so they follow the user's interface font size."),
            );
        }

        let spacing = [
            ("Theme::SPACE_XS", Theme::SPACE_XS),
            ("Theme::SPACE_SM", Theme::SPACE_SM),
            ("Theme::SPACE_MD", Theme::SPACE_MD),
            ("Theme::SPACE_LG", Theme::SPACE_LG),
        ];
        let radii = [
            ("Theme::CONTROL_RADIUS", Theme::CONTROL_RADIUS),
            ("w::OPTION_CARD_RADIUS", w::OPTION_CARD_RADIUS),
            ("Theme::PANEL_RADIUS", Theme::PANEL_RADIUS),
            ("section_card", 12.0),
            ("Theme::BUBBLE_RADIUS", Theme::BUBBLE_RADIUS),
        ];
        let spacing: Vec<_> = spacing
            .into_iter()
            .filter(|(name, _)| matches(query, &[name, "spacing"]))
            .collect();
        let radii: Vec<_> = radii
            .into_iter()
            .filter(|(name, _)| matches(query, &[name, "radius"]))
            .collect();
        if !spacing.is_empty() || !radii.is_empty() {
            let mut space_card = w::section_card(t).mt(px(0.0));
            for (index, (name, value)) in spacing.iter().enumerate() {
                space_card = space_card.child(
                    w::card_row(t, index == 0)
                        .flex_nowrap()
                        .child(
                            div()
                                .w(px(44.0))
                                .flex_none()
                                .text_size(typography::ui_rems(12.0))
                                .text_color(t.text_muted)
                                .child(format!("{value} px")),
                        )
                        .child(
                            div().flex_1().child(
                                div()
                                    .w(px(*value * 4.0))
                                    .h(px(12.0))
                                    .rounded(px(3.0))
                                    .bg(t.accent),
                            ),
                        )
                        .child(self.copy_chip(
                            t,
                            format!("workbench-space-{index}"),
                            *name,
                            *name,
                            cx,
                        )),
                );
            }
            let mut radius_grid = self.grid(150.0);
            for (index, (name, value)) in radii.iter().enumerate() {
                radius_grid = radius_grid.child(
                    panel(t)
                        .p(px(12.0))
                        .flex()
                        .flex_col()
                        .gap(px(10.0))
                        .child(
                            div()
                                .h(px(56.0))
                                .rounded(px(*value))
                                .border_1()
                                .border_color(t.border_strong)
                                .bg(t.surface_card),
                        )
                        .child(caption(t, format!("{value} px")))
                        .child(self.copy_chip(
                            t,
                            format!("workbench-radius-{index}"),
                            *name,
                            *name,
                            cx,
                        )),
                );
            }
            sections.push(
                Section::new(
                    "Spacing and radius",
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(GRID_GAP))
                        .when(!spacing.is_empty(), |col| col.child(space_card))
                        .when(!radii.is_empty(), |col| col.child(radius_grid)),
                )
                .note("Nested surfaces stay concentric: outer radius equals inner radius plus the inset between them."),
            );
        }

        if let Some(groups) = self.metrics["metrics"].as_object() {
            let mut body = div().flex().flex_col().gap(px(20.0));
            let mut total = 0;
            for (group, rows) in groups {
                let rows: Vec<&serde_json::Value> = rows
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|row| {
                        matches(
                            query,
                            &[
                                row["name"].as_str().unwrap_or(""),
                                row["source"].as_str().unwrap_or(""),
                                group,
                            ],
                        )
                    })
                    .collect();
                if rows.is_empty() {
                    continue;
                }
                let max = rows
                    .iter()
                    .filter_map(|row| row["value"].as_f64())
                    .fold(1.0_f64, f64::max);
                let mut card = w::section_card(t).mt(px(0.0));
                for (index, row) in rows.iter().enumerate() {
                    let name = row["name"].as_str().unwrap_or("metric");
                    let value = row["value"].as_f64().unwrap_or(0.0);
                    let source = row["source"].as_str().unwrap_or("");
                    card = card.child(
                        w::card_row(t, index == 0)
                            .min_h(px(44.0))
                            .py(px(8.0))
                            .flex_nowrap()
                            .child(
                                div()
                                    .w(px(220.0))
                                    .flex_none()
                                    .font_family(t.font_mono.clone())
                                    .text_size(typography::ui_rems(12.0))
                                    .child(name.to_string()),
                            )
                            .child(
                                div()
                                    .w(px(64.0))
                                    .flex_none()
                                    .text_size(typography::ui_rems(12.0))
                                    .text_color(t.text_muted)
                                    .child(format!("{value} px")),
                            )
                            .child(
                                div().flex_1().min_w(px(40.0)).child(
                                    div()
                                        .w(gpui::relative((value / max) as f32))
                                        .h(px(6.0))
                                        .rounded(px(3.0))
                                        .bg(t.accent.opacity(0.6)),
                                ),
                            )
                            .child(self.copy_chip(
                                t,
                                format!("workbench-metric-{group}-{name}"),
                                source.rsplit('/').next().unwrap_or(source).to_string(),
                                source.to_string(),
                                cx,
                            )),
                    );
                }
                total += rows.len();
                body = body.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(10.0))
                        .child(subheading(t, group, rows.len()))
                        .child(card),
                );
            }
            if total > 0 {
                sections.push(
                    Section::new("Layout metrics", body)
                        .count(total)
                        .note("Generated into metrics.json from production constants. Each row links to its declaration; bars compare values within a group."),
                );
            }
        }

        if let Some(specs) = self.metrics["motion"].as_array() {
            let specs: Vec<&serde_json::Value> = specs
                .iter()
                .filter(|spec| {
                    matches(
                        query,
                        &[
                            spec["name"].as_str().unwrap_or(""),
                            spec["curve"].as_str().unwrap_or(""),
                            "motion",
                        ],
                    )
                })
                .collect();
            if !specs.is_empty() {
                let longest = specs
                    .iter()
                    .filter_map(|spec| spec["duration"].as_u64())
                    .max()
                    .unwrap_or(1)
                    .max(1);
                let mut card = w::section_card(t).mt(px(0.0));
                for (index, spec) in specs.iter().enumerate() {
                    let name = spec["name"].as_str().unwrap_or("motion");
                    let duration = spec["duration"].as_u64().unwrap_or(0);
                    let curve = spec["curve"].as_str().unwrap_or("");
                    let source = spec["source"].as_str().unwrap_or("");
                    card = card.child(
                        w::card_row(t, index == 0)
                            .min_h(px(44.0))
                            .py(px(8.0))
                            .flex_nowrap()
                            .child(
                                div()
                                    .w(px(220.0))
                                    .flex_none()
                                    .font_family(t.font_mono.clone())
                                    .text_size(typography::ui_rems(12.0))
                                    .child(name.to_string()),
                            )
                            .child(
                                div()
                                    .w(px(160.0))
                                    .flex_none()
                                    .text_size(typography::ui_rems(12.0))
                                    .text_color(t.text_muted)
                                    .child(format!("{duration} ms · {curve}")),
                            )
                            .child(
                                div().flex_1().min_w(px(40.0)).child(
                                    div()
                                        .w(gpui::relative(duration as f32 / longest as f32))
                                        .h(px(6.0))
                                        .rounded(px(3.0))
                                        .bg(t.accent.opacity(0.6)),
                                ),
                            )
                            .child(self.copy_chip(
                                t,
                                format!("workbench-motion-{name}"),
                                source.rsplit('/').next().unwrap_or(source).to_string(),
                                source.to_string(),
                                cx,
                            )),
                    );
                }
                sections.push(
                    Section::new("Motion timing", card)
                        .count(specs.len())
                        .note("Durations and curves from motion.rs. Bars compare durations; the app honors the system's reduced-motion setting."),
                );
            }
        }

        PageContent {
            lede: "The effective GPUI theme, including installed custom variants, with the type roles, spacing, radii and timing that production code uses. Select any value to copy it.".into(),
            banner: None,
            sections,
        }
    }

    // ── Components ───────────────────────────────────────────────────────

    fn specimen_card(
        &self,
        t: &Theme,
        def: &SpecimenDef,
        stage: AnyElement,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        panel(t)
            .min_w_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_1()
                    .min_h(px(148.0))
                    .p(px(20.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(stage),
            )
            .child(
                div()
                    .border_t_1()
                    .border_color(w::row_divider(t))
                    .pl(px(16.0))
                    .pr(px(10.0))
                    .py(px(10.0))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(w::row_title(t, def.title))
                    .child(
                        self.copy_chip(
                            t,
                            format!("workbench-specimen-copy-{}", def.title),
                            def.symbol,
                            def.symbol,
                            cx,
                        )
                        .ml(px(-6.0)),
                    )
                    .when(!def.note.is_empty(), |footer| {
                        footer.child(caption(t, def.note))
                    }),
            )
    }

    fn components(&self, t: &Theme, query: &str, cx: &mut Context<Self>) -> PageContent {
        let mut sections = Vec::new();
        for group in SPECIMEN_GROUPS {
            let defs: Vec<&SpecimenDef> = SPECIMENS
                .iter()
                .filter(|def| {
                    def.group == group && matches(query, &[def.title, def.symbol, def.note, group])
                })
                .collect();
            if defs.is_empty() {
                continue;
            }
            let mut grid = self.grid(300.0);
            for def in &defs {
                let stage = self.specimen_stage(def.id, t, cx);
                grid = grid.child(self.specimen_card(t, def, stage, cx));
            }
            sections.push(Section::new(group, grid).count(defs.len()));
        }
        let unmounted = self
            .components
            .iter()
            .filter(|entry| entry.workbench != "mounted")
            .count();
        PageContent {
            lede: format!(
                "Interact with production GPUI builders. Each card names the builder it calls; copy it to find the source. {unmounted} data-bound or unresolved patterns are listed in Inventory & gaps."
            )
            .into(),
            banner: None,
            sections,
        }
    }

    fn specimen_stage(&self, id: SpecimenId, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        match id {
            SpecimenId::DialogActions => div()
                .flex()
                .flex_wrap()
                .gap(px(8.0))
                .child(popover::btn_primary(t, "Continue"))
                .child(popover::btn_ghost(t, "Cancel", "workbench-ghost"))
                .child(popover::btn_danger(t, "Delete"))
                .into_any_element(),
            SpecimenId::SettingsActions => {
                let mut row = div().flex().flex_wrap().gap(px(8.0));
                for (tone, label) in [
                    (w::ActionTone::Quiet, "Quiet"),
                    (w::ActionTone::Outlined, "Outlined"),
                    (w::ActionTone::Filled, "Filled"),
                    (w::ActionTone::Solid, "Solid"),
                ] {
                    row = row.child(
                        focusable(
                            t,
                            w::action_button(t, tone)
                                .id(SharedString::from(format!("workbench-action-{label}")))
                                .role(gpui::Role::Button)
                                .aria_label(label)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.action_pressed = !this.action_pressed;
                                    cx.notify();
                                })),
                        )
                        .child(label),
                    );
                }
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(10.0))
                    .child(row)
                    .child(caption(
                        t,
                        if self.action_pressed {
                            "Activated"
                        } else {
                            "Choose a tone"
                        },
                    ))
                    .into_any_element()
            }
            SpecimenId::DiffToggle => div()
                .flex()
                .items_center()
                .gap(px(10.0))
                .child(
                    changes::Changes::header_toggle(
                        "workbench-diff-layout",
                        icons::SPLIT_COLUMNS,
                        self.split_on,
                        t,
                    )
                    .role(gpui::Role::Button)
                    .aria_label("Toggle diff layout")
                    .aria_selected(self.split_on)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.split_on = !this.split_on;
                        cx.notify();
                    })),
                )
                .child(body_text(
                    t,
                    if self.split_on { "Split" } else { "Unified" },
                ))
                .into_any_element(),
            SpecimenId::SurfaceToolbar => surface_chrome::toolbar(t)
                .w_full()
                .child(
                    surface_chrome::input()
                        .child(
                            icons::icon(icons::MAGNIFER)
                                .size(px(16.0))
                                .text_color(t.text_muted),
                        )
                        .child("Search files"),
                )
                .into_any_element(),
            SpecimenId::IconControl => div()
                .flex()
                .items_center()
                .gap(px(16.0))
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
                )
                .into_any_element(),
            SpecimenId::ToolbarIconButtons => div()
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
                )
                .into_any_element(),
            SpecimenId::ToggleSwitch => focusable(
                t,
                div()
                    .id("workbench-toggle")
                    .role(gpui::Role::Switch)
                    .aria_label("Toggle switch")
                    .aria_selected(self.switch_on)
                    .rounded(px(8.0))
                    .p(px(4.0))
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.switch_on = !this.switch_on;
                        cx.notify();
                    })),
            )
            .flex()
            .items_center()
            .gap(px(12.0))
            .child(w::toggle_switch(t, self.switch_on, "workbench-switch"))
            .child(body_text(t, if self.switch_on { "On" } else { "Off" }))
            .into_any_element(),
            SpecimenId::SettingsSelect => w::select(
                "workbench-settings-select",
                "Density",
                t,
                |this: &mut Workbench| &mut this.select_state,
            )
            .options(
                [
                    w::SelectOption::new("Comfortable"),
                    w::SelectOption::new("Compact"),
                    w::SelectOption::new("Dense"),
                ],
                self.select_choice,
            )
            .width(220.0)
            .on_select(|this, choice, _, cx| {
                this.select_choice = choice;
                cx.notify();
            })
            .render(&self.select_state, cx)
            .into_any_element(),
            SpecimenId::TaskCheckbox => {
                let entity = cx.entity().downgrade();
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        markdown::render::task_checkbox(
                            "workbench-task-checkbox".into(),
                            self.checkbox_on,
                            true,
                            "Toggle task".into(),
                            t,
                        )
                        .on_change(move |_, _, _, cx| {
                            entity
                                .update(cx, |this, cx| {
                                    this.checkbox_on = !this.checkbox_on;
                                    cx.notify();
                                })
                                .ok();
                        }),
                    )
                    .child(body_text(
                        t,
                        if self.checkbox_on { "Done" } else { "To do" },
                    ))
                    .into_any_element()
            }
            SpecimenId::FileSearch => {
                let search_focus = self.search.focus_handle(cx);
                surface_chrome::toolbar(t)
                    .w_full()
                    .child(
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
                    )
                    .into_any_element()
            }
            SpecimenId::DialogField => div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(w::field_label(t, "Device name"))
                .child(popover::dialog_field(self.field.clone().into_any_element()))
                .into_any_element(),
            SpecimenId::OptionCards => {
                let option = |icon: &'static str, label: &'static str, on: bool| {
                    w::option_card(
                        t,
                        icon,
                        label,
                        on,
                        if on { 1.0 } else { 0.0 },
                        div()
                            .w_full()
                            .h_full()
                            .rounded(px(w::OPTION_CARD_RADIUS))
                            .bg(t.surface)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(icons::icon(icon).size(px(22.0)).text_color(if on {
                                t.accent
                            } else {
                                t.text_muted
                            }))
                            .into_any_element(),
                    )
                };
                w::option_card_row()
                    .child(focusable(
                        t,
                        option(icons::WIDGET, "Compact", self.option_compact)
                            .id("workbench-option-compact")
                            .role(gpui::Role::RadioButton)
                            .aria_label("Compact layout")
                            .aria_selected(self.option_compact)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.option_compact = true;
                                cx.notify();
                            })),
                    ))
                    .child(focusable(
                        t,
                        option(icons::LIST, "List", !self.option_compact)
                            .id("workbench-option-list")
                            .role(gpui::Role::RadioButton)
                            .aria_label("List layout")
                            .aria_selected(!self.option_compact)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.option_compact = false;
                                cx.notify();
                            })),
                    ))
                    .into_any_element()
            }
            SpecimenId::Badges => div()
                .flex()
                .gap(px(8.0))
                .child(w::badge(t, "Default"))
                .child(w::badge_active(t, "Active"))
                .into_any_element(),
            SpecimenId::ErrorStrip => div()
                .w_full()
                .child(w::error_strip(t, "Could not connect to this device.").mt(px(0.0)))
                .into_any_element(),
            SpecimenId::WarningStrip => div()
                .w_full()
                .child(w::warning_strip(t, "A new version is available.").mt(px(0.0)))
                .into_any_element(),
            SpecimenId::PullRequestStates => {
                let summary = |state| ChangeRequestSummary {
                    provider: "github".into(),
                    number: 418,
                    title: "Refine the native workbench".into(),
                    url: "https://github.com/zeronsh/zeron/pull/418".into(),
                    state,
                    base_ref: "main".into(),
                    head_ref: "design/ui-workbench".into(),
                };
                let mut row = div().flex().flex_wrap().gap(px(8.0));
                for (key, state) in [
                    ("open", ChangeRequestState::Open),
                    ("merged", ChangeRequestState::Merged),
                    ("closed", ChangeRequestState::Closed),
                ] {
                    row = row.child(change_requests::pull_request_badge_preview(
                        format!("workbench-pr-{key}").into(),
                        summary(state),
                        change_requests::ChangeRequestBadgeSurface::Composer,
                        t,
                    ));
                }
                row.into_any_element()
            }
            SpecimenId::FailureNotices => div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(8.0))
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
                ))
                .into_any_element(),
            SpecimenId::LoadingGlyph => div()
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
                .child(body_text(t, "Working"))
                .into_any_element(),
            SpecimenId::UploadProgress => div()
                .size(px(46.0))
                .rounded(px(9.0))
                .bg(t.text_muted.opacity(0.6))
                .flex()
                .items_center()
                .justify_center()
                .child(loaders::upload_progress_ring(64, 32.0))
                .into_any_element(),
            SpecimenId::MarkLoader => {
                loaders::zeron_mark_loader("workbench-mark", t, 68.0, cx.entity_id(), cx)
                    .into_any_element()
            }
            SpecimenId::Skeletons => div()
                .w_full()
                .flex()
                .gap(px(12.0))
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
                )))
                .into_any_element(),
            SpecimenId::PopoverError => {
                popover::error_row(t, "Unable to load options.").into_any_element()
            }
            SpecimenId::PopoverMenu => popover::popover_card(t)
                .child(popover::menu_heading(t, "Actions"))
                .child(
                    popover::menu_row(t, false, "workbench-menu-row")
                        .id("workbench-menu-row")
                        .child("New session"),
                )
                .into_any_element(),
            SpecimenId::PageHeading => div()
                .flex()
                .flex_col()
                .child(w::page_header(t, "Appearance", Some(7)))
                .child(w::page_subtitle(t, "Choose a theme and accent."))
                .into_any_element(),
            SpecimenId::EmptySessions => shell::empty_sessions_message(t).into_any_element(),
            SpecimenId::CommandPalette => popover::palette_card(t, px(270.0), 12.0)
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
                ))
                .into_any_element(),
            SpecimenId::SettingsRow => w::section_card(t)
                .mt(px(0.0))
                .w_full()
                .child(
                    w::card_row(t, true)
                        .child(w::row_tile(t, icons::BELL))
                        .child(w::row_title(t, "Notifications")),
                )
                .into_any_element(),
            SpecimenId::DiffHunk => {
                let patch = "diff --git a/src/main.rs b/src/main.rs\nindex 1100000..2200000 100644\n--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1,3 +1,3 @@\n fn main() {\n-    let gap = 12;\n+    let gap = 8;\n }\n";
                let file = changes::parse_patch(patch)
                    .into_iter()
                    .next()
                    .expect("workbench diff fixture");
                div()
                    .w_full()
                    .child(changes::render_file_body_with_syntax(&file, None, t))
                    .into_any_element()
            }
            SpecimenId::ContextBadge => {
                let badge = badges::MessageBadge {
                    icon: icons::INFO_CIRCLE,
                    label: "Review context".into(),
                    details: vec![badges::BadgeDetail {
                        location: "src/main.rs:42".into(),
                        tag: Some("R".into()),
                        body: "Check this branch before merging.".into(),
                    }],
                };
                badges::render("workbench-context-badge", &badge, t).into_any_element()
            }
            SpecimenId::DialogSurface => popover::dialog_card(t)
                .child(popover::dialog_title(t, "Create a space"))
                .child(popover::dialog_body(t, "Choose a name for your workspace."))
                .into_any_element(),
        }
    }

    // ── Current icons ────────────────────────────────────────────────────

    fn size_switch(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        self.segmented(
            t,
            "workbench-icon-size",
            "Preview size",
            vec![
                (16.0_f32 as u32, "16".into()),
                (20, "20".into()),
                (24, "24".into()),
            ],
            self.icon_size as u32,
            |this, size, _, _| this.icon_size = size as f32,
            cx,
        )
    }

    fn current_icons(&self, t: &Theme, query: &str, cx: &mut Context<Self>) -> PageContent {
        let visible: Vec<&(String, String)> = self
            .current_icons
            .iter()
            .filter(|(name, path)| matches(query, &[name, path]))
            .collect();
        let mut sections = Vec::new();
        if !visible.is_empty() {
            let mut grid = self.grid(148.0);
            for (name, path) in &visible {
                let value = SharedString::from(format!("icons::{name}"));
                let copied = self.is_copied(&value);
                let file = path.strip_prefix("icons/").unwrap_or(path).to_string();
                grid = grid.child(
                    focusable(
                        t,
                        panel(t)
                            .id(SharedString::from(format!("workbench-current-{name}")))
                            .min_w_0()
                            .p(px(8.0))
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .cursor_pointer()
                            .hover(|s| s.bg(t.wash(0.07)))
                            .role(gpui::Role::Button)
                            .aria_label(format!("Copy {value}"))
                            .tooltip(w::text_tooltip(format!("Copy {value} · {file}")))
                            .on_click(cx.listener({
                                let value = value.clone();
                                move |this, _, _, cx| this.copy(value.clone(), cx)
                            })),
                    )
                    .child(
                        div()
                            .h(px(56.0))
                            .rounded(px(INSET_RADIUS))
                            .bg(t.wash(0.03))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                svg()
                                    .path(SharedString::from(path.clone()))
                                    .size(px(self.icon_size))
                                    .text_color(t.text),
                            ),
                    )
                    .child(
                        div()
                            .px(px(4.0))
                            .pb(px(2.0))
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .min_w_0()
                            .font_family(t.font_mono.clone())
                            .text_size(typography::ui_rems(11.0))
                            .text_color(if copied { t.success } else { t.text_muted })
                            .when(copied, |row| {
                                row.child(icons::icon(icons::CHECK).flex_none().size(px(12.0)))
                            })
                            .child(
                                div()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .whitespace_nowrap()
                                    .child(if copied {
                                        "Copied".to_string()
                                    } else {
                                        name.clone()
                                    }),
                            ),
                    ),
                );
            }
            sections.push(
                Section::new("Control glyphs", grid)
                    .count(if query.is_empty() {
                        visible.len().to_string()
                    } else {
                        format!("{} of {}", visible.len(), self.current_icons.len())
                    })
                    .trailing(self.size_switch(t, cx)),
            );
        }
        PageContent {
            lede: "The control and identity SVGs production loads through icons::Assets. Select a glyph to copy its constant; the proposed replacement set lives in the Icon atelier.".into(),
            banner: None,
            sections,
        }
    }

    // ── Icon atelier ─────────────────────────────────────────────────────

    fn icon_matches(icon: &IconRecord, query: &str) -> bool {
        matches(
            query,
            &[
                &icon.name,
                &icon.note,
                &icon.category,
                &icon.legacy.join(" "),
            ],
        )
    }

    fn inspector(&self, t: &Theme, icon: &IconRecord, cx: &mut Context<Self>) -> gpui::Div {
        let regular = format!("custom-icons/{}.svg", icon.name);
        let optical = format!("custom-icons/small/{}.svg", icon.name);
        let constants = self
            .proposed_names
            .get(&regular)
            .cloned()
            .unwrap_or_default();
        let well = |content: AnyElement, label: &str| {
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(6.0))
                .child(
                    div()
                        .size(px(56.0))
                        .rounded(px(WELL_RADIUS))
                        .bg(t.wash(0.04))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(content),
                )
                .child(caption(t, label.to_string()))
        };
        let mut sizes = div().flex().flex_wrap().gap(px(12.0));
        sizes = sizes
            .child(well(
                svg()
                    .path(regular.clone())
                    .size(px(40.0))
                    .text_color(t.text)
                    .into_any_element(),
                "40",
            ))
            .child(well(
                svg()
                    .path(regular.clone())
                    .size(px(24.0))
                    .text_color(t.text)
                    .into_any_element(),
                "24",
            ))
            .child(well(
                svg()
                    .path(regular.clone())
                    .size(px(20.0))
                    .text_color(t.text)
                    .into_any_element(),
                "20",
            ))
            .child(well(
                svg()
                    .path(optical)
                    .size(px(16.0))
                    .text_color(t.text)
                    .into_any_element(),
                "16 optical",
            ));
        for legacy in &icon.legacy {
            sizes = sizes.child(well(
                svg()
                    .path(SharedString::from(format!("icons/{legacy}.svg")))
                    .size(px(20.0))
                    .text_color(t.text_muted)
                    .into_any_element(),
                "current",
            ));
        }

        let mut facts = div()
            .flex_1()
            .min_w(px(240.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .font_family(t.font_mono.clone())
                            .text_size(typography::ui_rems(15.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(icon.name.clone()),
                    )
                    .child(w::badge(t, icon.status.clone())),
            )
            .child(caption(t, icon.category.clone()))
            .when(!icon.note.is_empty(), |col| {
                col.child(body_text(t, icon.note.clone()))
            });
        let mut copies = div().flex().flex_wrap().gap(px(4.0)).ml(px(-6.0));
        for constant in &constants {
            let value = format!("experimental_icons::{constant}");
            copies = copies.child(self.copy_chip(
                t,
                format!("workbench-inspect-const-{constant}"),
                value.clone(),
                value,
                cx,
            ));
        }
        copies = copies.child(self.copy_chip(
            t,
            "workbench-inspect-file",
            format!("{}.svg", icon.name),
            format!("crates/ui/assets/custom-icons/{}.svg", icon.name),
            cx,
        ));
        facts = facts.child(copies);
        if !icon.legacy.is_empty() {
            facts = facts.child(caption(
                t,
                format!(
                    "Replaces icons/{}",
                    icon.legacy.join(".svg, icons/") + ".svg"
                ),
            ));
        }
        if !icon.references.is_empty() {
            let mut refs = div().flex().flex_wrap().gap(px(4.0)).ml(px(-6.0));
            for (index, reference) in icon.references.iter().enumerate() {
                refs = refs.child(self.copy_chip(
                    t,
                    format!("workbench-inspect-ref-{index}"),
                    reference.clone(),
                    reference.clone(),
                    cx,
                ));
            }
            facts = facts
                .child(caption(
                    t,
                    format!(
                        "Used at {} location{}",
                        icon.references.len(),
                        if icon.references.len() == 1 { "" } else { "s" }
                    ),
                ))
                .child(refs);
        }
        panel(t)
            .p(px(20.0))
            .flex()
            .flex_wrap()
            .items_start()
            .gap(px(24.0))
            .child(sizes)
            .child(facts)
    }

    fn icons(&self, t: &Theme, query: &str, cx: &mut Context<Self>) -> PageContent {
        let mut sections = Vec::new();

        let pairs = [
            ("Settings", icons::SETTINGS, experimental_icons::SETTINGS),
            ("Folder", icons::FOLDER, experimental_icons::FOLDER),
            (
                "Pull request",
                icons::PULL_REQUEST,
                experimental_icons::PULL_REQUEST,
            ),
            ("Notification", icons::BELL, experimental_icons::BELL),
            ("Light", icons::SUN, experimental_icons::SUN),
            ("Dark", icons::MOON, experimental_icons::MOON),
        ];
        let pairs: Vec<_> = pairs
            .into_iter()
            .filter(|(label, _, _)| matches(query, &[label, "compare current target"]))
            .collect();
        if !pairs.is_empty() {
            let mut grid = self.grid(160.0);
            for (label, current, proposed) in &pairs {
                let glyph = |content: AnyElement, tag: &'static str| {
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(6.0))
                        .child(
                            div()
                                .h(px(44.0))
                                .w_full()
                                .rounded(px(INSET_RADIUS))
                                .bg(t.wash(0.03))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(content),
                        )
                        .child(caption(t, tag))
                };
                grid = grid.child(
                    panel(t)
                        .p(px(8.0))
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .child(
                            div()
                                .flex()
                                .gap(px(8.0))
                                .child(glyph(
                                    icons::icon(current)
                                        .size(px(22.0))
                                        .text_color(t.text)
                                        .into_any_element(),
                                    "Current",
                                ))
                                .child(glyph(
                                    experimental_icons::icon(proposed)
                                        .size(px(22.0))
                                        .text_color(t.text)
                                        .into_any_element(),
                                    "Target",
                                )),
                        )
                        .child(div().px(px(4.0)).pb(px(2.0)).child(w::row_title(t, *label))),
                );
            }
            sections.push(
                Section::new("Current and target", grid)
                    .note("The same role at 22px in today's glyph and the proposal. Inspect any glyph below for its full size ramp and replacement."),
            );
        }

        let morphs: Vec<(usize, &Morph)> = self
            .morphs
            .iter()
            .enumerate()
            .filter(|(_, morph)| matches(query, &[&morph.title, &morph.context, &morph.mode]))
            .collect();
        if !morphs.is_empty() {
            let limit = if self.show_all_morphs || !query.is_empty() {
                morphs.len()
            } else {
                FEATURED_MORPHS
            };
            let mut grid = self.grid(220.0);
            for (index, morph) in morphs.iter().take(limit) {
                let index = *index;
                let on = self.morphed[index];
                let path = if on { morph.to } else { morph.from };
                grid = grid.child(
                    focusable(
                        t,
                        panel(t)
                            .id(SharedString::from(format!("workbench-motion-tile-{index}")))
                            .p(px(8.0))
                            .flex()
                            .flex_col()
                            .gap(px(10.0))
                            .cursor_pointer()
                            .hover(|s| s.bg(t.wash(0.07)))
                            .role(gpui::Role::Button)
                            .aria_label(format!("Play {}", morph.title))
                            .aria_selected(on)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.morphed[index] = !this.morphed[index];
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .h(px(64.0))
                            .rounded(px(INSET_RADIUS))
                            .bg(t.wash(0.03))
                            .flex()
                            .items_center()
                            .justify_center()
                            .gap(px(16.0))
                            .child(
                                experimental_icons::icon(morph.from)
                                    .size(px(16.0))
                                    .text_color(if on { t.text_faint } else { t.text_muted }),
                            )
                            .child(
                                experimental_icons::icon(path)
                                    .morph(SharedString::from(format!("workbench-morph-{index}")))
                                    .size(px(32.0))
                                    .text_color(t.text),
                            )
                            .child(
                                experimental_icons::icon(morph.to)
                                    .size(px(16.0))
                                    .text_color(if on { t.text_muted } else { t.text_faint }),
                            ),
                    )
                    .child(
                        div()
                            .px(px(4.0))
                            .pb(px(2.0))
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap(px(8.0))
                                    .child(w::row_title(t, morph.title.clone()))
                                    .child(caption(
                                        t,
                                        if on { "End state" } else { "Start state" },
                                    )),
                            )
                            .child(body_text(t, morph.context.clone()))
                            .child(caption(
                                t,
                                format!("{} ms · {}", morph.duration, morph.mode),
                            )),
                    ),
                );
            }
            let hidden = morphs.len().saturating_sub(limit);
            let toggle = (query.is_empty()).then(|| {
                focusable(
                    t,
                    w::text_action(
                        t,
                        w::ActionTone::Outlined,
                        if self.show_all_morphs {
                            format!("Show {FEATURED_MORPHS} featured")
                        } else {
                            format!("Show all {}", morphs.len())
                        },
                    )
                    .id("workbench-more-motion")
                    .role(gpui::Role::Button)
                    .aria_expanded(self.show_all_morphs)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.show_all_morphs = !this.show_all_morphs;
                        cx.notify();
                    })),
                )
            });
            let mut section = Section::new("State transitions", grid)
                .count(if hidden > 0 {
                    format!("{limit} of {}", morphs.len())
                } else {
                    morphs.len().to_string()
                })
                .note(
                    "Select a tile to play its reversible transition; select it again to reverse.",
                );
            if let Some(toggle) = toggle {
                section = section.trailing(toggle);
            }
            sections.push(section);
        }

        let visible: Vec<&IconRecord> = self
            .icons
            .iter()
            .filter(|icon| Self::icon_matches(icon, query))
            .collect();
        let mut chips = div().flex().flex_wrap().gap(px(6.0));
        for category in std::iter::once(None).chain(ICON_CATEGORIES.into_iter().map(Some)) {
            let count = visible
                .iter()
                .filter(|icon| category.is_none_or(|c| icon.category == c))
                .count();
            let on = self.icon_category == category;
            let label = category.unwrap_or("All");
            chips = chips.child(
                focusable(
                    t,
                    div()
                        .id(SharedString::from(format!("workbench-category-{label}")))
                        .h(px(28.0))
                        .px(px(10.0))
                        .rounded(px(14.0))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .cursor_pointer()
                        .text_size(typography::ui_rems(12.0))
                        .map(|chip| {
                            if on {
                                chip.bg(t.solid).text_color(t.on_solid)
                            } else {
                                chip.bg(t.wash(0.05))
                                    .text_color(t.text_muted)
                                    .hover(|s| s.bg(t.glass_hover()).text_color(t.text))
                            }
                        })
                        .role(gpui::Role::RadioButton)
                        .aria_label(format!("{label}, {count} glyphs"))
                        .aria_selected(on)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.icon_category = category;
                            cx.notify();
                        })),
                )
                .child(label)
                .child(div().opacity(0.6).child(count.to_string())),
            );
        }
        let mut library = div().flex().flex_col().gap(px(20.0)).child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .child(chips)
                .child(self.size_switch(t, cx)),
        );
        if let Some(selected) = self
            .selected_icon
            .as_deref()
            .and_then(|name| self.icons.iter().find(|icon| icon.name == name))
        {
            library = library.child(self.inspector(t, selected, cx));
        }
        let mut library_count = 0;
        for category in ICON_CATEGORIES {
            if self
                .icon_category
                .is_some_and(|selected| selected != category)
            {
                continue;
            }
            let glyphs: Vec<&&IconRecord> = visible
                .iter()
                .filter(|icon| icon.category == category)
                .collect();
            if glyphs.is_empty() {
                continue;
            }
            library_count += glyphs.len();
            let mut grid = self.grid(124.0);
            for icon in &glyphs {
                let regular = format!("custom-icons/{}.svg", icon.name);
                let optical = format!("custom-icons/small/{}.svg", icon.name);
                let on = self.selected_icon.as_deref() == Some(icon.name.as_str());
                let name = icon.name.clone();
                grid = grid.child(
                    focusable(
                        t,
                        panel(t)
                            .id(SharedString::from(format!("workbench-icon-{name}")))
                            .min_w_0()
                            .p(px(8.0))
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .cursor_pointer()
                            .when(on, |tile| tile.shadow(accent_ring(t)))
                            .hover(|s| s.bg(t.wash(0.07)))
                            .role(gpui::Role::Button)
                            .aria_label(format!("Inspect {name}"))
                            .aria_selected(on)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.selected_icon = Some(name.clone());
                                this.scroll.scroll_to_top_of_item(4);
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .h(px(52.0))
                            .rounded(px(INSET_RADIUS))
                            .bg(t.wash(0.03))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                svg()
                                    .path(if self.icon_size <= 16.0 {
                                        optical
                                    } else {
                                        regular
                                    })
                                    .size(px(self.icon_size))
                                    .text_color(t.text),
                            ),
                    )
                    .child(
                        div()
                            .px(px(4.0))
                            .pb(px(2.0))
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .font_family(t.font_mono.clone())
                            .text_size(typography::ui_rems(11.0))
                            .text_color(if on { t.text } else { t.text_muted })
                            .child(icon.name.clone()),
                    ),
                );
            }
            library = library.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .child(subheading(t, category, glyphs.len()))
                    .child(grid),
            );
        }
        if library_count == 0 && !visible.is_empty() {
            library = library.child(body_text(t, "No glyphs in this category match the filter."));
        }
        if !visible.is_empty() {
            sections.push(
                Section::new("Glyph library", library)
                    .count(if query.is_empty() {
                        self.icons.len().to_string()
                    } else {
                        format!("{} of {}", visible.len(), self.icons.len())
                    })
                    .note("Select a glyph to inspect it at every size beside the current icon it replaces, with its constant, asset and call sites ready to copy. At 16px the optical master is shown."),
            );
        }

        PageContent {
            lede: format!(
                "A proposed icon language, isolated from the shipped app: {} glyphs with optical 16px masters and {} reversible transitions. These assets load only in this workbench.",
                self.icons.len(),
                self.morphs.len()
            )
            .into(),
            banner: Some(
                div()
                    .flex()
                    .items_start()
                    .gap(px(10.0))
                    .px(px(14.0))
                    .py(px(12.0))
                    .rounded(px(CARD_RADIUS))
                    .bg(t.accent_wash)
                    .child(
                        icons::icon(icons::INFO_CIRCLE)
                            .flex_none()
                            .mt(px(1.0))
                            .size(px(16.0))
                            .text_color(t.accent),
                    )
                    .child(body_text(
                        t,
                        "Experimental. Production still registers icons::Assets and today's glyphs; the ui-workbench feature keeps these assets out of the release binary.",
                    ))
                    .into_any_element(),
            ),
            sections,
        }
    }

    // ── Inventory ────────────────────────────────────────────────────────

    fn coverage(&self, t: &Theme, query: &str, cx: &mut Context<Self>) -> PageContent {
        let mut sections = Vec::new();
        let files = self.audit["sourceFiles"].as_u64().unwrap_or(0);
        let cataloged = self.audit["catalogedSourceFiles"].as_u64().unwrap_or(0);
        let support = self.audit["supportModules"].as_u64().unwrap_or(0);
        let unreviewed = self.audit["unreviewedSourceFiles"].as_u64().unwrap_or(0);
        let fingerprint = self.audit["sourceFingerprint"].as_str().unwrap_or("");
        let status_count = |status: &str| {
            self.components
                .iter()
                .filter(|entry| entry.workbench == status)
                .count()
        };
        let mounted = status_count("mounted");
        let needs_fixture = status_count("needs-fixture");
        let consolidate = status_count("needs-consolidation");

        if query.is_empty() {
            let mut stats = self.grid(180.0);
            for (value, label, detail, tone) in [
                (
                    format!("{}", files - unreviewed),
                    "Modules classified",
                    format!("{cataloged} pattern · {support} support · {unreviewed} unreviewed"),
                    if unreviewed == 0 {
                        t.success
                    } else {
                        t.warning
                    },
                ),
                (
                    mounted.to_string(),
                    "Mounted patterns",
                    format!("of {} in the inventory", self.components.len()),
                    t.text,
                ),
                (
                    needs_fixture.to_string(),
                    "Need a fixture",
                    "Data-bound views without a faithful mount".to_string(),
                    t.warning,
                ),
                (
                    self.decisions.len().to_string(),
                    "Open decisions",
                    format!("{consolidate} pattern waiting on consolidation"),
                    t.warning,
                ),
            ] {
                stats = stats.child(
                    panel(t)
                        .p(px(16.0))
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(div().size(px(6.0)).rounded_full().bg(tone))
                                .child(
                                    div()
                                        .text_size(typography::ui_rems(26.0))
                                        .line_height(typography::ui_rems(32.0))
                                        .child(value),
                                ),
                        )
                        .child(w::row_title(t, label))
                        .child(caption(t, detail)),
                );
            }
            sections.push(Section::new("Summary", stats).trailing(self.copy_chip(
                t,
                "workbench-fingerprint",
                format!("audit {fingerprint}"),
                "python3 apps/ui-workbench/audit.py --check",
                cx,
            )));
        }

        // Pattern inventory: the actionable list, first.
        let filter = self.segmented(
            t,
            "workbench-pattern-filter",
            "Pattern status",
            vec![
                (
                    PatternFilter::All,
                    format!("All {}", self.components.len()).into(),
                ),
                (
                    PatternFilter::NeedsFixture,
                    format!("Need fixture {needs_fixture}").into(),
                ),
                (
                    PatternFilter::Consolidate,
                    format!("Consolidate {consolidate}").into(),
                ),
                (PatternFilter::Mounted, format!("Mounted {mounted}").into()),
            ],
            self.pattern_filter,
            |this, filter, _, _| this.pattern_filter = filter,
            cx,
        );
        let mut groups = div().flex().flex_col().gap(px(20.0));
        let mut pattern_count = 0;
        for group in PATTERN_GROUPS {
            let entries: Vec<&ComponentEntry> = self
                .components
                .iter()
                .filter(|entry| {
                    entry.group == group
                        && self.pattern_filter.admits(&entry.workbench)
                        && matches(query, &[&entry.name, &entry.description, &entry.source])
                })
                .collect();
            if entries.is_empty() {
                continue;
            }
            pattern_count += entries.len();
            let mut card = w::section_card(t).mt(px(0.0));
            for (index, entry) in entries.iter().enumerate() {
                let (label, color) = match entry.workbench.as_str() {
                    "mounted" => ("Mounted", t.success),
                    "needs-consolidation" => ("Consolidate", t.warning),
                    _ => ("Needs fixture", t.text_muted),
                };
                card = card.child(
                    w::card_row(t, index == 0)
                        .flex_nowrap()
                        .items_start()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap(px(2.0))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(8.0))
                                        .child(w::row_title(t, entry.name.clone()))
                                        .child(status_pill(t, label, color)),
                                )
                                .child(body_text(t, entry.description.clone())),
                        )
                        .child(
                            self.copy_chip(
                                t,
                                format!("workbench-pattern-{group}-{index}"),
                                entry
                                    .source
                                    .trim_start_matches("crates/ui/src/")
                                    .to_string(),
                                entry.source.clone(),
                                cx,
                            ),
                        ),
                );
            }
            groups = groups.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .child(subheading(t, group, entries.len()))
                    .child(card),
            );
        }
        if pattern_count == 0 {
            groups = groups.child(body_text(t, "No patterns match this status and filter."));
        }
        sections.push(
            Section::new("Pattern inventory", groups)
                .count(pattern_count)
                .trailing(filter)
                .note("components.json maps each desktop pattern to its source. Mounted patterns have a live specimen; the rest need a faithful fixture or a shared builder first."),
        );

        let decisions: Vec<&Decision> = self
            .decisions
            .iter()
            .filter(|decision| {
                matches(
                    query,
                    &[
                        &decision.family,
                        &decision.current,
                        &decision.target,
                        &decision.sources.join(" "),
                    ],
                )
            })
            .collect();
        if !decisions.is_empty() {
            let mut card = w::section_card(t).mt(px(0.0));
            for (index, decision) in decisions.iter().enumerate() {
                let id = decision.id.clone();
                let expanded =
                    self.selected_decision.as_deref() == Some(id.as_str()) || !query.is_empty();
                let mut row = w::card_row(t, index == 0)
                    .flex_col()
                    .items_stretch()
                    .gap(px(6.0))
                    .child(
                        focusable(
                            t,
                            div()
                                .id(SharedString::from(format!("workbench-decision-{id}")))
                                .mx(px(-8.0))
                                .px(px(8.0))
                                .py(px(4.0))
                                .rounded(px(6.0))
                                .flex()
                                .items_center()
                                .gap(px(12.0))
                                .cursor_pointer()
                                .hover(|s| s.bg(t.wash(0.03)))
                                .role(gpui::Role::Button)
                                .aria_label(format!("{}: show source locations", decision.family))
                                .aria_expanded(expanded)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.selected_decision =
                                        if this.selected_decision.as_deref() == Some(id.as_str()) {
                                            None
                                        } else {
                                            Some(id.clone())
                                        };
                                    cx.notify();
                                })),
                        )
                        .child(
                            icons::icon(if expanded {
                                icons::ALT_ARROW_DOWN
                            } else {
                                icons::ALT_ARROW_RIGHT
                            })
                            .flex_none()
                            .size(px(14.0))
                            .text_color(t.text_muted),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(w::row_title(t, decision.family.clone())),
                        )
                        .child(status_pill(
                            t,
                            &format!("{} priority", decision.priority),
                            if decision.priority == "High" {
                                t.warning
                            } else {
                                t.text_muted
                            },
                        ))
                        .child(caption(t, format!("{} sources", decision.sources.len()))),
                    )
                    .child(
                        div()
                            .pl(px(26.0))
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(body_text(t, format!("Now: {}", decision.current)))
                            .child(
                                div()
                                    .text_size(typography::ui_rems(12.0))
                                    .line_height(typography::ui_rems(17.0))
                                    .text_color(t.text)
                                    .child(format!("Proposed: {}", decision.target)),
                            ),
                    );
                if expanded {
                    let mut sources = div().pl(px(20.0)).flex().flex_wrap().gap(px(4.0));
                    for (source_index, source) in decision.sources.iter().enumerate() {
                        sources = sources.child(self.copy_chip(
                            t,
                            format!("workbench-decision-{}-{source_index}", decision.id),
                            source.clone(),
                            source.clone(),
                            cx,
                        ));
                    }
                    row = row.child(sources);
                }
                card = card.child(row);
            }
            sections.push(
                Section::new("Consolidation decisions", card)
                    .count(decisions.len())
                    .note("Open proposals from consolidation.json. Expand one for its source locations. These are review questions, not product changes."),
            );
        }

        let signals: Vec<&serde_json::Value> = self.audit["signals"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|signal| {
                matches(
                    query,
                    &[
                        signal["name"].as_str().unwrap_or(""),
                        signal["note"].as_str().unwrap_or(""),
                    ],
                )
            })
            .collect();
        if !signals.is_empty() {
            let mut grid = self.grid(300.0);
            for (index, signal) in signals.iter().enumerate() {
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
                    .join("   ");
                let mut examples = div().flex().flex_col().gap(px(2.0)).ml(px(-6.0));
                for (example_index, location) in signal["locations"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .take(2)
                    .filter_map(|location| location.as_str())
                    .enumerate()
                {
                    examples = examples.child(self.copy_chip(
                        t,
                        format!("workbench-signal-{index}-{example_index}"),
                        location.trim_start_matches("crates/ui/src/").to_string(),
                        location.to_string(),
                        cx,
                    ));
                }
                grid = grid.child(
                    panel(t)
                        .p(px(16.0))
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .child(
                            div()
                                .flex()
                                .items_baseline()
                                .justify_between()
                                .gap(px(12.0))
                                .child(w::row_title(t, name.to_string()))
                                .child(
                                    div()
                                        .text_size(typography::ui_rems(20.0))
                                        .child(count.to_string()),
                                ),
                        )
                        .child(body_text(t, note.to_string()))
                        .when(!values.is_empty(), |card| {
                            card.child(
                                div()
                                    .font_family(t.font_mono.clone())
                                    .text_size(typography::ui_rems(11.5))
                                    .text_color(t.text)
                                    .child(values.clone()),
                            )
                        })
                        .child(div().mt_auto().pt(px(4.0)).child(examples)),
                );
            }
            sections.push(
                Section::new("Source signals", grid)
                    .count(signals.len())
                    .note("Source-wide counts that show where shared tokens, accessible control treatment or an explicit exception deserve review. A count is an occurrence, not a defect."),
            );
        }

        let families: Vec<&serde_json::Value> = self.audit["families"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|family| {
                let builders = family["builders"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|builder| builder["symbol"].as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                matches(query, &[family["name"].as_str().unwrap_or(""), &builders])
            })
            .collect();
        if !families.is_empty() {
            let mut grid = self.grid(300.0);
            for family in &families {
                let mut rows = div().flex().flex_col();
                for builder in family["builders"].as_array().into_iter().flatten() {
                    let symbol = builder["symbol"].as_str().unwrap_or("");
                    let references = builder["references"].as_u64().unwrap_or(0);
                    rows = rows.child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(8.0))
                            .child(
                                self.copy_chip(
                                    t,
                                    format!("workbench-family-{symbol}"),
                                    symbol.to_string(),
                                    symbol.to_string(),
                                    cx,
                                )
                                .ml(px(-6.0)),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .text_size(typography::ui_rems(12.0))
                                    .text_color(if references == 0 {
                                        t.warning
                                    } else {
                                        t.text_muted
                                    })
                                    .child(if references == 0 {
                                        "unused".to_string()
                                    } else {
                                        format!("{references} refs")
                                    }),
                            ),
                    );
                }
                grid = grid.child(
                    panel(t)
                        .p(px(16.0))
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .child(w::row_title(
                            t,
                            family["name"].as_str().unwrap_or("").to_string(),
                        ))
                        .child(rows),
                );
            }
            sections.push(
                Section::new("Builder families", grid)
                    .count(families.len())
                    .note("Different native recipes for one semantic role. An unused builder or an overlapping family is a consolidation question, not a verdict."),
            );
        }

        let modules: Vec<&serde_json::Value> = self.audit["modules"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|module| {
                matches(
                    query,
                    &[
                        module["path"].as_str().unwrap_or(""),
                        module["role"].as_str().unwrap_or(""),
                        module["scopeReason"].as_str().unwrap_or(""),
                    ],
                )
            })
            .collect();
        if !modules.is_empty() {
            let show = self.show_modules || !query.is_empty();
            let mut body = div().flex().flex_col();
            if show {
                let mut card = w::section_card(t).mt(px(0.0));
                for (index, module) in modules.iter().enumerate() {
                    let path = module["path"].as_str().unwrap_or("");
                    let role = module["role"].as_str().unwrap_or("unreviewed");
                    let names = |field: &str| {
                        let values = module[field].as_array().cloned().unwrap_or_default();
                        let mut labels = values
                            .iter()
                            .take(6)
                            .filter_map(|value| value.as_str())
                            .collect::<Vec<_>>()
                            .join(", ");
                        if values.len() > 6 {
                            labels.push_str(&format!(" and {} more", values.len() - 6));
                        }
                        labels
                    };
                    let detail = [
                        module["scopeReason"].as_str().map(str::to_string),
                        (!names("exports").is_empty())
                            .then(|| format!("Functions: {}", names("exports"))),
                        (!names("views").is_empty()).then(|| format!("Views: {}", names("views"))),
                    ]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" · ");
                    card = card.child(
                        w::card_row(t, index == 0)
                            .min_h(px(48.0))
                            .py(px(8.0))
                            .flex_nowrap()
                            .items_start()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.0))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(8.0))
                                            .child(
                                                self.copy_chip(
                                                    t,
                                                    format!("workbench-module-{index}"),
                                                    path.trim_start_matches("crates/ui/src/")
                                                        .to_string(),
                                                    path.to_string(),
                                                    cx,
                                                )
                                                .ml(px(-6.0)),
                                            )
                                            .child(status_pill(
                                                t,
                                                role,
                                                if role == "pattern" {
                                                    t.success
                                                } else if role == "support" {
                                                    t.text_muted
                                                } else {
                                                    t.warning
                                                },
                                            )),
                                    )
                                    .when(!detail.is_empty(), |col| col.child(caption(t, detail))),
                            )
                            .child(caption(
                                t,
                                format!(
                                    "{} fns · {} views",
                                    module["publicFunctions"].as_u64().unwrap_or(0),
                                    module["renderImpls"].as_u64().unwrap_or(0)
                                ),
                            )),
                    );
                }
                body = body.child(card);
            }
            let mut section = Section::new("Scanned modules", body)
                .count(modules.len())
                .note("Every desktop UI module with its role. Pattern modules appear in the inventory; support modules are documented in support.json.");
            if query.is_empty() {
                section = section.trailing(focusable(
                    t,
                    w::text_action(
                        t,
                        w::ActionTone::Outlined,
                        if self.show_modules {
                            "Hide modules".to_string()
                        } else {
                            format!("Show {} modules", modules.len())
                        },
                    )
                    .id("workbench-audit-modules")
                    .role(gpui::Role::Button)
                    .aria_expanded(self.show_modules)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.show_modules = !this.show_modules;
                        cx.notify();
                    })),
                ));
            }
            sections.push(section);
        }

        PageContent {
            lede: "Every desktop UI module is classified as a native pattern or a support module. Mounted patterns have a live specimen; the rest stay named here, with their source, until they have a faithful fixture.".into(),
            banner: None,
            sections,
        }
    }
}

impl Render for Workbench {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = Theme::of(cx).clone();
        let viewport = window.viewport_size().width.to_f64() as f32;
        self.content_width = (viewport - NAV_WIDTH).min(PAGE_MAX_WIDTH) - 2.0 * PAGE_PAD_X;
        let query = self.query(cx);
        let nav = self.navigation(&t, window, cx);
        let top = self.topbar(&t, &query, cx);
        let content = match self.page {
            Page::Overview => self.overview(&t, &query, cx),
            Page::Foundations => self.foundations(&t, &query, cx),
            Page::Components => self.components(&t, &query, cx),
            Page::CurrentIcons => self.current_icons(&t, &query, cx),
            Page::Icons => self.icons(&t, &query, cx),
            Page::Coverage => self.coverage(&t, &query, cx),
        };
        let blocks = self.page_blocks(&t, content, &query, cx);
        let toast = self.copied.clone().map(|value| {
            div()
                .absolute()
                .bottom(px(20.0))
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(
                    div()
                        .max_w(px(520.0))
                        .px(px(12.0))
                        .py(px(8.0))
                        .rounded(px(10.0))
                        .bg(popover::surface_bg(&t))
                        .border_1()
                        .border_color(t.border)
                        .shadow_md()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .text_size(typography::ui_rems(12.0))
                        .id("workbench-toast")
                        .role(gpui::Role::Status)
                        .child(
                            icons::icon(icons::CHECK)
                                .size(px(14.0))
                                .text_color(t.success),
                        )
                        .child("Copied")
                        .child(
                            div()
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .font_family(t.font_mono.clone())
                                .text_color(t.text_muted)
                                .child(value),
                        ),
                )
        });
        div()
            .size_full()
            .flex()
            .bg(t.bg)
            .text_color(t.text)
            .font_family(t.font_sans.clone())
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key_down))
            .child(nav)
            .child(
                div()
                    .relative()
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
                            .children(blocks.into_iter().map(|block| {
                                div()
                                    .w_full()
                                    .max_w(px(PAGE_MAX_WIDTH))
                                    .mx_auto()
                                    .px(px(PAGE_PAD_X))
                                    .child(block)
                            })),
                    )
                    .children(toast),
            )
    }
}

// ── Free helpers ─────────────────────────────────────────────────────────

/// Case-insensitive substring match across fields; an empty query matches.
fn matches(query: &str, fields: &[&str]) -> bool {
    query.is_empty()
        || fields
            .iter()
            .any(|field| field.to_lowercase().contains(query))
}

/// The workbench's one card recipe: the Settings block fill, no outline.
fn panel(t: &Theme) -> gpui::Div {
    div().rounded(px(CARD_RADIUS)).bg(w::block_fill(t))
}

/// An inset focus ring: painted over the fill at the edge, no layout shift.
fn focus_ring(t: &Theme) -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: t.accent,
        offset: point(px(0.0), px(0.0)),
        blur_radius: px(0.0),
        spread_radius: px(2.0),
        inset: true,
    }]
}

fn accent_ring(t: &Theme) -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: t.accent.opacity(0.7),
        offset: point(px(0.0), px(0.0)),
        blur_radius: px(0.0),
        spread_radius: px(1.5),
        inset: true,
    }]
}

/// A hairline that separates a swatch from any surface behind it.
fn outline_ring(t: &Theme) -> Vec<BoxShadow> {
    let color = match t.appearance {
        Appearance::Dark => gpui::hsla(0.0, 0.0, 1.0, 0.1),
        Appearance::Light => gpui::hsla(0.0, 0.0, 0.0, 0.1),
    };
    vec![BoxShadow {
        color,
        offset: point(px(0.0), px(0.0)),
        blur_radius: px(0.0),
        spread_radius: px(1.0),
        inset: false,
    }]
}

/// Keyboard reachability and a visible focus ring for a custom control.
fn focusable(t: &Theme, element: gpui::Stateful<gpui::Div>) -> gpui::Stateful<gpui::Div> {
    let ring = focus_ring(t);
    element
        .tab_index(0)
        .focus_visible(move |style| style.shadow(ring.clone()))
}

fn body_text(t: &Theme, text: impl Into<SharedString>) -> gpui::Div {
    div()
        .text_size(typography::ui_rems(12.0))
        .line_height(typography::ui_rems(17.0))
        .text_color(t.text_muted)
        .child(text.into())
}

fn caption(t: &Theme, text: impl Into<SharedString>) -> gpui::Div {
    div()
        .text_size(typography::ui_rems(11.0))
        .line_height(typography::ui_rems(15.0))
        .text_color(t.text_faint)
        .child(text.into())
}

fn subheading(t: &Theme, label: &str, count: usize) -> gpui::Div {
    div()
        .flex()
        .items_baseline()
        .gap(px(8.0))
        .child(
            div()
                .text_size(typography::ui_rems(13.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .child(label.to_string()),
        )
        .child(caption(t, count.to_string()))
}

fn status_pill(t: &Theme, label: &str, color: Hsla) -> gpui::Div {
    let _ = t;
    div()
        .flex_none()
        .px(px(7.0))
        .rounded(px(9.0))
        .bg(color.opacity(0.12))
        .text_size(typography::ui_rems(11.0))
        .line_height(typography::ui_rems(18.0))
        .text_color(color)
        .child(label.to_string())
}

fn key_hint(t: &Theme, keys: &str) -> gpui::Div {
    div()
        .flex_none()
        .px(px(6.0))
        .h(px(20.0))
        .rounded(px(5.0))
        .flex()
        .items_center()
        .bg(t.wash(0.06))
        .text_size(typography::ui_rems(11.0))
        .text_color(t.text_muted)
        .child(keys.to_string())
}

fn shortcut_row(t: &Theme, keys: &str, label: &str) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(8.0))
        .child(caption(t, label.to_string()))
        .child(key_hint(t, keys))
}

fn window_title(page: Page) -> String {
    format!("{} — Zeron UI Workbench", page.label())
}

fn hex(color: Hsla) -> (String, f32) {
    let rgba: Rgba = color.into();
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    (
        format!(
            "#{:02X}{:02X}{:02X}",
            channel(rgba.r),
            channel(rgba.g),
            channel(rgba.b)
        ),
        rgba.a,
    )
}

/// WCAG 2 contrast ratio of `fg` composited over an opaque `bg`.
fn contrast(fg: Hsla, bg: Hsla) -> f32 {
    let bg: Rgba = bg.into();
    let fg: Rgba = fg.into();
    let over = |f: f32, b: f32| f * fg.a + b * (1.0 - fg.a);
    let linear = |v: f32| {
        if v <= 0.039_28 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    let luminance =
        |r: f32, g: f32, b: f32| 0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b);
    let back = luminance(bg.r, bg.g, bg.b);
    let front = luminance(over(fg.r, bg.r), over(fg.g, bg.g), over(fg.b, bg.b));
    let (light, dark) = if front > back {
        (front, back)
    } else {
        (back, front)
    };
    (light + 0.05) / (dark + 0.05)
}

/// `(CONSTANT, "icons/<file>.svg")` for every production control icon, read
/// from the `icon_assets!` table in `icons.rs`.
fn current_icon_names() -> Vec<(String, String)> {
    let mut names: Vec<(String, String)> = CURRENT_ICON_SOURCE
        .lines()
        .filter_map(|line| {
            let entry = line.trim().strip_prefix('(')?;
            let (name, rest) = entry.split_once(',')?;
            let file = rest.trim().strip_prefix('"')?.split('"').next()?;
            name.chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
                .then(|| (name.to_string(), format!("icons/{file}.svg")))
        })
        .collect();
    names.sort();
    names
}

/// Asset path → proposed constants, read from the generated table.
fn proposed_icon_names() -> HashMap<String, Vec<String>> {
    let mut names: HashMap<String, Vec<String>> = HashMap::new();
    for line in PROPOSED_ICON_SOURCE.lines() {
        let Some(rest) = line.trim().strip_prefix("pub const ") else {
            continue;
        };
        let Some((name, value)) = rest.split_once(": &str = \"") else {
            continue;
        };
        let Some(path) = value.split('"').next() else {
            continue;
        };
        names
            .entry(path.to_string())
            .or_default()
            .push(name.to_string());
    }
    names
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
                ("accent", t.accent),
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
pub fn run(data_dir: PathBuf, options: Options) {
    let page = match options.page.as_deref() {
        None => Page::Overview,
        Some(slug) => Page::from_slug(slug).unwrap_or_else(|| {
            eprintln!(
                "unknown workbench page {slug:?}; expected one of: {}",
                page_slugs().join(", ")
            );
            Page::Overview
        }),
    };
    let appearance_override = match options.appearance.as_deref() {
        None => None,
        Some("light") => Some(Appearance::Light),
        Some("dark") => Some(Appearance::Dark),
        Some(other) => {
            eprintln!("unknown appearance {other:?}; expected light or dark");
            None
        }
    };
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
                    size(px(1320.0), px(900.0)),
                    cx,
                ))),
                window_min_size: Some(size(px(860.0), px(600.0))),
                titlebar: Some(TitlebarOptions {
                    title: Some(window_title(page).into()),
                    appears_transparent: false,
                    traffic_light_position: None,
                }),
                window_background: Theme::of(cx).window_background_appearance(),
                app_id: Some("zeron-ui-workbench".into()),
                ..Default::default()
            },
            |window, cx| {
                window.set_rem_size(px(typography::font_size(cx).pixels()));
                let view = cx.new(|cx| Workbench::new(page, cx));
                view.update(cx, |workbench, cx| {
                    if let Some(appearance) = appearance_override {
                        workbench.appearance = appearance;
                        workbench.install_theme(cx);
                    }
                });
                let focus = view.read(cx).focus.clone();
                window.focus(&focus, cx);
                view
            },
        )
        .expect("open native UI workbench");
        cx.activate(true);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_current_icon_has_a_constant() {
        let names = current_icon_names();
        let mut paths: Vec<String> = icons::Assets
            .list("icons/")
            .unwrap()
            .into_iter()
            .map(|path| path.to_string())
            .collect();
        paths.sort();
        let mut named: Vec<String> = names.iter().map(|(_, path)| path.clone()).collect();
        named.sort();
        named.dedup();
        assert_eq!(named, paths);
    }

    #[test]
    fn proposed_constants_resolve_to_assets() {
        let names = proposed_icon_names();
        assert!(names.contains_key(experimental_icons::SETTINGS));
        for path in names.keys() {
            assert!(
                experimental_icons::Assets.load(path).unwrap().is_some(),
                "{path}"
            );
        }
    }

    #[test]
    fn page_slugs_round_trip() {
        for page in Page::ALL {
            assert!(Page::from_slug(page.slug()) == Some(page));
        }
    }

    #[test]
    fn contrast_matches_wcag_extremes() {
        let black = gpui::hsla(0.0, 0.0, 0.0, 1.0);
        let white = gpui::hsla(0.0, 0.0, 1.0, 1.0);
        assert!((contrast(black, white) - 21.0).abs() < 0.01);
        assert!((contrast(white, white) - 1.0).abs() < 0.01);
    }
}

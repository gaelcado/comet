//! Isolated native evidence for production completion controls and rich composer.
use gpui::{
    AppContext, AsyncApp, Bounds, Context, Entity, Render, Window, WindowBounds, WindowOptions,
    div, prelude::*, px, size,
};
use std::{path::PathBuf, time::Duration};
use zeron_ui::*;

struct Fixture {
    composer: Entity<composer::Composer>,
    agents: Entity<settings::harnesses::HarnessesPage>,
    settings: bool,
}
impl Render for Fixture {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = theme::Theme::of(cx).clone();
        div()
            .id("fixture")
            .size_full()
            .bg(theme.surface)
            .text_color(theme.text)
            .font_family(theme.font_sans.clone())
            .p(px(24.0))
            .overflow_y_scroll()
            .child(if self.settings {
                div()
                    .flex()
                    .flex_col()
                    .child(settings::widgets::page_header(&theme, "Agents", None))
                    .child(
                        self.agents
                            .update(cx, |page, cx| page.fixture_completion(cx)),
                    )
                    .into_any_element()
            } else {
                div()
                    .h_full()
                    .flex()
                    .flex_col()
                    .justify_center()
                    .gap(px(20.0))
                    .child(settings::widgets::page_header(&theme, "Composer", None))
                    .child(self.composer.clone())
                    .into_any_element()
            })
    }
}
async fn pause(cx: &mut AsyncApp) {
    cx.background_executor()
        .timer(Duration::from_millis(400))
        .await;
}
fn main() -> anyhow::Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    let _guard = runtime.enter();
    let output = PathBuf::from(std::env::args().nth(1).expect("output directory"));
    std::fs::create_dir_all(&output)?;
    let temp = tempfile::tempdir()?;
    let data = temp.path().to_path_buf();
    gpui_platform::application().with_assets(icons::Assets).run(move |cx| {
        gpui_tokio::init(cx); gpui_base::init(cx);
        let prefs = settings::UiSettings::default();
        settings::init(prefs.clone(), data.clone(), cx);
        let fonts = typography::register_fonts(cx);
        typography::init(prefs.ui_font_family.clone(), prefs.ui_font_size, prefs.terminal_font_family.clone(), prefs.terminal_font_size, prefs.code_font_family.clone(), prefs.code_font_size, fonts, cx);
        theme_library::init(data.clone(), cx);
        appearance::init(appearance::AppearanceMode::Dark, prefs.theme_selection, prefs.accent, prefs.surface, cx);
        composer::init(cx, prefs.composer_send_behavior);
        let state = cx.new(|_| state::AppState::new());
        let composer = cx.new(|cx| composer::Composer::new(state.clone(), cx));
        let agents = cx.new(|cx| settings::harnesses::HarnessesPage::new(state.clone(), cx));
        let skill = zeron_proto::invocation::Invocation::Skill { name: "review-changes".into(), path: "/project/.agents/skills/review-changes/SKILL.md".into(), command: None }.link();
        let file = zeron_proto::file_mentions::local_file_link("src/composer.rs", false);
        let long_file = zeron_proto::file_mentions::local_file_link("src/components/very-long-internationalized-component-name.test.tsx", false);
        let draft = format!("Review {file} with {skill}\nAlso check {long_file}\n**Keep the layout calm** and _easy to edit_.\n- Preserve keyboard navigation\n- Check café and 日本語\n```rust\nlet chips = render(&draft);\n```\nContinue here");
        composer.update(cx, |view, cx| view.fixture_rich_draft(&draft, cx));
        let window = cx.open_window(WindowOptions { window_bounds: Some(WindowBounds::Windowed(Bounds::new(gpui::point(px(40.), px(40.)), size(px(840.), px(960.))))), ..Default::default() }, |_, cx| cx.new(|_| Fixture { composer, agents, settings: true })).unwrap();
        cx.activate(true);
        cx.spawn(async move |cx| {
            for light in [false, true] {
                cx.update(|cx| appearance::set_mode(if light { appearance::AppearanceMode::Light } else { appearance::AppearanceMode::Dark }, cx));
                for settings in [true, false] {
                    for width in [840., 440.] {
                        window.update(cx, |view, w, cx| { view.settings = settings; w.resize(size(px(width), px(960.))); cx.notify(); }).unwrap();
                        pause(cx).await;
                        let name = format!("{}-{}-{}.png", if settings { "agents" } else { "composer" }, if light { "light" } else { "dark" }, width as u32);
                        let capture_window: gpui::AnyWindowHandle = window.into();
                        capture_window.update(cx, |_, w, cx| { w.draw(cx).clear(); w.render_to_image().unwrap().save(output.join(name)).unwrap(); }).unwrap();
                    }
                }
            }
            cx.update(|cx| cx.quit());
        }).detach();
    });
    Ok(())
}

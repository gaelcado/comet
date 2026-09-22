//! Per-agent composer completion preferences inside Settings → Agents.

use super::HarnessesPage;
use crate::{settings, settings::widgets, theme::Theme};
use gpui::{AnyElement, Context, div, prelude::*, px};
use zeron_proto::HarnessId;

impl HarnessesPage {
    fn toggle_completion(&mut self, harness: HarnessId, dollar: bool, cx: &mut Context<Self>) {
        settings::update(settings::SavePolicy::Immediate, cx, |settings| {
            let mut preferences = settings.skill_completion(harness);
            if dollar {
                preferences.dollar = !preferences.dollar;
            } else {
                preferences.separate_from_slash = !preferences.separate_from_slash;
            }
            settings
                .skill_completion_by_harness
                .insert(harness, preferences);
        });
        cx.notify();
    }

    pub(super) fn render_completion_for(
        &self,
        harness: HarnessId,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let current = settings::current(cx);
        let preferences = current.skill_completion(harness);
        let rows = [
            (true, "Use $ for skills", preferences.dollar),
            (
                false,
                "Separate / commands",
                preferences.separate_from_slash,
            ),
        ]
        .into_iter()
        .enumerate()
        .map(|(ix, (dollar, label, enabled))| {
            div()
                .id(format!("completion-{harness:?}-{dollar}"))
                .min_h(px(42.0))
                .px(px(4.0))
                .py(px(7.0))
                .when(ix > 0, |row| {
                    row.border_t_1().border_color(theme.border.opacity(0.5))
                })
                .flex()
                .flex_row()
                .items_center()
                .gap(px(12.0))
                .role(gpui::Role::Switch)
                .aria_label(format!("{harness:?}: {label}"))
                .aria_toggled(if enabled {
                    gpui::Toggled::True
                } else {
                    gpui::Toggled::False
                })
                .tab_index(0)
                .cursor_pointer()
                .focus_visible(|s| s.border_2().border_color(theme.accent))
                .on_click(
                    cx.listener(move |page, _, _, cx| page.toggle_completion(harness, dollar, cx)),
                )
                .on_key_down(cx.listener(move |page, event: &gpui::KeyDownEvent, _, cx| {
                    if !event.is_held && matches!(event.keystroke.key.as_str(), "enter" | "space") {
                        page.toggle_completion(harness, dollar, cx);
                        cx.stop_propagation();
                    }
                }))
                .child(div().flex_1().child(widgets::row_title(theme, label)))
                .child(widgets::toggle_switch(
                    theme,
                    enabled,
                    format!("completion-switch-{harness:?}-{dollar}"),
                ))
        });
        div().flex().flex_col().children(rows).into_any_element()
    }
}

#[cfg(test)]
mod completion_tests {
    use super::*;

    #[gpui::test]
    fn completion_preferences_save_independently_per_agent(cx: &mut gpui::TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        cx.update(|cx| settings::init(Default::default(), dir.path(), cx));
        let state = cx.new(|_| crate::state::AppState::new());
        let page = cx.new(|cx| HarnessesPage::new(state, cx));
        page.update(cx, |page, cx| {
            page.toggle_completion(HarnessId::ClaudeCode, true, cx);
            page.toggle_completion(HarnessId::ClaudeCode, false, cx);
            page.toggle_completion(HarnessId::Opencode, true, cx);
        });
        let loaded = settings::UiSettings::load(dir.path());
        assert!(loaded.skill_completion(HarnessId::ClaudeCode).dollar);
        assert!(
            loaded
                .skill_completion(HarnessId::ClaudeCode)
                .separate_from_slash
        );
        assert!(loaded.skill_completion(HarnessId::Opencode).dollar);
        assert!(
            loaded
                .skill_completion_by_harness
                .contains_key(&HarnessId::ClaudeCode)
        );
    }
}

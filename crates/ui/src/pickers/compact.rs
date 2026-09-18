//! Alternate presentation of the same model and option mutations.
use super::*;

impl Pickers {
    pub(super) fn render_compact_model_row(
        &mut self,
        ix: usize,
        row: &ModelRowData,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).for_popup();
        let selected = self.effective_harness(cx) == Some(row.harness)
            && self
                .selected_model(cx)
                .is_some_and(|model| model.id == row.model.id);
        let favorite = self.defaults.is_favorite(row.harness, &row.model.id);
        let (icon, tint) = harness_brand_icon(row.harness);
        let harness = row.harness;
        let model = row.model.id.clone();
        let label: SharedString = row.model.label.clone().into();
        // Provider attribution remains available for catalogs with repeated model names.
        let subtitle: SharedString = row
            .model
            .description
            .as_deref()
            .filter(|description| !description.eq_ignore_ascii_case(&row.harness_name))
            .map(|description| format!("{} · {description}", row.harness_name))
            .unwrap_or_else(|| row.harness_name.to_string())
            .into();
        let item = div()
            .id(("model-row", ix))
            .h(px(48.0))
            .px(px(8.0))
            .py(px(6.0))
            .rounded(px(popover::MENU_ITEM_RADIUS))
            .flex()
            .items_center()
            .gap(px(10.0))
            .cursor_pointer()
            .text_color(theme.text)
            .when(selected, |el| el.bg(crate::theme::card_selected_bg()))
            .when(!selected && self.active == ix, |el| {
                el.bg(crate::theme::ink(0.05))
            })
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered && this.active != ix {
                    this.active = ix;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this, _, _, cx| this.activate_model_index(ix, cx)))
            .child(
                crate::icons::icon(icon)
                    .size(px(17.0))
                    .text_color(tint.unwrap_or(theme.text_muted)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(div().truncate().child(label))
                    .child(
                        div()
                            .truncate()
                            .text_size(crate::typography::ui_rems(11.0))
                            .text_color(theme.text_muted)
                            .child(subtitle),
                    ),
            )
            .child(div().size(px(14.0)).when(selected, |el| {
                el.child(
                    crate::icons::icon(crate::icons::CHECK)
                        .size(px(14.0))
                        .text_color(theme.accent),
                )
            }))
            .child(
                div()
                    .id(("model-star", ix))
                    .role(gpui::Role::Button)
                    .aria_label("Toggle favorite model")
                    .size(px(22.0))
                    .rounded(px(6.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .hover(|s| s.bg(crate::theme::ink(0.08)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.toggle_model_favorite(harness, &model, cx);
                    }))
                    .child(
                        crate::icons::icon(if favorite {
                            crate::icons::STAR_BOLD
                        } else {
                            crate::icons::STAR
                        })
                        .size(px(13.0))
                        .text_color(if favorite {
                            theme.accent
                        } else {
                            theme.text_muted
                        }),
                    ),
            );
        div()
            .pb(px(popover::MENU_GAP))
            .child(item)
            .into_any_element()
    }

    pub(super) fn compact_model_picker(&self, cx: &App) -> bool {
        crate::settings::compact_model_picker(cx)
    }

    pub(super) fn show_compact_models(&mut self, cx: &mut Context<Self>) {
        self.setting_menu = None;
        self.compact_model_list = true;
        self.model_rail = ModelRail::All;
        self.active = self.selected_model_index(cx);
        self.model_scroll
            .scroll_to_item(self.active, gpui::ScrollStrategy::Nearest);
        self.focus_on_mount = true;
        cx.notify();
    }

    pub(super) fn compact_model_back_header(&self, cx: &mut Context<Self>) -> gpui::Div {
        let theme = Theme::of(cx).for_popup();
        div()
            .h(px(40.0))
            .flex_none()
            .px(px(popover::CARD_INSET))
            .flex()
            .items_center()
            .child(
                popover::menu_row(&theme, false, "compact-model-back")
                    .id("compact-model-back")
                    .role(gpui::Role::Button)
                    .aria_label("Back to effort")
                    .w_full()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.compact_model_list = false;
                        this.focus_on_mount = true;
                        cx.notify();
                    }))
                    .child(
                        crate::icons::icon(crate::icons::ALT_ARROW_LEFT)
                            .size(px(14.0))
                            .text_color(theme.text_muted),
                    )
                    .child("Select model"),
            )
    }

    pub(super) fn compact_fast_choice(&self, cx: &App) -> Option<(String, String, bool, bool)> {
        let option = self
            .selected_model(cx)?
            .options
            .iter()
            .find(|o| o.id == "serviceTier")?;
        if !option.choices.iter().any(|c| c.id == "fast") || option.default_choice == "fast" {
            return None;
        }
        let options = self.explicit_options(cx);
        let fast = options
            .get(&option.id)
            .and_then(|v| v.as_str())
            .unwrap_or(&option.default_choice)
            == "fast";
        Some((
            option.id.clone(),
            if fast {
                option.default_choice.clone()
            } else {
                "fast".into()
            },
            fast,
            fast,
        ))
    }

    pub(super) fn reset_compact_options(&mut self, cx: &mut Context<Self>) {
        // One mutation for an existing thread; retain the selected model.
        if self.harness_locked(cx) {
            self.update_chat_config(cx, |config| {
                config.reasoning = None;
                config.model_options.clear();
            });
        } else {
            self.config.reasoning = None;
            self.defaults.reasoning = None;
            if let (Some(harness), Some(model)) = (
                self.effective_harness(cx),
                self.selected_model(cx).map(|m| m.id.clone()),
            ) {
                self.defaults.model_options_mut(harness, &model).clear();
            }
            self.save_defaults();
        }
        cx.notify();
    }

    pub(super) fn pick_effort_at(&mut self, x: gpui::Pixels, cx: &mut Context<Self>) {
        let Some(bounds) = self.effort_bounds else {
            return;
        };
        let levels = self.trait_ladder(cx);
        let Some(index) = effort_index(
            f32::from(x - bounds.left()),
            f32::from(bounds.size.width),
            levels.len(),
        ) else {
            return;
        };
        if self.effective_reasoning(cx) != Some(levels[index]) {
            self.pick_reasoning(levels[index], cx);
        }
    }

    pub(super) fn compact_panel_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let levels = self.trait_ladder(cx);
        let index = levels
            .iter()
            .position(|level| Some(*level) == self.effective_reasoning(cx))
            .unwrap_or(0);
        match event.keystroke.key.as_str() {
            "escape" => self.animate_close(cx),
            "enter" => {
                let base = self.model_rows_len(cx);
                if let Some(group) = self.active.checked_sub(base).and_then(|i| {
                    self.setting_groups(cx)
                        .get(i)
                        .filter(|g| g.id != ModelSetting::Reasoning)
                        .map(|g| g.id.clone())
                }) {
                    self.open_setting(group, cx);
                } else {
                    self.show_compact_models(cx);
                }
            }
            "up" | "down" => {
                let base = self.model_rows_len(cx);
                let indices: Vec<_> = self
                    .setting_groups(cx)
                    .iter()
                    .enumerate()
                    .filter(|(_, group)| group.id != ModelSetting::Reasoning)
                    .map(|(i, _)| base + i)
                    .collect();
                let current = indices.iter().position(|i| *i == self.active);
                let next = popover::menu_step(
                    current,
                    indices.len(),
                    if event.keystroke.key == "up" { -1 } else { 1 },
                );
                if let Some(next) = next {
                    self.active = indices[next];
                }
            }
            "left" | "right" | "home" | "end" if !levels.is_empty() => {
                let next = match event.keystroke.key.as_str() {
                    "left" => index.saturating_sub(1),
                    "right" => (index + 1).min(levels.len() - 1),
                    "home" => 0,
                    _ => levels.len() - 1,
                };
                self.pick_reasoning(levels[next], cx);
            }
            _ => return,
        }
        cx.notify();
        cx.stop_propagation();
    }

    pub(super) fn render_compact_model_panel(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).for_popup();
        let levels = self.trait_ladder(cx);
        let selected = levels
            .iter()
            .position(|l| Some(*l) == self.effective_reasoning(cx))
            .unwrap_or(0);
        let label: SharedString = self
            .selected_model(cx)
            .map(|m| m.label.clone())
            .unwrap_or_else(|| "Select model".into())
            .into();
        let effort: SharedString = self
            .effective_reasoning(cx)
            .filter(|_| !levels.is_empty())
            .map(reasoning_label)
            .unwrap_or("Select model")
            .into();
        let mut header = div().flex().items_center().gap(px(4.0));
        if let Some((option, choice, default, fast)) = self.compact_fast_choice(cx) {
            header = header.child(
                popover::menu_row(&theme, fast, "compact-fast")
                    .id("compact-fast")
                    .role(gpui::Role::Button)
                    .aria_label("Toggle fast mode")
                    .size(px(32.0))
                    .p(px(8.0))
                    .text_color(if fast { theme.accent } else { theme.text_muted })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.pick_option(option.clone(), choice.clone(), default, cx)
                    }))
                    .child(
                        crate::icons::icon(crate::icons::FAST_TIER)
                            .size(px(16.0))
                            .text_color(if fast { theme.accent } else { theme.text_muted }),
                    ),
            );
        }
        header = header
            .child(
                div()
                    .id("compact-select-model")
                    .role(gpui::Role::Button)
                    .aria_label("Select model")
                    .flex_1()
                    .min_w_0()
                    .rounded(px(popover::MENU_ITEM_RADIUS))
                    .py(px(6.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(crate::theme::ink(0.05)))
                    .on_click(cx.listener(|this, _, _, cx| this.show_compact_models(cx)))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .gap(px(4.0))
                            .text_color(theme.accent)
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(effort)
                            .child(
                                crate::icons::icon(crate::icons::ALT_ARROW_RIGHT)
                                    .size(px(12.0))
                                    .text_color(theme.accent),
                            ),
                    )
                    .child(
                        div()
                            .text_center()
                            .truncate()
                            .text_size(crate::typography::ui_rems(11.0))
                            .text_color(theme.text_muted)
                            .child(label),
                    ),
            )
            .child(
                popover::menu_row(&theme, false, "compact-reset")
                    .id("compact-reset")
                    .role(gpui::Role::Button)
                    .aria_label("Reset model options")
                    .size(px(32.0))
                    .p(px(8.0))
                    .on_click(cx.listener(|this, _, _, cx| this.reset_compact_options(cx)))
                    .child(
                        crate::icons::icon(crate::icons::RESTART)
                            .size(px(16.0))
                            .text_color(theme.text_muted),
                    ),
            );
        let mut panel = div().flex().flex_col().gap(px(4.0)).child(header);
        if !levels.is_empty() {
            let fraction = if levels.len() > 1 {
                selected as f32 / (levels.len() - 1) as f32
            } else {
                0.0
            };
            let entity = cx.entity().downgrade();
            let drag_entity = entity.clone();
            let slider = div()
                .id("compact-effort-slider")
                .role(gpui::Role::Slider)
                .aria_label("Reasoning effort")
                .aria_value(SharedString::from(
                    levels
                        .get(selected)
                        .copied()
                        .map(reasoning_label)
                        .unwrap_or("Default"),
                ))
                .relative()
                .h(px(36.0))
                .mx(px(8.0))
                .my(px(4.0))
                .rounded_full()
                .bg(crate::theme::ink(0.10))
                .cursor_pointer()
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                        window.focus(&this.focus, cx);
                        this.effort_dragging = true;
                        this.pick_effort_at(event.position.x, cx);
                        cx.stop_propagation();
                    }),
                )
                .child(
                    gpui::canvas(
                        move |bounds, _, cx| {
                            let _ = entity.update(cx, |this, _| this.effort_bounds = Some(bounds));
                        },
                        move |_, _, window, _| {
                            let release = drag_entity.clone();
                            window.on_mouse_event(move |_: &gpui::MouseUpEvent, phase, _, cx| {
                                if phase == gpui::DispatchPhase::Bubble {
                                    let _ =
                                        release.update(cx, |this, _| this.effort_dragging = false);
                                }
                            });
                            let entity = drag_entity.clone();
                            window.on_mouse_event(
                                move |event: &gpui::MouseMoveEvent, phase, _, cx| {
                                    if phase == gpui::DispatchPhase::Bubble
                                        && event.pressed_button == Some(gpui::MouseButton::Left)
                                    {
                                        let _ = entity.update(cx, |this, cx| {
                                            if this.effort_dragging {
                                                this.pick_effort_at(event.position.x, cx);
                                            }
                                        });
                                    }
                                },
                            );
                        },
                    )
                    .absolute()
                    .inset_0(),
                )
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .rounded_full()
                        .overflow_hidden()
                        .child(
                            div()
                                .h_full()
                                .w(gpui::relative(0.06 + fraction * 0.88))
                                .rounded_full()
                                .bg(theme.accent.opacity(0.65)),
                        ),
                )
                .child(
                    div()
                        .absolute()
                        .left(px(18.0))
                        .right(px(18.0))
                        .top(px(15.0))
                        .h(px(6.0))
                        .flex()
                        .justify_between()
                        .children(levels.iter().map(|_| {
                            div()
                                .size(px(5.0))
                                .rounded_full()
                                .bg(theme.text.opacity(0.35))
                        })),
                )
                .child(
                    div()
                        .absolute()
                        .left(px(18.0))
                        .right(px(18.0))
                        .top(px(3.0))
                        .child(
                            div()
                                .absolute()
                                .left(gpui::relative(fraction))
                                .ml(px(-15.0))
                                .size(px(30.0))
                                .rounded_full()
                                .bg(theme.text),
                        ),
                );
            panel = panel.child(slider);
        }
        let options = self.render_traits_sections(cx);
        let scrollbar = popover::rail(self, "compact-options-scrollbar", &theme, cx);
        panel
            .child(
                popover::menu_scroll_host("compact-options-host")
                    .on_hover(cx.listener(Self::on_menu_list_hover))
                    .child(popover::faded_menu_list(
                        &self.menu_scroll,
                        popover::menu_scroll_list("compact-options", &self.menu_scroll)
                            .max_h(px((self.menu_geometry().height - 112.0).max(30.0)))
                            .child(options),
                    ))
                    .children(scrollbar),
            )
            .into_any_element()
    }
}

/// Slider stops include both endpoints and only advertised reasoning levels.
fn effort_index(x: f32, width: f32, count: usize) -> Option<usize> {
    (count > 0).then(|| {
        (((x - 18.0) / (width - 36.0).max(1.0)).clamp(0.0, 1.0) * count.saturating_sub(1) as f32)
            .round() as usize
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effort_stops_clamp_and_round_to_advertised_levels() {
        assert_eq!(effort_index(100.0, 200.0, 0), None);
        assert_eq!(effort_index(100.0, 200.0, 1), Some(0));
        assert_eq!(effort_index(-20.0, 200.0, 5), Some(0));
        assert_eq!(effort_index(100.0, 200.0, 5), Some(2));
        assert_eq!(effort_index(250.0, 200.0, 5), Some(4));
    }
}

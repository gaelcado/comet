//! Temporary setup geometry. Never persist intermediate animation frames as
//! the user's workspace geometry.
use gpui::{App, Pixels, Size, Window, px, size};
use std::time::Instant;

use super::OnboardingStep;

pub(crate) fn preferred_size(step: OnboardingStep) -> Size<Pixels> {
    size(
        px(620.0),
        px(match step {
            OnboardingStep::Workspace | OnboardingStep::Project | OnboardingStep::FirstSession => {
                480.0
            }
            OnboardingStep::Appearance => 600.0,
            OnboardingStep::Harnesses | OnboardingStep::Defaults => 680.0,
            OnboardingStep::Titles => 560.0,
        }),
    )
}

#[derive(Default)]
pub(crate) struct WindowSizing {
    restore: Option<Size<Pixels>>,
    target: Option<Size<Pixels>>,
    transition: Option<(Size<Pixels>, Instant)>,
    step: Option<OnboardingStep>,
}

impl WindowSizing {
    pub(crate) fn start(&mut self, restore: Size<Pixels>, step: OnboardingStep) {
        self.restore = Some(restore);
        self.target = Some(preferred_size(step));
        self.step = Some(step);
    }

    pub(crate) fn owns_geometry(&self) -> bool {
        self.restore.is_some()
    }

    pub(crate) fn update(
        &mut self,
        step: Option<OnboardingStep>,
        window: &mut Window,
        cx: &App,
    ) -> bool {
        // Do not fight the OS fullscreen animation or alter fullscreen bounds.
        if window.is_fullscreen() {
            return false;
        }
        let current = window.viewport_size();
        if self.step != step {
            if let Some(step) = step {
                self.restore.get_or_insert(current);
                self.target = Some(preferred_size(step));
            } else {
                self.target = self.restore;
            }
            // Retarget from the actual presented size, including an interrupted
            // transition or a manual resize. No jump to an obsolete endpoint.
            self.transition = self.target.map(|_| (current, Instant::now()));
            self.step = step;
        }
        let Some(target) = self.target else {
            return false;
        };
        let Some((from, started)) = self.transition else {
            return false;
        };
        let target = window.display(cx).map_or(target, |display| {
            let visible = display.visible_bounds().size;
            size(
                target.width.min(visible.width),
                target.height.min(visible.height),
            )
        });
        let duration = crate::motion::RESIZE
            .total()
            .mul_f32(crate::motion::speed_scale());
        let progress = if cx.reduce_motion() {
            1.0
        } else {
            (started.elapsed().as_secs_f32() / duration.as_secs_f32()).min(1.0)
        };
        let eased = crate::motion::RESIZE.progress(progress);
        let next = size(
            px(crate::motion::lerp(
                from.width.into(),
                target.width.into(),
                eased,
            )),
            px(crate::motion::lerp(
                from.height.into(),
                target.height.into(),
                eased,
            )),
        );
        resize(window, next, step.is_some() || progress < 1.0, cx);
        if progress < 1.0 {
            true
        } else {
            self.transition = None;
            if step.is_none() {
                self.restore = None;
                self.target = None;
            }
            false
        }
    }
}

fn resize(window: &mut Window, next: Size<Pixels>, compact: bool, cx: &App) {
    // Test windows deliberately panic when asked for a native handle.
    if cx
        .background_executor()
        .scheduler_executor()
        .scheduler()
        .as_test()
        .is_some()
    {
        window.resize(next);
        return;
    }
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::NSView;
        use objc2_foundation::NSSize;
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        if let Ok(handle) = window.window_handle()
            && let RawWindowHandle::AppKit(handle) = handle.as_raw()
        {
            // GPUI owns this NSView and calls render on the main thread. Keep
            // the native frame centered while allowing the setup's smaller min.
            let view = unsafe { &*handle.ns_view.as_ptr().cast::<NSView>() };
            if let Some(native) = view.window() {
                // AppKit sends resize notifications synchronously. Follow
                // GPUI's native resize path and run after the current render
                // releases its window borrow.
                cx.foreground_executor()
                    .spawn(async move {
                        native.setContentMinSize(if compact {
                            NSSize::new(520.0, 440.0)
                        } else {
                            NSSize::new(900.0, 600.0)
                        });
                        let old = native.frame();
                        let mut frame =
                            native.frameRectForContentRect(objc2_foundation::NSRect::new(
                                old.origin,
                                NSSize::new(
                                    f32::from(next.width) as f64,
                                    f32::from(next.height) as f64,
                                ),
                            ));
                        frame.origin.x = old.origin.x + (old.size.width - frame.size.width) / 2.0;
                        frame.origin.y = old.origin.y + (old.size.height - frame.size.height) / 2.0;
                        if let Some(screen) = native.screen() {
                            let visible = screen.visibleFrame();
                            frame.origin.x = frame.origin.x.clamp(
                                visible.origin.x,
                                (visible.origin.x + visible.size.width - frame.size.width)
                                    .max(visible.origin.x),
                            );
                            frame.origin.y = frame.origin.y.clamp(
                                visible.origin.y,
                                (visible.origin.y + visible.size.height - frame.size.height)
                                    .max(visible.origin.y),
                            );
                        }
                        native.setFrame_display(frame, false);
                    })
                    .detach();
                return;
            }
        }
    }
    let _ = compact;
    window.resize(next);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn onboarding_resize_retargets_and_restores_without_saving_intermediate_geometry(
        cx: &mut gpui::TestAppContext,
    ) {
        let cx = cx.add_empty_window();
        let original = size(px(1320.0), px(880.0));
        cx.simulate_resize(original);
        let mut sizing = WindowSizing::default();
        cx.update(|window, cx| {
            cx.set_reduce_motion(false);
            sizing.update(Some(OnboardingStep::Workspace), window, cx);
            assert!(sizing.owns_geometry());
            assert_eq!(sizing.restore, Some(original));

            // Sample a presented frame halfway through the collapse, then
            // reverse direction. The new transition must start at that frame.
            sizing.transition.as_mut().unwrap().1 =
                Instant::now() - crate::motion::RESIZE.total() / 2;
            sizing.update(Some(OnboardingStep::Workspace), window, cx);
            window.bounds_changed(cx);
            let midway = window.viewport_size();
            assert!(midway.width > px(560.0) && midway.width < original.width);
            sizing.update(Some(OnboardingStep::Harnesses), window, cx);
            assert_eq!(sizing.transition.unwrap().0, midway);
            assert_eq!(sizing.restore, Some(original));

            cx.set_reduce_motion(true);
            sizing.update(Some(OnboardingStep::Harnesses), window, cx);
            window.bounds_changed(cx);
            assert_eq!(
                window.viewport_size(),
                preferred_size(OnboardingStep::Harnesses)
            );
            assert!(sizing.transition.is_none());
            assert!(sizing.owns_geometry());

            sizing.update(None, window, cx);
            window.bounds_changed(cx);
            assert_eq!(window.viewport_size(), original);
            assert!(!sizing.owns_geometry());
            assert!(sizing.transition.is_none());
        });
    }
}

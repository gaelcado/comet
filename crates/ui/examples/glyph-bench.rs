//! Untracked: a workbench for the voice glyph (`zeron_ui::voice::glyph`).
//! One state at a time, chosen at the top, shown on every surface the glyph
//! lives in: the stage orb (the pointer lights it), the call strip, the
//! dictation track and the footer's miniature. Alive at 30 fps.
//!
//!   cargo run -p zeron-ui --features appshots-fixture --example glyph-bench
//!   ZERON_PALETTE_LIGHT=1 …                      # the light theme
//!   … glyph-bench /tmp/out                       # a still per mode
use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use gpui::{
    AppContext, Bounds, Context, InteractiveElement, IntoElement, ParentElement, Render,
    SharedString, StatefulInteractiveElement, Styled, Window, WindowBounds, WindowOptions, div,
    px, size,
};
use zeron_orb::glyph::{Beam, Field, Grid, Mode, Outline, Rhythm, Rings, Touch, Voice};
use gpui::prelude::FluentBuilder;
use zeron_ui::voice::glyph;
use zeron_ui::workbench;
use zeron_ui::*;

/// The situations a call is in, each a mode and who speaks in it: the
/// user's turns, Codex's turns, what both share, and the dictation track's
/// own two.
#[derive(Clone, Copy, PartialEq)]
enum Situation {
    Connecting,
    Idle,
    YouSlow,
    YouFast,
    YouMuted,
    CodexSlow,
    CodexFast,
    CodexThinks,
    CodexAsks,
    Both,
    DictationListening,
    DictationProcessing,
}

const SITUATIONS: [(Situation, &str); 12] = [
    (Situation::Connecting, "Connecting"),
    (Situation::Idle, "Live, nobody speaking"),
    (Situation::YouSlow, "You speak, slowly"),
    (Situation::YouFast, "You speak, fast"),
    (Situation::YouMuted, "You speak, muted"),
    (Situation::CodexSlow, "Codex speaks, slowly"),
    (Situation::CodexFast, "Codex speaks, fast"),
    (Situation::CodexThinks, "Codex works"),
    (Situation::CodexAsks, "Codex asks"),
    (Situation::Both, "Both at once"),
    (Situation::DictationListening, "Dictation · listening"),
    (Situation::DictationProcessing, "Dictation · transcribing"),
];

/// The modes a capture walks, Codex speaking slowly where a voice fits.
const MODES: [(Situation, &str); 6] = [
    (Situation::Connecting, "Connecting"),
    (Situation::Idle, "Listening"),
    (Situation::CodexThinks, "Thinking"),
    (Situation::CodexAsks, "Asking"),
    (Situation::DictationProcessing, "Processing"),
    (Situation::YouMuted, "Muted"),
];

/// Slow speech: words a little over one a second, with troughs between.
fn slow(t: f32) -> f32 {
    let word = (t * 1.2).fract();
    if word < 0.55 {
        0.35 + 0.4 * (word / 0.55 * std::f32::consts::PI).sin()
    } else {
        0.0
    }
}

/// Fast, continuous speech: loud throughout, six syllables a second with
/// no troughs between words, each a little louder or softer than the last.
fn fast(t: f32) -> f32 {
    0.5 + 0.22 * (t * 38.0).sin() + 0.1 * (t * 17.0).cos()
}

impl Situation {
    fn mode(self) -> Mode {
        match self {
            Situation::Connecting => Mode::Connecting,
            Situation::YouSlow | Situation::YouFast | Situation::CodexSlow | Situation::CodexFast | Situation::Both => Mode::Listening,
            Situation::Idle => Mode::Listening,
            Situation::YouMuted => Mode::Muted,
            Situation::CodexThinks => Mode::Thinking,
            Situation::CodexAsks => Mode::Asking,
            Situation::DictationListening => Mode::Listening,
            Situation::DictationProcessing => Mode::Processing,
        }
    }

    /// The two voices' peaks (user, agent) at `t`.
    fn peaks(self, t: f32) -> (f32, f32) {
        match self {
            Situation::YouSlow | Situation::YouMuted | Situation::DictationListening => (slow(t), 0.0),
            Situation::YouFast => (fast(t), 0.0),
            Situation::CodexSlow => (0.0, slow(t)),
            Situation::CodexFast => (0.0, fast(t)),
            Situation::Both => (slow(t + 0.4), slow(t)),
            Situation::Connecting
            | Situation::Idle
            | Situation::CodexThinks
            | Situation::CodexAsks
            | Situation::DictationProcessing => (0.0, 0.0),
        }
    }

    /// Dictation lives on the track alone; a call on everything but it.
    fn dictation(self) -> bool {
        matches!(self, Situation::DictationListening | Situation::DictationProcessing)
    }
}

/// A scripted passage: Thinking, Asking 2 s in, Listening 0.3 s after that
/// (mid-passage), then back to the chosen mode after 5 s.
const PASSAGE: [(f32, Mode); 3] = [
    (0.0, Mode::Thinking),
    (2.0, Mode::Asking),
    (2.3, Mode::Listening),
];

const ORB: (usize, f32) = (44, 3.0);

struct Bench {
    situation: Situation,
    passage: Option<Instant>,
    started: Instant,
    /// The stage's field, the beams' one beam, and the voices' rhythms.
    field: Field,
    beam: Beam,
    user: Rhythm,
    agent: Rhythm,
    /// Where the orb was last laid out, for the pointer over it.
    orb_origin: Rc<Cell<(f32, f32)>>,
    /// The pointer on the orb, from its corner, and when it last moved.
    touch: Option<(f32, f32, Instant)>,
    /// How the glyph is drawn (`None`: follow the background's effect), and
    /// the background effect the stage's artwork is shown in.
    style: Option<glyph::Style>,
    effect: workbench::NewThreadBackgroundEffect,
}

impl Bench {
    fn new() -> Self {
        Self {
            situation: Situation::CodexSlow,
            passage: None,
            started: Instant::now(),
            field: Field::new(Rings::Round)
                .soft_rimmed()
                .falling(0.78)
                .resting(0.45),
            beam: Beam::default(),
            user: Rhythm::default(),
            agent: Rhythm::default(),
            orb_origin: Rc::new(Cell::new((0.0, 0.0))),
            touch: None,
            style: Some(glyph::Style::Dots),
            effect: workbench::NewThreadBackgroundEffect::None,
        }
    }

    /// The mode shown now: the passage's while one runs.
    fn shown(&self) -> Mode {
        let Some(since) = self.passage else {
            return self.situation.mode();
        };
        let at = since.elapsed().as_secs_f32();
        if at > 5.0 {
            return self.situation.mode();
        }
        PASSAGE
            .iter()
            .rev()
            .find(|(from, _)| at >= *from)
            .map_or(self.situation.mode(), |(_, mode)| *mode)
    }

    fn tick(&mut self) {
        let now = Instant::now();
        let t = self.started.elapsed().as_secs_f32();
        let (user, agent) = self.situation.peaks(t);
        self.user.hear(user, now);
        self.agent.hear(agent, now);
        let voice = Voice {
            user: self.user.bands(),
            agent: self.agent.bands(),
        };
        let mode = self.shown();
        self.field.hear(voice, now);
        self.beam.hear(voice, now);
        self.field.shift(mode);
        self.beam.shift(mode);
    }

    /// The style shown: the chosen one, or the background effect's twin.
    fn glyph_style(&self) -> glyph::Style {
        use workbench::NewThreadBackgroundEffect as E;
        self.style.unwrap_or(match self.effect {
            E::None => glyph::Style::Dots,
            E::Dither => glyph::Style::Dither,
            E::Ascii => glyph::Style::Ascii,
            E::Halftone => glyph::Style::Halftone,
            E::Scanlines => glyph::Style::Scanlines,
        })
    }

    fn orb_touch(&self) -> Option<Touch> {
        let (x, y, at) = self.touch?;
        let strength = Touch::strength_after(at.elapsed().as_secs_f32());
        (strength > 0.0).then_some(Touch { x, y, strength })
    }
}

impl Render for Bench {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = theme::Theme::of(cx).clone();
        let inks = glyph::inks(&theme);
        let mode = self.shown();
        let style = self.glyph_style();
        let font: SharedString = theme.font_mono.clone().into();
        let (dots, dot) = style.resolution(ORB);
        let grid = Grid::dense(dot);
        let (room, _) = grid.extent(dots, dots);
        let light = matches!(theme.appearance, theme::Appearance::Light);
        let glass = theme.is_frost();

        let chip = |id: SharedString, label: &'static str, on: bool| {
            div()
                .id(id)
                .px(px(10.0))
                .py(px(5.0))
                .rounded(px(8.0))
                .text_size(px(12.0))
                .cursor_pointer()
                .bg(if on {
                    theme.accent_wash
                } else {
                    theme.glass_hover()
                })
                .text_color(if on { theme.text } else { theme.text_muted })
                .child(SharedString::from(label))
        };
        let row = || div().flex().flex_row().flex_wrap().gap(px(6.0));
        let situations = row()
            .children(SITUATIONS.iter().map(|(it, label)| {
                let it = *it;
                chip(format!("s-{label}").into(), label, self.situation == it && self.passage.is_none())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.situation = it;
                        this.passage = None;
                        cx.notify();
                    }))
            }))
            .child(
                chip("passage".into(), "Codex works → asks → listens ▶", self.passage.is_some())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.passage = Some(Instant::now());
                        cx.notify();
                    })),
            );
        let styles = row()
            .children(glyph::Style::ALL.iter().map(|st| {
                let st = *st;
                chip(format!("g-{}", st.label()).into(), st.label(), self.style == Some(st)).on_click(
                    cx.listener(move |this, _, _, cx| {
                        this.style = Some(st);
                        cx.notify();
                    }),
                )
            }))
            .child(
                chip("g-follow".into(), "Follow the background", self.style.is_none()).on_click(
                    cx.listener(|this, _, _, cx| {
                        this.style = None;
                        cx.notify();
                    }),
                ),
            );
        let effects = row().children(workbench::NewThreadBackgroundEffect::ALL.iter().map(|e| {
            let e = *e;
            chip(format!("e-{}", e.label()).into(), e.label(), self.effect == e).on_click(
                cx.listener(move |this, _, _, cx| {
                    this.effect = e;
                    cx.notify();
                }),
            )
        }));
        let looks = row()
            .child(chip("t-dark".into(), "Dark", !light).on_click(cx.listener(|_, _, _, cx| {
                appearance::set_mode(appearance::AppearanceMode::Dark, cx);
            })))
            .child(chip("t-light".into(), "Light", light).on_click(cx.listener(|_, _, _, cx| {
                appearance::set_mode(appearance::AppearanceMode::Light, cx);
            })))
            .child(div().w(px(12.0)))
            .child(chip("m-glass".into(), "Glass", glass).on_click(cx.listener(|_, _, _, cx| {
                appearance::set_surface(zeron_theme::SurfacePreference::Frosted, cx);
            })))
            .child(chip("m-solid".into(), "Solid", !glass).on_click(cx.listener(|_, _, _, cx| {
                appearance::set_surface(zeron_theme::SurfacePreference::Opaque, cx);
            })));
        let caption = |text: &'static str| {
            div()
                .text_size(px(11.0))
                .text_color(theme.text_faint)
                .child(SharedString::from(text))
        };
        let labelled = |label: &'static str, controls: gpui::Div| {
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(12.0))
                .child(div().w(px(110.0)).flex_none().child(caption(label)))
                .child(controls)
        };

        // The stage: the new-thread artwork in the chosen effect, the orb
        // in the composer's disc over it. The pointer lights the dots.
        let origin = self.orb_origin.clone();
        let orb = div()
            .id("orb")
            .relative()
            .size(px(room))
            .rounded_full()
            .border_1()
            .border_color(theme.composer_surface_border())
            .bg(theme.composer_surface_bg())
            .when(!glass, |el| el.shadow_lg())
            .flex()
            .items_center()
            .justify_center()
            .on_mouse_move(cx.listener(move |this, event: &gpui::MouseMoveEvent, _, cx| {
                let (ox, oy) = this.orb_origin.get();
                let (x, y) = (f32::from(event.position.x) - ox, f32::from(event.position.y) - oy);
                if (x - room / 2.0).hypot(y - room / 2.0) <= room / 2.0 {
                    this.touch = Some((x, y, Instant::now()));
                    cx.notify();
                }
            }))
            .child(
                gpui::canvas(
                    move |bounds, _, _| {
                        origin.set((f32::from(bounds.origin.x), f32::from(bounds.origin.y)));
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .child(glyph::field_styled(
                &self.field,
                dots,
                dots,
                grid,
                mode,
                &inks,
                Outline::Disc,
                self.orb_touch(),
                style,
                font.clone(),
            ));
        let effect = self.effect;
        let stage = div()
            .relative()
            .w(px(620.0))
            .h(px(420.0))
            .flex_none()
            .rounded(px(16.0))
            .overflow_hidden()
            .border_1()
            .border_color(theme.border)
            .when(!glass, |el| el.bg(theme.bg))
            .child(
                gpui::canvas(|_, _, _| {}, move |bounds, _, window, cx| {
                    let mut top = bounds;
                    top.size.height = top.size.height * 0.72;
                    workbench::paint_artwork(effect, top, window, cx);
                })
                .absolute()
                .inset_0(),
            )
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(orb),
            );
        // Each surface as the app holds it: the beam fills its container to
        // the border, with no padding of its own.
        let beam = |width: f32, height: f32, dot: f32, outline: Outline| {
            let dot = match style {
                glyph::Style::Ascii => dot * 2.6,
                glyph::Style::Dither | glyph::Style::Scanlines => dot * 1.1,
                _ => dot,
            };
            glyph::beam_styled(&self.beam, Grid::dense(dot), mode, inks, outline, style, font.clone())
                .w(px(width))
                .h(px(height))
                .flex_none()
                .into_any_element()
        };
        // The call strip: the queue's tray, round on top, the composer's
        // edge under it.
        let strip = |width: f32| {
            div()
                .flex()
                .flex_col()
                .child(
                    div()
                        .rounded_t(px(16.0))
                        .border_1()
                        .border_b_0()
                        .border_color(theme.border)
                        .bg(theme.input_glass_bg())
                        .overflow_hidden()
                        .child(beam(width, 30.0, glyph::BEAM_DOT, Outline::Over(15.0))),
                )
                .child(div().w(px(width + 2.0)).h(px(1.0)).bg(theme.border))
        };
        let track = |width: f32| {
            div()
                .rounded_full()
                .border_1()
                .border_color(theme.border)
                .bg(theme.input_glass_bg())
                .overflow_hidden()
                .child(beam(width, 28.0, glyph::BEAM_DOT, Outline::Stadium))
        };
        let surfaces = if self.situation.dictation() {
            div()
                .flex()
                .flex_col()
                .items_start()
                .gap(px(8.0))
                .child(caption("Dictation track 600×28"))
                .child(track(600.0))
                .child(caption("Narrow 320"))
                .child(track(320.0))
        } else {
            div()
                .flex()
                .flex_col()
                .items_start()
                .gap(px(8.0))
                .child(caption("Call strip 560 wide, 30 above the composer"))
                .child(strip(560.0))
                .child(caption("Narrow 320"))
                .child(strip(320.0))
                .child(caption("Footer 22×11"))
                .child(beam(22.0, 11.0, 1.4, Outline::Stadium))
        };
        div()
            .size_full()
            .when(!glass, |el| el.bg(theme.bg))
            .p(px(24.0))
            .pt(px(40.0))
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(labelled("Situation", situations))
            .child(labelled("Glyph", styles))
            .child(labelled("Background", effects))
            .child(labelled("Look", looks))
            .child(
                div()
                    .mt(px(12.0))
                    .flex()
                    .flex_row()
                    .gap(px(32.0))
                    .items_start()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .child(caption("Stage · the wallpaper in its effect, the orb over it"))
                            .child(stage),
                    )
                    .child(surfaces),
            )
    }
}

fn main() -> anyhow::Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    let _guard = runtime.enter();
    tracing_subscriber::fmt().with_env_filter("warn").init();
    let output = std::env::args().nth(1).map(std::path::PathBuf::from);
    if let Some(output) = &output {
        std::fs::create_dir_all(output)?;
    }
    let temp = tempfile::tempdir()?;
    let data = temp.path().to_path_buf();
    gpui_platform::application().with_assets(icons::Assets).run(move |cx| {
        gpui_tokio::init(cx);
        gpui_base::init(cx);
        let home = std::env::var("HOME").unwrap();
        let mut settings: settings::UiSettings =
            std::fs::read_to_string(format!("{home}/.zeron/ui-settings.json"))
                .ok()
                .and_then(|text| serde_json::from_str(&text).ok())
                .unwrap_or_default();
        settings.reduce_motion = motion::ReduceMotion::Off;
        settings.surface = zeron_theme::SurfacePreference::Frosted;
        settings.save(&data).unwrap();
        settings::init(settings.clone(), data.clone(), cx);
        let fonts = typography::register_fonts(cx);
        typography::init(
            settings.ui_font_family.clone(),
            settings.ui_font_size,
            settings.terminal_font_family.clone(),
            settings.terminal_font_size,
            settings.code_font_family.clone(),
            settings.code_font_size,
            fonts,
            cx,
        );
        theme_library::init(data.clone(), cx);
        appearance::init(
            if std::env::var_os("ZERON_PALETTE_LIGHT").is_some() {
                appearance::AppearanceMode::Light
            } else {
                appearance::AppearanceMode::Dark
            },
            settings.theme_selection,
            settings.accent,
            settings.surface,
            cx,
        );
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        gpui::point(px(12.), px(30.)),
                        size(px(1440.), px(760.)),
                    ))),
                    // Glass is the window's own blur showing through, as in the app.
                    window_background: gpui::WindowBackgroundAppearance::Blurred,
                    ..Default::default()
                },
                |window, cx| {
                    appearance::observe_window(window, cx).detach();
                    cx.new(|cx| {
                        cx.spawn(async move |this, cx| {
                            loop {
                                cx.background_executor()
                                    .timer(std::time::Duration::from_millis(33))
                                    .await;
                                if this
                                    .update(cx, |bench: &mut Bench, cx| {
                                        bench.tick();
                                        cx.notify();
                                    })
                                    .is_err()
                                {
                                    break;
                                }
                            }
                        })
                        .detach();
                        Bench::new()
                    })
                },
            )
            .unwrap();
        let Some(output) = output.clone() else {
            return;
        };
        // A still per situation, two seconds into each, and a second one
        // 150 ms later (`-b`): the difference between them is its motion.
        let handle: gpui::AnyWindowHandle = window.into();
        let probe = std::env::var_os("BENCH_MOTION").is_some();
        cx.spawn(async move |cx| {
            let walk: Vec<(Situation, &str)> = if probe {
                SITUATIONS.to_vec()
            } else {
                MODES.to_vec()
            };
            // `BENCH_STYLES=1`: one still per glyph style, Codex speaking, each
            // over the background effect it would follow.
            let styles = std::env::var_os("BENCH_STYLES").is_some();
            let walk: Vec<(Situation, &str)> = if styles {
                glyph::Style::ALL.iter().map(|s| (Situation::CodexFast, s.label())).collect()
            } else {
                walk
            };
            for (index, (situation, label)) in walk.into_iter().enumerate() {
                window
                    .update(cx, |bench, _, _| {
                        bench.situation = situation;
                        bench.passage = None;
                        if styles {
                            bench.style = None;
                            bench.effect = workbench::NewThreadBackgroundEffect::ALL
                                [[0usize, 3, 1, 2, 4][index]];
                        }
                    })
                    .ok();
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(2000))
                    .await;
                let file = label.replace([' ', '·', ','], "-");
                handle
                    .update(cx, |_, window, cx| {
                        window.draw(cx).clear();
                        window
                            .render_to_image()
                            .expect("render")
                            .save(output.join(format!("{file}.png")))
                            .expect("save");
                    })
                    .ok();
                if probe {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(150))
                        .await;
                    handle
                        .update(cx, |_, window, cx| {
                            window.draw(cx).clear();
                            window
                                .render_to_image()
                                .expect("render")
                                .save(output.join(format!("{file}-b.png")))
                                .expect("save");
                        })
                        .ok();
                }
            }
            cx.update(|cx| cx.quit());
        })
        .detach();
    });
    Ok(())
}

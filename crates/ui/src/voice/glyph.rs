//! The voice glyph, painted through GPUI: the shared engine
//! ([`zeron_orb::glyph`]) decides every dot, this module lays them out and
//! lights them. Fields that stand alone are sized in dots; a beam fills the
//! box it is given, counting its dots at paint time, so it always reaches
//! that box's edges.
use gpui::{
    App, Bounds, Hsla, IntoElement, Pixels, Rgba, SharedString, Styled, Window, point, px, size,
};
pub use zeron_orb::glyph::{
    BANDS, Beam, Cell, Field, Grid, Inks, Mode, Outline, Rings, Skip, Touch, Voice,
};
use zeron_orb::glyph::{Rgb, look};

use crate::theme::Theme;

/// The dots of every beam: the dictation track's, the call strip's and the
/// footer's alike, so the one voice reads the same wherever it runs.
pub const BEAM_DOT: f32 = 2.4;

fn rgb(colour: Hsla) -> Rgb {
    let Rgba { r, g, b, .. } = colour.into();
    [r, g, b]
}

fn hsla([r, g, b]: Rgb, a: f32) -> Hsla {
    Rgba { r, g, b, a }.into()
}

/// Codex in sunrise, the user in the theme's accent.
pub fn inks(theme: &Theme) -> Inks {
    Inks::new(theme.glyph.rows().map(rgb))
}

/// Lights `cells` laid out on `grid` from `origin`: one quad a dot, its
/// light told by colour alone, so even a large field stays cheap.
fn paint(window: &mut Window, origin: gpui::Point<Pixels>, grid: Grid, cells: &[Cell]) {
    let pitch = grid.pitch();
    // Every dot lands on whole device pixels, its centre and its size alike:
    // a dot straddling pixels differently from its neighbour reads as a
    // second, slower pattern over the grid (moiré), not as light.
    let scale = window.scale_factor().max(1.0);
    let snap = |v: f32| (v * scale).round() / scale;
    for cell in cells {
        let look = look(cell);
        if look.alpha < 0.01 {
            continue;
        }
        let across = snap(grid.dot * look.size).max(1.0 / scale);
        let centre = (
            f32::from(origin.x) + cell.col * pitch + grid.dot / 2.0,
            f32::from(origin.y) + cell.row * pitch + grid.dot / 2.0,
        );
        let at = Bounds::new(
            point(
                px(snap(centre.0 - across / 2.0)),
                px(snap(centre.1 - across / 2.0)),
            ),
            size(px(across), px(across)),
        );
        window
            .paint_quad(gpui::fill(at, hsla(look.fill, look.alpha)).corner_radii(px(across / 2.0)));
    }
}

/// How the glyph's cells are drawn. The engine decides each cell's light,
/// ink and presence the same way for all of them; only the mark changes,
/// so a style can follow the new-thread background's effect.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Style {
    /// Round dots that swell as they light: the default.
    #[default]
    Dots,
    /// A print's screen: dots on rows offset by half a step, from a speck at
    /// rest to touching when lit, the paper showing between them.
    Halftone,
    /// Square pixels, on or off against a 4×4 Bayer threshold: light is how
    /// many are on, never how bright one is.
    Dither,
    /// Characters from a density ramp, denser as they light.
    Ascii,
    /// Horizontal display lines, each row one bar lit along its length.
    Scanlines,
}

impl Style {
    pub const ALL: [Self; 5] = [
        Self::Dots,
        Self::Halftone,
        Self::Dither,
        Self::Ascii,
        Self::Scanlines,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Dots => "Dots",
            Self::Halftone => "Halftone",
            Self::Dither => "Dither",
            Self::Ascii => "ASCII",
            Self::Scanlines => "Scanlines",
        }
    }

    /// The grid this style reads best at, for a field `dots` across on dots
    /// of `dot`: characters need room, pixels and lines want more of them.
    pub fn resolution(self, (dots, dot): (usize, f32)) -> (usize, f32) {
        let span = dots as f32 * Grid::dense(dot).pitch();
        let at = |pitch: f32| {
            let n = (span / pitch).round().max(1.0) as usize;
            // `Grid::dense` makes a pitch of dot × 4/3.
            (n, pitch * 0.75)
        };
        match self {
            Self::Dots | Self::Halftone => (dots, dot),
            Self::Dither => at(3.0),
            Self::Ascii => at(7.0),
            Self::Scanlines => at(4.0),
        }
    }
}

/// The density ramp ASCII draws with, sparsest first.
const RAMP: &[u8] = b".:-=+*#%@";
/// The 4×4 Bayer threshold matrix, in sixteenths.
const BAYER: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

/// Lights `cells` on `grid` from `origin` in `style`. ASCII shapes one line
/// per row in `font`; every other style is quads.
fn paint_styled(
    window: &mut Window,
    cx: &mut App,
    origin: gpui::Point<Pixels>,
    grid: Grid,
    cells: &[Cell],
    style: Style,
    font: &SharedString,
) {
    let pitch = grid.pitch();
    let scale = window.scale_factor().max(1.0);
    let snap = |v: f32| (v * scale).round() / scale;
    let (ox, oy) = (f32::from(origin.x), f32::from(origin.y));
    let square = |window: &mut Window, x: f32, y: f32, w: f32, h: f32, colour: Hsla| {
        window.paint_quad(gpui::fill(
            Bounds::new(
                point(px(snap(x)), px(snap(y))),
                size(px(snap(w).max(1.0 / scale)), px(snap(h).max(1.0 / scale))),
            ),
            colour,
        ));
    };
    match style {
        Style::Dots => paint(window, origin, grid, cells),
        Style::Halftone => {
            for cell in cells {
                let look = look(cell);
                let level = cell.level.clamp(0.0, 1.0);
                // A speck at rest, touching its neighbours fully lit.
                let across = pitch * (0.16 + 1.12 * level.sqrt());
                // The paper's own specks stay readable at rest.
                let alpha = look.alpha.max(0.22 * cell.presence);
                if alpha < 0.01 {
                    continue;
                }
                let shift = if cell.row as i32 % 2 == 1 {
                    pitch / 2.0
                } else {
                    0.0
                };
                let centre = (
                    ox + cell.col * pitch + shift + grid.dot / 2.0,
                    oy + cell.row * pitch + grid.dot / 2.0,
                );
                let across = snap(across).max(1.0 / scale);
                window.paint_quad(
                    gpui::fill(
                        Bounds::new(
                            point(
                                px(snap(centre.0 - across / 2.0)),
                                px(snap(centre.1 - across / 2.0)),
                            ),
                            size(px(across), px(across)),
                        ),
                        hsla(look.fill, alpha),
                    )
                    .corner_radii(px(across / 2.0)),
                );
            }
        }
        Style::Dither => {
            for cell in cells {
                let look = look(cell);
                let threshold =
                    (BAYER[cell.row as usize % 4][cell.col as usize % 4] as f32 + 0.5) / 16.0;
                let on = cell.level > threshold;
                // Off pixels keep the grid's own rest, faint; on ones are ink.
                let alpha = if on { 1.0 } else { look.alpha * 0.55 };
                if alpha < 0.01 {
                    continue;
                }
                let colour = hsla(look.fill, alpha * cell.presence.max(on as u8 as f32));
                square(
                    window,
                    ox + cell.col * pitch,
                    oy + cell.row * pitch,
                    pitch - 0.5,
                    pitch - 0.5,
                    colour,
                );
            }
        }
        Style::Scanlines => {
            for cell in cells {
                let look = look(cell);
                if look.alpha < 0.01 {
                    continue;
                }
                // One bar a row: lit cells join into a line along it.
                let thick = pitch * (0.34 + 0.26 * cell.level.clamp(0.0, 1.0));
                square(
                    window,
                    ox + cell.col * pitch,
                    oy + cell.row * pitch + (pitch - thick) / 2.0,
                    pitch,
                    thick,
                    hsla(look.fill, look.alpha),
                );
            }
        }
        Style::Ascii => {
            let font = gpui::font(font.clone());
            let font_size = px(pitch * 1.3);
            let mut rows: std::collections::BTreeMap<i32, Vec<&Cell>> = Default::default();
            for cell in cells {
                rows.entry(cell.row as i32).or_default().push(cell);
            }
            for (row, mut line) in rows {
                line.sort_by(|a, b| a.col.total_cmp(&b.col));
                for cell in line {
                    let look = look(cell);
                    if look.alpha < 0.02 {
                        continue;
                    }
                    let level = cell.level.clamp(0.0, 1.0);
                    let index = ((level.sqrt() * (RAMP.len() - 1) as f32).round() as usize)
                        .min(RAMP.len() - 1);
                    let text: SharedString = (RAMP[index] as char).to_string().into();
                    let run = gpui::TextRun {
                        len: 1,
                        font: font.clone(),
                        color: hsla(look.fill, look.alpha),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    let shaped = window
                        .text_system()
                        .shape_line(text, font_size, &[run], None);
                    let width = f32::from(shaped.width);
                    let at = point(
                        px(snap(ox + cell.col * pitch + (pitch - width) / 2.0)),
                        px(snap(oy + row as f32 * pitch - pitch * 0.12)),
                    );
                    let _ =
                        shaped.paint(at, px(pitch * 1.2), gpui::TextAlign::Left, None, window, cx);
                }
            }
        }
    }
}

/// [`field`] drawn in `style`, its characters in `font` for ASCII.
#[allow(clippy::too_many_arguments)]
pub fn field_styled(
    field: &Field,
    cols: usize,
    rows: usize,
    grid: Grid,
    mode: Mode,
    inks: &Inks,
    outline: Outline,
    touch: Option<Touch>,
    style: Style,
    font: SharedString,
) -> impl IntoElement {
    let (w, h) = grid.extent(cols, rows);
    let cells = field.cells_touched(cols, rows, grid, mode, inks, outline, &[], touch);
    gpui::canvas(
        |_, _, _| {},
        move |bounds, _, window, cx| {
            paint_styled(window, cx, bounds.origin, grid, &cells, style, &font)
        },
    )
    .w(px(w))
    .h(px(h))
    .flex_none()
}

/// [`beam`] drawn in `style`.
pub fn beam_styled(
    beam: &Beam,
    grid: Grid,
    mode: Mode,
    inks: Inks,
    outline: Outline,
    style: Style,
    font: SharedString,
) -> gpui::Canvas<()> {
    let beam = beam.clone();
    let outline = match outline {
        Outline::Rounded(radius) => Outline::Rounded(radius / grid.pitch()),
        Outline::Under(radius) => Outline::Under(radius / grid.pitch()),
        Outline::Over(radius) => Outline::Over(radius / grid.pitch()),
        other => other,
    };
    gpui::canvas(
        |_, _, _| {},
        move |bounds, _, window, cx| {
            let (width, height) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
            let ((cols, rows), (mx, my)) = grid.cover(width, height);
            let area = (width / grid.pitch(), height / grid.pitch());
            let cells = beam.cells_over(cols, rows, area, mode, &inks, outline);
            window.paint_quad(gpui::fill(bounds, gpui::transparent_black()));
            paint_styled(
                window,
                cx,
                bounds.origin + point(px(mx), px(my)),
                grid,
                &cells,
                style,
                &font,
            );
        },
    )
}

/// A field `cols` × `rows` dots on `grid`, inside `outline`, with a
/// pointer's `touch` on it (in points from its corner) if there is one.
#[allow(clippy::too_many_arguments)]
pub fn field(
    field: &Field,
    cols: usize,
    rows: usize,
    grid: Grid,
    mode: Mode,
    inks: &Inks,
    outline: Outline,
    touch: Option<Touch>,
) -> impl IntoElement {
    let (w, h) = grid.extent(cols, rows);
    let cells = field.cells_touched(cols, rows, grid, mode, inks, outline, &[], touch);
    gpui::canvas(
        |_, _, _| {},
        move |bounds, _, window, _| paint(window, bounds.origin, grid, &cells),
    )
    .w(px(w))
    .h(px(h))
    .flex_none()
}

/// A matrix filling its box: `build` is asked for the dots once the box is
/// known, given how many fit and the rooms `skip` leaves, which are measured
/// from the box's corner and moved here onto the matrix's first dot.
fn filling(
    grid: Grid,
    skip: Vec<Skip>,
    build: impl FnOnce(usize, usize, &[Skip]) -> Vec<Cell> + 'static,
) -> gpui::Canvas<()> {
    gpui::canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let ((cols, rows), (mx, my)) =
                grid.fit(f32::from(bounds.size.width), f32::from(bounds.size.height));
            let skip: Vec<_> = skip.iter().map(|room| room.shifted(-mx, -my)).collect();
            let cells = build(cols, rows, &skip);
            paint(window, bounds.origin + point(px(mx), px(my)), grid, &cells);
        },
    )
}

/// A beam filling its box inside `outline`, whose radii are in points: it
/// meets the edges of what holds it and fades where it touches them.
pub fn beam(beam: &Beam, grid: Grid, mode: Mode, inks: Inks, outline: Outline) -> gpui::Canvas<()> {
    let beam = beam.clone();
    let outline = match outline {
        Outline::Rounded(radius) => Outline::Rounded(radius / grid.pitch()),
        Outline::Under(radius) => Outline::Under(radius / grid.pitch()),
        Outline::Over(radius) => Outline::Over(radius / grid.pitch()),
        other => other,
    };
    // The matrix covers the box and runs past it: the box's own outline is
    // where the dots fade out, so the beam fills it to its curves.
    gpui::canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let (width, height) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
            let ((cols, rows), (mx, my)) = grid.cover(width, height);
            let area = (width / grid.pitch(), height / grid.pitch());
            let cells = beam.cells_over(cols, rows, area, mode, &inks, outline);
            window.paint_quad(gpui::fill(bounds, gpui::transparent_black()));
            paint(window, bounds.origin + point(px(mx), px(my)), grid, &cells);
        },
    )
}

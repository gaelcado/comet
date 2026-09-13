//! Non-destructive treatments for the optional new-thread hero artwork.

use std::path::{Path, PathBuf};

use gpui::{
    AnyElement, BorderStyle, Empty, IntoElement, Pixels, SharedString, TextRun, div, prelude::*, px,
};

use crate::settings::NewThreadBackgroundEffect;
use crate::theme::Theme;

const ASCII_FONT_SIZE: f32 = 6.0;
const ASCII_LINE_HEIGHT: f32 = 8.0;

#[derive(Debug)]
struct BackgroundLuminance {
    width: u32,
    height: u32,
    pixels: Box<[u8]>,
    colors: Box<[[u8; 4]]>,
    dither: std::sync::OnceLock<Option<std::sync::Arc<gpui::Image>>>,
}

impl BackgroundLuminance {
    fn dither_image(&self) -> Option<std::sync::Arc<gpui::Image>> {
        self.dither
            .get_or_init(|| {
                const BAYER: [[u8; 4]; 4] =
                    [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];
                let pixels = image::RgbaImage::from_fn(self.width, self.height, |x, y| {
                    let [r, g, b, a] = self.colors[(y * self.width + x) as usize];
                    let threshold = BAYER[y as usize % 4][x as usize % 4];
                    image::Rgba([
                        quantize(r, threshold),
                        quantize(g, threshold),
                        quantize(b, threshold),
                        a,
                    ])
                });
                let mut bytes = std::io::Cursor::new(Vec::new());
                pixels.write_to(&mut bytes, image::ImageFormat::Png).ok()?;
                Some(std::sync::Arc::new(gpui::Image::from_bytes(
                    gpui::ImageFormat::Png,
                    bytes.into_inner(),
                )))
            })
            .clone()
    }

    fn sample_cover(&self, bounds: gpui::Size<Pixels>, x: f32, y: f32) -> u8 {
        self.pixels[self.cover_index(bounds, x, y)]
    }

    fn color_cover(&self, bounds: gpui::Size<Pixels>, x: f32, y: f32) -> gpui::Hsla {
        let [r, g, b, a] = self.colors[self.cover_index(bounds, x, y)];
        gpui::rgba(u32::from_be_bytes([r, g, b, a])).into()
    }

    fn cover_index(&self, bounds: gpui::Size<Pixels>, x: f32, y: f32) -> usize {
        let width = f32::from(bounds.width).max(1.0);
        let height = f32::from(bounds.height).max(1.0);
        let source_width = self.width as f32;
        let source_height = self.height as f32;
        let scale = (width / source_width).max(height / source_height);
        let visible_width = width / scale;
        let visible_height = height / scale;
        let source_x = ((source_width - visible_width) * 0.5 + x / scale)
            .clamp(0.0, source_width - 1.0) as u32;
        let source_y = ((source_height - visible_height) * 0.5 + y / scale)
            .clamp(0.0, source_height - 1.0) as u32;
        (source_y * self.width + source_x) as usize
    }
}

fn needs_luminance(effect: NewThreadBackgroundEffect) -> bool {
    matches!(
        effect,
        NewThreadBackgroundEffect::Dither
            | NewThreadBackgroundEffect::Ascii
            | NewThreadBackgroundEffect::Halftone
    )
}

fn background_luminance(path: &Path) -> Option<std::sync::Arc<BackgroundLuminance>> {
    type Cache = Vec<(PathBuf, std::sync::Arc<BackgroundLuminance>)>;
    static CACHE: std::sync::OnceLock<std::sync::Mutex<Cache>> = std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(|| std::sync::Mutex::new(Vec::new()));
    if let Some(sample) = cache
        .lock()
        .ok()?
        .iter()
        .find_map(|(cached, sample)| (cached == path).then(|| sample.clone()))
    {
        return Some(sample);
    }

    // Bound the retained color/luminance proxy independently of window size.
    let decoded = image::ImageReader::open(path).ok()?.decode().ok()?;
    let proxy = decoded.thumbnail(2048, 2048);
    let gray = proxy.to_luma8();
    let sample = std::sync::Arc::new(BackgroundLuminance {
        width: gray.width(),
        height: gray.height(),
        pixels: gray.into_raw().into_boxed_slice(),
        colors: proxy.to_rgba8().pixels().map(|pixel| pixel.0).collect(),
        dither: Default::default(),
    });
    let mut cache = cache.lock().ok()?;
    cache.push((path.to_path_buf(), sample.clone()));
    if cache.len() > 4 {
        cache.remove(0);
    }
    Some(sample)
}

/// Returns the image opacity and optional texture layer as one resolved
/// treatment. Unsupported raster decoding falls back to the original image.
pub(super) fn treatment(
    requested: NewThreadBackgroundEffect,
    theme: &Theme,
    path: &Path,
    base_opacity: f32,
) -> (f32, AnyElement) {
    let luminance = needs_luminance(requested)
        .then(|| background_luminance(path))
        .flatten();
    let effect = if needs_luminance(requested) && luminance.is_none() {
        NewThreadBackgroundEffect::None
    } else {
        requested
    };
    let image_opacity = base_opacity
        * match effect {
            NewThreadBackgroundEffect::None => 1.0,
            NewThreadBackgroundEffect::Dither => 0.0,
            NewThreadBackgroundEffect::Ascii => 0.0,
            NewThreadBackgroundEffect::Halftone => 0.0,
            NewThreadBackgroundEffect::Scanlines => 1.0,
        };
    if effect == NewThreadBackgroundEffect::None {
        return (image_opacity, Empty.into_any_element());
    }
    // Transform once, not hundreds of thousands of canvas quads on every
    // animation frame. The processed raster follows the original cover crop.
    if effect == NewThreadBackgroundEffect::Dither {
        return match luminance.as_ref().and_then(|source| source.dither_image()) {
            Some(image) => (
                0.0,
                gpui::img(image)
                    .absolute()
                    .inset_0()
                    .size_full()
                    .object_fit(gpui::ObjectFit::Cover)
                    .opacity(base_opacity)
                    .into_any_element(),
            ),
            None => (base_opacity, Empty.into_any_element()),
        };
    }

    let color = gpui::white();
    let ascii_font = theme.font_mono.clone();
    let prepaint_luminance = luminance.clone();
    let texture = gpui::canvas(
        move |bounds, window, _| {
            if effect != NewThreadBackgroundEffect::Ascii {
                return Vec::new();
            }
            let Some(luminance) = prepaint_luminance.as_ref() else {
                return Vec::new();
            };
            let font = gpui::font(ascii_font.clone());
            // Font size is not glyph advance. Using it as cell width made
            // the rendered ASCII field stop halfway across the artwork and
            // compressed its source sampling into the wrong horizontal span.
            let cell_width = ascii_cell_width(window, &font, color);
            let columns = (f32::from(bounds.size.width) / cell_width).ceil() as usize + 1;
            let rows = (f32::from(bounds.size.height) / ASCII_LINE_HEIGHT).ceil() as usize + 1;
            let ramp = b" .:-=+*#%@";
            (0..rows)
                .map(|row| {
                    let mut text = String::with_capacity(columns);
                    let mut runs = Vec::with_capacity(columns);
                    for column in 0..columns {
                        let luma = luminance.sample_cover(
                            bounds.size,
                            (column as f32 + 0.5) * cell_width,
                            (row as f32 + 0.5) * ASCII_LINE_HEIGHT,
                        );
                        let index =
                            ((luma as f32 / 255.0).sqrt() * (ramp.len() - 1) as f32) as usize;
                        text.push(ramp[index] as char);
                        runs.push(TextRun {
                            len: 1,
                            font: font.clone(),
                            color: luminance.color_cover(
                                bounds.size,
                                (column as f32 + 0.5) * cell_width,
                                (row as f32 + 0.5) * ASCII_LINE_HEIGHT,
                            ),
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        });
                    }
                    let text: SharedString = text.into();
                    window
                        .text_system()
                        .shape_line(text, px(ASCII_FONT_SIZE), &runs, None)
                })
                .collect::<Vec<_>>()
        },
        move |bounds, ascii_lines, window, cx| match effect {
            NewThreadBackgroundEffect::None => {}
            NewThreadBackgroundEffect::Dither => {}
            NewThreadBackgroundEffect::Ascii => {
                let line_height = px(ASCII_LINE_HEIGHT);
                for (row, line) in ascii_lines.iter().enumerate() {
                    let _ = line.paint(
                        gpui::point(bounds.left(), bounds.top() + line_height * row as f32),
                        line_height,
                        gpui::TextAlign::Left,
                        Some(bounds.size.width),
                        window,
                        cx,
                    );
                }
            }
            NewThreadBackgroundEffect::Halftone => {
                let step = 4.0;
                let columns = (f32::from(bounds.size.width) / step).ceil() as usize;
                let rows = (f32::from(bounds.size.height) / step).ceil() as usize;
                for row in 0..rows {
                    for column in 0..columns {
                        let Some(luminance) = luminance.as_ref() else {
                            continue;
                        };
                        let luma = luminance.sample_cover(
                            bounds.size,
                            column as f32 * step,
                            row as f32 * step,
                        );
                        let dot = step * (0.3 + 0.7 * (luma as f32 / 255.0).sqrt());
                        let color = luminance.color_cover(
                            bounds.size,
                            (column as f32 + 0.5) * step,
                            (row as f32 + 0.5) * step,
                        );
                        paint_dot(
                            window,
                            bounds,
                            column as f32 * step + (step - dot) * 0.5,
                            row as f32 * step + (step - dot) * 0.5,
                            dot,
                            color,
                        );
                    }
                }
            }
            NewThreadBackgroundEffect::Scanlines => {
                let rows = (f32::from(bounds.size.height) / 3.0).ceil() as usize;
                for row in 0..rows {
                    window.paint_quad(gpui::quad(
                        gpui::Bounds::new(
                            gpui::point(bounds.left(), bounds.top() + px(row as f32 * 3.0)),
                            gpui::size(bounds.size.width, px(1.0)),
                        ),
                        px(0.0),
                        gpui::black().opacity(0.48),
                        px(0.0),
                        gpui::transparent_black(),
                        BorderStyle::default(),
                    ));
                }
            }
        },
    )
    .absolute()
    .inset_0();

    let layer = div()
        .absolute()
        .inset_0()
        .opacity(base_opacity)
        .when(
            matches!(
                effect,
                NewThreadBackgroundEffect::Ascii | NewThreadBackgroundEffect::Halftone
            ),
            |layer| layer.bg(gpui::black()),
        )
        .child(texture)
        .into_any_element();
    (image_opacity, layer)
}

// Four levels per channel, with a centered ordered threshold: actual color
// quantization rather than a disconnected stipple over the original photograph.
fn quantize(value: u8, threshold: u8) -> u8 {
    let level = (value as f32 / 85.0 + (threshold as f32 + 0.5) / 16.0 - 0.5)
        .round()
        .clamp(0.0, 3.0);
    (level * 85.0) as u8
}

fn ascii_cell_width(window: &gpui::Window, font: &gpui::Font, color: gpui::Hsla) -> f32 {
    let run = TextRun {
        len: 1,
        font: font.clone(),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let probe = window
        .text_system()
        .shape_line("M".into(), px(ASCII_FONT_SIZE), &[run], None);
    f32::from(probe.width).max(1.0)
}

fn paint_dot(
    window: &mut gpui::Window,
    bounds: gpui::Bounds<Pixels>,
    x: f32,
    y: f32,
    diameter: f32,
    color: gpui::Hsla,
) {
    window.paint_quad(gpui::quad(
        gpui::Bounds::new(
            gpui::point(bounds.left() + px(x), bounds.top() + px(y)),
            gpui::size(px(diameter), px(diameter)),
        ),
        px(diameter / 2.0),
        color,
        px(0.0),
        gpui::transparent_black(),
        BorderStyle::default(),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordered_palette_preserves_endpoints_and_distributes_midtones() {
        for threshold in 0..16 {
            assert_eq!(quantize(0, threshold), 0);
            assert_eq!(quantize(255, threshold), 255);
            for value in 0..=255 {
                assert_eq!(quantize(value, threshold) % 85, 0);
            }
        }
        let levels: Vec<_> = (0..16).map(|threshold| quantize(128, threshold)).collect();
        assert!(levels.contains(&85));
        assert!(levels.contains(&170));
        let average = levels.iter().map(|&v| v as f32).sum::<f32>() / 16.0;
        assert!((average - 128.0).abs() < 6.0);
    }

    #[test]
    fn processed_artwork_is_reused_across_frames() {
        let sample = BackgroundLuminance {
            width: 4,
            height: 4,
            pixels: vec![128; 16].into_boxed_slice(),
            colors: vec![[180, 100, 220, 255]; 16].into_boxed_slice(),
            dither: Default::default(),
        };
        let first = sample.dither_image().unwrap();
        assert!(std::sync::Arc::ptr_eq(
            &first,
            &sample.dither_image().unwrap()
        ));
    }

    #[gpui::test]
    fn ascii_advance_covers_narrow_and_fullscreen_artwork(cx: &mut gpui::TestAppContext) {
        struct Fixture;
        impl gpui::Render for Fixture {
            fn render(
                &mut self,
                _: &mut gpui::Window,
                _: &mut gpui::Context<Self>,
            ) -> impl IntoElement {
                div()
            }
        }
        let handle = cx.add_window(|_, _| Fixture);
        cx.update_window(handle.into(), |_, window, _| {
            let font = gpui::font("Menlo");
            let color = gpui::white();
            let advance = ascii_cell_width(window, &font, color);
            for width in [320.0, 768.0, 2560.0] {
                let columns = (width / advance).ceil() as usize + 1;
                let run = TextRun {
                    len: columns,
                    font: font.clone(),
                    color,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let line = window.text_system().shape_line(
                    "M".repeat(columns).into(),
                    px(ASCII_FONT_SIZE),
                    &[run],
                    None,
                );
                let painted_width = f32::from(line.width);
                assert!(painted_width >= width, "pattern stopped before {width}px");
                assert!(painted_width < width + 2.0 * advance + 0.1);
            }
        })
        .unwrap();
    }

    #[test]
    fn cover_sampling_crops_the_long_axis_from_the_center() {
        let sample = BackgroundLuminance {
            width: 4,
            height: 2,
            pixels: vec![0, 1, 2, 3, 10, 11, 12, 13].into_boxed_slice(),
            colors: vec![[0, 0, 0, 255]; 8].into_boxed_slice(),
            dither: Default::default(),
        };
        let square = gpui::size(px(100.0), px(100.0));
        assert_eq!(sample.sample_cover(square, 0.0, 0.0), 1);
        assert_eq!(sample.sample_cover(square, 99.0, 99.0), 12);
    }

    #[test]
    fn adaptive_effects_are_the_only_ones_that_need_pixels() {
        assert!(!needs_luminance(NewThreadBackgroundEffect::None));
        assert!(needs_luminance(NewThreadBackgroundEffect::Dither));
        assert!(needs_luminance(NewThreadBackgroundEffect::Ascii));
        assert!(needs_luminance(NewThreadBackgroundEffect::Halftone));
        assert!(!needs_luminance(NewThreadBackgroundEffect::Scanlines));
    }
}

//! Non-destructive treatments for the optional new-thread hero artwork.

use std::path::{Path, PathBuf};

use gpui::{
    AnyElement, BorderStyle, Empty, IntoElement, Pixels, SharedString, TextRun, div, prelude::*, px,
};

use crate::settings::NewThreadBackgroundEffect;
use crate::theme::Theme;

const ASCII_FONT_SIZE: f32 = 6.0;
const ASCII_LINE_HEIGHT: f32 = 8.0;
const ASCII_STRENGTH: f32 = 0.72;

type RasterKey = (u32, u32, NewThreadBackgroundEffect);

#[derive(Debug, Default)]
struct RasterCache {
    requested: Option<RasterKey>,
    running: bool,
    ready: Option<(RasterKey, std::sync::Arc<gpui::RenderImage>)>,
    // A few recent sizes also prevent different windows from continuously
    // invalidating each other's sole cached result when refreshed together.
    older: Vec<(RasterKey, std::sync::Arc<gpui::RenderImage>)>,
}

type AsciiCache = Option<(
    (u32, u32, gpui::Font),
    std::sync::Arc<Vec<gpui::ShapedLine>>,
)>;

#[derive(Debug)]
struct BackgroundLuminance {
    width: u32,
    height: u32,
    pixels: Box<[u8]>,
    colors: Box<[[u8; 4]]>,
    dither: std::sync::Mutex<RasterCache>,
    ascii: std::sync::Mutex<AsciiCache>,
}

impl BackgroundLuminance {
    fn raster_image(
        self: &std::sync::Arc<Self>,
        key: RasterKey,
        cx: &mut gpui::App,
    ) -> Option<std::sync::Arc<gpui::RenderImage>> {
        let mut cache = self.dither.lock().unwrap_or_else(|e| e.into_inner());
        cache.requested = Some(key);
        if let Some((_, image)) = cache.older.iter().find(|(old, _)| *old == key) {
            return Some(image.clone());
        }
        let image = cache
            .ready
            .as_ref()
            .filter(|(ready, _)| ready.2 == key.2)
            .map(|(_, image)| image.clone());
        if cache.running || cache.ready.as_ref().is_some_and(|(ready, _)| *ready == key) {
            return image;
        }
        cache.running = true;
        drop(cache);
        let source = self.clone();
        cx.spawn(async move |cx| {
            loop {
                let key = source.dither.lock().unwrap().requested.unwrap();
                let worker = source.clone();
                let pixels = cx
                    .background_executor()
                    .spawn(async move {
                        match key.2 {
                            NewThreadBackgroundEffect::Halftone => {
                                worker.halftone_pixels(key.0, key.1)
                            }
                            _ => worker.dither_pixels(key.0, key.1),
                        }
                    })
                    .await;
                let next = std::sync::Arc::new(gpui::RenderImage::new([image::Frame::new(pixels)]));
                let done = cx.update(|cx| {
                    let mut cache = source.dither.lock().unwrap();
                    let current = cache.requested == Some(key);
                    // Never replace a visible result with an obsolete resize.
                    if current || cache.ready.is_none() {
                        if let Some(previous) = cache.ready.replace((key, next)) {
                            cache.older.push(previous);
                            if cache.older.len() > 3 {
                                let (_, expired) = cache.older.remove(0);
                                gpui::ImageSource::Render(expired).evict(None, cx);
                            }
                        }
                        cx.refresh_windows();
                    }
                    if current {
                        cache.running = false;
                    }
                    current
                });
                if done {
                    break;
                }
            }
        })
        .detach();
        image
    }

    fn halftone_pixels(&self, width: u32, height: u32) -> image::RgbaImage {
        let bounds = gpui::size(px(width as f32), px(height as f32));
        let mut pixels = image::RgbaImage::from_pixel(width, height, image::Rgba([0, 0, 0, 255]));
        for y in (0..height).step_by(4) {
            for x in (0..width).step_by(4) {
                let luma = self.sample_cover(bounds, x as f32, y as f32);
                let radius = 2.0 * (0.3 + 0.7 * (luma as f32 / 255.0).sqrt());
                let [r, g, b, a] =
                    self.colors[self.cover_index(bounds, x as f32 + 2.0, y as f32 + 2.0)];
                for dy in 0..4.min(height - y) {
                    for dx in 0..4.min(width - x) {
                        let distance =
                            ((dx as f32 - 1.5).powi(2) + (dy as f32 - 1.5).powi(2)).sqrt();
                        let coverage = (radius + 0.5 - distance).clamp(0.0, 1.0) * a as f32 / 255.0;
                        pixels.put_pixel(
                            x + dx,
                            y + dy,
                            image::Rgba([
                                (b as f32 * coverage) as u8,
                                (g as f32 * coverage) as u8,
                                (r as f32 * coverage) as u8,
                                255,
                            ]),
                        );
                    }
                }
            }
        }
        pixels
    }

    fn dither_pixels(&self, width: u32, height: u32) -> image::RgbaImage {
        const BAYER: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];
        let bounds = gpui::size(px(width as f32), px(height as f32));
        let mut pixels = image::RgbaImage::new(width, height);
        for y in (0..height).step_by(2) {
            for x in (0..width).step_by(2) {
                // Sample/quantize once per dot, not four times per 2x2 cell.
                let index = self.cover_index(bounds, (x + 1) as f32, (y + 1) as f32);
                let [r, g, b, a] = dither_color(
                    self.colors[index],
                    BAYER[y as usize / 2 % 4][x as usize / 2 % 4],
                );
                for dy in 0..2.min(height - y) {
                    for dx in 0..2.min(width - x) {
                        // RenderImage consumes BGRA.
                        pixels.put_pixel(x + dx, y + dy, image::Rgba([b, g, r, a]));
                    }
                }
            }
        }
        pixels
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
        ascii: Default::default(),
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
    // Cache one screen-sized raster. Resizes regenerate the crop and retire
    // the previous GPU image; docking reuses it without resampling the dots.
    if matches!(
        effect,
        NewThreadBackgroundEffect::Dither | NewThreadBackgroundEffect::Halftone
    ) {
        let source = luminance.expect("decoded dither source");
        let texture = gpui::canvas(
            move |bounds, _, cx| {
                let width = f32::from(bounds.size.width).ceil().clamp(1.0, 8192.0) as u32;
                let height = f32::from(bounds.size.height).ceil().clamp(1.0, 440.0) as u32;
                source.raster_image((width, height, effect), cx)
            },
            |bounds, image, window, _| {
                if let Some(image) = image {
                    let _ = window.paint_image(bounds, gpui::Corners::default(), image, 0, false);
                }
            },
        )
        .absolute()
        .inset_0();
        return (
            0.0,
            div()
                .absolute()
                .inset_0()
                .opacity(base_opacity)
                .child(
                    gpui::img(path.to_path_buf())
                        .absolute()
                        .inset_0()
                        .size_full()
                        .object_fit(gpui::ObjectFit::Cover),
                )
                .child(texture)
                .into_any_element(),
        );
    }

    let color = gpui::white();
    let ascii_font = theme.font_mono.clone();
    let prepaint_luminance = luminance.clone();
    let texture = gpui::canvas(
        move |bounds, window, _| {
            if effect != NewThreadBackgroundEffect::Ascii {
                return std::sync::Arc::new(Vec::new());
            }
            let Some(luminance) = prepaint_luminance.as_ref() else {
                return std::sync::Arc::new(Vec::new());
            };
            let font = gpui::font(ascii_font.clone());
            // Small overscan buckets avoid reshaping for every one-pixel drag.
            // Paint remains clipped to the real bounds, including when shrinking.
            let (width, height) = ascii_bucket(bounds.size);
            let key = (width, height, font.clone());
            let mut cached = luminance.ascii.lock().unwrap_or_else(|e| e.into_inner());
            if let Some((previous, lines)) = cached.as_ref() {
                if previous == &key {
                    return lines.clone();
                }
            }
            let sample_size = gpui::size(px(width as f32), px(height as f32));
            // Font size is not glyph advance. Using it as cell width made
            // the rendered ASCII field stop halfway across the artwork and
            // compressed its source sampling into the wrong horizontal span.
            let cell_width = ascii_cell_width(window, &font, color);
            let columns = (width as f32 / cell_width).ceil() as usize + 1;
            let rows = (height as f32 / ASCII_LINE_HEIGHT).ceil() as usize + 1;
            let ramp = b" .:-=+*#%@";
            let lines = (0..rows)
                .map(|row| {
                    let mut text = String::with_capacity(columns);
                    let mut runs = Vec::with_capacity(columns);
                    for column in 0..columns {
                        let luma = luminance.sample_cover(
                            sample_size,
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
                                sample_size,
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
                .collect::<Vec<_>>();
            let lines = std::sync::Arc::new(lines);
            *cached = Some((key, lines.clone()));
            lines
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
            NewThreadBackgroundEffect::Halftone => {}
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

    let surface = div()
        .absolute()
        .inset_0()
        .when(effect == NewThreadBackgroundEffect::Ascii, |surface| {
            surface.opacity(ASCII_STRENGTH)
        })
        .when(
            matches!(
                effect,
                NewThreadBackgroundEffect::Ascii | NewThreadBackgroundEffect::Halftone
            ),
            |layer| layer.bg(gpui::black()),
        )
        .child(texture);
    // Mix the artwork and glyph treatment before applying glass transparency.
    let layer = div()
        .absolute()
        .inset_0()
        .opacity(base_opacity)
        .when(effect == NewThreadBackgroundEffect::Ascii, |layer| {
            layer.child(
                gpui::img(path.to_path_buf())
                    .absolute()
                    .inset_0()
                    .size_full()
                    .object_fit(gpui::ObjectFit::Cover),
            )
        })
        .child(surface)
        .into_any_element();
    (image_opacity, layer)
}

// Dither between a dark ink and a bright, hue-preserving source color.
// RGB-channel quantization mostly posterized the artwork and its fine Bayer
// pattern vanished when the source raster was downsampled.
fn dither_color([r, g, b, a]: [u8; 4], threshold: u8) -> [u8; 4] {
    let peak = r.max(g).max(b) as f32;
    let bright = peak / 255.0 > (threshold as f32 + 0.5) / 16.0;
    let gain = if bright { 255.0 / peak.max(1.0) } else { 0.08 };
    [
        (r as f32 * gain).round() as u8,
        (g as f32 * gain).round() as u8,
        (b as f32 * gain).round() as u8,
        a,
    ]
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

fn ascii_bucket(size: gpui::Size<Pixels>) -> (u32, u32) {
    (
        ((f32::from(size.width).max(1.0) / 32.0).ceil() as u32) * 32,
        ((f32::from(size.height).max(1.0) / 8.0).ceil() as u32) * 8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn raster_requests_coalesce_and_keep_last_frame_until_ready(cx: &mut gpui::TestAppContext) {
        let source = std::sync::Arc::new(dither_fixture());
        for effect in [
            NewThreadBackgroundEffect::Dither,
            NewThreadBackgroundEffect::Halftone,
        ] {
            cx.update(|cx| {
                for width in 320..420 {
                    let _ = source.raster_image((width, 16, effect), cx);
                }
                let cache = source.dither.lock().unwrap();
                assert!(cache.running);
                assert_eq!(cache.requested, Some((419, 16, effect)));
            });
            cx.run_until_parked();
            let first = {
                let cache = source.dither.lock().unwrap();
                assert!(!cache.running);
                assert_eq!(cache.ready.as_ref().unwrap().0, (419, 16, effect));
                cache.ready.as_ref().unwrap().1.clone()
            };
            cx.update(|cx| {
                let resized = source.raster_image((500, 16, effect), cx).unwrap();
                assert!(std::sync::Arc::ptr_eq(&first, &resized));
            });
            cx.run_until_parked();
            cx.update(|cx| {
                let latest = source.raster_image((500, 16, effect), cx).unwrap();
                assert!(!std::sync::Arc::ptr_eq(&first, &latest));
                assert!(!source.dither.lock().unwrap().running);
                let previous_size = source.raster_image((419, 16, effect), cx).unwrap();
                assert!(std::sync::Arc::ptr_eq(&first, &previous_size));
                assert!(!source.dither.lock().unwrap().running);
            });
        }
    }

    #[test]
    fn ascii_resize_buckets_bound_rebuilds_and_cover_the_viewport() {
        let mut buckets = std::collections::BTreeSet::new();
        for width in 800..1120 {
            let bucket = ascii_bucket(gpui::size(px(width as f32), px(437.0)));
            assert!(bucket.0 >= width && bucket.0 < width + 32);
            assert_eq!(bucket.1, 440);
            buckets.insert(bucket);
        }
        assert!(
            buckets.len() <= 11,
            "one-pixel resize frames should reuse shaped lines"
        );
    }

    fn dither_fixture() -> BackgroundLuminance {
        BackgroundLuminance {
            width: 4,
            height: 4,
            pixels: vec![128; 16].into_boxed_slice(),
            colors: vec![[128, 64, 32, 200]; 16].into_boxed_slice(),
            dither: Default::default(),
            ascii: Default::default(),
        }
    }

    #[test]
    fn dither_has_visible_contrast_without_changing_hue_or_alpha() {
        let dark = dither_color([128, 64, 32, 200], 15);
        let bright = dither_color([128, 64, 32, 200], 0);
        assert!(bright[0] - dark[0] > 200);
        assert_eq!(bright, [255, 128, 64, 200]);
        assert_eq!(dark[3], 200);
        for threshold in 0..16 {
            assert_eq!(dither_color([0, 0, 0, 0], threshold), [0, 0, 0, 0]);
            assert_eq!(dither_color([255, 255, 255, 255], threshold), [255; 4]);
        }
    }

    #[test]
    fn dither_cells_remain_two_pixels_across_window_sizes() {
        let source = dither_fixture();
        for width in [320, 768, 2560] {
            let pixels = source.dither_pixels(width, 8);
            assert_eq!(pixels.dimensions(), (width, 8));
            for x in (0..width).step_by(2) {
                assert_eq!(pixels.get_pixel(x, 0), pixels.get_pixel(x + 1, 0));
                assert_eq!(pixels.get_pixel(x, 0), pixels.get_pixel(x, 1));
            }
            assert_ne!(pixels.get_pixel(0, 0), pixels.get_pixel(2, 0));
            // Direct GPU uploads are BGRA, including the original alpha.
            assert_eq!(pixels.get_pixel(0, 0).0, [64, 128, 255, 200]);
        }
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
            ascii: Default::default(),
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

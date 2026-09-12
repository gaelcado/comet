//! Non-destructive treatments for the optional new-thread hero artwork.

use std::path::{Path, PathBuf};

use gpui::{
    AnyElement, BorderStyle, Empty, IntoElement, Pixels, SharedString, TextRun, div, prelude::*, px,
};

use crate::settings::NewThreadBackgroundEffect;
use crate::theme::{Appearance, Theme};

#[derive(Debug)]
struct BackgroundLuminance {
    width: u32,
    height: u32,
    pixels: Box<[u8]>,
}

impl BackgroundLuminance {
    fn sample_cover(&self, bounds: gpui::Size<Pixels>, x: f32, y: f32) -> u8 {
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
        self.pixels[(source_y * self.width + source_x) as usize]
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

    // The hero never exceeds 440px high. Keeping a 2048px luminance proxy
    // preserves more than enough detail while bounding retained memory.
    let decoded = image::ImageReader::open(path).ok()?.decode().ok()?;
    let gray = decoded.thumbnail(2048, 2048).to_luma8();
    let sample = std::sync::Arc::new(BackgroundLuminance {
        width: gray.width(),
        height: gray.height(),
        pixels: gray.into_raw().into_boxed_slice(),
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
            NewThreadBackgroundEffect::Dither => 0.94,
            NewThreadBackgroundEffect::Ascii => 0.78,
            NewThreadBackgroundEffect::Halftone => 0.90,
            NewThreadBackgroundEffect::Scanlines => 0.96,
        };
    if effect == NewThreadBackgroundEffect::None {
        return (image_opacity, Empty.into_any_element());
    }

    let color = theme.text.opacity(match effect {
        NewThreadBackgroundEffect::Dither => 0.13,
        NewThreadBackgroundEffect::Ascii => 0.26,
        NewThreadBackgroundEffect::Halftone => 0.15,
        NewThreadBackgroundEffect::Scanlines => 0.11,
        NewThreadBackgroundEffect::None => 0.0,
    });
    let light = matches!(theme.appearance, Appearance::Light);
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
            let columns = (f32::from(bounds.size.width) / 7.0).ceil() as usize + 1;
            let rows = (f32::from(bounds.size.height) / 9.0).ceil() as usize;
            let ramp = b" .:-=+*#%@";
            (0..rows)
                .map(|row| {
                    let mut text = String::with_capacity(columns);
                    for column in 0..columns {
                        let luma = luminance.sample_cover(
                            bounds.size,
                            column as f32 * 7.0,
                            row as f32 * 9.0,
                        );
                        let ink = if light { 255 - luma } else { luma };
                        let index = ink as usize * (ramp.len() - 1) / 255;
                        text.push(ramp[index] as char);
                    }
                    let text: SharedString = text.into();
                    let run = TextRun {
                        len: text.len(),
                        font: gpui::font(ascii_font.clone()),
                        color,
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    window.text_system().shape_line(text, px(7.0), &[run], None)
                })
                .collect::<Vec<_>>()
        },
        move |bounds, ascii_lines, window, cx| match effect {
            NewThreadBackgroundEffect::None => {}
            NewThreadBackgroundEffect::Dither => {
                const BAYER: [[u8; 4]; 4] =
                    [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];
                let step = 7.0;
                let columns = (f32::from(bounds.size.width) / step).ceil() as usize;
                let rows = (f32::from(bounds.size.height) / step).ceil() as usize;
                for row in 0..rows {
                    for column in 0..columns {
                        let threshold = BAYER[row % 4][column % 4];
                        let Some(luminance) = luminance.as_ref() else {
                            continue;
                        };
                        let luma = luminance.sample_cover(
                            bounds.size,
                            column as f32 * step,
                            row as f32 * step,
                        );
                        let ink = if light { 255 - luma } else { luma };
                        if ink / 16 <= threshold {
                            continue;
                        }
                        let dot = if threshold < 3 { 1.8 } else { 1.0 };
                        paint_dot(
                            window,
                            bounds,
                            column as f32 * step,
                            row as f32 * step,
                            dot,
                            color,
                        );
                    }
                }
            }
            NewThreadBackgroundEffect::Ascii => {
                let line_height = px(9.0);
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
                let step = 12.0;
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
                        let ink = if light { 255 - luma } else { luma };
                        let dot = 0.8 + ink as f32 / 255.0 * 4.2;
                        paint_dot(
                            window,
                            bounds,
                            column as f32 * step,
                            row as f32 * step,
                            dot,
                            color,
                        );
                    }
                }
            }
            NewThreadBackgroundEffect::Scanlines => {
                let rows = (f32::from(bounds.size.height) / 5.0).ceil() as usize;
                for row in 0..rows {
                    window.paint_quad(gpui::quad(
                        gpui::Bounds::new(
                            gpui::point(bounds.left(), bounds.top() + px(row as f32 * 5.0)),
                            gpui::size(bounds.size.width, px(1.0)),
                        ),
                        px(0.0),
                        color,
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
        .when(
            matches!(
                effect,
                NewThreadBackgroundEffect::Dither
                    | NewThreadBackgroundEffect::Ascii
                    | NewThreadBackgroundEffect::Halftone
            ),
            |layer| layer.bg(theme.bg.opacity(0.18)),
        )
        .child(texture)
        .into_any_element();
    (image_opacity, layer)
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

/// Deterministic dissolve grain. It requires no image decode on the first
/// navigation frame, and its cells evaporate instead of re-randomizing each
/// frame. The caller scopes this texture to the artwork's contracting mask.
pub(super) fn dissolve_grain(theme: &Theme, progress: f32) -> AnyElement {
    let color = theme.text;
    gpui::canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            const BAYER: [[u8; 4]; 4] =
                [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];
            let width = f32::from(bounds.size.width);
            let height = f32::from(bounds.size.height);
            let step = 6.0;
            let left = (width * 0.38 * progress / step).floor() as usize;
            let right = ((width - width * 0.38 * progress) / step).ceil() as usize;
            let top = (height * 0.82 * progress / step).floor() as usize;
            let bottom = (height / step).ceil() as usize;
            for row in top..bottom {
                for column in left..right {
                    let threshold = (BAYER[row % 4][column % 4] as f32 + 1.0) / 17.0;
                    let alpha = 1.0
                        - crate::composer_dock::stage(
                            progress,
                            threshold * 0.5,
                            0.5 + threshold * 0.5,
                        );
                    if alpha < 0.01 {
                        continue;
                    }
                    paint_dot(
                        window,
                        bounds,
                        column as f32 * step,
                        row as f32 * step,
                        1.0 + alpha,
                        color.opacity(0.3 * alpha),
                    );
                }
            }
        },
    )
    .absolute()
    .inset_0()
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cover_sampling_crops_the_long_axis_from_the_center() {
        let sample = BackgroundLuminance {
            width: 4,
            height: 2,
            pixels: vec![0, 1, 2, 3, 10, 11, 12, 13].into_boxed_slice(),
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

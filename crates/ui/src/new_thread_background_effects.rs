//! Effects are source-space images. Resizing only changes ObjectFit::Cover.
use crate::settings::NewThreadBackgroundEffect;
use crate::theme::Theme;
use gpui::{AnyElement, Empty, IntoElement, Pixels, prelude::*, px};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
type EffectEntry = (NewThreadBackgroundEffect, Option<Arc<gpui::RenderImage>>);
#[derive(Debug)]
struct BackgroundLuminance {
    width: u32,
    height: u32,
    pixels: Box<[u8]>,
    colors: Box<[[u8; 4]]>,
    effects: Mutex<Vec<EffectEntry>>,
}
impl BackgroundLuminance {
    fn raster_image(
        self: &Arc<Self>,
        effect: NewThreadBackgroundEffect,
        cx: &mut gpui::App,
    ) -> Option<Arc<gpui::RenderImage>> {
        let mut effects = self.effects.lock().unwrap();
        if let Some((_, image)) = effects.iter().find(|(key, _)| *key == effect) {
            return image.clone();
        }
        // None marks the single pending job for this source/effect, not a viewport.
        effects.push((effect, None));
        drop(effects);
        let source = self.clone();
        cx.spawn(async move |cx| {
            let worker = source.clone();
            let image = cx
                .background_executor()
                .spawn(async move {
                    let pixels = match effect {
                        NewThreadBackgroundEffect::Dither => {
                            worker.dither_pixels(worker.width, worker.height)
                        }
                        NewThreadBackgroundEffect::Halftone => {
                            worker.halftone_pixels(worker.width, worker.height)
                        }
                        NewThreadBackgroundEffect::Ascii => worker.ascii_pixels(),
                        _ => worker.scanline_pixels(),
                    };
                    Arc::new(gpui::RenderImage::new([image::Frame::new(pixels)]))
                })
                .await;
            cx.update(|cx| {
                if let Some((_, ready)) = source
                    .effects
                    .lock()
                    .unwrap()
                    .iter_mut()
                    .find(|(key, _)| *key == effect)
                {
                    *ready = Some(image);
                }
                cx.refresh_windows();
            });
        })
        .detach();
        None
    }
    fn scanline_pixels(&self) -> image::RgbaImage {
        image::RgbaImage::from_fn(self.width, self.height, |x, y| {
            let [r, g, b, a] = self.colors[(y * self.width + x) as usize];
            let gain = if y % 3 == 0 { 0.52 } else { 1.0 };
            image::Rgba([
                (b as f32 * gain) as u8,
                (g as f32 * gain) as u8,
                (r as f32 * gain) as u8,
                a,
            ])
        })
    }
    fn ascii_pixels(&self) -> image::RgbaImage {
        // Five-column bitmap glyphs, one column/row of spacing. These are
        // artwork pixels rather than thousands of shaped UI text runs.
        const GLYPHS: [[u8; 7]; 10] = [
            [0, 0, 0, 0, 0, 0, 0],
            [0, 0, 0, 0, 0, 4, 0],
            [0, 4, 0, 0, 4, 0, 0],
            [0, 0, 0, 14, 0, 0, 0],
            [0, 0, 14, 0, 14, 0, 0],
            [0, 4, 4, 31, 4, 4, 0],
            [0, 21, 14, 31, 14, 21, 0],
            [10, 10, 31, 10, 31, 10, 10],
            [17, 2, 4, 4, 8, 16, 17],
            [14, 17, 23, 21, 23, 16, 14],
        ];
        image::RgbaImage::from_fn(self.width, self.height, |x, y| {
            let sx = (x / 6 * 6 + 3).min(self.width - 1);
            let sy = (y / 8 * 8 + 4).min(self.height - 1);
            let sample = (sy * self.width + sx) as usize;
            let index = ((self.pixels[sample] as f32 / 255.0).sqrt() * 9.0) as usize;
            let ink =
                x % 6 < 5 && y % 8 < 7 && GLYPHS[index][y as usize % 8] & (1 << (4 - x % 6)) != 0;
            let [r, g, b, a] = self.colors[(y * self.width + x) as usize];
            let [cr, cg, cb, _] = self.colors[sample];
            let mix = |base: u8, glyph: u8| {
                (base as f32 * 0.28 + if ink { glyph as f32 * 0.72 } else { 0.0 }) as u8
            };
            image::Rgba([mix(b, cb), mix(g, cg), mix(r, cr), a])
        })
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

fn background_luminance(path: &Path) -> Option<Arc<BackgroundLuminance>> {
    type Cache = Vec<(PathBuf, Arc<BackgroundLuminance>)>;
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(Vec::new()));
    if let Some(source) = cache
        .lock()
        .ok()?
        .iter()
        .find_map(|(key, source)| (key == path).then(|| source.clone()))
    {
        return Some(source);
    }
    let proxy = image::ImageReader::open(path)
        .ok()?
        .decode()
        .ok()?
        .thumbnail(2048, 2048);
    let gray = proxy.to_luma8();
    let source = Arc::new(BackgroundLuminance {
        width: gray.width(),
        height: gray.height(),
        pixels: gray.into_raw().into_boxed_slice(),
        colors: proxy.to_rgba8().pixels().map(|pixel| pixel.0).collect(),
        effects: Mutex::new(Vec::new()),
    });
    let mut cache = cache.lock().ok()?;
    cache.push((path.to_path_buf(), source.clone()));
    if cache.len() > 4 {
        cache.remove(0);
    }
    Some(source)
}
pub(super) fn treatment(
    effect: NewThreadBackgroundEffect,
    _theme: &Theme,
    path: &Path,
    base_opacity: f32,
    cx: &mut gpui::App,
) -> (f32, AnyElement) {
    if effect == NewThreadBackgroundEffect::None {
        return (base_opacity, Empty.into_any_element());
    }
    match background_luminance(path).and_then(|source| source.raster_image(effect, cx)) {
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
    }
}
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

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Arc<BackgroundLuminance> {
        Arc::new(BackgroundLuminance {
            width: 60,
            height: 32,
            pixels: vec![128; 1920].into_boxed_slice(),
            colors: vec![[128, 64, 32, 200]; 1920].into_boxed_slice(),
            effects: Mutex::new(Vec::new()),
        })
    }
    #[gpui::test]
    fn every_effect_is_generated_once_independently_of_viewport(cx: &mut gpui::TestAppContext) {
        let source = fixture();
        for effect in [
            NewThreadBackgroundEffect::Dither,
            NewThreadBackgroundEffect::Ascii,
            NewThreadBackgroundEffect::Halftone,
            NewThreadBackgroundEffect::Scanlines,
        ] {
            cx.update(|cx| {
                for _ in 0..100 {
                    assert!(source.raster_image(effect, cx).is_none());
                }
                assert_eq!(
                    source
                        .effects
                        .lock()
                        .unwrap()
                        .iter()
                        .filter(|(key, _)| *key == effect)
                        .count(),
                    1
                );
            });
            cx.run_until_parked();
            cx.update(|cx| {
                let first = source.raster_image(effect, cx).unwrap();
                assert_eq!(first.size(0).width.0, 60);
                assert_eq!(first.size(0).height.0, 32);
                for _ in 0..100 {
                    assert!(Arc::ptr_eq(
                        &first,
                        &source.raster_image(effect, cx).unwrap()
                    ));
                }
            });
        }
    }
    #[test]
    fn raster_treatments_preserve_source_dimensions_and_alpha() {
        let source = fixture();
        for image in [
            source.dither_pixels(60, 32),
            source.ascii_pixels(),
            source.scanline_pixels(),
        ] {
            assert_eq!(image.dimensions(), (60, 32));
            assert!(image.pixels().all(|pixel| pixel.0[3] == 200));
        }
        assert_eq!(source.halftone_pixels(60, 32).dimensions(), (60, 32));
    }
}

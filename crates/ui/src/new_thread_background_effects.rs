//! Effects remain cached source-space images; a separate alpha mask follows layout.
use crate::settings::NewThreadBackgroundEffect;
use crate::theme::Theme;
use gpui::{AnyElement, Empty, IntoElement, Pixels, prelude::*, px};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
type EffectEntry = (
    (NewThreadBackgroundEffect, bool),
    Option<Arc<gpui::RenderImage>>,
);
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
        light: bool,
        cx: &mut gpui::App,
    ) -> Option<Arc<gpui::RenderImage>> {
        let mut effects = self.effects.lock().unwrap();
        let key = (
            effect,
            light
                && !matches!(
                    effect,
                    NewThreadBackgroundEffect::Dither | NewThreadBackgroundEffect::None
                ),
        );
        if let Some((_, image)) = effects.iter().find(|(cached, _)| *cached == key) {
            return image.clone();
        }
        // None marks the single pending job for this source/effect, not a viewport.
        effects.push((key, None));
        drop(effects);
        let source = self.clone();
        cx.spawn(async move |cx| {
            let worker = source.clone();
            let image = cx
                .background_executor()
                .spawn(async move {
                    let pixels = match effect {
                        NewThreadBackgroundEffect::None => {
                            image::RgbaImage::from_fn(worker.width, worker.height, |x, y| {
                                let [r, g, b, a] = worker.colors[(y * worker.width + x) as usize];
                                image::Rgba([b, g, r, a])
                            })
                        }
                        NewThreadBackgroundEffect::Dither => {
                            worker.dither_pixels(worker.width, worker.height)
                        }
                        NewThreadBackgroundEffect::Halftone => {
                            worker.halftone_pixels(worker.width, worker.height, light)
                        }
                        NewThreadBackgroundEffect::Ascii => worker.ascii_pixels(light),
                        _ => worker.scanline_pixels(light),
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
                    .find(|(cached, _)| *cached == key)
                {
                    *ready = Some(image);
                }
                cx.refresh_windows();
            });
        })
        .detach();
        None
    }
    fn scanline_pixels(&self, light: bool) -> image::RgbaImage {
        image::RgbaImage::from_fn(self.width, self.height, |x, y| {
            let [r, g, b, a] = self.colors[(y * self.width + x) as usize];
            let gain = if y % 3 == 0 { 0.52 } else { 1.0 };
            let channel = |value: u8| {
                if light {
                    (value as f32 + (255.0 - value as f32) * (1.0 - gain)) as u8
                } else {
                    (value as f32 * gain) as u8
                }
            };
            image::Rgba([channel(b), channel(g), channel(r), a])
        })
    }
    fn ascii_pixels(&self, light: bool) -> image::RgbaImage {
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
            let ink_density = if light {
                255 - self.pixels[sample]
            } else {
                self.pixels[sample]
            };
            let index = ((ink_density as f32 / 255.0).sqrt() * 9.0) as usize;
            let ink =
                x % 6 < 5 && y % 8 < 7 && GLYPHS[index][y as usize % 8] & (1 << (4 - x % 6)) != 0;
            let [r, g, b, a] = self.colors[(y * self.width + x) as usize];
            let [cr, cg, cb, _] = self.colors[sample];
            let mix = |base: u8, glyph: u8| {
                let paper = if light { 255.0 } else { 0.0 };
                // Keep a colored image beneath the glyph texture in both themes.
                (base as f32 * 0.60
                    + if ink {
                        glyph as f32 * 0.40
                    } else {
                        paper * 0.40
                    }) as u8
            };
            image::Rgba([mix(b, cb), mix(g, cg), mix(r, cr), a])
        })
    }
    fn halftone_pixels(&self, width: u32, height: u32, light: bool) -> image::RgbaImage {
        let bounds = gpui::size(px(width as f32), px(height as f32));
        let paper = if light { 255 } else { 0 };
        let mut pixels =
            image::RgbaImage::from_pixel(width, height, image::Rgba([paper, paper, paper, 255]));
        for y in (0..height).step_by(4) {
            for x in (0..width).step_by(4) {
                let luma = self.sample_cover(bounds, x as f32, y as f32);
                let luma = if light { 255 - luma } else { luma };
                let radius = 2.0 * (0.3 + 0.7 * (luma as f32 / 255.0).sqrt());
                let [r, g, b, a] =
                    self.colors[self.cover_index(bounds, x as f32 + 2.0, y as f32 + 2.0)];
                for dy in 0..4.min(height - y) {
                    for dx in 0..4.min(width - x) {
                        let distance =
                            ((dx as f32 - 1.5).powi(2) + (dy as f32 - 1.5).powi(2)).sqrt();
                        let coverage = (radius + 0.5 - distance).clamp(0.0, 1.0) * a as f32 / 255.0;
                        let [sr, sg, sb, sa] = self.colors[((y + dy) * width + x + dx) as usize];
                        let blend = |source: u8, dot: u8| {
                            (source as f32 * 0.60
                                + (dot as f32 * coverage + paper as f32 * (1.0 - coverage)) * 0.40)
                                as u8
                        };
                        pixels.put_pixel(
                            x + dx,
                            y + dy,
                            image::Rgba([blend(sb, b), blend(sg, g), blend(sr, r), sa]),
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
    theme: &Theme,
    path: &Path,
    base_opacity: f32,
    mask: crate::new_thread_background_mask::Mask,
    cx: &mut gpui::App,
) -> AnyElement {
    let light = matches!(theme.appearance, crate::theme::Appearance::Light);
    match background_luminance(path)
        .and_then(|source| source.raster_image(effect, light, cx))
        .and_then(|source| crate::new_thread_background_mask::image(source, mask, cx))
    {
        Some(image) => gpui::img(image)
            .absolute()
            .inset_0()
            .size_full()
            .object_fit(gpui::ObjectFit::Cover)
            .opacity(base_opacity)
            .into_any_element(),
        None => Empty.into_any_element(),
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
                    assert!(source.raster_image(effect, false, cx).is_none());
                }
                assert_eq!(
                    source
                        .effects
                        .lock()
                        .unwrap()
                        .iter()
                        .filter(|(key, _)| *key == (effect, false))
                        .count(),
                    1
                );
            });
            cx.run_until_parked();
            cx.update(|cx| {
                let first = source.raster_image(effect, false, cx).unwrap();
                assert_eq!(first.size(0).width.0, 60);
                assert_eq!(first.size(0).height.0, 32);
                for _ in 0..100 {
                    assert!(Arc::ptr_eq(
                        &first,
                        &source.raster_image(effect, false, cx).unwrap()
                    ));
                }
            });
        }
    }
    #[test]
    fn light_treatments_use_light_paper_without_inverting_source_hues() {
        let source = fixture();
        for (light, dark) in [
            (source.ascii_pixels(true), source.ascii_pixels(false)),
            (source.scanline_pixels(true), source.scanline_pixels(false)),
            (
                source.halftone_pixels(60, 32, true),
                source.halftone_pixels(60, 32, false),
            ),
        ] {
            let brightness = |image: &image::RgbaImage| -> u64 {
                image
                    .pixels()
                    .map(|p| p.0[..3].iter().map(|c| u64::from(*c)).sum::<u64>())
                    .sum()
            };
            assert!(brightness(&light) > brightness(&dark));
            assert_eq!(light.dimensions(), dark.dimensions());
            // Raster output is BGRA; the warm source remains warm on light paper.
            assert!(light.pixels().all(|p| p.0[2] >= p.0[1] && p.0[1] >= p.0[0]));
        }
    }

    #[gpui::test]
    fn appearance_changes_cache_both_variants_and_share_unchanged_dither(
        cx: &mut gpui::TestAppContext,
    ) {
        let source = fixture();
        for effect in [
            NewThreadBackgroundEffect::Ascii,
            NewThreadBackgroundEffect::Halftone,
            NewThreadBackgroundEffect::Scanlines,
            NewThreadBackgroundEffect::Dither,
        ] {
            cx.update(|cx| {
                source.raster_image(effect, false, cx);
                source.raster_image(effect, true, cx);
            });
            cx.run_until_parked();
            cx.update(|cx| {
                let dark = source.raster_image(effect, false, cx).unwrap();
                let light = source.raster_image(effect, true, cx).unwrap();
                assert_eq!(
                    Arc::ptr_eq(&dark, &light),
                    effect == NewThreadBackgroundEffect::Dither
                );
                for _ in 0..100 {
                    assert!(Arc::ptr_eq(
                        &dark,
                        &source.raster_image(effect, false, cx).unwrap()
                    ));
                    assert!(Arc::ptr_eq(
                        &light,
                        &source.raster_image(effect, true, cx).unwrap()
                    ));
                }
            });
        }
        assert_eq!(source.effects.lock().unwrap().len(), 7);
    }

    #[test]
    fn raster_treatments_preserve_source_dimensions_and_alpha() {
        let source = fixture();
        for image in [
            source.dither_pixels(60, 32),
            source.ascii_pixels(false),
            source.scanline_pixels(false),
            source.ascii_pixels(true),
            source.scanline_pixels(true),
        ] {
            assert_eq!(image.dimensions(), (60, 32));
            assert!(image.pixels().all(|pixel| pixel.0[3] == 200));
        }
        assert_eq!(source.halftone_pixels(60, 32, false).dimensions(), (60, 32));
    }
}

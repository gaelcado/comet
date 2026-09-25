//! Proposed icon and motion system, isolated from the production icon API.
//! Generated from the reviewed icon atelier; see `apps/icon-lab/README.md`.
use gpui::{AssetSource, Result, SharedString};
use std::borrow::Cow;

mod element;
#[allow(dead_code)] // Complete reviewed family includes states not exposed by every platform yet.
mod generated;
mod transition;
pub use element::Icon;
pub use generated::*;

#[derive(rust_embed::RustEmbed)]
#[folder = "assets/custom-icons"]
struct CustomAssets;
/// Composite asset source used only by the native workbench. Production uses
/// `icons::Assets` and therefore never resolves a proposed glyph by accident.
pub struct Assets;
impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some(key) = path.strip_prefix("custom-icons/") {
            return Ok(CustomAssets::get(key).map(|file| file.data));
        }
        if let Some(key) = path.strip_prefix("icon-motion/") {
            return Ok(transition::asset(key));
        }
        crate::icons::Assets.load(path)
    }
    fn list(&self, prefix: &str) -> Result<Vec<SharedString>> {
        let mut paths: Vec<SharedString> = CustomAssets::iter()
            .map(|p| format!("custom-icons/{p}").into())
            .filter(|p: &SharedString| p.starts_with(prefix))
            .collect();
        paths.extend(crate::icons::Assets.list(prefix)?);
        Ok(paths)
    }
}

/// Static icons choose the small optical master at 16px and below.
/// Add `.morph("glyph")` inside a uniquely identified control to animate state changes.
pub fn icon(path: &'static str) -> Icon {
    Icon::new(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proposed_assets_are_not_in_the_production_source() {
        let path = "custom-icons/settings.svg";
        assert!(crate::icons::Assets.load(path).unwrap().is_none());
        assert!(Assets.load(path).unwrap().is_some());
    }

    #[test]
    fn native_renderer_produces_nonempty_icon_and_motion_masks() {
        let renderer = gpui::SvgRenderer::new(std::sync::Arc::new(Assets));
        let assets = Assets;
        let mut paths = assets.list("custom-icons/").unwrap();
        paths.sort();
        assert_eq!(paths.len(), 270);
        let mut sheet = image::RgbaImage::from_pixel(1080, 1620, image::Rgba([248, 247, 244, 255]));
        for (i, path) in paths.iter().enumerate() {
            let bytes = assets.load(path).unwrap().unwrap();
            let render = renderer.render_single_frame(&bytes, 1.0).unwrap();
            let pixels = render.as_bytes(0).unwrap();
            assert!(pixels.chunks_exact(4).any(|p| p[3] > 0), "empty {path}");
            let width = render.size(0).width.0 as u32;
            let height = render.size(0).height.0 as u32;
            for (n, pixel) in pixels.chunks_exact(4).enumerate() {
                let x = (i % 12) as u32 * 90 + 15 + n as u32 % width;
                let y = (i / 12) as u32 * 70 + 10 + n as u32 / width;
                if x < sheet.width() && y < sheet.height() && n < (width * height) as usize {
                    let a = pixel[3] as u16;
                    sheet.put_pixel(
                        x,
                        y,
                        image::Rgba([
                            ((248 * (255 - a) + 35 * a) / 255) as u8,
                            ((247 * (255 - a) + 37 * a) / 255) as u8,
                            ((244 * (255 - a) + 34 * a) / 255) as u8,
                            255,
                        ]),
                    );
                }
            }
        }
        for m in transition::transitions() {
            for frame in m.frames.iter().chain(&m.small_frames) {
                let render = renderer
                    .render_single_frame(frame.as_bytes(), 2.0 / 3.0)
                    .unwrap();
                assert!(
                    render
                        .as_bytes(0)
                        .unwrap()
                        .chunks_exact(4)
                        .any(|p| p[3] > 0),
                    "empty motion: {} to {}",
                    m.from,
                    m.to
                );
            }
        }
        if let Some(path) = std::env::var_os("ZERON_ICON_RASTER_SHEET") {
            sheet.save(path).unwrap();
        }
    }

    #[test]
    fn every_registered_icon_loads_and_parses() {
        let assets = Assets;
        for path in assets.list("custom-icons/").unwrap() {
            let bytes = assets
                .load(&path)
                .unwrap()
                .unwrap_or_else(|| panic!("missing asset {path}"));
            let text = std::str::from_utf8(&bytes).expect("icon svg is utf-8");
            assert!(text.contains("<svg"), "{path} is not an svg");
            assert!(text.contains("viewBox"), "{path} lacks a viewBox");
        }
    }

    #[test]
    fn unknown_paths_are_none() {
        assert!(Assets.load("icons/nope.svg").unwrap().is_none());
    }

    #[test]
    fn list_filters_by_prefix() {
        assert!(!Assets.list("custom-icons/").unwrap().is_empty());
        assert!(Assets.list("fonts/").unwrap().is_empty());
    }
}

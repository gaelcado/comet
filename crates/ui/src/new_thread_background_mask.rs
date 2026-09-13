//! A source-alpha feather following the measured composer, independent of theme.
//! Raster work is coalesced off-thread; route opacity never invalidates the mask.
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Mask {
    pub width: f32,
    pub height: f32,
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub radius: f32,
}

fn smooth(value: f32) -> f32 {
    let t = value.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Mask {
    fn alpha(self, x: f32, y: f32) -> f32 {
        // Signed distance to the actual rounded surface, with a small quiet
        // margin. Equal-distance contours naturally wrap its top corners.
        let radius = self.radius.min((self.right - self.left).max(0.0) * 0.5);
        let qx =
            (x - (self.left + self.right) * 0.5).abs() - ((self.right - self.left) * 0.5 - radius);
        let qy =
            (y - (self.top + self.bottom) * 0.5).abs() - ((self.bottom - self.top) * 0.5 - radius);
        let distance = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius;
        let feather = (self.height * 0.52).clamp(120.0, 220.0);
        let around_composer = smooth((distance - 8.0) / feather);
        // Finish the side tails smoothly as well, without imposing a straight
        // wide wash across the middle of the artwork.
        let bottom = smooth((self.height - y) / (self.height * 0.22).max(1.0));
        around_composer.min(bottom)
    }

    fn raster(self, source: &gpui::RenderImage) -> image::RgbaImage {
        let size = source.size(0);
        let width = size.width.0 as u32;
        let height = size.height.0 as u32;
        let mut pixels =
            image::RgbaImage::from_raw(width, height, source.as_bytes(0).unwrap().to_vec())
                .unwrap();
        let scale = (self.width / width as f32).max(self.height / height as f32);
        let offset_x = (self.width - width as f32 * scale) * 0.5;
        let offset_y = (self.height - height as f32 * scale) * 0.5;
        for (x, y, pixel) in pixels.enumerate_pixels_mut() {
            let alpha = self.alpha(
                (x as f32 + 0.5) * scale + offset_x,
                (y as f32 + 0.5) * scale + offset_y,
            );
            pixel.0[3] = (pixel.0[3] as f32 * alpha).round() as u8;
        }
        pixels
    }
}

struct Entry {
    ready: Option<(Mask, Arc<gpui::RenderImage>)>,
    busy: bool,
}

pub(crate) fn image(
    source: Arc<gpui::RenderImage>,
    mask: Mask,
    cx: &mut gpui::App,
) -> Option<Arc<gpui::RenderImage>> {
    type Cache = Vec<(gpui::ImageId, Arc<Mutex<Entry>>)>;
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    let entry = {
        let mut cache = CACHE.get_or_init(Default::default).lock().unwrap();
        if let Some(index) = cache.iter().position(|(id, _)| *id == source.id) {
            let item = cache.remove(index);
            let entry = item.1.clone();
            cache.push(item);
            entry
        } else {
            let entry = Arc::new(Mutex::new(Entry {
                ready: None,
                busy: false,
            }));
            cache.push((source.id, entry.clone()));
            if cache.len() > 4 {
                cache.remove(0);
            }
            entry
        }
    };
    let mut state = entry.lock().unwrap();
    let ready = state.ready.as_ref().map(|(_, image)| image.clone());
    if state.busy || state.ready.as_ref().is_some_and(|(key, _)| *key == mask) {
        return ready;
    }
    state.busy = true;
    drop(state);
    cx.spawn(async move |cx| {
        let image = cx
            .background_executor()
            .spawn(async move {
                Arc::new(gpui::RenderImage::new([image::Frame::new(
                    mask.raster(&source),
                )]))
            })
            .await;
        cx.update(|cx| {
            let mut state = entry.lock().unwrap();
            // A resize can supersede this work; retain the last ready image
            // until the next frame starts the latest requested geometry.
            state.ready = Some((mask, image));
            state.busy = false;
            cx.refresh_windows();
        });
    })
    .detach();
    ready
}

#[cfg(test)]
mod tests {
    use super::*;
    fn mask() -> Mask {
        Mask {
            width: 1000.0,
            height: 440.0,
            left: 160.0,
            right: 840.0,
            top: 360.0,
            bottom: 484.0,
            radius: 26.0,
        }
    }
    #[test]
    fn contour_wraps_surface_and_preserves_upper_artwork() {
        let mask = mask();
        assert_eq!(mask.alpha(500.0, 20.0), 1.0);
        assert_eq!(mask.alpha(500.0, 360.0), 0.0);
        assert!(mask.alpha(100.0, 340.0) > mask.alpha(500.0, 340.0));
        assert_eq!(mask.alpha(0.0, 440.0), 0.0);
        for y in 0..440 {
            assert!((mask.alpha(100.0, y as f32) - mask.alpha(900.0, y as f32)).abs() < 0.00001);
        }
    }
    #[test]
    fn feather_has_soft_endpoints_and_keeps_midpoint_color() {
        assert_eq!(smooth(0.5), 0.5);
        assert!(smooth(0.01) < 0.001);
        assert!(smooth(0.99) > 0.999);
    }

    #[test]
    fn masking_changes_only_alpha_and_never_amplifies_source_opacity() {
        let source = gpui::RenderImage::new([image::Frame::new(image::RgbaImage::from_pixel(
            1000,
            440,
            image::Rgba([173, 89, 231, 180]),
        ))]);
        let output = mask().raster(&source);
        assert!(
            output
                .pixels()
                .all(|pixel| pixel.0[..3] == [173, 89, 231] && pixel.0[3] <= 180)
        );
        assert_eq!(output.get_pixel(500, 10).0[3], 180);
        assert_eq!(output.get_pixel(500, 360).0[3], 0);
    }
}

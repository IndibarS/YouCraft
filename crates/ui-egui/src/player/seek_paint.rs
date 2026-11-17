//! Flatten the rail and thumb before applying UI opacity, so their overlap fades only once.

use egui::{Color32, ColorImage, Rect, TextureHandle, TextureOptions, pos2, vec2};

#[derive(Clone, Copy, PartialEq)]
struct Geometry {
    size: [usize; 2],
    width: f32,
    height: f32,
    playhead: f32,
    hover: f32,
    radius: f32,
}

#[derive(Clone)]
struct CachedBar {
    geometry: Geometry,
    texture: TextureHandle,
}

pub(super) fn paint(ui: &egui::Ui, rail: Rect, playhead: f32, hover: f32, radius: f32) {
    if !rail.width().is_finite() || !rail.height().is_finite() || rail.width() <= 0.0 || rail.height() <= 0.0 {
        return;
    }
    let padding = radius + 1.0;
    let bounds = Rect::from_center_size(rail.center(), vec2(rail.width() + 2.0 * padding, 2.0 * padding));
    let scale = ui.ctx().pixels_per_point();
    let scale = if scale.is_finite() { scale.clamp(0.5, 4.0) } else { 1.0 };
    let size = [(bounds.width() * scale).ceil().clamp(1.0, 4096.0) as usize, (bounds.height() * scale).ceil().clamp(1.0, 64.0) as usize];
    let pixel_width = bounds.width() / size[0] as f32;
    let quantize = |x: f32| ((x - rail.left()) / pixel_width).round() * pixel_width;
    let geometry = Geometry {
        size,
        width: rail.width(),
        height: rail.height(),
        playhead: quantize(playhead).clamp(0.0, rail.width()),
        hover: quantize(hover).clamp(0.0, rail.width()),
        radius,
    };
    // Watch and miniplayer share one bounded cache; switching videos does not retain textures.
    let key = egui::Id::unique("player-composited-seek-bar");
    let cached = ui.ctx().data(|data| data.get_temp::<CachedBar>(key));
    let texture = match cached {
        Some(cached) if cached.geometry == geometry => cached.texture,
        Some(mut cached) => {
            cached.texture.set(rasterize(geometry), TextureOptions::LINEAR);
            cached.texture
        }
        None => ui.ctx().load_texture("composited-seek-bar", rasterize(geometry), TextureOptions::LINEAR),
    };
    // This single image receives the painter's opacity once, including the circle/rail union.
    ui.painter().image(texture.id(), bounds, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    ui.ctx().data_mut(|data| data.insert_temp(key, CachedBar { geometry, texture }));
}

fn rasterize(g: Geometry) -> ColorImage {
    // Geometry is private and constructed above with bounded dimensions (at most 1 MiB RGBA).
    let mut image = ColorImage::filled(g.size, Color32::TRANSPARENT);
    let padding = g.radius + 1.0;
    let step = vec2((g.width + 2.0 * padding) / g.size[0] as f32, 2.0 * padding / g.size[1] as f32);
    for (index, pixel) in image.pixels.iter_mut().enumerate() {
        let x = (index % g.size[0]) as f32;
        let y = (index / g.size[0]) as f32;
        let mut sum = [0_u16; 4];
        // Four coverage samples provide antialiasing without overlapping translucent shapes.
        for (sx, sy) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] {
            let x = (x + sx) * step.x - padding;
            let y = (y + sy) * step.y - padding;
            let circle = (x - g.playhead).powi(2) + y * y <= g.radius * g.radius;
            let rail = (0.0..=g.width).contains(&x) && y.abs() <= g.height * 0.5;
            let color = if circle || (rail && x <= g.playhead) {
                [255, 0, 51, 255]
            } else if rail {
                let gray = if x <= g.hover { 145 } else { 48 };
                [gray, gray, gray, 255]
            } else {
                [0; 4]
            };
            for (sum, value) in sum.iter_mut().zip(color) {
                *sum += value;
            }
        }
        *pixel = Color32::from_rgba_premultiplied((sum[0] / 4) as u8, (sum[1] / 4) as u8, (sum[2] / 4) as u8, (sum[3] / 4) as u8);
    }
    image
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circle_and_rail_overlap_is_one_opaque_red_surface_before_fading() {
        let image = rasterize(Geometry { size: [114, 14], width: 100.0, height: 4.0, playhead: 50.0, hover: 50.0, radius: 6.0 });
        let red = Color32::from_rgb(255, 0, 51);
        assert_eq!(image[(30, 7)], red, "rail interior");
        assert_eq!(image[(55, 7)], red, "circle/rail overlap");
        assert_eq!(image[(57, 3)], red, "circle beyond the rail");
        assert_eq!(image[(80, 7)], Color32::from_gray(48));
        assert_eq!(image[(30, 1)], Color32::TRANSPARENT);
    }
}

//! The desk around the unit (task-179): in a window whose shape is not the panel's, the free
//! space left and right (or above and below) of the panel is a dark matte desk with the
//! unit's soft cast shadow on it, in place of a flat letterbox color.
//!
//! Nothing of it is rasterized per window size, so a resize drag costs no SVG work: the
//! desk's vertical gradient, the darkening toward the window's edges and the shadows are
//! meshes with colored vertices, and the speckle and the brushing are one tile of 256 x 256
//! points (`desk_tile`, rasterized once, at one pixel per point) repeated over the window. The
//! numbers are the approved mockup's (`backdrop()` of mockup/179/mock.py, removed with
//! task-179): the gradient `#1d2023` to `#131517`, the vignette's radial gradient (clear to
//! 62 % of the radius, then to 50 % black), and the shadows: a band of 12 panel pixels
//! from 5 left of the unit at 55 % and one of 20 from 8 inside its right edge at 85 %, both
//! blurred with a deviation of 5; the same above and below in a tall window.

use eframe::egui::{Color32, Mesh, Pos2, Rect, Shape, TextureOptions, Ui, Vec2};

use crate::app::{View, part_texture};
use crate::art;
use crate::layout::{PANEL_H, PANEL_W};

/// The desk's gradient, top and bottom.
const DESK_TOP: Color32 = Color32::from_rgb(0x1d, 0x20, 0x23);
const DESK_BOTTOM: Color32 = Color32::from_rgb(0x13, 0x15, 0x17);
/// The panel's dark seams, painted under the panel so no desk shows between its parts.
pub const SEAM: Color32 = Color32::from_rgb(0x14, 0x18, 0x1b);
/// The side of the desk's tile in points (`DESK_TILE` of orb-ui-art).
pub const TILE: f32 = 256.0;

/// Paints the desk over `avail` (the window below the menu bar) and the panel's seam color
/// under the panel of `view`.
pub fn paint(ui: &Ui, avail: Rect, view: &View) {
    let painter = ui.painter();
    let mut m = Mesh::default();
    // The desk follows the classic skin's grain tone so the whole surface stays coherent.
    let grain = if crate::art::ui_variant() == crate::settings::UiVariant::Classic {
        crate::art::grain()
    } else {
        crate::art::GrainTransform::None
    };
    let (top, bottom) = match grain {
        crate::art::GrainTransform::None => (DESK_TOP, DESK_BOTTOM),
        crate::art::GrainTransform::Hue(h) => (hue_rotate(DESK_TOP, h), hue_rotate(DESK_BOTTOM, h)),
        crate::art::GrainTransform::Gray => (desaturate(DESK_TOP), desaturate(DESK_BOTTOM)),
        crate::art::GrainTransform::Black => (
            desaturate_scale(DESK_TOP, 0.4),
            desaturate_scale(DESK_BOTTOM, 0.4),
        ),
    };
    for (p, c) in [
        (avail.left_top(), top),
        (avail.right_top(), top),
        (avail.right_bottom(), bottom),
        (avail.left_bottom(), bottom),
    ] {
        m.colored_vertex(p, c);
    }
    m.add_triangle(0, 1, 2);
    m.add_triangle(0, 2, 3);
    painter.add(Shape::mesh(m));
    let unit = view.rect(0.0, 0.0, PANEL_W, PANEL_H);
    let wide = unit.width() < avail.width() - 0.5;
    let tall = unit.height() < avail.height() - 0.5;
    // The tile only where the desk shows, and at one pixel per point whatever the density: its
    // noise is the start's most expensive raster (about 200 ms at 2x on an M-series Mac), and
    // a soft speckle at a tenth of opacity on a dark desk does not need more. A clean desk has
    // no tile (the gradient alone).
    if let Some(part) = art::desk_tile_part()
        .and_then(art::part)
        .filter(|_| wide || tall)
    {
        let px = TILE;
        let tex = part_texture(
            ui.ctx(),
            part.uri,
            part.svg,
            Vec2::splat(px),
            Vec2::splat(TILE),
            TextureOptions::LINEAR_REPEAT,
            crate::art::GrainTransform::None,
        );
        if let Some(tex) = tex {
            let mut m = Mesh::with_texture(tex.id);
            let uv = Rect::from_min_max(Pos2::ZERO, (avail.size() / TILE).to_pos2());
            m.add_rect_with_uv(avail, uv, Color32::WHITE);
            painter.add(Shape::mesh(m));
        }
    }
    painter.add(Shape::mesh(vignette(avail)));
    let mut shadow = Mesh::default();
    let s = view.scale;
    if wide {
        blurred_rect(
            &mut shadow,
            view.rect(-5.0, 0.0, 12.0, PANEL_H),
            5.0 * s,
            0.55,
        );
        blurred_rect(
            &mut shadow,
            view.rect(616.0, 0.0, 20.0, PANEL_H),
            5.0 * s,
            0.85,
        );
    }
    if tall {
        blurred_rect(
            &mut shadow,
            view.rect(0.0, -5.0, PANEL_W, 12.0),
            5.0 * s,
            0.55,
        );
        blurred_rect(
            &mut shadow,
            view.rect(0.0, 502.0, PANEL_W, 20.0),
            5.0 * s,
            0.85,
        );
    }
    if !shadow.is_empty() {
        painter.add(Shape::mesh(shadow));
    }
    painter.rect_filled(unit, 0.0, SEAM);
}

/// Rotates a color's hue by `degrees` (in place of a full image pass), preserving lightness and
/// saturation so the desk stays dark.
fn hue_rotate(c: Color32, degrees: f32) -> Color32 {
    let cos = degrees.to_radians().cos();
    let sin = degrees.to_radians().sin();
    let m: [[f32; 3]; 3] = [
        [
            0.213 + cos * 0.787 - sin * 0.213,
            0.715 - cos * 0.715 - sin * 0.715,
            0.072 - cos * 0.072 + sin * 0.928,
        ],
        [
            0.213 - cos * 0.213 + sin * 0.143,
            0.715 + cos * 0.285 + sin * 0.140,
            0.072 - cos * 0.072 - sin * 0.283,
        ],
        [
            0.213 - cos * 0.213 - sin * 0.787,
            0.715 - cos * 0.715 + sin * 0.715,
            0.072 + cos * 0.928 + sin * 0.072,
        ],
    ];
    let (r, g, b) = (c.r() as f32, c.g() as f32, c.b() as f32);
    let out = [
        m[0][0] * r + m[0][1] * g + m[0][2] * b,
        m[1][0] * r + m[1][1] * g + m[1][2] * b,
        m[2][0] * r + m[2][1] * g + m[2][2] * b,
    ];
    Color32::from_rgb(
        out[0].round().clamp(0.0, 255.0) as u8,
        out[1].round().clamp(0.0, 255.0) as u8,
        out[2].round().clamp(0.0, 255.0) as u8,
    )
}

/// Desaturates a color to its luminance, keeping the desk's darkness.
fn desaturate(c: Color32) -> Color32 {
    desaturate_scale(c, 1.0)
}

/// Desaturates a color to its luminance scaled by `scale`, for the almost-black gray.
fn desaturate_scale(c: Color32, scale: f32) -> Color32 {
    let luma = scale * (0.213 * c.r() as f32 + 0.715 * c.g() as f32 + 0.072 * c.b() as f32);
    let l = luma.round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(l, l, l)
}

/// The window's edges darkened: the mockup's radial gradient over the whole area (an ellipse
/// of 0.75 of the area's width and height), as a grid of colored vertices.
fn vignette(r: Rect) -> Mesh {
    const N: u32 = 16;
    let mut m = Mesh::default();
    for j in 0..=N {
        for i in 0..=N {
            let (u, v) = (i as f32 / N as f32, j as f32 / N as f32);
            let t = ((u - 0.5).powi(2) + (v - 0.5).powi(2)).sqrt() / 0.75;
            let a = if t < 0.62 {
                0.0
            } else {
                0.5 * ((t - 0.62) / 0.38).min(1.0)
            };
            let p = r.min + Vec2::new(u * r.width(), v * r.height());
            m.colored_vertex(p, Color32::from_black_alpha((a * 255.0).round() as u8));
        }
    }
    grid(&mut m, 0, N + 1, N + 1);
    m
}

/// The triangles of a grid of `cols` x `rows` vertices added from `base` on, row by row.
fn grid(m: &mut Mesh, base: u32, cols: u32, rows: u32) {
    for j in 0..rows - 1 {
        for i in 0..cols - 1 {
            let a = base + j * cols + i;
            m.add_triangle(a, a + 1, a + cols + 1);
            m.add_triangle(a, a + cols + 1, a + cols);
        }
    }
}

/// The error function (Abramowitz and Stegun 7.1.26, error below 1.5e-7).
fn erf(x: f32) -> f32 {
    let t = 1.0 / (1.0 + 0.327_591_1 * x.abs());
    let y = 1.0
        - (((((1.061_405_4 * t - 1.453_152_1) * t) + 1.421_413_8) * t - 0.284_496_74) * t
            + 0.254_829_6)
            * t
            * (-x * x).exp();
    y.copysign(x)
}

/// How much of a band from `a` to `b` a Gaussian blur of deviation `sigma` leaves at `x`.
fn band(x: f32, a: f32, b: f32, sigma: f32) -> f32 {
    let k = std::f32::consts::SQRT_2 * sigma;
    0.5 * (erf((x - a) / k) - erf((x - b) / k))
}

/// The positions a blurred edge from `a` to `b` is sampled at: across each edge, 3 deviations
/// to either side.
fn samples(a: f32, b: f32, sigma: f32) -> Vec<f32> {
    let mut v = Vec::new();
    for e in [a, b] {
        for k in 0..=8 {
            v.push(e - 3.0 * sigma + 6.0 * sigma * k as f32 / 8.0);
        }
    }
    v.sort_by(f32::total_cmp);
    v.dedup_by(|x, y| (*x - *y).abs() < 0.01);
    v
}

/// A black rectangle `r` blurred with a Gaussian of deviation `sigma` at `opacity`, as a grid
/// of colored vertices (the mockup's `feGaussianBlur` shadow).
fn blurred_rect(m: &mut Mesh, r: Rect, sigma: f32, opacity: f32) {
    if sigma <= 0.0 {
        return;
    }
    let xs = samples(r.min.x, r.max.x, sigma);
    let ys = samples(r.min.y, r.max.y, sigma);
    let base = m.vertices.len() as u32;
    for &y in &ys {
        let fy = band(y, r.min.y, r.max.y, sigma);
        for &x in &xs {
            let a = opacity * fy * band(x, r.min.x, r.max.x, sigma);
            m.colored_vertex(
                Pos2::new(x, y),
                Color32::from_black_alpha((a * 255.0).round().clamp(0.0, 255.0) as u8),
            );
        }
    }
    grid(m, base, xs.len() as u32, ys.len() as u32);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blurred_band() {
        // the middle of a wide band is full, its edges half, far outside nothing
        assert!((band(50.0, 0.0, 100.0, 5.0) - 1.0).abs() < 1e-4);
        assert!((band(0.0, 0.0, 100.0, 5.0) - 0.5).abs() < 1e-4);
        assert!(band(-20.0, 0.0, 100.0, 5.0) < 1e-4);
        assert!((erf(1.0) - 0.842_700_8).abs() < 1e-5);
    }

    #[test]
    fn the_tile_is_the_art_generators() {
        assert_eq!(TILE, orb_ui_art::parts::DESK_TILE);
        let p = art::part("desk_tile").unwrap();
        assert_eq!((p.w, p.h, p.bleed), (TILE, TILE, 0.0));
    }

    #[test]
    fn every_desk_texture_resolves_to_a_256_tile_or_none_for_clean() {
        for texture in crate::settings::DeskTexture::ALL {
            crate::art::set_desk_texture(texture);
            match crate::art::desk_tile_part() {
                Some(name) => {
                    let p = art::part(name).unwrap();
                    assert_eq!((p.w, p.h, p.bleed), (TILE, TILE, 0.0), "{name}");
                }
                None => assert_eq!(texture, crate::settings::DeskTexture::Clean),
            }
        }
    }
}

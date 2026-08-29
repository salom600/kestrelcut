//! Software compositing for the preview when layers use non-Normal blend
//! modes (egui can only alpha-composite via meshes). Canvas-size CPU blend —
//! real per-pixel math matching the ffmpeg `blend` modes used at export.
//! Activated only when at least one visible layer blends; the default path
//! stays on the fast GPU painter.

use crate::model::{BlendMode, Transform};

pub struct Layer {
    pub rgba: std::sync::Arc<Vec<u8>>,
    pub w: u32,
    pub h: u32,
    pub transform: Transform,
    pub blend: BlendMode,
}

/// Compose `layers` (bottom→top) into a canvas of `cw×ch`. Returns the RGBA
/// buffer (straight alpha, opaque where covered).
pub fn compose(cw: u32, ch: u32, layers: &[Layer], canvas: &mut Vec<u8>) {
    let n = (cw as usize) * (ch as usize) * 4;
    if canvas.len() != n { canvas.resize(n, 0); }
    canvas.fill(0);
    for l in layers {
        blit_layer(cw, ch, canvas, l);
    }
    // make fully transparent regions black (video frames over nothing)
    for px in canvas.chunks_exact_mut(4) {
        if px[3] == 0 { px[0] = 0; px[1] = 0; px[2] = 0; px[3] = 255; }
        else if px[3] < 255 {
            // composite onto black
            let a = px[3] as f32 / 255.0;
            px[0] = (px[0] as f32 * a) as u8;
            px[1] = (px[1] as f32 * a) as u8;
            px[2] = (px[2] as f32 * a) as u8;
            px[3] = 255;
        }
    }
}

fn blit_layer(cw: u32, ch: u32, canvas: &mut [u8], l: &Layer) {
    let tf = &l.transform;
    let scale = tf.scale.max(0.01);
    let rot = tf.rotation.to_radians();
    let (sn, cs) = rot.sin_cos();
    // layer center on the canvas
    let cx = cw as f32 / 2.0 + tf.x * cw as f32 / 2.0;
    let cy = ch as f32 / 2.0 + tf.y * ch as f32 / 2.0;
    let hw = l.w as f32 * scale / 2.0;
    let hh = l.h as f32 * scale / 2.0;
    let opacity = tf.opacity.clamp(0.0, 1.0);
    if opacity <= 0.001 { return; }

    let identity = rot.abs() < 0.001 && (scale - 1.0).abs() < 0.001;
    // destination bbox (screen-space quad bounds)
    let (min_x, min_y, max_x, max_y) = if identity {
        (cx - hw, cy - hh, cx + hw, cy + hh)
    } else {
        let corners = [
            (cx + (-hw) * cs - (-hh) * sn, cy + (-hw) * sn + (-hh) * cs),
            (cx + hw * cs - (-hh) * sn, cy + hw * sn + (-hh) * cs),
            (cx + hw * cs - hh * sn, cy + hw * sn + hh * cs),
            (cx + (-hw) * cs - hh * sn, cy + (-hw) * sn + hh * cs),
        ];
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for (x, y) in corners {
            x0 = x0.min(x); y0 = y0.min(y); x1 = x1.max(x); y1 = y1.max(y);
        }
        (x0, y0, x1, y1)
    };
    let x0 = min_x.max(0.0).floor() as i32;
    let y0 = min_y.max(0.0).floor() as i32;
    let x1 = (max_x.min(cw as f32)).ceil() as i32;
    let y1 = (max_y.min(ch as f32)).ceil() as i32;

    for py in y0..y1 {
        for px in x0..x1 {
            // inverse-map canvas px → source px
            let (dx, dy) = (px as f32 + 0.5 - cx, py as f32 + 0.5 - cy);
            let (sx, sy) = if identity {
                (dx + hw, dy + hh)
            } else {
                let ux = (dx * cs + dy * sn) / scale + l.w as f32 / 2.0;
                let uy = (-dx * sn + dy * cs) / scale + l.h as f32 / 2.0;
                (ux, uy)
            };
            if sx < 0.0 || sy < 0.0 || sx >= l.w as f32 || sy >= l.h as f32 { continue; }
            let sxi = sx as usize;
            let syi = sy as usize;
            let si = (syi * l.w as usize + sxi) * 4;
            if si + 3 >= l.rgba.len() { continue; }
            let (sr, sg, sb, sa) = (
                l.rgba[si] as f32 / 255.0,
                l.rgba[si + 1] as f32 / 255.0,
                l.rgba[si + 2] as f32 / 255.0,
                l.rgba[si + 3] as f32 / 255.0,
            );
            let a = sa * opacity;
            if a <= 0.0 { continue; }
            let di = ((py as usize) * (cw as usize) + px as usize) * 4;
            let (dr, dg, db) = (
                canvas[di] as f32 / 255.0,
                canvas[di + 1] as f32 / 255.0,
                canvas[di + 2] as f32 / 255.0,
            );
            let (br, bg, bb) = blend_px(l.blend, sr, sg, sb, dr, dg, db);
            // mix blended result with the layer's alpha; new dst alpha grows
            let na = a + (1.0 - a) * canvas[di + 3] as f32 / 255.0;
            let mix = |b: f32, d: f32| ((b * a + d * (1.0 - a)) * 255.0).round().clamp(0.0, 255.0) as u8;
            canvas[di] = mix(br, dr);
            canvas[di + 1] = mix(bg, dg);
            canvas[di + 2] = mix(bb, db);
            canvas[di + 3] = (na * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
}

/// Per-pixel blend math (0..1 floats) — mirrors ffmpeg blend filter modes.
fn blend_px(mode: BlendMode, sr: f32, sg: f32, sb: f32, dr: f32, dg: f32, db: f32) -> (f32, f32, f32) {
    match mode {
        BlendMode::Normal => (sr, sg, sb),
        BlendMode::Multiply => (sr * dr, sg * dg, sb * db),
        BlendMode::Screen => (sr + dr - sr * dr, sg + dg - sg * dg, sb + db - sb * db),
        BlendMode::Overlay => (overlay(sr, dr), overlay(sg, dg), overlay(sb, db)),
        BlendMode::HardLight => (overlay(dr, sr), overlay(dg, sg), overlay(db, sb)),
        BlendMode::SoftLight => (soft(dr, sr), soft(dg, sg), soft(db, sb)),
        BlendMode::Darken => (sr.min(dr), sg.min(dg), sb.min(db)),
        BlendMode::Lighten => (sr.max(dr), sg.max(dg), sb.max(db)),
        BlendMode::Difference => ((sr - dr).abs(), (sg - dg).abs(), (sb - db).abs()),
    }
}

fn overlay(s: f32, d: f32) -> f32 {
    if d < 0.5 { 2.0 * s * d } else { 1.0 - 2.0 * (1.0 - s) * (1.0 - d) }
}

fn soft(d: f32, s: f32) -> f32 {
    // W3C soft-light
    if s <= 0.5 {
        d - (1.0 - 2.0 * s) * d * (1.0 - d)
    } else {
        let dd = if d <= 0.25 { ((16.0 * d - 12.0) * d + 4.0) * d } else { d.sqrt() };
        d + (2.0 * s - 1.0) * (dd - d)
    }
}

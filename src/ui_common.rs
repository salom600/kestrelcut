//! Shared UI building blocks: icon buttons, gradient sliders, transformed
//! image painting, texture upload.

use crate::app::App;
use egui::{Align2, Color32, ColorImage, FontId, Painter, Pos2, Rect, Rounding, Sense, Shape, Stroke, TextureHandle, TextureOptions, Vec2};

pub type TexCache = std::collections::HashMap<u64, TextureHandle>;

pub fn icon_btn(app: &App, ui: &mut egui::Ui, size: f32, tip: &str, draw: impl Fn(&Painter, Rect, Color32)) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let col = if resp.hovered() { app.theme.text } else { app.theme.dim };
    let bg = if resp.hovered() { app.theme.panel3 } else { Color32::TRANSPARENT };
    ui.painter().rect_filled(r, Rounding::same(3), bg);
    draw(ui.painter(), r.shrink(2.0), col);
    resp.on_hover_text(tip.to_string())
}

pub fn icon_toggle(app: &App, ui: &mut egui::Ui, size: f32, on: bool, tip: &str, draw: impl Fn(&Painter, Rect, Color32)) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let col = if on { app.theme.accent } else if resp.hovered() { app.theme.text } else { app.theme.dim };
    let bg = if on { app.theme.accent_dim.gamma_multiply(1.2) } else if resp.hovered() { app.theme.panel3 } else { Color32::TRANSPARENT };
    ui.painter().rect_filled(r, Rounding::same(3), bg);
    draw(ui.painter(), r.shrink(2.0), col);
    resp.on_hover_text(tip.to_string())
}

/// Toggle button at an explicit rect (used by track headers). Interaction is
/// registered through `ui.interact`; painting goes to `ui.painter_at`.
pub fn icon_toggle_shared(app: &App, ui: &mut egui::Ui, r: Rect, id: egui::Id, on: bool, kind: &str) -> egui::Response {
    let resp = ui.interact(r, id, Sense::click());
    let hovered = resp.hovered();
    let col = if on { app.theme.accent } else if hovered { app.theme.text } else { app.theme.dim };
    let bg = if on { app.theme.accent_dim.gamma_multiply(1.2) } else if hovered { app.theme.panel3 } else { Color32::TRANSPARENT };
    let p = ui.painter_at(r);
    p.rect_filled(r, Rounding::same(3), bg);
    match kind {
        "lock" => crate::ui_icons::lock_closed(&p, r.shrink(3.0), col),
        "eye" => crate::ui_icons::eye(&p, r.shrink(3.0), col, on),
        "mute" => crate::ui_icons::letter(&p, r.shrink(3.0), col, 'M'),
        "solo" => crate::ui_icons::letter(&p, r.shrink(3.0), col, 'S'),
        _ => crate::ui_icons::mic(&p, r.shrink(3.0), col),
    }
    resp
}

/// Tab button with active underline (workspace tabs).
pub fn tab_btn(app: &App, ui: &mut egui::Ui, label: &str, active: bool) -> egui::Response {
    let font = FontId::proportional(13.0);
    let galley = ui.painter().layout_no_wrap(label.to_string(), font, Color32::WHITE);
    let size = galley.size() + egui::vec2(16.0, 10.0);
    let (r, resp) = ui.allocate_exact_size(size, Sense::click());
    if active {
        ui.painter().rect_filled(r, 0.0, app.theme.panel2);
        ui.painter().rect_filled(Rect::from_min_max(Pos2::new(r.left(), r.bottom() - 2.0), Pos2::new(r.right(), r.bottom())), 0.0, app.theme.accent);
    } else if resp.hovered() {
        ui.painter().rect_filled(r, 0.0, app.theme.panel.gamma_multiply(1.3));
    }
    let col = if active { app.theme.text } else { app.theme.dim };
    ui.painter().galley(Pos2::new(r.center().x - galley.size().x / 2.0, r.center().y - galley.size().y / 2.0), galley, col);
    resp
}

fn gradient_rect(p: &Painter, track: Rect, c0: Color32, c1: Color32, steps: usize) {
    let w = track.width() / steps as f32;
    for i in 0..steps {
        let t = i as f32 / (steps - 1).max(1) as f32;
        let col = Color32::from_rgba_unmultiplied(
            (c0.r() as f32 * (1.0 - t) + c1.r() as f32 * t) as u8,
            (c0.g() as f32 * (1.0 - t) + c1.g() as f32 * t) as u8,
            (c0.b() as f32 * (1.0 - t) + c1.b() as f32 * t) as u8,
            (c0.a() as f32 * (1.0 - t) + c1.a() as f32 * t) as u8,
        );
        let x = track.left() + i as f32 * w;
        p.rect_filled(Rect::from_min_max(Pos2::new(x, track.top()), Pos2::new(x + w + 0.5, track.bottom())), 0.0, col);
    }
}

/// Horizontal slider with gradient track + value, like the reference panel.
#[allow(clippy::too_many_arguments)]
pub fn gradient_slider(
    ui: &mut egui::Ui,
    th: crate::util::Theme,
    label: &str,
    value: f32,
    min: f32,
    max: f32,
    gradient: [Color32; 2],
    fmt: impl Fn(f32) -> String,
    on_change: &mut dyn FnMut(f32),
) {
    ui.horizontal(|ui| {
        let (lr, _) = ui.allocate_exact_size(Vec2::new(92.0, 18.0), Sense::hover());
        ui.painter().text(Pos2::new(lr.left(), lr.center().y), Align2::LEFT_CENTER, label, FontId::proportional(12.0), th.dim);
        let avail = (ui.available_width() - 66.0).max(60.0);
        let (r, resp) = ui.allocate_exact_size(Vec2::new(avail, 14.0), Sense::click_and_drag());
        let track = Rect::from_min_max(Pos2::new(r.left(), r.center().y - 2.0), Pos2::new(r.right(), r.center().y + 2.0));
        ui.painter().rect_filled(track, 2.0, th.panel3);
        gradient_rect(ui.painter(), track, gradient[0], gradient[1], 32);
        ui.painter().line_segment(
            [Pos2::new(r.center().x, r.top() - 1.0), Pos2::new(r.center().x, r.bottom() + 1.0)],
            egui::Stroke::new(1.0, th.border2));
        let t = ((value - min) / (max - min)).clamp(0.0, 1.0);
        let hx = r.left() + t * r.width();
        ui.painter().circle_filled(Pos2::new(hx, r.center().y), 6.0, th.panel);
        ui.painter().circle_stroke(Pos2::new(hx, r.center().y), 6.0, egui::Stroke::new(2.0, th.text));
        let (vr, _) = ui.allocate_exact_size(Vec2::new(50.0, 18.0), Sense::hover());
        ui.painter().text(Pos2::new(vr.right(), vr.center().y), Align2::RIGHT_CENTER, &fmt(value), FontId::proportional(12.0), th.accent_text);

        let pick = resp.interact_pointer_pos().map(|p| {
            let t = ((p.x - r.left()) / r.width()).clamp(0.0, 1.0);
            min + t * (max - min)
        });
        if resp.dragged() || resp.clicked() {
            if let Some(v) = pick { on_change(v); }
        }
    });
}

/// Draw a texture into `dst` with full transform (position/scale/rotate/opacity).
pub fn draw_transformed(p: &Painter, tex: &TextureHandle, dst: Rect, tf: &crate::model::Transform) {
    let size = dst.size();
    let scale = tf.scale.max(0.01);
    let center = Pos2::new(dst.center().x + tf.x * size.x / 2.0, dst.center().y + tf.y * size.y / 2.0);
    let rad = tf.rotation.to_radians();
    let (sn, cs) = rad.sin_cos();
    let hw = size.x * scale / 2.0;
    let hh = size.y * scale / 2.0;
    let corner = |sx: f32, sy: f32| -> Pos2 {
        let (lx, ly) = (sx * hw, sy * hh);
        Pos2::new(center.x + lx * cs - ly * sn, center.y + lx * sn + ly * cs)
    };
    let a = (tf.opacity.clamp(0.0, 1.0) * 255.0) as u8;
    let white = Color32::from_rgba_unmultiplied(255, 255, 255, a);
    let mut mesh = egui::Mesh::with_texture(tex.id());
    let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
    let push = |m: &mut egui::Mesh, pos: Pos2, uv: Pos2| {
        m.vertices.push(egui::epaint::Vertex { pos, uv, color: white });
        (m.vertices.len() - 1) as u32
    };
    let v00 = push(&mut mesh, corner(-1.0, -1.0), uv.min);
    let v10 = push(&mut mesh, corner(1.0, -1.0), Pos2::new(uv.max.x, uv.min.y));
    let v11 = push(&mut mesh, corner(1.0, 1.0), uv.max);
    let v01 = push(&mut mesh, corner(-1.0, 1.0), Pos2::new(uv.min.x, uv.max.y));
    mesh.add_triangle(v00, v10, v11);
    mesh.add_triangle(v00, v11, v01);
    p.add(Shape::mesh(mesh));
}

/// Upload (or update) a raw RGBA texture in a cache.
pub fn upload_tex(cache: &mut TexCache, ctx: &egui::Context, key: u64, w: u32, h: u32, rgba: &[u8]) -> TextureHandle {
    let img = ColorImage::from_rgba_unmultiplied([w as usize, h as usize], rgba);
    if let Some(tex) = cache.get_mut(&key) {
        if tex.size()[0] as u32 == w && tex.size()[1] as u32 == h {
            tex.set(img, TextureOptions::LINEAR);
            return tex.clone();
        }
    }
    let tex = ctx.load_texture(format!("k{key}"), img, TextureOptions::LINEAR);
    cache.insert(key, tex.clone());
    tex
}

/// Decode PNG bytes (media pool thumbs) into a texture, cached by path.
pub fn png_tex(cache: &mut std::collections::HashMap<std::path::PathBuf, TextureHandle>,
               ctx: &egui::Context, path: &std::path::PathBuf, png: &[u8]) -> Option<TextureHandle> {
    let img = image::load_from_memory(png).ok()?.to_rgba8();
    let (w, h) = (img.width(), img.height());
    let tex = ctx.load_texture(
        format!("thumb:{}", path.display()),
        ColorImage::from_rgba_unmultiplied([w as usize, h as usize], img.as_raw()),
        TextureOptions::LINEAR);
    cache.insert(path.clone(), tex.clone());
    Some(tex)
}

/// Panel section header.
pub fn section_header(app: &App, ui: &mut egui::Ui, title: &str, extra: Option<&str>) {
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        ui.label(egui::RichText::new(title).size(13.5).strong().color(app.theme.text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(10.0);
            if let Some(e) = extra {
                ui.label(egui::RichText::new(e).size(11.0).color(app.theme.faint));
            }
        });
    });
    ui.add_space(6.0);
}

pub fn hline(app: &App, ui: &mut egui::Ui) {
    let r = ui.available_rect_before_wrap();
    ui.painter().line_segment(
        [Pos2::new(r.left() + 8.0, r.top() + 0.5), Pos2::new(r.right() - 8.0, r.top() + 0.5)],
        egui::Stroke::new(1.0, app.theme.border));
    ui.add_space(7.0);
}

/// Toast overlay (bottom-right).
pub fn draw_toast(app: &App, p: &Painter, screen: Rect, msg: &str, kind: u8, alpha: f32) {
    let font = FontId::proportional(13.0);
    let galley = p.layout_no_wrap(msg.to_string(), font, Color32::WHITE);
    let pad = Vec2::new(18.0, 10.0);
    let size = galley.size() + pad * 2.0;
    let pos = Pos2::new(screen.right() - size.x - 18.0, screen.bottom() - size.y - 18.0);
    let r = Rect::from_min_size(pos, size);
    let a = (alpha * 255.0) as u8;
    let bg = Color32::from_rgba_unmultiplied(30, 30, 36, a);
    let edge = match kind {
        1 => app.theme.ok,
        2 => app.theme.err,
        _ => app.theme.accent,
    };
    p.rect_filled(r, Rounding::same(6), bg);
    p.rect_filled(Rect::from_min_max(Pos2::new(r.left(), r.top()), Pos2::new(r.left() + 3.0, r.bottom())), 0.0, edge.gamma_multiply(alpha));
    p.galley(Pos2::new(r.left() + pad.x, r.center().y - galley.size().y / 2.0), galley, app.theme.text.gamma_multiply(alpha));
}

/// Draw a SUB-REGION of a texture into a SUB-REGION of `dst` (u0..u1 / v0..v1
/// are fractions of both the texture and the destination). Used by wipe
/// transitions — real mesh geometry, no shaders.
pub fn draw_transformed_region(
    p: &Painter, tex: &TextureHandle, dst: Rect, tf: &crate::model::Transform,
    u0: f32, u1: f32, v0: f32, v1: f32,
) {
    let size = dst.size();
    let scale = tf.scale.max(0.01);
    let center = Pos2::new(dst.center().x + tf.x * size.x / 2.0, dst.center().y + tf.y * size.y / 2.0);
    let rad = tf.rotation.to_radians();
    let (sn, cs) = rad.sin_cos();
    let hw = size.x * scale / 2.0;
    let hh = size.y * scale / 2.0;
    let corner = |sx: f32, sy: f32| -> Pos2 {
        let (lx, ly) = (sx * hw, sy * hh);
        Pos2::new(center.x + lx * cs - ly * sn, center.y + lx * sn + ly * cs)
    };
    let a = (tf.opacity.clamp(0.0, 1.0) * 255.0) as u8;
    let white = Color32::from_rgba_unmultiplied(255, 255, 255, a);
    let mut mesh = egui::Mesh::with_texture(tex.id());
    let uv = |u: f32, v: f32| Pos2::new(u, v);
    let push = |m: &mut egui::Mesh, pos: Pos2, uv: Pos2| {
        m.vertices.push(egui::epaint::Vertex { pos, uv, color: white });
        (m.vertices.len() - 1) as u32
    };
    let v00 = push(&mut mesh, corner(u0 * 2.0 - 1.0, v0 * 2.0 - 1.0), uv(u0, v0));
    let v10 = push(&mut mesh, corner(u1 * 2.0 - 1.0, v0 * 2.0 - 1.0), uv(u1, v0));
    let v11 = push(&mut mesh, corner(u1 * 2.0 - 1.0, v1 * 2.0 - 1.0), uv(u1, v1));
    let v01 = push(&mut mesh, corner(u0 * 2.0 - 1.0, v1 * 2.0 - 1.0), uv(u0, v1));
    mesh.add_triangle(v00, v10, v11);
    mesh.add_triangle(v00, v11, v01);
    p.add(Shape::mesh(mesh));
}

// ------------------------------------------------------------------ v0.3 widgets

/// Animated collapsible section (chevron + smooth height easing). Content is
/// painted when `open` so sliders inside keep live-updating during animation
/// is NOT needed — egui clips; we animate only the reveal fraction via the
/// ctx animation clock for a professional feel.
pub fn animated_section<R>(
    app: &mut App, ui: &mut egui::Ui, id_src: &str, title: &str,
    icon: impl Fn(&Painter, Rect, Color32),
    body: impl FnOnce(&mut App, &mut egui::Ui) -> R,
) -> Option<R> {
    let id = egui::Id::new(("sec", id_src));
    let mut open = ui.ctx().data(|d| d.get_temp::<bool>(id)).unwrap_or(true);
    // header
    let hdr_h = 22.0;
    let (rect, resp) = ui.allocate_exact_size(egui::Vec2::new(ui.available_width(), hdr_h), egui::Sense::click());
    let hover = resp.hovered();
    if hover {
        ui.painter().rect_filled(rect, 3.0, app.theme.panel2);
    }
    // chevron with animated rotation
    let t = ui.ctx().animate_value_with_time(id.with("anim"), if open { 1.0 } else { 0.0 }, 0.16);
    let cx = rect.left() + 12.0;
    let cy = rect.center().y;
    let u = 4.0;
    let (dy, dx) = if t > 0.5 { (u * 0.6, -u * 0.6) } else { (-u * 0.6, u * 0.6) };
    let blend = (t - 0.5).abs() * 2.0;
    let tip_y = cy + dy * (1.0 - blend) + dy * blend;
    let _ = tip_y;
    let p0 = Pos2::new(cx - dx, cy + dy * (1.0 - blend).max(0.0));
    let pm = Pos2::new(cx, cy + dy * (1.0 - blend).max(0.0) + u * 0.5 * (1.0 - blend));
    let p1 = Pos2::new(cx + dx, cy + dy * (1.0 - blend).max(0.0));
    let _ = (p0, pm);
    // simple two-line chevron, rotated by animation
    let ang = (1.0 - t) * -90.0_f32.to_radians();
    let (sn, cs) = ang.sin_cos();
    let rot = |x: f32, y: f32| Pos2::new(cx + x * cs - y * sn, cy + x * sn + y * cs);
    ui.painter().line_segment([rot(-u, -u * 0.6), rot(0.0, u * 0.5)], Stroke::new(1.6, app.theme.text));
    ui.painter().line_segment([rot(0.0, u * 0.5), rot(u, -u * 0.6)], Stroke::new(1.6, app.theme.text));
    // icon + label
    icon(ui.painter(), Rect::from_min_size(Pos2::new(rect.left() + 22.0, rect.top() + 3.0), egui::Vec2::splat(16.0)),
        if hover { app.theme.text } else { app.theme.dim });
    ui.painter().text(Pos2::new(rect.left() + 44.0, cy), Align2::LEFT_CENTER, title,
        FontId::proportional(12.5), app.theme.text);
    if resp.clicked() {
        open = !open;
        ui.ctx().data_mut(|d| d.insert_temp(id, open));
    }
    if !open { return None; }
    Some(body(app, ui))
}

/// Keyframe diamond button with active state (any kf near the playhead).
pub fn kf_button(app: &mut App, ui: &mut egui::Ui, tip: &str, has_kf_here: bool, on_add: impl FnOnce(&mut App)) {
    let (r, resp) = ui.allocate_exact_size(egui::Vec2::splat(18.0), egui::Sense::click());
    let col = if has_kf_here { app.theme.warn } else if resp.hovered() { app.theme.text } else { app.theme.dim };
    if has_kf_here {
        ui.painter().rect_filled(r, 3.0, app.theme.accent_dim.gamma_multiply(0.7));
    } else if resp.hovered() {
        ui.painter().rect_filled(r, 3.0, app.theme.panel3);
    }
    let c = r.center();
    let u = 5.0;
    ui.painter().add(egui::Shape::convex_polygon(vec![
        Pos2::new(c.x, c.y - u), Pos2::new(c.x + u, c.y), Pos2::new(c.x, c.y + u), Pos2::new(c.x - u, c.y),
    ], col, Stroke::NONE));
    resp.clone().on_hover_text(tip.to_string()).on_hover_cursor(egui::CursorIcon::PointingHand);
    if resp.clicked() { on_add(app); }
}

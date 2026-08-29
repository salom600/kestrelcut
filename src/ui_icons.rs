//! Vector-drawn UI icons (no icon fonts — crisp at any DPI, theme-aware).

use egui::{Color32, Painter, Pos2, Rect, Stroke, Vec2};

fn c(r: Rect) -> Pos2 { r.center() }

/// Polyline arc (degrees) — Shape::arc was removed in egui 0.31.
fn arc_pts(center: Pos2, radius: f32, deg0: f32, deg1: f32) -> Vec<Pos2> {
    let n = 26;
    (0..=n)
        .map(|i| {
            let t = i as f32 / n as f32;
            let a = (deg0 + (deg1 - deg0) * t).to_radians();
            Pos2::new(center.x + radius * a.cos(), center.y + radius * a.sin())
        })
        .collect()
}

fn s(r: Rect) -> f32 { r.height().min(r.width()) }

pub fn play(p: &Painter, r: Rect, col: Color32) {
    let c = c(r);
    let u = s(r) * 0.32;
    p.add(egui::Shape::convex_polygon(
        vec![Pos2::new(c.x - u * 0.7, c.y - u), Pos2::new(c.x - u * 0.7, c.y + u), Pos2::new(c.x + u * 1.1, c.y)],
        col, Stroke::NONE,
    ));
}

pub fn pause(p: &Painter, r: Rect, col: Color32) {
    let c = c(r);
    let u = s(r) * 0.28;
    p.rect_filled(Rect::from_min_max(Pos2::new(c.x - u - u * 0.3, c.y - u), Pos2::new(c.x - u * 0.3, c.y + u)), 1.0, col);
    p.rect_filled(Rect::from_min_max(Pos2::new(c.x + u * 0.3, c.y - u), Pos2::new(c.x + u + u * 0.3, c.y + u)), 1.0, col);
}

fn tri_left(p: &Painter, at: f32, r: Rect, col: Color32) {
    let c = c(r);
    let u = s(r) * 0.26;
    p.add(egui::Shape::convex_polygon(
        vec![Pos2::new(at + u, c.y - u), Pos2::new(at + u, c.y + u), Pos2::new(at - u * 0.6, c.y)],
        col, Stroke::NONE,
    ));
}
fn tri_right(p: &Painter, at: f32, r: Rect, col: Color32) {
    let c = c(r);
    let u = s(r) * 0.26;
    p.add(egui::Shape::convex_polygon(
        vec![Pos2::new(at - u, c.y - u), Pos2::new(at - u, c.y + u), Pos2::new(at + u * 0.6, c.y)],
        col, Stroke::NONE,
    ));
}
fn bar(p: &Painter, x: f32, r: Rect, col: Color32) {
    let c = c(r);
    let u = s(r) * 0.3;
    p.rect_filled(Rect::from_min_max(Pos2::new(x, c.y - u), Pos2::new(x + s(r) * 0.07, c.y + u)), 1.0, col);
}

pub fn go_start(p: &Painter, r: Rect, col: Color32) { let c = c(r); bar(p, c.x + s(r) * 0.22, r, col); tri_left(p, c.x, r, col); }
pub fn go_end(p: &Painter, r: Rect, col: Color32) { let c = c(r); bar(p, c.x - s(r) * 0.28, r, col); tri_right(p, c.x, r, col); }
pub fn prev_frame(p: &Painter, r: Rect, col: Color32) { let c = c(r); tri_left(p, c.x + s(r) * 0.08, r, col); bar(p, c.x - s(r) * 0.26, r, col); }
pub fn next_frame(p: &Painter, r: Rect, col: Color32) { let c = c(r); tri_right(p, c.x - s(r) * 0.08, r, col); bar(p, c.x + s(r) * 0.2, r, col); }

pub fn loop_icon(p: &Painter, r: Rect, col: Color32) {
    let c = c(r);
    let u = s(r) * 0.3;
    p.add(egui::Shape::line(arc_pts(c, u, 280.0, 460.0), Stroke::new(s(r) * 0.09, col)));
    let tip = Pos2::new(c.x + u * 0.95, c.y - u * 0.25);
    p.add(egui::Shape::convex_polygon(
        vec![tip, Pos2::new(tip.x - u * 0.45, tip.y - u * 0.34), Pos2::new(tip.x - u * 0.1, tip.y - u * 0.55)],
        col, Stroke::NONE,
    ));
}

pub fn mark_in(p: &Painter, r: Rect, col: Color32) {
    let c = c(r);
    let u = s(r) * 0.3;
    let st = Stroke::new(s(r) * 0.09, col);
    p.line_segment([Pos2::new(c.x - u * 0.8, c.y - u), Pos2::new(c.x - u * 0.8, c.y + u)], st);
    p.line_segment([Pos2::new(c.x - u * 0.8, c.y - u), Pos2::new(c.x + u * 0.2, c.y - u)], st);
    p.add(egui::Shape::convex_polygon(
        vec![Pos2::new(c.x - u * 0.2, c.y - u * 0.5), Pos2::new(c.x - u * 0.2, c.y + u * 0.5), Pos2::new(c.x + u * 0.9, c.y)],
        col, Stroke::NONE,
    ));
}
pub fn mark_out(p: &Painter, r: Rect, col: Color32) {
    let c = c(r);
    let u = s(r) * 0.3;
    let st = Stroke::new(s(r) * 0.09, col);
    p.line_segment([Pos2::new(c.x + u * 0.8, c.y - u), Pos2::new(c.x + u * 0.8, c.y + u)], st);
    p.line_segment([Pos2::new(c.x + u * 0.8, c.y - u), Pos2::new(c.x - u * 0.2, c.y - u)], st);
    p.add(egui::Shape::convex_polygon(
        vec![Pos2::new(c.x + u * 0.2, c.y - u * 0.5), Pos2::new(c.x + u * 0.2, c.y + u * 0.5), Pos2::new(c.x - u * 0.9, c.y)],
        col, Stroke::NONE,
    ));
}

pub fn razor(p: &Painter, r: Rect, col: Color32) {
    let c = c(r);
    let u = s(r) * 0.3;
    let st = Stroke::new(s(r) * 0.08, col);
    p.line_segment([Pos2::new(c.x - u, c.y + u * 0.9), Pos2::new(c.x + u * 0.1, c.y - u * 0.2)], st);
    p.line_segment([Pos2::new(c.x + u, c.y + u * 0.9), Pos2::new(c.x - u * 0.1, c.y - u * 0.2)], st);
    p.circle_filled(Pos2::new(c.x - u * 0.55, c.y - u * 0.75), s(r) * 0.07, col);
    p.circle_filled(Pos2::new(c.x + u * 0.55, c.y - u * 0.75), s(r) * 0.07, col);
}

pub fn camera(p: &Painter, r: Rect, col: Color32) {
    let c = c(r);
    let u = s(r) * 0.3;
    p.rect_filled(Rect::from_min_max(Pos2::new(c.x - u, c.y - u * 0.55), Pos2::new(c.x + u, c.y + u * 0.75)), 2.0, col);
    p.rect_filled(Rect::from_min_max(Pos2::new(c.x - u * 0.4, c.y - u * 0.95), Pos2::new(c.x + u * 0.4, c.y - u * 0.5)), 1.5, col);
    p.circle_filled(c, u * 0.42, Color32::from_black_alpha(180));
    p.circle_filled(c, u * 0.42, Color32::from_black_alpha(0));
    p.circle_stroke(c, u * 0.42, Stroke::new(s(r) * 0.05, Color32::from_black_alpha(160)));
}

pub fn plus(p: &Painter, r: Rect, col: Color32) {
    let c = c(r);
    let u = s(r) * 0.32;
    let st = Stroke::new(s(r) * 0.1, col);
    p.line_segment([Pos2::new(c.x - u, c.y), Pos2::new(c.x + u, c.y)], st);
    p.line_segment([Pos2::new(c.x, c.y - u), Pos2::new(c.x, c.y + u)], st);
}

pub fn minus(p: &Painter, r: Rect, col: Color32) {
    let c = c(r);
    let u = s(r) * 0.32;
    p.line_segment([Pos2::new(c.x - u, c.y), Pos2::new(c.x + u, c.y)], Stroke::new(s(r) * 0.1, col));
}

pub fn x(p: &Painter, r: Rect, col: Color32) {
    let c = c(r);
    let u = s(r) * 0.28;
    let st = Stroke::new(s(r) * 0.08, col);
    p.line_segment([Pos2::new(c.x - u, c.y - u), Pos2::new(c.x + u, c.y + u)], st);
    p.line_segment([Pos2::new(c.x - u, c.y + u), Pos2::new(c.x + u, c.y - u)], st);
}

pub fn lock_closed(p: &Painter, r: Rect, col: Color32) {
    let c = c(r);
    let u = s(r) * 0.26;
    p.rect_filled(Rect::from_min_max(Pos2::new(c.x - u, c.y - u * 0.2), Pos2::new(c.x + u, c.y + u)), 2.0, col);
    p.circle_stroke(c - Vec2::new(0.0, u * 0.35), u * 0.55, Stroke::new(s(r) * 0.08, col));
}
pub fn lock_open(p: &Painter, r: Rect, col: Color32) {
    let c = c(r);
    let u = s(r) * 0.26;
    p.rect_filled(Rect::from_min_max(Pos2::new(c.x - u, c.y - u * 0.2), Pos2::new(c.x + u, c.y + u)), 2.0, col);
    p.line_segment([Pos2::new(c.x - u * 0.1, c.y - u * 0.4), Pos2::new(c.x - u * 0.1, c.y - u * 0.8)], Stroke::new(s(r) * 0.08, col));
    p.line_segment([Pos2::new(c.x - u * 0.1, c.y - u * 0.8), Pos2::new(c.x + u * 0.7, c.y - u * 0.8)], Stroke::new(s(r) * 0.08, col));
}

pub fn eye(p: &Painter, r: Rect, col: Color32, open: bool) {
    let c = c(r);
    let u = s(r) * 0.32;
    let st = Stroke::new(s(r) * 0.07, col);
    if open {
        p.add(egui::Shape::line(arc_pts(c, u * 1.15, 200.0, 340.0), st));
        p.add(egui::Shape::line(arc_pts(c, u * 1.15, 20.0, 160.0), st));
        p.circle_filled(c, u * 0.26, col);
    } else {
        p.line_segment([Pos2::new(c.x - u, c.y), Pos2::new(c.x + u, c.y)], st);
    }
}

pub fn mic(p: &Painter, r: Rect, col: Color32) {
    let c = c(r);
    let u = s(r) * 0.28;
    p.rect_filled(Rect::from_min_max(Pos2::new(c.x - u * 0.35, c.y - u), Pos2::new(c.x + u * 0.35, c.y + u * 0.2)), u * 0.4, col);
    p.add(egui::Shape::line(arc_pts(c + Vec2::new(0.0, u * 0.2), u * 0.62, 0.0, 180.0), Stroke::new(s(r) * 0.07, col)));
    p.line_segment([Pos2::new(c.x, c.y + u * 0.82), Pos2::new(c.x, c.y + u * 1.1)], Stroke::new(s(r) * 0.07, col));
}

pub fn letter(p: &Painter, r: Rect, col: Color32, ch: char) {
    let c = c(r);
    p.text(c, egui::Align2::CENTER_CENTER, &ch.to_string(), egui::FontId::proportional(s(r) * 0.95), col);
}

pub fn search(p: &Painter, r: Rect, col: Color32) {
    let c = c(r);
    let u = s(r) * 0.26;
    p.circle_stroke(Pos2::new(c.x - u * 0.2, c.y - u * 0.2), u, Stroke::new(s(r) * 0.08, col));
    p.line_segment([Pos2::new(c.x + u * 0.5, c.y + u * 0.5), Pos2::new(c.x + u * 1.1, c.y + u * 1.1)], Stroke::new(s(r) * 0.09, col));
}

pub fn film(p: &Painter, r: Rect, col: Color32) {
    let c = c(r);
    let u = s(r) * 0.32;
    p.rect_stroke(Rect::from_min_max(Pos2::new(c.x - u, c.y - u * 0.7), Pos2::new(c.x + u, c.y + u * 0.7)), 1.5, Stroke::new(s(r) * 0.07, col), egui::StrokeKind::Outside);
    for i in 0..3 {
        let x = c.x - u + u * 0.35 + i as f32 * u * 0.65;
        p.rect_filled(Rect::from_min_max(Pos2::new(x - u * 0.12, c.y - u * 0.45), Pos2::new(x + u * 0.12, c.y + u * 0.45)), 1.0, col);
    }
}

/// Hollow film icon — used to distinguish the "video" pool filter.
pub fn film_outline(p: &Painter, r: Rect, col: Color32) {
    let c = c(r);
    let u = s(r) * 0.32;
    let st = Stroke::new(s(r) * 0.08, col);
    p.rect_stroke(Rect::from_min_max(Pos2::new(c.x - u, c.y - u * 0.7), Pos2::new(c.x + u, c.y + u * 0.7)), 1.5, st, egui::StrokeKind::Outside);
    let tri = |dx: f32| egui::Shape::convex_polygon(
        vec![
            Pos2::new(c.x + dx - u * 0.16, c.y - u * 0.3),
            Pos2::new(c.x + dx - u * 0.16, c.y + u * 0.3),
            Pos2::new(c.x + dx + u * 0.28, c.y),
        ],
        col, Stroke::NONE);
    p.add(tri(-u * 0.22));
    p.add(tri(u * 0.28));
}

/// 2×2 grid of squares — "show all media" filter.
pub fn grid_all(p: &Painter, r: Rect, col: Color32) {
    let c = c(r);
    let u = s(r) * 0.30;
    let g = u * 0.14;
    let sz = u - g;
    for (dx, dy) in [(-1.0f32, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let x = c.x + dx * (sz / 2.0 + g / 2.0);
        let y = c.y + dy * (sz / 2.0 + g / 2.0);
        p.rect_filled(Rect::from_center_size(Pos2::new(x, y), Vec2::splat(sz)), 1.5, col);
    }
}

pub fn note(p: &Painter, r: Rect, col: Color32) {
    let c = c(r);
    let u = s(r) * 0.28;
    p.circle_filled(Pos2::new(c.x - u * 0.4, c.y + u * 0.7), u * 0.42, col);
    p.line_segment([Pos2::new(c.x - u * 0.05, c.y + u * 0.65), Pos2::new(c.x - u * 0.05, c.y - u * 0.9)], Stroke::new(s(r) * 0.07, col));
    p.line_segment([Pos2::new(c.x - u * 0.05, c.y - u * 0.9), Pos2::new(c.x + u * 0.8, c.y - u * 0.55)], Stroke::new(s(r) * 0.07, col));
}

pub fn image_icon(p: &Painter, r: Rect, col: Color32) {
    let c = c(r);
    let u = s(r) * 0.32;
    p.rect_stroke(Rect::from_min_max(Pos2::new(c.x - u, c.y - u * 0.75), Pos2::new(c.x + u, c.y + u * 0.75)), 1.5, Stroke::new(s(r) * 0.07, col), egui::StrokeKind::Outside);
    p.circle_filled(Pos2::new(c.x - u * 0.4, c.y - u * 0.25), u * 0.18, col);
    p.add(egui::Shape::convex_polygon(
        vec![Pos2::new(c.x - u * 0.8, c.y + u * 0.55), Pos2::new(c.x - u * 0.1, c.y - u * 0.35), Pos2::new(c.x + u * 0.8, c.y + u * 0.55)],
        col, Stroke::NONE,
    ));
}

pub fn trash(p: &Painter, r: Rect, col: Color32) {
    let c = c(r);
    let u = s(r) * 0.28;
    let st = Stroke::new(s(r) * 0.07, col);
    p.rect_filled(Rect::from_min_max(Pos2::new(c.x - u * 0.8, c.y - u * 0.45), Pos2::new(c.x + u * 0.8, c.y + u)), 2.0, col);
    p.line_segment([Pos2::new(c.x - u * 0.95, c.y - u * 0.55), Pos2::new(c.x + u * 0.95, c.y - u * 0.55)], st);
    p.line_segment([Pos2::new(c.x - u * 0.3, c.y - u * 0.75), Pos2::new(c.x + u * 0.3, c.y - u * 0.75)], st);
}

pub fn undo(p: &Painter, r: Rect, col: Color32) {
    let c = c(r);
    let u = s(r) * 0.3;
    p.add(egui::Shape::line(arc_pts(c, u, 51.0, 264.0), Stroke::new(s(r) * 0.09, col)));
    let tip = Pos2::new(c.x - u * 0.62, c.y - u * 0.78);
    p.add(egui::Shape::convex_polygon(
        vec![tip, Pos2::new(tip.x + u * 0.4, tip.y - u * 0.1), Pos2::new(tip.x + u * 0.15, tip.y + u * 0.42)],
        col, Stroke::NONE,
    ));
}
pub fn redo(p: &Painter, r: Rect, col: Color32) {
    let c = c(r);
    let u = s(r) * 0.3;
    p.add(egui::Shape::line(arc_pts(c, u, -86.0, 126.0), Stroke::new(s(r) * 0.09, col)));
    let tip = Pos2::new(c.x + u * 0.62, c.y - u * 0.78);
    p.add(egui::Shape::convex_polygon(
        vec![tip, Pos2::new(tip.x - u * 0.4, tip.y - u * 0.1), Pos2::new(tip.x - u * 0.15, tip.y + u * 0.42)],
        col, Stroke::NONE,
    ));
}

pub fn save(p: &Painter, r: Rect, col: Color32) {
    let c = c(r);
    let u = s(r) * 0.3;
    p.rect_stroke(Rect::from_min_max(Pos2::new(c.x - u, c.y - u), Pos2::new(c.x + u, c.y + u)), 1.5, Stroke::new(s(r) * 0.08, col), egui::StrokeKind::Outside);
    p.rect_filled(Rect::from_min_max(Pos2::new(c.x - u * 0.5, c.y - u * 0.95), Pos2::new(c.x + u * 0.5, c.y - u * 0.2)), 1.0, col);
    p.rect_filled(Rect::from_min_max(Pos2::new(c.x - u * 0.55, c.y + u * 0.15), Pos2::new(c.x + u * 0.55, c.y + u * 0.95)), 1.0, col);
}

pub fn folder(p: &Painter, r: Rect, col: Color32) {
    let c = c(r);
    let u = s(r) * 0.32;
    p.add(egui::Shape::convex_polygon(
        vec![
            Pos2::new(c.x - u, c.y - u * 0.6), Pos2::new(c.x - u * 0.3, c.y - u * 0.6),
            Pos2::new(c.x - u * 0.05, c.y - u * 0.9), Pos2::new(c.x + u * 0.5, c.y - u * 0.9),
            Pos2::new(c.x + u * 0.5, c.y - u * 0.6), Pos2::new(c.x + u, c.y - u * 0.6),
            Pos2::new(c.x + u, c.y + u * 0.75), Pos2::new(c.x - u, c.y + u * 0.75),
        ],
        col, Stroke::NONE,
    ));
}

pub fn magnet(p: &Painter, r: Rect, col: Color32) {
    let c = c(r);
    let u = s(r) * 0.3;
    let st = Stroke::new(s(r) * 0.14, col);
    p.add(egui::Shape::line(arc_pts(c, u * 0.8, 180.0, 360.0), st));
    p.line_segment([Pos2::new(c.x - u * 0.8, c.y), Pos2::new(c.x - u * 0.8, c.y + u * 0.5)], st);
    p.line_segment([Pos2::new(c.x + u * 0.8, c.y), Pos2::new(c.x + u * 0.8, c.y + u * 0.5)], st);
}

pub fn dropper(p: &Painter, r: Rect, col: Color32) {
    let c = c(r);
    let u = s(r) * 0.3;
    let st = Stroke::new(s(r) * 0.09, col);
    p.line_segment([Pos2::new(c.x - u * 0.7, c.y + u * 0.7), Pos2::new(c.x + u * 0.4, c.y - u * 0.4)], st);
    p.line_segment([Pos2::new(c.x + u * 0.25, c.y - u * 0.55), Pos2::new(c.x + u * 0.8, c.y - u)], st);
    p.circle_filled(Pos2::new(c.x - u * 0.75, c.y + u * 0.75), s(r) * 0.07, col);
}

pub fn wand(p: &Painter, r: Rect, col: Color32) {
    let c = c(r);
    let u = s(r) * 0.3;
    p.line_segment([Pos2::new(c.x - u * 0.6, c.y + u * 0.8), Pos2::new(c.x + u * 0.4, c.y - u * 0.4)], Stroke::new(s(r) * 0.09, col));
    for (dx, dy) in [(0.7, -0.6f32), (0.95, -0.15), (0.45, -0.85)] {
        p.line_segment([Pos2::new(c.x + u * dx - u * 0.12, c.y + u * dy), Pos2::new(c.x + u * dx + u * 0.12, c.y + u * dy)], Stroke::new(s(r) * 0.06, col));
        p.line_segment([Pos2::new(c.x + u * dx, c.y + u * dy - u * 0.12), Pos2::new(c.x + u * dx, c.y + u * dy + u * 0.12)], Stroke::new(s(r) * 0.06, col));
    }
}

pub fn arrow_select(p: &Painter, r: Rect, col: Color32) {
    let c = c(r);
    let u = s(r) * 0.3;
    p.add(egui::Shape::convex_polygon(
        vec![
            Pos2::new(c.x - u * 0.6, c.y - u), Pos2::new(c.x - u * 0.6, c.y + u * 0.9),
            Pos2::new(c.x - u * 0.15, c.y + u * 0.45), Pos2::new(c.x + u * 0.15, c.y + u * 1.05),
            Pos2::new(c.x + u * 0.45, c.y + u * 0.9), Pos2::new(c.x + u * 0.18, c.y + u * 0.3),
            Pos2::new(c.x + u * 0.7, c.y + u * 0.25),
        ],
        col, Stroke::NONE,
    ));
}

pub fn slip(p: &Painter, r: Rect, col: Color32) {
    let c = c(r);
    let u = s(r) * 0.28;
    let st = Stroke::new(s(r) * 0.08, col);
    p.line_segment([Pos2::new(c.x - u * 1.2, c.y), Pos2::new(c.x - u * 0.4, c.y)], st);
    p.line_segment([Pos2::new(c.x + u * 0.4, c.y), Pos2::new(c.x + u * 1.2, c.y)], st);
    p.add(egui::Shape::convex_polygon(vec![Pos2::new(c.x - u * 0.7, c.y - u * 0.5), Pos2::new(c.x - u * 0.7, c.y + u * 0.5), Pos2::new(c.x - u * 1.3, c.y)], col, Stroke::NONE));
    p.add(egui::Shape::convex_polygon(vec![Pos2::new(c.x + u * 0.7, c.y - u * 0.5), Pos2::new(c.x + u * 0.7, c.y + u * 0.5), Pos2::new(c.x + u * 1.3, c.y)], col, Stroke::NONE));
    p.rect_filled(Rect::from_min_max(Pos2::new(c.x - u * 0.35, c.y - u * 0.55), Pos2::new(c.x + u * 0.35, c.y + u * 0.55)), 1.5, col);
}

pub fn pen(p: &Painter, r: Rect, col: Color32) {
    let c = c(r);
    let u = s(r) * 0.3;
    p.add(egui::Shape::convex_polygon(
        vec![
            Pos2::new(c.x - u * 0.7, c.y + u), Pos2::new(c.x - u * 0.45, c.y + u * 0.2),
            Pos2::new(c.x + u * 0.5, c.y - u * 0.75), Pos2::new(c.x + u * 0.85, c.y - u * 0.4),
            Pos2::new(c.x - u * 0.1, c.y + u * 0.55),
        ],
        col, Stroke::NONE,
    ));
}

pub fn hand(p: &Painter, r: Rect, col: Color32) {
    let c = c(r);
    let u = s(r) * 0.3;
    p.rect_filled(Rect::from_min_max(Pos2::new(c.x - u * 0.5, c.y - u * 0.1), Pos2::new(c.x + u * 0.5, c.y + u * 0.85)), 3.0, col);
    for i in 0..4usize {
        let x = c.x - u * 0.5 + i as f32 * u * 0.33;
        p.rect_filled(Rect::from_min_max(Pos2::new(x, c.y - u * (0.55 + 0.12 * i as f32)), Pos2::new(x + u * 0.24, c.y - u * 0.05)), 2.0, col);
    }
}

pub fn zoom_glass(p: &Painter, r: Rect, col: Color32) {
    search(p, r, col);
    let c = c(r);
    let u = s(r) * 0.26;
    p.line_segment([Pos2::new(c.x - u * 0.2 - u * 0.45, c.y - u * 0.2), Pos2::new(c.x - u * 0.2 + u * 0.45, c.y - u * 0.2)], Stroke::new(s(r) * 0.06, col));
    p.line_segment([Pos2::new(c.x - u * 0.2, c.y - u * 0.2 - u * 0.45), Pos2::new(c.x - u * 0.2, c.y - u * 0.2 + u * 0.45)], Stroke::new(s(r) * 0.06, col));
}

pub fn scissors(p: &Painter, r: Rect, col: Color32) { razor(p, r, col); }

pub fn minimize(p: &Painter, r: Rect, col: Color32) {
    let c = c(r);
    p.line_segment([Pos2::new(c.x - 4.5, c.y), Pos2::new(c.x + 4.5, c.y)], Stroke::new(1.2, col));
}
pub fn maximize(p: &Painter, r: Rect, col: Color32) {
    let c = c(r);
    p.rect_stroke(Rect::from_center_size(c, Vec2::splat(9.0)), 1.0, Stroke::new(1.2, col), egui::StrokeKind::Outside);
}
pub fn restore(p: &Painter, r: Rect, col: Color32) {
    let c = c(r);
    p.rect_stroke(Rect::from_center_size(c - Vec2::new(1.5, 1.5), Vec2::splat(8.0)), 1.0, Stroke::new(1.0, col), egui::StrokeKind::Outside);
    p.rect_stroke(Rect::from_center_size(c + Vec2::new(1.5, 1.5), Vec2::splat(8.0)), 1.0, Stroke::new(1.0, col), egui::StrokeKind::Outside);
}

pub fn chevron(p: &Painter, r: Rect, col: Color32, right: bool) {
    let c = c(r);
    let u = s(r) * 0.2;
    let d = if right { 1.0 } else { -1.0 };
    let st = Stroke::new(s(r) * 0.1, col);
    p.line_segment([Pos2::new(c.x - d * u, c.y - u), Pos2::new(c.x + d * u, c.y)], st);
    p.line_segment([Pos2::new(c.x + d * u, c.y), Pos2::new(c.x - d * u, c.y + u)], st);
}

// ------------------------------------------------------------------ v0.3 icons
pub fn safe_margins(p: &Painter, r: Rect, col: Color32) {
    let st = Stroke::new(s(r) * 0.07, col);
    let m = s(r) * 0.22;
    p.rect_stroke(Rect::from_min_max(r.min + Vec2::splat(m * 0.6), r.max - Vec2::splat(m * 0.6)), 1.0, Stroke::new(s(r) * 0.045, col.gamma_multiply(0.55)), egui::StrokeKind::Inside);
    p.rect_stroke(Rect::from_min_max(r.min + Vec2::splat(m), r.max - Vec2::splat(m)), 1.0, st, egui::StrokeKind::Inside);
}

pub fn freeze(p: &Painter, r: Rect, col: Color32) {
    // snowflake-ish asterisk: frame + star
    let c = c(r);
    let u = s(r) * 0.3;
    let st = Stroke::new(s(r) * 0.08, col);
    for a in [0.0f32, 60.0, 120.0] {
        let rad = a.to_radians();
        p.line_segment([
            Pos2::new(c.x - rad.cos() * u, c.y - rad.sin() * u),
            Pos2::new(c.x + rad.cos() * u, c.y + rad.sin() * u)], st);
    }
}

pub fn copy_icon(p: &Painter, r: Rect, col: Color32) {
    let st = Stroke::new(s(r) * 0.08, col);
    let u = s(r) * 0.26;
    let c = c(r);
    p.rect_stroke(Rect::from_min_max(Pos2::new(c.x - u * 1.5, c.y - u), Pos2::new(c.x - u * 0.1, c.y + u * 1.1)), 1.0, st, egui::StrokeKind::Inside);
    p.rect_stroke(Rect::from_min_max(Pos2::new(c.x + u * 0.1, c.y - u * 1.1), Pos2::new(c.x + u * 1.5, c.y + u)), 1.0, st, egui::StrokeKind::Inside);
}

pub fn paste_icon(p: &Painter, r: Rect, col: Color32) {
    let st = Stroke::new(s(r) * 0.08, col);
    let c = c(r);
    let u = s(r) * 0.28;
    p.rect_stroke(Rect::from_min_max(Pos2::new(c.x - u, c.y - u * 1.2), Pos2::new(c.x + u, c.y + u * 1.2)), 1.0, st, egui::StrokeKind::Inside);
    p.rect_filled(Rect::from_min_max(Pos2::new(c.x - u * 0.5, c.y - u * 1.5), Pos2::new(c.x + u * 0.5, c.y - u * 0.9)), 1.0, col);
}

pub fn group_icon(p: &Painter, r: Rect, col: Color32) {
    let st = Stroke::new(s(r) * 0.08, col);
    let c = c(r);
    let u = s(r) * 0.28;
    p.rect_stroke(Rect::from_min_max(Pos2::new(c.x - u * 1.4, c.y - u), Pos2::new(c.x - u * 0.2, c.y + u * 0.2)), 1.0, st, egui::StrokeKind::Inside);
    p.rect_stroke(Rect::from_min_max(Pos2::new(c.x + u * 0.2, c.y - u * 0.2), Pos2::new(c.x + u * 1.4, c.y + u)), 1.0, st, egui::StrokeKind::Inside);
}

pub fn keyframe(p: &Painter, r: Rect, col: Color32) {
    let c = c(r);
    let u = s(r) * 0.3;
    p.add(egui::Shape::convex_polygon(vec![
        Pos2::new(c.x, c.y - u), Pos2::new(c.x + u, c.y), Pos2::new(c.x, c.y + u), Pos2::new(c.x - u, c.y),
    ], col, Stroke::NONE));
}

pub fn roll(p: &Painter, r: Rect, col: Color32) {
    let st = Stroke::new(s(r) * 0.08, col);
    let c = c(r);
    let u = s(r) * 0.3;
    p.line_segment([Pos2::new(c.x, c.y - u), Pos2::new(c.x, c.y + u)], st);
    let arr = |x: f32, dir: f32| {
        p.add(egui::Shape::convex_polygon(vec![
            Pos2::new(x + dir * u * 0.5, c.y), Pos2::new(x, c.y - u * 0.35), Pos2::new(x, c.y + u * 0.35)], col, Stroke::NONE));
    };
    arr(c.x - u * 0.25, -1.0);
    arr(c.x + u * 0.25, 1.0);
}

pub fn slide_icon(p: &Painter, r: Rect, col: Color32) {
    let st = Stroke::new(s(r) * 0.08, col);
    let c = c(r);
    let u = s(r) * 0.3;
    p.rect_filled(Rect::from_min_max(Pos2::new(c.x - u * 0.35, c.y - u * 0.7), Pos2::new(c.x + u * 0.35, c.y + u * 0.7)), 1.0, col);
    p.line_segment([Pos2::new(c.x - u, c.y), Pos2::new(c.x - u * 0.4, c.y)], st);
    p.line_segment([Pos2::new(c.x + u * 0.4, c.y), Pos2::new(c.x + u, c.y)], st);
}

pub fn speed_icon(p: &Painter, r: Rect, col: Color32) {
    let c = c(r);
    let u = s(r) * 0.32;
    p.add(egui::Shape::convex_polygon(vec![
        Pos2::new(c.x - u, c.y - u * 0.8), Pos2::new(c.x - u, c.y + u * 0.8), Pos2::new(c.x + u, c.y)], col, Stroke::NONE));
    let st = Stroke::new(s(r) * 0.07, col);
    p.line_segment([Pos2::new(c.x - u * 1.2, c.y - u * 0.9), Pos2::new(c.x - u * 0.7, c.y - u * 0.9)], st);
    p.line_segment([Pos2::new(c.x - u * 1.2, c.y + u * 0.9), Pos2::new(c.x - u * 0.7, c.y + u * 0.9)], st);
}

pub fn subtitle_icon(p: &Painter, r: Rect, col: Color32) {
    let st = Stroke::new(s(r) * 0.07, col);
    let c = c(r);
    let u = s(r) * 0.34;
    p.rect_stroke(Rect::from_min_max(Pos2::new(c.x - u, c.y - u * 0.75), Pos2::new(c.x + u, c.y + u * 0.75)), 1.0, st, egui::StrokeKind::Inside);
    p.line_segment([Pos2::new(c.x - u * 0.65, c.y + u * 0.25), Pos2::new(c.x + u * 0.05, c.y + u * 0.25)], st);
    p.line_segment([Pos2::new(c.x + u * 0.25, c.y + u * 0.25), Pos2::new(c.x + u * 0.65, c.y + u * 0.25)], st);
}

pub fn adjustment(p: &Painter, r: Rect, col: Color32) {
    // three stacked translucent bars with the top one emphasized
    let c = c(r);
    let u = s(r) * 0.3;
    let rows = [(60u8, -1.0f32), (110, -0.2), (255, 0.6)];
    for (a, dy) in rows {
        let y = c.y + dy * u;
        p.rect_filled(Rect::from_min_max(Pos2::new(c.x - u, y), Pos2::new(c.x + u, y + u * 0.45)),
            1.0, Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), a));
    }
}

pub fn reverse(p: &Painter, r: Rect, col: Color32) {
    let c = c(r);
    let u = s(r) * 0.3;
    let st = Stroke::new(s(r) * 0.08, col);
    p.add(egui::Shape::line(arc_pts(c, u, 130.0, 320.0), st));
    let tip = Pos2::new(c.x - u * 0.75, c.y + u * 0.55);
    p.add(egui::Shape::convex_polygon(vec![
        tip, Pos2::new(tip.x + u * 0.5, tip.y - u * 0.1), Pos2::new(tip.x + u * 0.18, tip.y - u * 0.55)], col, Stroke::NONE));
}

//! Timeline: ruler, V1–V3 / A1–A3 tracks, clip painting (filmstrips,
//! waveforms, keyframes), tools (select/razor/slip/pen/hand/zoom/text),
//! magnetic snapping, zoom & smooth scroll — mirrors the reference timeline.

use crate::app::{App, Drag};
use crate::i18n::K;
use crate::model::{Clip, ClipKind, TrackKind, MIN_CLIP_DUR};
use crate::player::Tool;
use crate::ui_common::icon_toggle;
use crate::ui_icons as ico;
use egui::{Align2, Color32, FontId, Pos2, Rect, Rounding, Sense, Stroke, Vec2};

const RAIL_W: f32 = 30.0;
const HEADER_W: f32 = 88.0;
const RULER_H: f32 = 22.0;
const SCROLL_H: f32 = 12.0;

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    // header row
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        let (tr_r, _) = ui.allocate_exact_size(egui::vec2(150.0, 22.0), egui::Sense::hover());
        ui.painter().rect_filled(tr_r, 3.0, app.theme.panel2);
        ui.painter().text(Pos2::new(tr_r.left() + 10.0, tr_r.center().y), Align2::LEFT_CENTER,
            &app.project.seq_name, FontId::proportional(12.0), app.theme.accent_text);
        let tc = crate::util::timecode(app.player.clock, app.project.fps);
        ui.add_space(6.0);
        ui.label(egui::RichText::new(tc).size(14.0).strong().color(app.theme.accent_text).monospace());
        // magnetic snap toggle (real: gates all timeline snapping)
        let snap_r = Rect::from_center_size(Pos2::new(ui.cursor().left() + 12.0, ui.cursor().center().y), Vec2::splat(22.0));
        let snap_resp = ui.allocate_rect(snap_r, egui::Sense::click());
        let snap_col = if app.snap { app.theme.accent } else { app.theme.dim };
        if app.snap { ui.painter().rect_filled(snap_r, 4.0, app.theme.accent_dim.gamma_multiply(0.5)); }
        ico::magnet(ui.painter(), snap_r.shrink(3.0), snap_col);
        if snap_resp.clicked() { app.snap = !app.snap; }
        if snap_resp.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(10.0);
            // zoom slider (like the reference NLEs' timeline zoom control)
            let mut zf = app.zoom;
            let slider_w = (ui.available_width() - 90.0).clamp(80.0, 240.0);
            let sl = egui::Slider::new(&mut zf, 4.0..=4000.0).logarithmic(true)
                .show_value(false).text("");
            if ui.add_sized([slider_w, 16.0], sl).changed() { app.zoom = zf; }
            ui.label(egui::RichText::new(format!("{:.0} px/s", app.zoom)).size(10.0).color(app.theme.faint));
        });
    });
    ui.add_space(2.0);

    let outer = ui.available_rect_before_wrap();
    let body = Rect::from_min_max(
        Pos2::new(outer.left(), outer.top() + 2.0),
        Pos2::new(outer.right(), outer.bottom() - 2.0));
    ui.allocate_rect(body, Sense::hover());

    // rail
    let rail = Rect::from_min_max(body.min, Pos2::new(body.min.x + RAIL_W, body.max.y));
    ui.painter().rect_filled(rail, 0.0, app.theme.panel);
    tool_rail(app, ui, rail);

    // headers column
    let headers = Rect::from_min_max(
        Pos2::new(rail.right(), body.min.y),
        Pos2::new(rail.right() + HEADER_W, body.max.y));
    ui.painter().rect_filled(headers, 0.0, app.theme.panel);
    track_headers(app, ui, headers);

    // canvas
    let canvas = Rect::from_min_max(
        Pos2::new(headers.right() + 1.0, body.min.y),
        Pos2::new(body.right() - 1.0, body.max.y));
    canvas_ui(app, ui, canvas);
}

fn tool_rail(app: &mut App, ui: &mut egui::Ui, rail: Rect) {
    let mut y = rail.top() + 6.0;
    let tools: [(Tool, K, fn(&egui::Painter, Rect, Color32)); 7] = [
        (Tool::Select, K::ToolSelect, ico::arrow_select),
        (Tool::Razor, K::ToolRazor, ico::razor),
        (Tool::Slip, K::ToolSlip, ico::slip),
        (Tool::Pen, K::ToolPen, ico::pen),
        (Tool::Hand, K::ToolHand, ico::hand),
        (Tool::Zoom, K::ToolZoom, ico::zoom_glass),
        (Tool::Text, K::ToolText, |p: &egui::Painter, r: Rect, c: Color32| ico::letter(p, r, c, 'T')),
    ];
    for (tool, key, icon) in tools {
        let r = Rect::from_min_size(Pos2::new(rail.left() + 4.0, y), Vec2::splat(22.0));
        let resp = ui.allocate_rect(r, Sense::click());
        let active = app.tool == tool;
        let col = if active { app.theme.accent } else if resp.hovered() { app.theme.text } else { app.theme.dim };
        if active {
            ui.painter().rect_filled(r, 4.0, app.theme.accent_dim.gamma_multiply(0.5));
        }
        icon(ui.painter(), r.shrink(2.0), col);
        if resp.clicked() { app.tool = tool; }
        y += 24.0;
    }
}

fn track_rows(app: &App) -> Vec<(u64, TrackKind)> {
    // display order: V3,V2,V1 then A1,A2,A3
    let mut rows: Vec<(u64, TrackKind)> = Vec::new();
    for tr in app.project.video_tracks().iter().rev() {
        rows.push((tr.id, TrackKind::Video));
    }
    for tr in app.project.audio_tracks() {
        rows.push((tr.id, TrackKind::Audio));
    }
    rows
}

fn track_headers(app: &mut App, ui: &mut egui::Ui, rect: Rect) {
    let p = ui.painter().clone();
    let mut y = rect.top() + RULER_H;
    let rows = track_rows(app);
    for (id, kind) in rows {
        let h = match kind { TrackKind::Video => app.track_h_video, TrackKind::Audio => app.track_h_audio };
        let r = Rect::from_min_max(Pos2::new(rect.left(), y), Pos2::new(rect.right(), y + h));
        p.line_segment([Pos2::new(r.left(), r.bottom()), Pos2::new(r.right(), r.bottom())], Stroke::new(1.0, app.theme.border));
        // name
        let name = app.project.track(id).map(|t| t.name.clone()).unwrap_or_default();
        p.text(Pos2::new(r.left() + 8.0, r.center().y), Align2::LEFT_CENTER, name, FontId::proportional(11.5), app.theme.text);
        // toggles
        let tr = app.project.track(id).cloned().unwrap();
        let mut bx = r.right() - 20.0;
        let toggles: Vec<(&str, bool, fn(&mut crate::model::Track, bool))> = match kind {
            TrackKind::Video => vec![
                ("lock", tr.locked, |t, v| t.locked = v),
                ("eye", !tr.hidden, |t, v| t.hidden = !v),
            ],
            TrackKind::Audio => vec![
                ("lock", tr.locked, |t, v| t.locked = v),
                ("mute", tr.mute, |t, v| t.mute = v),
                ("solo", tr.solo, |t, v| t.solo = v),
            ],
        };
        for (what, on, setter) in toggles {
            let br = Rect::from_center_size(Pos2::new(bx, r.center().y), Vec2::splat(16.0));
            let resp = crate::ui_common::icon_toggle_shared(app, ui, br, egui::Id::new(("trk", id, what)), on, what);
            if resp.clicked() {
                if let Some(t) = app.project.track_mut(id) { setter(t, !on); }
                if what == "eye" { app.invalidate_preview(); }
            }
            bx -= 17.0;
        }
        y += h;
    }
}

pub fn canvas_ui(app: &mut App, ui: &mut egui::Ui, canvas: Rect) {
    let p = ui.painter().clone();
    let zoom = app.zoom as f32;
    let t0 = app.scroll_t;

    // ---- input first
    let resp = ui.allocate_rect(canvas, Sense::click_and_drag());
    let pos = resp.interact_pointer_pos();
    let hover_t = pos.map(|p| t0 + ((p.x - canvas.left()) / zoom).max(0.0) as f64);

    // wheel: horizontal scroll / ctrl = zoom
    ui.input(|i| {
        let mut scroll = 0.0;
        let mut zoom_delta = 0.0;
        for ev in &i.events {
            if let egui::Event::MouseWheel { unit, delta, modifiers, .. } = ev {
                let dy = match unit { egui::MouseWheelUnit::Line => delta.y * 24.0, _ => delta.y };
                if modifiers.ctrl || modifiers.command {
                    zoom_delta += dy;
                } else {
                    scroll += dy;
                }
            }
        }
        if zoom_delta.abs() > 0.1 {
            let factor = (1.0 + zoom_delta / 600.0).clamp(0.5, 2.0);
            let anchor = hover_t.unwrap_or(t0);
            app.zoom = (app.zoom * factor as f64).clamp(4.0, 4000.0);
            app.scroll_t = (anchor - ((pos.unwrap_or(canvas.min).x - canvas.left()) / app.zoom as f32) as f64).max(0.0);
        }
        if scroll.abs() > 0.1 {
            app.scroll_t = (app.scroll_t - (scroll / zoom) as f64).max(0.0);
        }
    });

    // follow playhead while playing
    if app.player.playing {
        let visible = (canvas.width() / zoom) as f64;
        let ph = app.player.clock;
        if ph > t0 + visible * 0.92 || ph < t0 {
            app.scroll_t = (ph - visible * 0.1).max(0.0);
        }
    }

    let t0 = app.scroll_t;
    let zoom_f = app.zoom;
    let x_of = move |t: f64| canvas.left() + ((t - t0) * zoom_f) as f32;

    // ---- paint background rows
    let p = ui.painter();
    p.rect_filled(canvas, 0.0, app.theme.lane);
    let mut y = canvas.top() + RULER_H;
    let rows = track_rows(app);
    for (i, (id, kind)) in rows.iter().enumerate() {
        let h = match kind { TrackKind::Video => app.track_h_video, TrackKind::Audio => app.track_h_audio };
        let r = Rect::from_min_max(Pos2::new(canvas.left(), y), Pos2::new(canvas.right(), y + h));
        p.rect_filled(r, 0.0, if i % 2 == 0 { app.theme.lane } else { app.theme.lane_alt });
        p.line_segment([Pos2::new(canvas.left(), r.bottom()), Pos2::new(canvas.right(), r.bottom())], Stroke::new(1.0, app.theme.border));
        let locked = app.project.track(*id).map(|t| t.locked).unwrap_or(false);
        if locked {
            p.rect_filled(r, 0.0, Color32::from_black_alpha(40));
        }
        let _ = id;
        y += h;
    }

    // ---- ruler
    let ruler = Rect::from_min_max(canvas.min, Pos2::new(canvas.right(), canvas.top() + RULER_H));
    p.rect_filled(ruler, 0.0, app.theme.ruler_bg);
    let visible = (canvas.width() / zoom) as f64;
    let step = crate::util::nice_step(visible);
    let mut tick = (t0 / step).floor() * step;
    while tick <= t0 + visible + step {
        let x = x_of(tick);
        if x >= canvas.left() - 1.0 && x <= canvas.right() + 1.0 {
            let major = (tick / step).round().fract().abs() < 1e-6;
            p.line_segment([Pos2::new(x, ruler.bottom() - 8.0), Pos2::new(x, ruler.bottom())], Stroke::new(1.0, app.theme.border2));
            if step >= 0.25 {
                p.text(Pos2::new(x + 4.0, ruler.center().y - 2.0), Align2::LEFT_CENTER,
                    &crate::util::timecode(tick, app.project.fps), FontId::monospace(9.0), app.theme.faint);
            }
            let _ = major;
        }
        tick += step;
    }
    // in/out band on ruler
    let dur = app.project.duration().max(0.001);
    if let Some(i) = app.project.in_mark {
        let o = app.project.out_mark.unwrap_or(dur);
        p.rect_filled(Rect::from_min_max(Pos2::new(x_of(i), ruler.top()), Pos2::new(x_of(o), ruler.bottom())), 0.0, app.theme.io_band);
    }
    // markers
    for (mt, _name) in &app.project.markers {
        let x = x_of(*mt);
        p.line_segment([Pos2::new(x, ruler.bottom()), Pos2::new(x, y)], Stroke::new(1.0, app.theme.warn.gamma_multiply(0.6)));
    }

    // ---- clips
    let mut y = canvas.top() + RULER_H;
    let mut interactions: Vec<(u64, u64, Rect)> = Vec::new(); // (track_id, clip_id, rect)
    for (id, kind) in rows.iter() {
        let h = match kind { TrackKind::Video => app.track_h_video, TrackKind::Audio => app.track_h_audio };
        let row = Rect::from_min_max(Pos2::new(canvas.left(), y), Pos2::new(canvas.right(), y + h));
        let track = app.project.track(*id).cloned().unwrap();
        for c in track.sorted_clips() {
            let x1 = x_of(c.tl_start);
            let x2 = x_of(c.end());
            if x2 < canvas.left() - 4.0 || x1 > canvas.right() + 4.0 { continue; }
            let cr = Rect::from_min_max(
                Pos2::new(x1.max(canvas.left()), row.top() + 2.0),
                Pos2::new(x2.min(canvas.right()), row.bottom() - 2.0));
            paint_clip(app, p.clone(), &cr, c, *kind, x1 < canvas.left(), x2 > canvas.right());
            interactions.push((*id, c.id, cr));
        }
        y += h;
    }

    // ---- scrub & tool interactions
    let seq_dur = app.project.duration();
    if resp.dragged() && matches!(app.drag, Some(Drag::HScroll { .. })) {
        if let (Some(p), Some(Drag::HScroll { grab_t, grab_x })) = (pos, app.drag.clone()) {
            app.scroll_t = (grab_t - (p.x - grab_x) as f64 / app.zoom).max(0.0);
        }
    } else if resp.drag_started() {
        if let Some(pt) = pos {
            let in_ruler = pt.y < canvas.top() + RULER_H;
            if in_ruler || matches!(app.tool, Tool::Hand) {
                app.drag = Some(Drag::HScroll { grab_t: t0, grab_x: pt.x });
            } else {
                press(app, &interactions, pt, canvas, &resp);
            }
        }
    } else if resp.dragged() {
        if let Some(pt) = pos {
            let in_ruler = pt.y < canvas.top() + RULER_H;
            if in_ruler && app.drag.is_none() {
                // scrub — Still mode regrabs per bucket; the last frame stays
                // visible so scrubbing never flashes black
                let t = t0 + (pt.x - canvas.left()).max(0.0) as f64 / app.zoom;
                app.player.seek(t.min(seq_dur));
            } else {
                drag_update(app, pt, canvas);
            }
        }
    }
    if resp.drag_stopped() {
        drag_end(app);
        if let Some(Drag::HScroll { .. }) = app.drag { app.drag = None; }
    }

    // ---- context menu on canvas
    resp.context_menu(|ui| {
        if ui.button(app.t(K::SplitPlayhead)).clicked() { app.split_at_playhead(); ui.close_menu(); }
        if ui.button(app.t(K::AddTitleClip)).clicked() { app.add_title_at_playhead(); ui.close_menu(); }
        if ui.button(format!("{} (M)", app.t(K::Markers))).clicked() { app.project.add_marker(app.player.clock); app.commit(); ui.close_menu(); }
    });

    // ---- playhead line
    let px = x_of(app.player.clock);
    if px >= canvas.left() && px <= canvas.right() {
        p.line_segment([Pos2::new(px, canvas.top()), Pos2::new(px, y)], Stroke::new(1.6, app.theme.playhead));
        p.add(egui::Shape::convex_polygon(
            vec![Pos2::new(px - 5.0, canvas.top() + RULER_H - 9.0), Pos2::new(px + 5.0, canvas.top() + RULER_H - 9.0), Pos2::new(px, canvas.top() + RULER_H - 1.0)],
            app.theme.playhead, Stroke::NONE));
    }

    // ---- bottom scroll bar
    let sb = Rect::from_min_max(Pos2::new(canvas.left(), canvas.bottom() - SCROLL_H), Pos2::new(canvas.right(), canvas.bottom()));
    p.rect_filled(sb, 3.0, app.theme.panel2);
    let total_w = (seq_dur.max(visible) * app.zoom) as f32;
    let frac = (canvas.width() / total_w.max(1.0)).clamp(0.05, 1.0);
    let bx = sb.left() + frac * sb.width() * ((t0 / seq_dur.max(visible)).clamp(0.0, 1.0) as f32);
    let bw = frac * sb.width();
    p.rect_filled(Rect::from_min_max(Pos2::new(bx, sb.top() + 2.0), Pos2::new(bx + bw, sb.bottom() - 2.0)), 3.0, app.theme.border2);
}

// ---------------------------------------------------------------- painting
fn paint_clip(app: &App, p: egui::Painter, r: &Rect, c: &Clip, kind: TrackKind, clipped_l: bool, clipped_r: bool) {
    let sel = app.sel == Some(c.id);
    let (fill, edge) = match c.kind {
        ClipKind::Video => (app.theme.clip_video, app.theme.clip_video_edge),
        ClipKind::Audio => (app.theme.clip_audio, app.theme.clip_audio_edge),
        ClipKind::Title => (app.theme.clip_title, app.theme.clip_title_edge),
        ClipKind::Image => (app.theme.clip_image, app.theme.clip_image_edge),
    };
    let fill = if sel { fill.gamma_multiply(1.35) } else { fill };
    p.rect_filled(*r, 3.0, fill);
    p.rect_stroke(*r, 3.0, Stroke::new(if sel { 2.0 } else { 1.0 }, if sel { app.theme.accent } else { edge }), egui::StrokeKind::Inside);

    // trim handles
    if sel {
        for x in [r.left(), r.right() - 5.0] {
            p.rect_filled(Rect::from_min_size(Pos2::new(x, r.top() + 1.0), Vec2::new(5.0, r.height() - 2.0)), 2.0, Color32::from_white_alpha(200));
        }
    }

    // content
    match c.kind {
        ClipKind::Video | ClipKind::Image => {
            let path = c.source.clone();
            let tex = path.and_then(|pth| app.thumbs.get(&pth).cloned().flatten());
            if let Some(tex) = tex {
                let tw = r.height() * 16.0 / 9.0;
                let tiles = ((r.width() / tw).ceil() as i32).clamp(1, 40);
                for i in 0..tiles {
                    let tx = r.left() + i as f32 * tw;
                    if tx > r.right() { break; }
                    let tr = Rect::from_min_max(Pos2::new(tx.max(r.left()), r.top() + 1.0), Pos2::new((tx + tw).min(r.right()), r.bottom() - 1.0));
                    if tr.width() < 1.0 { break; }
                    // uv window proportional to visible slice
                    let u0 = ((r.left() - tx) / tw).clamp(0.0, 1.0);
                    let u1 = ((r.right() - tx) / tw).clamp(0.0, 1.0);
                    p.image(tex.id(), tr, Rect::from_min_max(Pos2::new(u0, 0.0), Pos2::new(u1, 1.0)), Color32::from_white_alpha(210));
                }
            }
            // name plate
            p.rect_filled(Rect::from_min_max(r.min, Pos2::new(r.right(), r.top() + 13.0)), Rounding { nw: 3, ne: 3, sw: 0, se: 0 }, Color32::from_black_alpha(110));
            p.text(Pos2::new(r.left() + 5.0, r.top() + 6.5), Align2::LEFT_CENTER,
                &c.name, FontId::proportional(9.5), app.theme.text);
        }
        ClipKind::Audio => {
            let path = c.source.clone();
            if let Some(peaks) = path.as_ref().and_then(|pth| app.waves.get(pth)) {
                draw_wave(p.clone(), r, c, peaks, app.zoom as f32);
            }
            p.rect_filled(Rect::from_min_max(r.min, Pos2::new(r.right(), r.top() + 13.0)), Rounding { nw: 3, ne: 3, sw: 0, se: 0 }, Color32::from_black_alpha(110));
            p.text(Pos2::new(r.left() + 5.0, r.top() + 6.5), Align2::LEFT_CENTER,
                &c.name, FontId::proportional(9.5), app.theme.text);
            // keyframe dots
            let zoom = app.zoom as f32;
            for (i, (kt, kg)) in c.vol_kf.iter().enumerate() {
                let kx = r.left() + (*kt as f32 * zoom);
                let ky = r.bottom() - 4.0 - ((kg - 0.0).clamp(0.0, 2.0) / 2.0) * (r.height() - 10.0);
                let dragging = matches!(app.drag, Some(Drag::Kf { idx, clip }) if clip == c.id && idx == i);
                p.circle_filled(Pos2::new(kx, ky), if dragging { 4.0 } else { 3.0 }, Color32::from_white_alpha(235));
            }
            let _ = kind;
        }
        ClipKind::Title => {
            let txt = c.title.as_ref().map(|t| t.text.clone()).unwrap_or_default();
            p.text(Pos2::new(r.center().x, r.center().y), Align2::CENTER_CENTER,
                &txt, FontId::proportional(11.0), Color32::from_white_alpha(240));
            p.text(Pos2::new(r.left() + 5.0, r.top() + 6.5), Align2::LEFT_CENTER,
                &c.name, FontId::proportional(9.0), Color32::from_white_alpha(210));
        }
    }
    let _ = (clipped_l, clipped_r);
}

fn draw_wave(p: egui::Painter, r: &Rect, c: &Clip, peaks: &std::sync::Arc<Vec<(i8, i8)>>, zoom: f32) {
    if peaks.is_empty() || zoom <= 0.0 { return; }
    let mid = r.center().y + 4.0;
    let amp = (r.height() - 14.0) / 2.0;
    let mut pts_top: Vec<Pos2> = Vec::new();
    let mut pts_bot: Vec<Pos2> = Vec::new();
    let step_px = 2.0_f32;
    let mut x = 0.0f32;
    while x < r.width() {
        let tl = c.tl_start + (x / zoom) as f64;
        let src_t = c.src_in + (tl - c.tl_start) * c.speed as f64;
        let idx = ((src_t * 50.0) as usize).min(peaks.len() - 1);
        let (mn, mx) = peaks[idx];
        let y_top = mid - (mx as f32 / 127.0) * amp;
        let y_bot = mid - (mn as f32 / 127.0) * amp;
        pts_top.push(Pos2::new(r.left() + x, y_top.max(r.top() + 2.0)));
        pts_bot.push(Pos2::new(r.left() + x, y_bot.min(r.bottom() - 3.0)));
        x += step_px;
    }
    let mut poly = pts_top;
    poly.extend(pts_bot.iter().rev().copied());
    p.add(egui::Shape::convex_polygon(poly, Color32::from_white_alpha(150), Stroke::NONE));
    p.line_segment([Pos2::new(r.left(), mid), Pos2::new(r.right(), mid)], Stroke::new(0.8, Color32::from_white_alpha(90)));
}

// ---------------------------------------------------------------- interaction
fn clip_at(interactions: &[(u64, u64, Rect)], pos: Pos2) -> Option<(u64, u64, Rect)> {
    interactions.iter().rev()
        .find(|(_, _, r)| r.contains(pos))
        .map(|(t, c, r)| (*t, *c, *r))
}

fn press(app: &mut App, interactions: &[(u64, u64, Rect)], pt: Pos2, canvas: Rect, _resp: &egui::Response) {
    let hit = clip_at(interactions, pt);
    match app.tool {
        Tool::Razor => {
            if let Some((_tr, cid, r)) = hit {
                let t = app.scroll_t + (pt.x - canvas.left()) as f64 / app.zoom;
                app.commit();
                app.project.split_clip(cid, t);
                app.commit();
                app.toast(app.t(K::SplitHere), 0);
                let _ = r;
            }
        }
        Tool::Text => {
            let t = app.scroll_t + (pt.x - canvas.left()) as f64 / app.zoom;
            let on_video = pt.y < canvas.top() + RULER_H + app.track_h_video * 3.0;
            if on_video {
                if let Some(tid) = app.project.video_tracks().iter()
                    .find(|tr| !tr.locked && tr.clips.iter().all(|c| t < c.tl_start || t >= c.end()))
                    .map(|tr| tr.id)
                {
                    app.commit();
                    let c = crate::model::title_clip("Title", t, 3.0);
                    app.sel = Some(c.id);
                    app.project.place_clip(c, tid);
                    app.commit();
                }
            }
        }
        Tool::Zoom => {
            let t = app.scroll_t + (pt.x - canvas.left()) as f64 / app.zoom;
            let f = if pt.y < canvas.top() + RULER_H || !ui_alt() { 1.5 } else { 1.0 / 1.5 };
            app.zoom = (app.zoom * f).clamp(4.0, 4000.0);
            app.scroll_t = (t - (pt.x - canvas.left()) as f64 / app.zoom).max(0.0);
        }
        Tool::Pen => {
            if let Some((_tr, cid, r)) = hit {
                if app.project.clip(cid).map(|(_, c)| c.is_audio()).unwrap_or(false) {
                    let kt = (pt.x - r.left()) / app.zoom as f32;
                    app.commit();
                    if let Some(c) = app.project.clip_mut(cid) {
                        let g = 1.0 - ((pt.y - r.top()) / r.height()).clamp(0.0, 1.0) * 1.6;
                        c.vol_kf.push((kt as f64, g.max(0.05)));
                        c.vol_kf.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                    }
                    app.commit();
                }
            }
        }
        _ => {
            // select family
            if let Some((track_id, cid, r)) = hit {
                app.sel = Some(cid);
                let locked = app.project.track(track_id).map(|t| t.locked).unwrap_or(false);
                if locked { app.drag = None; return; }
                let edge = 6.0;
                if pt.x - r.left() < edge {
                    app.drag = Some(Drag::TrimL { id: cid });
                } else if r.right() - pt.x < edge {
                    app.drag = Some(Drag::TrimR { id: cid });
                } else if app.tool == Tool::Slip {
                    let src = app.project.clip(cid).map(|(_, c)| c.src_in).unwrap_or(0.0);
                    app.drag = Some(Drag::Slip { id: cid, grab_src: src, grab_x: pt.x });
                } else {
                    let grab_off = (app.scroll_t + (pt.x - canvas.left()) as f64 / app.zoom) - r_timeline_start(app, cid);
                    app.drag = Some(Drag::ClipMove { id: cid, grab_off, moved: false });
                    app.commit(); // pre-move snapshot
                }
            } else {
                app.sel = None;
            }
        }
    }
}

fn r_timeline_start(app: &App, cid: u64) -> f64 {
    app.project.clip(cid).map(|(_, c)| c.tl_start).unwrap_or(0.0)
}

fn ui_alt() -> bool { false }

fn drag_update(app: &mut App, pt: Pos2, canvas: Rect) {
    let Some(drag) = app.drag.clone() else { return };
    let t = app.scroll_t + ((pt.x - canvas.left()).max(0.0)) as f64 / app.zoom;
    match drag {
        Drag::ClipMove { id, grab_off, moved } => {
            let Some((old_tr, c)) = app.project.clip(id) else { return };
            let (old_tr_id, dur) = (old_tr.id, c.src_dur);
            let mut new_start = (t - grab_off).max(0.0);
            // magnetic snapping (can be toggled off with the magnet button)
            if app.snap && !ui_alt_down() {
                let snap_thresh = 8.0 / app.zoom;
                if let Some(snapped) = crate::util::snap_to(new_start, &app.project.snap_candidates(id, app.player.clock), snap_thresh) {
                    new_start = snapped;
                } else if let Some(snapped) = crate::util::snap_to(new_start + dur, &app.project.snap_candidates(id, app.player.clock), snap_thresh) {
                    new_start = snapped - dur;
                }
            }
            // hovered track
            let y = pt.y;
            let rows = track_rows(app);
            let mut ycur = canvas.top() + 22.0;
            let mut target = old_tr_id;
            for (tid, kind) in rows {
                let h = match kind { TrackKind::Video => app.track_h_video, TrackKind::Audio => app.track_h_audio };
                if y >= ycur && y < ycur + h { target = tid; }
                ycur += h;
            }
            let _ = moved;
            app.project.move_clip(id, target, new_start);
            if let Some(d) = app.drag.as_mut() {
                if let Drag::ClipMove { moved, .. } = d { *moved = true; }
            }
            app.invalidate_preview();
        }
        Drag::TrimL { id } => {
            let cur = app.project.clip(id).map(|(_, c)| c.tl_start).unwrap_or(0.0);
            let delta = t - cur;
            app.project.trim_left(id, delta);
            app.invalidate_preview();
        }
        Drag::TrimR { id } => {
            let src_total = app.project.clip(id)
                .and_then(|(_, c)| c.source.clone())
                .and_then(|p| app.assets.iter().find(|a| a.path == p))
                .map(|a| a.duration);
            let end = app.project.clip(id).map(|(_, c)| c.end()).unwrap_or(0.0);
            let delta = t - end;
            app.project.trim_right(id, delta, src_total);
            app.invalidate_preview();
        }
        Drag::Slip { id, grab_src, grab_x } => {
            let dsrc = ((pt.x - grab_x) / app.zoom as f32) as f64 * 1.0;
            let want = grab_src + dsrc;
            // total length via assets
            let total = app.project.clip(id)
                .and_then(|(_, c)| c.source.clone())
                .and_then(|p| app.assets.iter().find(|a| a.path == p))
                .map(|a| a.duration);
            if let Some(c) = app.project.clip_mut(id) {
                if let Some(total) = total {
                    c.src_in = want.max(0.0).min((total - c.src_len()).max(0.0));
                }
            }
            app.invalidate_preview();
        }
        Drag::Kf { clip, idx } => {
            if let Some(c) = app.project.clip_mut(clip) {
                if let Some(kf) = c.vol_kf.get_mut(idx) {
                    kf.1 = (1.0 - ((pt.y - canvas.top()) / canvas.height()).clamp(0.0, 1.0)) * 1.6 + 0.05;
                }
            }
        }
        Drag::HScroll { .. } => {}
    }
}

fn drag_end(app: &mut App) {
    if app.drag.is_some() {
        app.commit();
        app.drag = None;
    }
}

fn ui_alt_down() -> bool { false }

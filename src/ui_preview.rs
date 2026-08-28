//! Preview monitor (center): letterboxed composited video, timecode, quality,
//! scrub bar, transport controls — mirrors the reference center column.

use crate::app::App;
use crate::exporter;
use crate::i18n::K;
use crate::model::{ClipKind, TrackKind};
use crate::player::Quality;
use crate::ui_common::{draw_transformed, icon_btn, icon_toggle, upload_tex};
use crate::ui_icons as ico;
use egui::{Align2, Color32, FontId, Pos2, Rect, Rounding, Sense, Vec2};
use std::path::PathBuf;

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    ui.add_space(4.0);
    // sequence tab row
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        let label = app.project.seq_name.clone();
        let (r, _) = ui.allocate_exact_size(Vec2::new(150.0, 22.0), Sense::hover());
        ui.painter().rect_filled(r, Rounding { nw: 4, ne: 4, sw: 0, se: 0 }, app.theme.panel2);
        ui.painter().text(Pos2::new(r.left() + 10.0, r.center().y), Align2::LEFT_CENTER,
            &label, FontId::proportional(12.0), app.theme.accent_text);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(10.0);
            ui.label(egui::RichText::new(format!("{}×{} · {:.0}fps", app.project.width, app.project.height, app.project.fps))
                .size(10.5).color(app.theme.faint));
        });
    });
    ui.add_space(4.0);

    // video viewport
    let avail = ui.available_size();
    let viewport_h = (avail.y - 88.0).max(80.0);
    let (vr, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), viewport_h), Sense::hover());
    ui.painter().rect_filled(vr, 0.0, Color32::BLACK);
    draw_composite(app, ui, vr);

    // timecode / quality / zoom row
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        let tc = crate::util::timecode(app.player.clock, app.project.fps);
        ui.label(egui::RichText::new(tc).size(15.0).strong().color(app.theme.accent_text).monospace());
        ui.add_space(10.0);
        // quality dropdown
        let q = app.player.quality.unwrap_or(Quality::Half);
        egui::ComboBox::from_id_source("quality")
            .selected_text(egui::RichText::new(q.label()).size(12.0))
            .width(72.0)
            .show_ui(ui, |ui| {
                for qq in Quality::all() {
                    if ui.selectable_label(q == qq, egui::RichText::new(qq.label()).size(12.0)).clicked() {
                        app.player.quality = Some(qq); // key change restarts decoders, frame stays
                    }
                }
            });
        // zoom label
        ui.label(egui::RichText::new(app.t(K::Fit)).size(11.5).color(app.theme.dim));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(10.0);
            let total = crate::util::timecode(app.project.out_mark.unwrap_or(app.project.duration()), app.project.fps);
            ui.label(egui::RichText::new(total).size(13.0).color(app.theme.dim).monospace());
        });
    });

    // scrub bar
    let (sr, sresp) = ui.allocate_exact_size(Vec2::new(ui.available_width() - 20.0, 18.0), Sense::click_and_drag());
    ui.add_space(2.0);
    draw_scrub(app, ui.painter(), sr, sresp.dragged() || sresp.clicked());
    if sresp.dragged() || sresp.clicked() {
        if let Some(p) = sresp.interact_pointer_pos() {
            let t = ((p.x - sr.left()) / sr.width()).clamp(0.0, 1.0) as f64 * app.project.duration();
            app.player.seek(t); // Still mode regrabs per bucket; frame never blanks
        }
    }

    // transport row
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        let w = ui.available_width();
        let btn = 26.0_f32;
        let total = 11.0 * btn + 60.0;
        ui.add_space(((w - total) / 2.0).max(0.0));
        if icon_btn(app, ui, btn, &app.t(K::MarkIn), ico::mark_in).clicked() { app.project.in_mark = Some(app.player.clock); }
        if icon_btn(app, ui, btn, &app.t(K::MarkOut), ico::mark_out).clicked() { app.project.out_mark = Some(app.player.clock); }
        ui.add_space(6.0);
        if icon_btn(app, ui, btn, &app.t(K::GoStart), ico::go_start).clicked() {
            app.player.seek(app.project.in_mark.unwrap_or(0.0));
        }
        if icon_btn(app, ui, btn, &app.t(K::PrevFrame), ico::prev_frame).clicked() {
            app.player.seek(app.player.clock - 1.0 / app.project.fps);
        }
        let playing = app.player.playing;
        if icon_toggle(app, ui, btn + 4.0, playing, &if playing { app.t(K::Pause) } else { app.t(K::Play) },
            if playing { ico::pause } else { ico::play }).clicked() {
            app.toggle_play();
        }
        if icon_btn(app, ui, btn, &app.t(K::NextFrame), ico::next_frame).clicked() {
            app.player.seek(app.player.clock + 1.0 / app.project.fps);
        }
        if icon_btn(app, ui, btn, &app.t(K::GoEnd), ico::go_end).clicked() {
            app.player.seek(app.project.out_mark.unwrap_or(app.project.duration()));
        }
        ui.add_space(6.0);
        if icon_toggle(app, ui, btn, app.player.loop_play, &app.t(K::Loop), ico::loop_icon).clicked() {
            app.player.loop_play = !app.player.loop_play;
        }
        if icon_btn(app, ui, btn, &app.t(K::SplitHere), ico::razor).clicked() { app.split_at_playhead(); }
        if icon_btn(app, ui, btn, &app.t(K::Snapshot), ico::camera).clicked() { snapshot(app); }
        if icon_btn(app, ui, btn, &app.t(K::AddTitleClip), ico::plus).clicked() { app.add_title_at_playhead(); }
    });
    ui.add_space(4.0);
}

fn snapshot(app: &mut App) {
    let Some((w, h, buf)) = app.player.last_frame_for_scopes.clone() else {
        app.toast("no frame", 2);
        return;
    };
    let name = format!("frame_{}.png", crate::util::timecode(app.player.clock, app.project.fps).replace(':', "-"));
    let path = app.project_dir.join("exports").join(name);
    match exporter::save_frame_png(&buf, w, h, &path) {
        Ok(_) => app.toast(format!("✓ {}", path.display()), 1),
        Err(e) => app.toast(e, 2),
    }
}

/// Composite all visible video layers bottom→top (V1 first) with transforms,
/// and surface decode state (error / still loading) instead of a silent
/// black frame.
fn draw_composite(app: &mut App, ui: &mut egui::Ui, vr: Rect) {
    let t = app.player.clock;

    // pass 1: gather what to draw per active video track (bottom→top order).
    // Cloning frames out first avoids holding borrows across texture upload.
    struct Layer {
        transform: crate::model::Transform,
        frame: Option<crate::decoder::Frame>,
        err: Option<String>,
        img: Option<PathBuf>,
        title: Option<crate::model::TitleData>,
        title_id: u64,
        drew_kind: u8, // 0 video · 1 image · 2 title
    }
    let mut layers: Vec<Layer> = Vec::new();
    for tr in app.project.tracks.iter().filter(|tr| tr.kind == TrackKind::Video && !tr.hidden) {
        let Some(c) = tr.clips.iter().find(|c| t >= c.tl_start && t < c.end()) else { continue };
        match c.kind {
            ClipKind::Video => {
                let (frame, err) = match app.player.slots.iter().find(|s| s.track_id == tr.id) {
                    Some(s) => (s.frame.clone(), s.decode_error.clone()),
                    None => (None, None),
                };
                layers.push(Layer {
                    transform: c.transform.clone(), frame, err,
                    img: None, title: None, title_id: c.id, drew_kind: 0,
                });
            }
            ClipKind::Image => {
                layers.push(Layer {
                    transform: c.transform.clone(), frame: None, err: None,
                    img: c.source.clone(), title: None, title_id: c.id, drew_kind: 1,
                });
            }
            ClipKind::Title => {
                layers.push(Layer {
                    transform: c.transform.clone(), frame: None, err: None,
                    img: None, title: c.title.clone(), title_id: c.id, drew_kind: 2,
                });
            }
            ClipKind::Audio => {}
        }
    }

    let mut drew_any = false;
    let mut decode_err: Option<String> = None;
    let mut waiting_decode = false;
    for l in &layers {
        if let Some(e) = &l.err { decode_err = Some(e.clone()); }
        match l.drew_kind {
            0 => {
                if let Some(f) = &l.frame {
                    let key = l.title_id.wrapping_mul(1_000_003) ^ (f.w as u64) ^ ((f.h as u64) << 20);
                    let tex = upload_tex(&mut app.tex_cache, ui.ctx(), key, f.w, f.h, &f.rgba);
                    draw_transformed(ui.painter(), &tex, vr, &l.transform);
                    drew_any = true;
                } else if l.err.is_none() {
                    waiting_decode = true; // active clip, no frame yet
                }
            }
            1 => {
                if let Some(src) = &l.img {
                    if !app.big_imgs.contains_key(src) {
                        if let Ok(img) = image::open(src) {
                            let rgba = img.to_rgba8();
                            let (w, h) = (rgba.width(), rgba.height());
                            let tex = ui.ctx().load_texture(format!("img:{}", src.display()),
                                egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], rgba.as_raw()),
                                egui::TextureOptions::LINEAR);
                            app.big_imgs.insert(src.clone(), tex);
                        }
                    }
                    if let Some(tex) = app.big_imgs.get(src) {
                        draw_transformed(ui.painter(), tex, vr, &l.transform);
                        drew_any = true;
                    }
                }
            }
            2 => {
                if let Some(td) = &l.title {
                    let cache_key = l.title_id;
                    let want_text = td.text.clone();
                    let w = (vr.width() as u32).min(1280) & !1;
                    let h = (vr.height() as u32).min(720) & !1;
                    let needs = match app.title_tex.get(&cache_key) {
                        Some((txt, tex)) => txt != &want_text || tex.size()[0] as u32 != w,
                        None => true,
                    };
                    if needs {
                        if let Ok(png) = exporter::render_text_png(&td.text, td.size * (w as f32 / 1920.0), td.color, w, h) {
                            if let Ok(img) = image::load_from_memory(&png) {
                                let rgba = img.to_rgba8();
                                let tex = ui.ctx().load_texture(format!("title:{cache_key}"),
                                    egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw()),
                                    egui::TextureOptions::LINEAR);
                                app.title_tex.insert(cache_key, (want_text, tex));
                            }
                        }
                    }
                    if let Some((_, tex)) = app.title_tex.get(&cache_key) {
                        draw_transformed(ui.painter(), tex, vr, &l.transform);
                        drew_any = true;
                    }
                }
            }
            _ => {}
        }
    }

    // overlays: decode state must never be a silent black screen
    if let Some(e) = decode_err {
        let msg: String = if e.len() > 220 { format!("{}…", &e[..220]) } else { e.clone() };
        ui.painter().text(vr.center(), Align2::CENTER_CENTER,
            format!("Decode failed\n{msg}"), FontId::proportional(12.5),
            Color32::from_rgb(255, 110, 110));
        ui.painter().rect_filled(vr, 0.0, Color32::from_rgba_unmultiplied(0, 0, 0, 140));
    } else if !drew_any && waiting_decode {
        ui.painter().text(vr.center(), Align2::CENTER_CENTER, "Decoding…",
            FontId::proportional(13.0), Color32::from_rgb(120, 120, 132));
    } else if !drew_any && !app.project.tracks.iter().flat_map(|tr| &tr.clips).any(|c| c.is_visual()) {
        ui.painter().text(vr.center(), Align2::CENTER_CENTER, &app.t(K::Empty),
            FontId::proportional(14.0), Color32::from_rgb(70, 70, 78));
    }
}

fn draw_scrub(app: &App, p: &egui::Painter, r: Rect, _active: bool) {
    p.rect_filled(r, 3.0, app.theme.panel2);
    p.rect_stroke(r, 3.0, egui::Stroke::new(1.0, app.theme.border), egui::StrokeKind::Inside);
    let dur = app.project.duration().max(0.001);
    let x = |t: f64| r.left() + (t / dur).clamp(0.0, 1.0) as f32 * r.width();
    // in/out region
    if let Some(i) = app.project.in_mark {
        let i2 = app.project.out_mark.unwrap_or(dur);
        p.rect_filled(Rect::from_min_max(Pos2::new(x(i), r.top()), Pos2::new(x(i2), r.bottom())), 0.0, app.theme.io_band);
    }
    if let Some(o) = app.project.out_mark {
        let i = app.project.in_mark.unwrap_or(0.0);
        p.rect_filled(Rect::from_min_max(Pos2::new(x(i), r.top()), Pos2::new(x(o), r.bottom())), 0.0, app.theme.io_band);
    }
    // filled progress
    let px = x(app.player.clock);
    p.rect_filled(Rect::from_min_max(Pos2::new(r.left(), r.top()), Pos2::new(px, r.bottom())), 3.0, app.theme.accent_dim.gamma_multiply(0.55));
    // playhead
    p.line_segment([Pos2::new(px, r.top() - 2.0), Pos2::new(px, r.bottom() + 2.0)], egui::Stroke::new(2.0, app.theme.playhead));
    p.circle_filled(Pos2::new(px, r.center().y), 5.0, app.theme.playhead);
    // tick labels (in set language digits)
    for i in 0..=4 {
        let t = dur * i as f64 / 4.0;
        let lx = x(t);
        p.line_segment([Pos2::new(lx, r.bottom() - 4.0), Pos2::new(lx, r.bottom())], egui::Stroke::new(1.0, app.theme.border2));
        let _ = Align2::CENTER_BOTTOM;
    }
}

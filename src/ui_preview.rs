//! Preview monitor (center): letterboxed composited video, transitions
//! (dissolve/dip/wipe/slide/zoom drawn as meshes), software blend-mode
//! compositing, timecode, quality, playback speed, scrub bar, transport
//! controls, safe-margins overlay, white-balance eyedropper.

use crate::app::App;
use crate::exporter;
use crate::i18n::K;
use crate::model::{ClipKind, TrackKind, TransKind};
use crate::player::Quality;
use crate::ui_common::{draw_transformed, draw_transformed_region, icon_btn, icon_toggle, upload_tex};
use crate::ui_icons as ico;
use egui::{Align2, Color32, ColorImage, FontId, Pos2, Rect, Rounding, Sense, TextureHandle, TextureOptions, Vec2};
use std::path::{Path, PathBuf};
use std::sync::Arc;

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
            if icon_toggle(app, ui, 20.0, app.safe_margins, &app.t(K::SafeMargins), ico::safe_margins).clicked() {
                app.safe_margins = !app.safe_margins;
            }
            if icon_toggle(app, ui, 20.0, app.wb_pick, &app.t(K::PickGray), ico::dropper).clicked() {
                app.wb_pick = !app.wb_pick;
            }
        });
    });
    ui.add_space(4.0);

    // video viewport
    let avail = ui.available_size();
    let viewport_h = (avail.y - 92.0).max(80.0);
    let (vr, vresp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), viewport_h), Sense::click());
    ui.painter().rect_filled(vr, 0.0, Color32::BLACK);
    draw_composite(app, ui, vr);

    // white-balance eyedropper: click samples the frame
    if app.wb_pick && vresp.clicked() {
        if let Some(p) = vresp.interact_pointer_pos() {
            wb_sample(app, vr, p);
            app.wb_pick = false;
        }
    }

    // timecode / quality / speed row
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        let tc = crate::util::timecode(app.player.clock, app.project.fps);
        ui.label(egui::RichText::new(tc).size(15.0).strong().color(app.theme.accent_text).monospace());
        ui.add_space(10.0);
        let q = app.player.quality.unwrap_or(Quality::Half);
        egui::ComboBox::from_id_source("quality")
            .selected_text(egui::RichText::new(q.label()).size(12.0))
            .width(72.0)
            .show_ui(ui, |ui| {
                for qq in Quality::all() {
                    if ui.selectable_label(q == qq, egui::RichText::new(qq.label()).size(12.0)).clicked() {
                        app.player.quality = Some(qq);
                    }
                }
            });
        // playback speed
        let speeds = [0.25f32, 0.5, 1.0, 1.5, 2.0, 4.0];
        egui::ComboBox::from_id_source("playspeed")
            .selected_text(egui::RichText::new(format!("{}×", app.player.speed)).size(12.0))
            .width(64.0)
            .show_ui(ui, |ui| {
                for sp in speeds {
                    if ui.selectable_label((app.player.speed - sp).abs() < 0.01,
                        egui::RichText::new(format!("{sp}×")).size(12.0)).clicked() {
                        app.player.speed = sp;
                    }
                }
            });
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
    draw_scrub(app, ui.painter(), sr);
    if sresp.dragged() || sresp.clicked() {
        if let Some(p) = sresp.interact_pointer_pos() {
            let t = ((p.x - sr.left()) / sr.width()).clamp(0.0, 1.0) as f64 * app.project.duration();
            app.player.seek(t);
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
        if icon_btn(app, ui, btn, &app.t(K::FreezeFrame), ico::freeze).clicked() { app.freeze_frame_at_playhead(); }
        if icon_btn(app, ui, btn, &app.t(K::Snapshot), ico::camera).clicked() { snapshot(app); }
    });
    ui.add_space(4.0);
}

pub fn snapshot(app: &mut App) {
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

/// White-balance: sample a neutral gray point from the composited frame and
/// derive temp/tint correction (real calculation over real pixels).
fn wb_sample(app: &mut App, vr: Rect, click: Pos2) {
    let Some((w, h, buf)) = app.player.last_frame_for_scopes.clone() else { return };
    // map click within the letterboxed drawn image to frame coords: the
    // composite is fit into vr — reuse draw math by sampling relatively
    let u = ((click.x - vr.left()) / vr.width()).clamp(0.0, 1.0);
    let v = ((click.y - vr.top()) / vr.height()).clamp(0.0, 1.0);
    let x = (u * w as f32) as usize;
    let y = (v * h as f32) as usize;
    let i = (y * w as usize + x) * 4;
    if i + 2 >= buf.len() { return; }
    // average a 5x5 patch
    let mut r = 0u32; let mut g = 0u32; let mut b = 0u32; let mut n = 0;
    for dy in -2i32..=2 {
        for dx in -2i32..=2 {
            let xx = x as i32 + dx; let yy = y as i32 + dy;
            if xx < 0 || yy < 0 || xx >= w as i32 || yy >= h as i32 { continue; }
            let j = ((yy * w as i32 + xx) * 4) as usize;
            if j + 2 < buf.len() { r += buf[j] as u32; g += buf[j + 1] as u32; b += buf[j + 2] as u32; n += 1; }
        }
    }
    if n == 0 { return; }
    let (r, g, b) = (r as f32 / n as f32, g as f32 / n as f32, b as f32 / n as f32);
    let lum = 0.299 * r + 0.587 * g + 0.114 * b;
    // cast = how far each channel is from gray
    let cast_r = (r - lum) / 255.0;
    let cast_b = (b - lum) / 255.0;
    let cast_g = (g - lum) / 255.0;
    let temp = (-cast_b * 900.0 + cast_r * 300.0).clamp(-100.0, 100.0);
    let tint = (-cast_g * 500.0).clamp(-100.0, 100.0);
    app.set_grade_of_selection(|gr| { gr.temp = temp; gr.tint = tint; });
    app.toast(format!("{}  temp {temp:+.0}  tint {tint:+.0}", app.t(K::WhiteBalance)), 1);
}

// ---------------------------------------------------------------- composite
struct Layer {
    tf: crate::model::Transform,
    frame: Option<crate::decoder::Frame>,
    err: Option<String>,
    img: Option<PathBuf>,
    title: Option<crate::model::TitleData>,
    title_id: u64,
    kind: ClipKind,
    blend: crate::model::BlendMode,
}

#[allow(clippy::too_many_arguments)]
fn transition_mesh_params(kind: TransKind, p: f32) -> (f32, f32, f32, f32) {
    // returns (alpha_top, uv/pos window fraction x0, x1, extra zoom)
    match kind {
        TransKind::Dissolve => (p, 0.0, 1.0, 1.0),
        TransKind::DipToBlack => (1.0, 0.0, 1.0, 1.0),
        TransKind::WipeLeft => (1.0, 1.0 - p, 1.0, 1.0),   // reveals from left
        TransKind::WipeRight => (1.0, 0.0, p, 1.0),
        TransKind::SlideLeft => (1.0, 0.0, 1.0, 1.0),
        TransKind::SlideRight => (1.0, 0.0, 1.0, 1.0),
        TransKind::Zoom => (p.min(0.999), 0.0, 1.0, 0.2 + 0.8 * p),
    }
}

fn draw_composite(app: &mut App, ui: &mut egui::Ui, vr: Rect) {
    let t = app.player.clock;

    // gather layers bottom→top
    let mut layers: Vec<Layer> = Vec::new();
    for tr in app.project.tracks.iter().filter(|tr| tr.kind == TrackKind::Video && !tr.hidden) {
        let Some(c) = tr.clips.iter().find(|c| t >= c.tl_start && t < c.end()) else { continue };
        if c.kind == ClipKind::Adjustment { continue; } // applied via filter merge
        let frame = if c.kind == ClipKind::Video {
            app.player.slots.iter().find(|s| s.clip_id == c.id)
                .map(|s| (s.frame.clone(), s.decode_error.clone())).unwrap_or((None, None))
        } else { (None, None) };
        layers.push(Layer {
            tf: c.transform_at(t),
            frame: frame.0,
            err: frame.1,
            img: c.source.clone(),
            title: c.title.clone(),
            title_id: c.id,
            kind: c.kind,
            blend: c.blend,
        });
        // transition window: outgoing left clip UNDER this one
        if let (Some(trans), Some(left_id)) = (c.trans_in, app.project.seam_for(c.id)) {
            let tw1 = c.tl_start + trans.dur.min(c.src_dur);
            if t >= c.tl_start && t < tw1 {
                if let Some((_, l)) = app.project.clip(left_id) {
                    if l.kind == ClipKind::Video {
                        let lf = app.player.slots.iter().find(|s| s.clip_id == l.id)
                            .map(|s| (s.frame.clone(), s.decode_error.clone())).unwrap_or((None, None));
                        layers.insert(layers.len() - 1, Layer {
                            tf: l.transform_at(t),
                            frame: lf.0, err: lf.1, img: None, title: None,
                            title_id: l.id, kind: ClipKind::Video, blend: l.blend,
                        });
                    }
                }
            }
        }
    }

    // active transition (right clip + kind) if any
    let mut active_trans: Option<(u64, crate::model::Transition)> = None;
    for tr in app.project.tracks.iter().filter(|tr| tr.kind == TrackKind::Video && !tr.hidden) {
        for c in &tr.clips {
            if let Some(trans) = c.trans_in {
                let tw1 = c.tl_start + trans.dur.min(c.src_dur);
                if t >= c.tl_start && t < tw1 { active_trans = Some((c.id, trans)); }
            }
        }
    }

    // ---- software blend path when any layer blends ------------------------
    let any_blend = layers.iter().any(|l| l.blend != crate::model::BlendMode::Normal);
    if any_blend {
        let pw = (vr.width() as u32).min(1280) & !1;
        let ph = (vr.height() as u32).min(720) & !1;
        if pw >= 4 && ph >= 4 {
            let mut clayers: Vec<crate::compositor::Layer> = Vec::new();
            for l in &layers {
                let rgba: Option<Arc<Vec<u8>>> = match l.kind {
                    ClipKind::Video => l.frame.as_ref().map(|f| f.rgba.clone()),
                    ClipKind::Image => app.big_imgs_cpu.get(l.img.as_ref().map(|p| p.as_path()).unwrap_or(Path::new(""))).cloned(),
                    ClipKind::Title => app.title_tex.get(&l.title_id).map(|(_, _, rgba)| rgba.clone()),
                    _ => None,
                };
                if let Some(rgba) = rgba {
                    let (w2, h2) = match l.kind {
                        ClipKind::Video => l.frame.as_ref().map(|f| (f.w, f.h)).unwrap_or((pw, ph)),
                        ClipKind::Image => app.big_imgs_cpu.get(l.img.as_ref().map(|p| p.as_path()).unwrap_or(Path::new("")))
                            .and_then(|a| image::load_from_memory(a).ok())
                            .map(|i| (i.width(), i.height())).unwrap_or((pw, ph)),
                        _ => (pw, ph),
                    };
                    clayers.push(crate::compositor::Layer {
                        rgba, w: w2, h: h2, transform: l.tf, blend: l.blend,
                    });
                }
            }
            if !clayers.is_empty() {
                let mut canvas = std::mem::take(&mut app.soft_canvas).2;
                crate::compositor::compose(pw, ph, &clayers, &mut canvas);
                app.soft_canvas = (pw, ph, canvas.clone());
                let key = 0xC0A5_1A7;
                let tex = upload_tex(&mut app.tex_cache, ui.ctx(), key, pw, ph, &canvas);
                ui.painter().image(tex.id(), vr, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                draw_state_overlays(app, ui, vr, &layers, true);
                return;
            }
        }
    }

    // ---- fast painter path -------------------------------------------------
    let mut drew_any = false;
    let mut decode_err: Option<String> = None;
    let mut waiting_decode = false;
    let n_layers = layers.len();
    let (trans_clip, trans) = active_trans.clone().unwrap_or((0, crate::model::Transition::default()));
    let trans_p = active_trans.as_ref().map(|(cid, tr)| {
        let c_start = find_clip_start(app, *cid);
        let dur = tr.dur.max(0.05);
        ((((t - c_start) / dur).clamp(0.0, 1.0)) as f32, tr.kind)
    });

    // freeze at sequence end: hold the last frame
    if layers.is_empty() && !app.player.playing {
        for s in app.player.slots.iter().rev() {
            if let Some(f) = &s.frame {
                let key = s.clip_id.wrapping_mul(1_000_003) ^ (f.w as u64) ^ ((f.h as u64) << 20);
                let tex = upload_tex(&mut app.tex_cache, ui.ctx(), key, f.w, f.h, &f.rgba);
                draw_transformed(ui.painter(), &tex, vr, &crate::model::Transform::default());
                drew_any = true;
                break;
            }
        }
    }

    for (li, l) in layers.iter().enumerate() {
        if let Some(e) = &l.err { decode_err = Some(e.clone()); }
        // is this layer the incoming transition clip?
        let is_trans_top = trans_clip != 0 && l.title_id == trans_clip && li == n_layers.saturating_sub(1);
        let is_trans_bottom = trans_clip != 0 && n_layers >= 2 && li + 2 == n_layers;
        match l.kind {
            ClipKind::Video => {
                if let Some(f) = &l.frame {
                    let key = l.title_id.wrapping_mul(1_000_003) ^ (f.w as u64) ^ ((f.h as u64) << 20);
                    let tex = upload_tex(&mut app.tex_cache, ui.ctx(), key, f.w, f.h, &f.rgba);
                    if let Some((p, kind)) = trans_p {
                        if is_trans_top {
                            draw_transition_top(app, ui, &tex, vr, &l.tf, kind, p);
                            drew_any = true;
                            continue;
                        } else if is_trans_bottom {
                            draw_transition_bottom(app, ui, &tex, vr, &l.tf, kind, p);
                            drew_any = true;
                            continue;
                        }
                        let _ = p;
                    }
                    draw_transformed(ui.painter(), &tex, vr, &l.tf);
                    drew_any = true;
                } else if l.err.is_none() {
                    waiting_decode = true;
                }
            }
            ClipKind::Image => {
                if let Some(src) = &l.img {
                    if !app.big_imgs.contains_key(src) {
                        if let Ok(img) = image::open(src) {
                            let rgba = img.to_rgba8();
                            let (w, h) = (rgba.width(), rgba.height());
                            let tex = ui.ctx().load_texture(format!("img:{}", src.display()),
                                ColorImage::from_rgba_unmultiplied([w as usize, h as usize], rgba.as_raw()),
                                TextureOptions::LINEAR);
                            app.big_imgs.insert(src.clone(), tex);
                            app.big_imgs_cpu.insert(src.clone(), Arc::new(rgba.into_raw()));
                        }
                    }
                    if let Some(tex) = app.big_imgs.get(src) {
                        draw_transformed(ui.painter(), tex, vr, &l.tf);
                        drew_any = true;
                    }
                }
            }
            ClipKind::Title => {
                if let Some(td) = &l.title {
                    let cache_key = l.title_id;
                    let want_text = td.text.clone();
                    let w = (vr.width() as u32).min(1280) & !1;
                    let h = (vr.height() as u32).min(720) & !1;
                    let needs = match app.title_tex.get(&cache_key) {
                        Some((txt, tex, _)) => txt != &want_text || tex.size()[0] as u32 != w,
                        None => true,
                    };
                    if needs {
                        if let Ok(png) = exporter::render_text_png(&td.text, td.size * (w as f32 / 1920.0), td.color, w, h,
                            td.pos, td.bar, td.bar_color, td.shadow) {
                            if let Ok(img) = image::load_from_memory(&png) {
                                let rgba = img.to_rgba8();
                                let tex = ui.ctx().load_texture(format!("title:{cache_key}"),
                                    ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw()),
                                    TextureOptions::LINEAR);
                                app.title_tex.insert(cache_key, (want_text, tex, Arc::new(rgba.into_raw())));
                            }
                        }
                    }
                    if let Some((_, tex, _)) = app.title_tex.get(&cache_key) {
                        draw_transformed(ui.painter(), tex, vr, &l.tf);
                        drew_any = true;
                    }
                }
            }
            _ => {}
        }
    }

    draw_state_overlays(app, ui, vr, &layers, drew_any || waiting_decode);
    if !drew_any && !waiting_decode && decode_err.is_none()
        && !app.project.tracks.iter().flat_map(|tr| &tr.clips).any(|c| c.is_visual()) {
        ui.painter().text(vr.center(), Align2::CENTER_CENTER, &app.t(K::Empty),
            FontId::proportional(14.0), Color32::from_rgb(70, 70, 78));
    }
    if decode_err.is_none() {
        let _ = waiting_decode; // "Decoding…" handled in overlays
    }
}

fn find_clip_start(app: &App, clip_id: u64) -> f64 {
    app.project.clip(clip_id).map(|(_, c)| c.tl_start).unwrap_or(0.0)
}

// ---- transition drawing (real mesh math, matches the xfade export) --------
/// Incoming clip: alpha/wipe/slide/zoom progression.
fn draw_transition_top(app: &App, ui: &mut egui::Ui, tex: &TextureHandle, vr: Rect,
                       tf: &crate::model::Transform, kind: TransKind, p: f32) {
    let p = p.clamp(0.0, 1.0);
    let a = (p.clamp(0.0, 1.0) * 255.0) as u8;
    let mut tf2 = *tf;
    match kind {
        TransKind::Dissolve => { tf2.opacity *= p; draw_transformed(ui.painter(), tex, vr, &tf2); }
        TransKind::DipToBlack => { if p > 0.5 { tf2.opacity *= (p - 0.5) * 2.0; draw_transformed(ui.painter(), tex, vr, &tf2); } }
        TransKind::WipeLeft | TransKind::WipeRight => {
            let (x0, x1) = if kind == TransKind::WipeLeft { (1.0 - p, 1.0) } else { (0.0, p) };
            draw_transformed_region(ui.painter(), tex, vr, &tf2, x0, x1, 0.0, 1.0);
            let _ = a;
        }
        TransKind::SlideLeft => {
            let off = (1.0 - p) * vr.width() * -1.0; // enters from right
            let vr2 = Rect::from_min_size(Pos2::new(vr.left() - off, vr.top()), vr.size());
            draw_transformed(ui.painter(), tex, vr2, &tf2);
        }
        TransKind::SlideRight => {
            let off = (1.0 - p) * vr.width();
            let vr2 = Rect::from_min_size(Pos2::new(vr.left() + off, vr.top()), vr.size());
            draw_transformed(ui.painter(), tex, vr2, &tf2);
        }
        TransKind::Zoom => {
            tf2.scale *= 0.2 + 0.8 * p;
            tf2.opacity *= p;
            draw_transformed(ui.painter(), tex, vr, &tf2);
        }
    }
    let _ = app;
}

/// Outgoing clip during a transition.
fn draw_transition_bottom(app: &App, ui: &mut egui::Ui, tex: &TextureHandle, vr: Rect,
                          tf: &crate::model::Transform, kind: TransKind, p: f32) {
    let p = p.clamp(0.0, 1.0);
    let mut tf2 = *tf;
    match kind {
        TransKind::Dissolve => { draw_transformed(ui.painter(), tex, vr, &tf2); }
        TransKind::DipToBlack => { if p < 0.5 { tf2.opacity *= 1.0 - p * 2.0; draw_transformed(ui.painter(), tex, vr, &tf2); } }
        TransKind::WipeLeft | TransKind::WipeRight => draw_transformed(ui.painter(), tex, vr, &tf2),
        TransKind::SlideLeft => {
            let off = p * vr.width();
            let vr2 = Rect::from_min_size(Pos2::new(vr.left() - off, vr.top()), vr.size());
            draw_transformed(ui.painter(), tex, vr2, &tf2);
        }
        TransKind::SlideRight => {
            let off = p * vr.width();
            let vr2 = Rect::from_min_size(Pos2::new(vr.left() + off, vr.top()), vr.size());
            draw_transformed(ui.painter(), tex, vr2, &tf2);
        }
        TransKind::Zoom => { draw_transformed(ui.painter(), tex, vr, &tf2); }
    }
    let _ = app;
}

/// Decode-state overlays: never a silent black screen.
fn draw_state_overlays(app: &App, ui: &mut egui::Ui, vr: Rect,
                       layers: &[Layer], drew_any: bool) {
    let p = ui.painter();
    if let Some(l) = layers.iter().find(|l| l.err.is_some()) {
        let e = l.err.clone().unwrap_or_default();
        let msg: String = if e.len() > 220 { format!("{}…", &e[..220]) } else { e };
        p.rect_filled(vr, 0.0, Color32::from_rgba_unmultiplied(0, 0, 0, 140));
        p.text(vr.center(), Align2::CENTER_CENTER, format!("Decode failed\n{msg}"),
            FontId::proportional(12.5), Color32::from_rgb(255, 110, 110));
    } else if !drew_any {
        p.text(vr.center(), Align2::CENTER_CENTER, "Decoding…",
            FontId::proportional(13.0), Color32::from_rgb(120, 120, 132));
    }
    // safe margins
    if app.safe_margins {
        let a = Rect::from_center_size(vr.center(), Vec2::new(vr.width() * 0.9, vr.height() * 0.9));
        let t = Rect::from_center_size(vr.center(), Vec2::new(vr.width() * 0.8, vr.height() * 0.8));
        p.rect_stroke(a, 0.0, egui::Stroke::new(1.0, Color32::from_rgb(230, 230, 90)), egui::StrokeKind::Inside);
        p.rect_stroke(t, 0.0, egui::Stroke::new(1.0, Color32::from_rgb(120, 220, 160)), egui::StrokeKind::Inside);
        p.text(Pos2::new(a.left(), a.top() - 7.0), Align2::LEFT_CENTER, "90%",
            FontId::monospace(9.0), Color32::from_rgb(230, 230, 90));
        p.text(Pos2::new(t.left(), t.bottom() + 7.0), Align2::LEFT_CENTER, "80%",
            FontId::monospace(9.0), Color32::from_rgb(120, 220, 160));
    }
    // reverse indicator on reversed clips
    let t = app.player.clock;
    if app.project.clips_at(t, TrackKind::Video).iter()
        .any(|(_, c)| c.reverse) {
        p.text(Pos2::new(vr.right() - 12.0, vr.top() + 12.0), Align2::RIGHT_CENTER,
            "⏪ REVERSE", FontId::proportional(11.0), Color32::from_rgb(255, 190, 90));
    }
}

fn draw_scrub(app: &App, p: &egui::Painter, r: Rect) {
    p.rect_filled(r, 3.0, app.theme.panel2);
    p.rect_stroke(r, 3.0, egui::Stroke::new(1.0, app.theme.border), egui::StrokeKind::Inside);
    let dur = app.project.duration().max(0.001);
    let x = |t: f64| r.left() + (t / dur).clamp(0.0, 1.0) as f32 * r.width();
    if let Some(i) = app.project.in_mark {
        let i2 = app.project.out_mark.unwrap_or(dur);
        p.rect_filled(Rect::from_min_max(Pos2::new(x(i), r.top()), Pos2::new(x(i2), r.bottom())), 0.0, app.theme.io_band);
    }
    if let Some(o) = app.project.out_mark {
        let i = app.project.in_mark.unwrap_or(0.0);
        p.rect_filled(Rect::from_min_max(Pos2::new(x(i), r.top()), Pos2::new(x(o), r.bottom())), 0.0, app.theme.io_band);
    }
    let px = x(app.player.clock);
    p.rect_filled(Rect::from_min_max(Pos2::new(r.left(), r.top()), Pos2::new(px, r.bottom())), 3.0, app.theme.accent_dim.gamma_multiply(0.55));
    p.line_segment([Pos2::new(px, r.top() - 2.0), Pos2::new(px, r.bottom() + 2.0)], egui::Stroke::new(2.0, app.theme.playhead));
    p.circle_filled(Pos2::new(px, r.center().y), 5.0, app.theme.playhead);
    for i in 0..=4 {
        let t = dur * i as f64 / 4.0;
        let lx = x(t);
        p.line_segment([Pos2::new(lx, r.bottom() - 4.0), Pos2::new(lx, r.bottom())], egui::Stroke::new(1.0, app.theme.border2));
    }
}


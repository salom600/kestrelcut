//! Right column: Primary Color panel (LUT, temperature/tint/exposure/...,
//! Reset/Auto) + scopes (luma waveform, vectorscope, RGB parade) — mirrors
//! the reference right column.

use crate::app::App;
use crate::i18n::K;
use crate::ui_common::{gradient_slider, hline, icon_btn, section_header, upload_tex};
use crate::ui_icons as ico;
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Vec2};

pub fn show_color(app: &mut App, ui: &mut egui::Ui) {
    ui.add_space(4.0);
    section_header(app, ui, &app.t(K::PrimaryColor), None);
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        ui.label(egui::RichText::new(app.t(K::Lut)).size(12.0).color(app.theme.dim));
        let cur = app.selected_clip()
            .and_then(|c| c.fx.lut.clone())
            .map(|p| p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default())
            .unwrap_or_else(|| app.t(K::LutNone));
        let w = ui.available_width() - 24.0;
        egui::ComboBox::from_id_source("lut")
            .selected_text(egui::RichText::new(cur).size(12.0))
            .width(w.max(120.0))
            .show_ui(ui, |ui| {
                if ui.selectable_label(app.selected_clip().map(|c| c.fx.lut.is_none()).unwrap_or(true),
                    egui::RichText::new(app.t(K::LutNone)).size(12.0)).clicked()
                {
                    app.set_fx_of_selection(|fx| fx.lut = None);
                }
            });
    });
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        ui.label(egui::RichText::new(app.t(K::PrimaryTools)).size(12.0).strong().color(app.theme.text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(10.0);
            icon_btn(app, ui, 20.0, &app.t(K::ColorAuto), ico::dropper);
        });
    });
    hline(app, ui);

    let g = app.selected_clip().map(|c| c.grade).unwrap_or_default();
    let has_sel = app.sel.is_some();
    let mut changed = false;
    ui.add_space(4.0);
    gradient_slider(ui, app.theme, &app.t(K::Temperature), g.temp, -100.0, 100.0,
        [Color32::from_rgb(70, 130, 255), Color32::from_rgb(255, 150, 40)], |v| format!("{v:.1}"),
        &mut |v| { app.set_grade_of_selection(|gr| gr.temp = v); changed = true; });
    gradient_slider(ui, app.theme, &app.t(K::Tint), g.tint, -100.0, 100.0,
        [Color32::from_rgb(80, 220, 120), Color32::from_rgb(230, 70, 200)], |v| format!("{v:.1}"),
        &mut |v| { app.set_grade_of_selection(|gr| gr.tint = v); changed = true; });
    ui.add_space(4.0);
    gradient_slider(ui, app.theme, &app.t(K::Exposure), g.exposure, -4.0, 4.0,
        [Color32::from_rgb(20, 20, 24), Color32::from_rgb(240, 240, 245)], |v| format!("{v:.2}"),
        &mut |v| { app.set_grade_of_selection(|gr| gr.exposure = v); changed = true; });
    gradient_slider(ui, app.theme, &app.t(K::Contrast), g.contrast, -100.0, 100.0,
        [Color32::from_rgb(60, 60, 66), Color32::from_rgb(230, 230, 235)], |v| format!("{v:.1}"),
        &mut |v| { app.set_grade_of_selection(|gr| gr.contrast = v); changed = true; });
    gradient_slider(ui, app.theme, &app.t(K::Saturation), g.saturation, -100.0, 100.0,
        [Color32::from_rgb(120, 120, 126), Color32::from_rgb(255, 90, 90)], |v| format!("{v:.1}"),
        &mut |v| { app.set_grade_of_selection(|gr| gr.saturation = v); changed = true; });
    ui.add_space(4.0);
    gradient_slider(ui, app.theme, &app.t(K::Highlights), g.highlights, -100.0, 100.0,
        [Color32::from_rgb(40, 40, 46), Color32::from_rgb(250, 250, 255)], |v| format!("{v:.1}"),
        &mut |v| { app.set_grade_of_selection(|gr| gr.highlights = v); changed = true; });
    gradient_slider(ui, app.theme, &app.t(K::Whites), g.whites, -100.0, 100.0,
        [Color32::from_rgb(70, 70, 78), Color32::from_rgb(255, 255, 255)], |v| format!("{v:.1}"),
        &mut |v| { app.set_grade_of_selection(|gr| gr.whites = v); changed = true; });
    gradient_slider(ui, app.theme, &app.t(K::Blacks), g.blacks, -100.0, 100.0,
        [Color32::from_rgb(0, 0, 0), Color32::from_rgb(120, 120, 130)], |v| format!("{v:.1}"),
        &mut |v| { app.set_grade_of_selection(|gr| gr.blacks = v); changed = true; });
    let _ = changed;

    ui.add_space(8.0);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.add_space(12.0);
        let (bfill, bstroke) = (app.theme.panel3, app.theme.border2);
        let b = move |txt: &str| egui::Button::new(egui::RichText::new(txt).size(12.0))
            .fill(bfill).rounding(4).stroke(egui::Stroke::new(1.0, bstroke));
        if ui.add(b(&app.t(K::AutoBtn))).clicked() { app.auto_color(); }
        if ui.add(b(&app.t(K::Reset))).clicked() {
            app.set_grade_of_selection(|gr| *gr = crate::model::Grade::default());
            app.set_fx_of_selection(|fx| *fx = crate::model::Fx::default());
        }
    });
    if !has_sel {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            ui.label(egui::RichText::new(if app.lang == crate::i18n::Lang::Ar {
                crate::arabic::shape("حدد مقطعًا لتعديل الألوان")
            } else { "Select a clip to grade".into() }).size(10.5).color(app.theme.faint));
        });
    }
    ui.add_space(8.0);
    hline(app, ui);
    transform_section(app, ui);
}

fn transform_section(app: &mut App, ui: &mut egui::Ui) {
    let Some(c) = app.selected_clip().cloned() else { return };
    section_header(app, ui, &app.t(K::Transform), Some(&c.name));
    let tf = c.transform;
    let mut v;
    v = tf.x;
    gradient_slider(ui, app.theme, &format!("{} X", app.t(K::Position)), v, -1.0, 1.0,
        [app.theme.border2, app.theme.border2], |v| format!("{v:.2}"), &mut |nv| app.set_transform_of_selection(|t| t.x = nv));
    v = tf.y;
    gradient_slider(ui, app.theme, &format!("{} Y", app.t(K::Position)), v, -1.0, 1.0,
        [app.theme.border2, app.theme.border2], |v| format!("{v:.2}"), &mut |nv| app.set_transform_of_selection(|t| t.y = nv));
    v = tf.scale;
    gradient_slider(ui, app.theme, &app.t(K::Scale), v, 0.05, 4.0,
        [app.theme.border2, app.theme.border2], |v| format!("{v:.2}"), &mut |nv| app.set_transform_of_selection(|t| t.scale = nv));
    v = tf.rotation;
    gradient_slider(ui, app.theme, &app.t(K::Rotation), v, -180.0, 180.0,
        [app.theme.border2, app.theme.border2], |v| format!("{v:.1}°"), &mut |nv| app.set_transform_of_selection(|t| t.rotation = nv));
    v = tf.opacity;
    gradient_slider(ui, app.theme, &app.t(K::Opacity), v, 0.0, 1.0,
        [app.theme.border2, app.theme.border2], |v| format!("{v:.2}"), &mut |nv| app.set_transform_of_selection(|t| t.opacity = nv));
    if c.is_visual() {
        v = c.speed;
        gradient_slider(ui, app.theme, &app.t(K::Speed), v, 0.25, 4.0,
            [app.theme.border2, app.theme.border2], |v| format!("{v:.2}×"), &mut |nv| {
            if let Some(cl) = app.selected_clip_mut() {
                let ratio = cl.speed / nv;
                cl.src_dur *= ratio as f64;
                cl.speed = nv;
            }
            app.invalidate_preview();
        });
    }
    gradient_slider(ui, app.theme, &app.t(K::Blur), c.fx.blur, 0.0, 40.0,
        [app.theme.border2, app.theme.border2], |v| format!("{v:.0}"), &mut |nv| app.set_fx_of_selection(|f| f.blur = nv));
    gradient_slider(ui, app.theme, &app.t(K::FadeIn), c.fx.fade_in, 0.0, 5.0,
        [app.theme.border2, app.theme.border2], |v| format!("{v:.2}s"), &mut |nv| app.set_fx_of_selection(|f| f.fade_in = nv));
    gradient_slider(ui, app.theme, &app.t(K::FadeOut), c.fx.fade_out, 0.0, 5.0,
        [app.theme.border2, app.theme.border2], |v| format!("{v:.2}s"), &mut |nv| app.set_fx_of_selection(|f| f.fade_out = nv));
    if c.is_audio() {
        v = c.gain_db;
        gradient_slider(ui, app.theme, &app.t(K::Gain), v, -48.0, 12.0,
            [app.theme.border2, app.theme.border2], |v| format!("{v:.1}dB"), &mut |nv| {
            if let Some(cl) = app.selected_clip_mut() { cl.gain_db = nv; }
        });
    }
    if let Some(mut td) = c.title.clone() {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            ui.label(egui::RichText::new(app.t(K::TitleText)).size(12.0).color(app.theme.dim));
        });
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            ui.add_sized([ui.available_width() - 20.0, 22.0], egui::TextEdit::singleline(&mut td.text));
        });
        if let Some(cl) = app.selected_clip_mut() {
            if let Some(t2) = cl.title.as_mut() { t2.text = td.text.clone(); }
        }
        v = td.size;
        gradient_slider(ui, app.theme, &app.t(K::FontSize), v, 20.0, 220.0,
            [app.theme.border2, app.theme.border2], |v| format!("{v:.0}"), &mut |nv| {
            if let Some(cl) = app.selected_clip_mut() {
                if let Some(t2) = cl.title.as_mut() { t2.size = nv; }
            }
        });
        app.invalidate_preview();
    }
}

// ------------------------------------------------------------------ scopes
pub fn show_scopes(app: &mut App, ui: &mut egui::Ui) {
    ui.add_space(2.0);
    section_header(app, ui, &app.t(K::Scopes), None);
    let avail = ui.available_size();
    let each_h = ((avail.y - 62.0) / 3.0).max(52.0);
    let w = (ui.available_width() - 16.0).max(80.0);

    update_scope_images(app, ui.ctx());

    ui.horizontal(|ui| {
        ui.add_space(8.0);
        scope_box(app, ui, &app.t(K::Waveform), app.scopes.wave.as_ref(), Vec2::new(w, each_h));
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        scope_box(app, ui, &app.t(K::Vectorscope), app.scopes.vector.as_ref(), Vec2::new(w, each_h));
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        scope_box(app, ui, &app.t(K::Parade), app.scopes.parade.as_ref(), Vec2::new(w, each_h));
    });
}

fn scope_box(app: &App, ui: &mut egui::Ui, title: &str, tex: Option<&egui::TextureHandle>, size: Vec2) {
    let (r, _) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect_filled(r, 3.0, app.theme.scope_bg);
    ui.painter().rect_stroke(r, 3.0, egui::Stroke::new(1.0, app.theme.border), egui::StrokeKind::Inside);
    ui.painter().text(Pos2::new(r.left() + 6.0, r.top() + 9.0), Align2::LEFT_CENTER,
        title, FontId::proportional(10.0), app.theme.faint);
    if let Some(tex) = tex {
        ui.painter().image(tex.id(), r.shrink2(Vec2::new(0.0, 16.0)),
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
    }
}

fn update_scope_images(app: &mut App, ctx: &egui::Context) {
    let now = std::time::Instant::now();
    if let Some(st) = app.scopes.stamp {
        if now.duration_since(st) < std::time::Duration::from_millis(280) { return; }
    }
    let Some((w, h, buf)) = app.player.last_frame_for_scopes.clone() else { return };
    app.scopes.stamp = Some(now);

    // downsample to 64x36 analysis grid
    const AW: usize = 64;
    const AH: usize = 36;
    let mut px = [[0u8; 3]; AW * AH];
    for ay in 0..AH {
        for ax in 0..AW {
            let sx = ((ax * w as usize) / AW).min(w as usize - 1);
            let sy = ((ay * h as usize) / AH).min(h as usize - 1);
            let i = (sy * w as usize + sx) * 4;
            if i + 2 < buf.len() {
                px[ay * AW + ax] = [buf[i], buf[i + 1], buf[i + 2]];
            }
        }
    }

    // ---- luma waveform (AW cols, 0..255 rows, accumulate)
    let mut wave = image::RgbaImage::new((AW * 2) as u32, 100);
    for (ax, p) in px.iter().enumerate() {
        let l = (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) as u8;
        let y = 99 - ((l as u32) * 99 / 255) as u32;
        let x = ((ax % AW) * 2) as u32;
        for dx in 0..2 {
            let xx = (x + dx).min((AW * 2 - 1) as u32);
            let cur = wave.get_pixel(xx, y).0;
            let b = (cur[0] as u32 + 60).min(235) as u8;
            wave.put_pixel(xx, y, image::Rgba([b, b, b, 255]));
        }
    }
    // graticule lines 0/25/50/75/100
    for (row, _) in [(0u32, 0u32), (25, 1), (50, 2), (75, 3), (99, 4)] {
        for x in 0..((AW * 2) as u32) {
            let cur = wave.get_pixel(x, row).0;
            if cur[0] < 40 { wave.put_pixel(x, row, image::Rgba([60, 60, 66, 255])); }
        }
    }

    // ---- vectorscope (Cb/Cr)
    let mut vec = image::RgbaImage::new(100, 100);
    let cx = 50.0f32;
    let cy = 50.0f32;
    for p in px.iter() {
        let (r_, g_, b_) = (p[0] as f32, p[1] as f32, p[2] as f32);
        let cb = -0.169 * r_ - 0.331 * g_ + 0.5 * b_;
        let cr = 0.5 * r_ - 0.419 * g_ - 0.081 * b_;
        let x = (cx + cb * 0.35) as i32;
        let y = (cy - cr * 0.35) as i32;
        if x >= 0 && x < 100 && y >= 0 && y < 100 {
            let cur = vec.get_pixel(x as u32, y as u32).0;
            let b = (cur[0] as u32 + 70).min(240) as u8;
            vec.put_pixel(x as u32, y as u32, image::Rgba([b, b, b, 255]));
        }
    }
    // circle graticule
    for a in 0..360 {
        let rad = a as f32 / 180.0 * std::f32::consts::PI;
        let x = (cx + 44.0 * rad.cos()) as i32;
        let y = (cy + 44.0 * rad.sin()) as i32;
        if x >= 0 && x < 100 && y >= 0 && y < 100 {
            vec.put_pixel(x as u32, y as u32, image::Rgba([55, 55, 62, 255]));
        }
    }

    // ---- RGB parade (3 panels)
    let mut parade = image::RgbaImage::new((AW * 3) as u32, 100);
    for (ax, p) in px.iter().enumerate() {
        for (ch, val) in p.iter().enumerate() {
            let y = 99 - ((*val as u32) * 99 / 255) as u32;
            let x = ((ax % AW) + ch * AW) as u32;
            let cur = parade.get_pixel(x, y).0;
            let base = [cur[0] as u32, cur[1] as u32, cur[2] as u32];
            let add = [200, 200, 200];
            let col = [
                if ch == 0 { base[0].saturating_add(add[0]).min(255) } else { base[0].saturating_add(30).min(255) },
                if ch == 1 { base[1].saturating_add(add[1]).min(255) } else { base[1].saturating_add(30).min(255) },
                if ch == 2 { base[2].saturating_add(add[2]).min(255) } else { base[2].saturating_add(30).min(255) },
                255u32,
            ];
            parade.put_pixel(x, y, image::Rgba([col[0] as u8, col[1] as u8, col[2] as u8, 255]));
        }
    }

    let to_tex = |img: &image::RgbaImage| -> Option<egui::TextureHandle> {
        Some(ctx.load_texture(
            format!("scope{}", rand_suffix()),
            egui::ColorImage::from_rgba_unmultiplied(
                [img.width() as usize, img.height() as usize], img.as_raw()),
            egui::TextureOptions::NEAREST))
    };
    app.scopes.wave = to_tex(&wave);
    app.scopes.vector = to_tex(&vec);
    app.scopes.parade = to_tex(&parade);
}

fn rand_suffix() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.subsec_nanos() as u64).unwrap_or(0)
}

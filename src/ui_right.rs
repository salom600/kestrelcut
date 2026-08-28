//! Right column panels, per workspace:
//!   Edit    → Inspector (Transform / Compositing / Speed / Titles)
//!   Color   → Lumetri-style primary panel + real Lift/Gamma/Gain wheels
//!   Audio   → clip gain, fades, track mute/solo
//!   Effects → blur, LUT, fades, speed + one-click look presets
//!   Scopes  → luma waveform, vectorscope, RGB parade (live, from preview)

use crate::app::App;
use crate::i18n::K;
use crate::ui_common::{gradient_slider, hline, icon_btn, section_header, upload_tex};
use crate::ui_icons as ico;
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};

const TAU: f32 = std::f32::consts::TAU;

// ===================================================================== color
pub fn show_color(app: &mut App, ui: &mut egui::Ui) {
    ui.add_space(4.0);
    section_header(app, ui, &app.t(K::PrimaryColor), None);

    // ---- Look / LUT -----------------------------------------------
    let cur = app.selected_clip()
        .and_then(|c| c.fx.lut.clone())
        .map(|p| p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default());
    let has_sel = app.sel.is_some();
    let mut browse = false;
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        ui.label(egui::RichText::new(app.t(K::Lut)).size(12.0).color(app.theme.dim));
        let w = ui.available_width() - 24.0;
        egui::ComboBox::from_id_source("lut")
            .selected_text(egui::RichText::new(cur.clone().unwrap_or_else(|| app.t(K::LutNone))).size(12.0))
            .width(w.max(120.0))
            .show_ui(ui, |ui| {
                if ui.selectable_label(cur.is_none(), egui::RichText::new(app.t(K::LutNone)).size(12.0)).clicked() {
                    app.set_fx_of_selection(|fx| fx.lut = None);
                }
                if cur.is_some() && ui.selectable_label(true, egui::RichText::new(app.t(K::LutRemove)).size(12.0)).clicked() {
                    app.set_fx_of_selection(|fx| fx.lut = None);
                }
                ui.separator();
                if ui.selectable_label(false, egui::RichText::new(app.t(K::Browse)).size(12.0)).clicked() {
                    browse = true;
                }
            });
    });
    if browse {
        app.dialog = Some(crate::app::Dialog::Fs(crate::app::FsState {
            dir: app.project_dir.clone(),
            mode: crate::app::FsMode::PickLut,
            name: String::new(),
        }));
    }

    // ---- header row: tools + Auto (real: balances from current frame)
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        ui.label(egui::RichText::new(app.t(K::PrimaryTools)).size(12.0).strong().color(app.theme.text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(10.0);
            if icon_btn(app, ui, 20.0, &app.t(K::ColorAuto), ico::dropper).clicked() {
                app.auto_color();
            }
        });
    });
    hline(app, ui);

    let g = app.selected_clip().map(|c| c.grade).unwrap_or_default();

    // ---- real interactive color wheels FIRST (Resolve-style, above fold)
    ui.add_space(4.0);
    section_header(app, ui, &app.t(K::ColorWheels), None);
    let wheels = [
        (app.t(K::Lift), g.lift),
        (app.t(K::Gamma), g.gamma),
        (app.t(K::Gain), g.gain),
    ];
    ui.horizontal(|ui| {
        let each = (ui.available_width() - 24.0) / 3.0;
        for (i, (label, val)) in wheels.iter().enumerate() {
            color_wheel(app, ui, format!("wheel{i}"), label, each, *val, i);
        }
    });
    gradient_slider(ui, app.theme, &app.t(K::Offset), g.offset, -100.0, 100.0,
        [Color32::from_rgb(30, 30, 34), Color32::from_rgb(200, 205, 215)], |v| format!("{v:.1}"),
        &mut |v| app.set_grade_of_selection(|gr| gr.offset = v));
    ui.add_space(6.0);
    hline(app, ui);

    ui.add_space(4.0);
    gradient_slider(ui, app.theme, &app.t(K::Temperature), g.temp, -100.0, 100.0,
        [Color32::from_rgb(70, 130, 255), Color32::from_rgb(255, 150, 40)], |v| format!("{v:.1}"),
        &mut |v| app.set_grade_of_selection(|gr| gr.temp = v));
    gradient_slider(ui, app.theme, &app.t(K::Tint), g.tint, -100.0, 100.0,
        [Color32::from_rgb(80, 220, 120), Color32::from_rgb(230, 70, 200)], |v| format!("{v:.1}"),
        &mut |v| app.set_grade_of_selection(|gr| gr.tint = v));
    ui.add_space(4.0);
    gradient_slider(ui, app.theme, &app.t(K::Exposure), g.exposure, -4.0, 4.0,
        [Color32::from_rgb(20, 20, 24), Color32::from_rgb(240, 240, 245)], |v| format!("{v:.2}"),
        &mut |v| app.set_grade_of_selection(|gr| gr.exposure = v));
    gradient_slider(ui, app.theme, &app.t(K::Contrast), g.contrast, -100.0, 100.0,
        [Color32::from_rgb(60, 60, 66), Color32::from_rgb(230, 230, 235)], |v| format!("{v:.1}"),
        &mut |v| app.set_grade_of_selection(|gr| gr.contrast = v));
    gradient_slider(ui, app.theme, &app.t(K::Saturation), g.saturation, -100.0, 100.0,
        [Color32::from_rgb(120, 120, 126), Color32::from_rgb(255, 90, 90)], |v| format!("{v:.1}"),
        &mut |v| app.set_grade_of_selection(|gr| gr.saturation = v));
    gradient_slider(ui, app.theme, &app.t(K::Vibrance), g.vibrance, -100.0, 100.0,
        [Color32::from_rgb(100, 120, 130), Color32::from_rgb(120, 220, 160)], |v| format!("{v:.1}"),
        &mut |v| app.set_grade_of_selection(|gr| gr.vibrance = v));
    ui.add_space(4.0);
    gradient_slider(ui, app.theme, &app.t(K::Highlights), g.highlights, -100.0, 100.0,
        [Color32::from_rgb(40, 40, 46), Color32::from_rgb(250, 250, 255)], |v| format!("{v:.1}"),
        &mut |v| app.set_grade_of_selection(|gr| gr.highlights = v));
    gradient_slider(ui, app.theme, &app.t(K::Whites), g.whites, -100.0, 100.0,
        [Color32::from_rgb(70, 70, 78), Color32::from_rgb(255, 255, 255)], |v| format!("{v:.1}"),
        &mut |v| app.set_grade_of_selection(|gr| gr.whites = v));
    gradient_slider(ui, app.theme, &app.t(K::Blacks), g.blacks, -100.0, 100.0,
        [Color32::from_rgb(0, 0, 0), Color32::from_rgb(120, 120, 130)], |v| format!("{v:.1}"),
        &mut |v| app.set_grade_of_selection(|gr| gr.blacks = v));

    // ---- reset / auto ---------------------------------------------
    ui.add_space(6.0);
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
            ui.label(egui::RichText::new("Select a clip to grade").size(10.5).color(app.theme.faint));
        });
    }
}

/// One interactive RGB color wheel (drag = set, right-click = reset).
fn color_wheel(app: &mut App, ui: &mut egui::Ui, id_src: String, label: &str, w: f32, val: [f32; 3], which: usize) {
    let size = w.min(86.0);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, size + 16.0), Sense::click_and_drag());
    let p = ui.painter();
    let wr = size / 2.0 - 6.0;
    let c = Pos2::new(rect.center().x, rect.top() + size / 2.0);

    // hue ring
    let seg = 64;
    for i in 0..seg {
        let a0 = i as f32 / seg as f32 * TAU;
        let a1 = (i + 1) as f32 / seg as f32 * TAU;
        let col = wheel_ring_color(a0);
        let p0 = Pos2::new(c.x + a0.cos() * wr, c.y - a0.sin() * wr);
        let p1 = Pos2::new(c.x + a1.cos() * wr, c.y - a1.sin() * wr);
        p.line_segment([p0, p1], Stroke::new(4.0, col));
    }
    p.circle_filled(c, wr - 3.0, Color32::from_rgb(22, 22, 27));
    p.circle_stroke(c, wr, Stroke::new(1.0, app.theme.border));

    // value dot
    let (ang, mag) = rgb_to_wheel(val);
    let dot = Pos2::new(c.x + ang.cos() * mag * (wr - 5.0), c.y - ang.sin() * mag * (wr - 5.0));
    p.line_segment([c, dot], Stroke::new(1.0, Color32::from_white_alpha(90)));
    p.circle_filled(dot, 4.5, Color32::WHITE);
    p.circle_stroke(dot, 4.5, Stroke::new(1.0, Color32::BLACK));

    // caption
    let strongest = val.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    p.text(Pos2::new(rect.center().x, rect.bottom() - 11.0), Align2::CENTER_CENTER,
        label, FontId::proportional(10.0), app.theme.text);
    p.text(Pos2::new(rect.center().x, rect.bottom() - 1.0), Align2::CENTER_CENTER,
        format!("{strongest:.2}"), FontId::monospace(9.0), app.theme.faint);

    // interaction: drag sets the value through the exact same setter the
    // sliders use; right-click resets to neutral.
    let mut changed = false;
    if resp.dragged() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let d = pos - c;
            let ang = d.y.atan2(d.x);
            let mag = (d.length() / (wr - 5.0)).clamp(0.0, 1.0);
            let v = wheel_to_rgb(ang, mag);
            app.set_grade_of_selection(|gr| match which {
                0 => gr.lift = v,
                1 => gr.gamma = v,
                _ => gr.gain = v,
            });
            changed = true;
        }
    }
    if resp.secondary_clicked() {
        app.set_grade_of_selection(|gr| match which {
            0 => gr.lift = [0.0; 3],
            1 => gr.gamma = [0.0; 3],
            _ => gr.gain = [0.0; 3],
        });
        changed = true;
    }
    if changed { ui.ctx().request_repaint(); }
    let _ = id_src;
}

/// Wheel angle+magnitude → per-channel offset (cos triple, phases 0/120/240°).
fn wheel_to_rgb(ang: f32, m: f32) -> [f32; 3] {
    [m * ang.cos(), m * (ang - TAU / 3.0).cos(), m * (ang - 2.0 * TAU / 3.0).cos()]
}

/// Stored rgb triple → (angle, magnitude) for the dot position.
fn rgb_to_wheel(v: [f32; 3]) -> (f32, f32) {
    let [r, g, b] = v;
    let ang = (3.0f32.sqrt() * (g - b)).atan2(2.0 * r - g - b);
    let len = (r * r + g * g + b * b).sqrt();
    let mag = (len / 1.224745).clamp(0.0, 1.0); // len of unit cos-triple = √1.5
    (ang, mag)
}

fn wheel_ring_color(a: f32) -> Color32 {
    let f = |x: f32| ((x * 0.5 + 0.5).clamp(0.0, 1.0) * 255.0) as u8;
    let v = wheel_to_rgb(a, 1.0);
    Color32::from_rgb(f(v[0]), f(v[1]), f(v[2]))
}

// ================================================================= inspector
pub fn show_inspector(app: &mut App, ui: &mut egui::Ui) {
    ui.add_space(4.0);
    let name = app.selected_clip().map(|c| c.name.clone()).unwrap_or_default();
    section_header(app, ui, &app.t(K::Inspector), Some(&name));
    let Some(c) = app.selected_clip().cloned() else {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            ui.label(egui::RichText::new("Select a clip on the timeline to edit it").size(11.0).color(app.theme.faint));
        });
        return;
    };
    hline(app, ui);

    // ---- Compositing (opacity — real alpha) ------------------------
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        ui.label(egui::RichText::new(app.t(K::Compositing)).size(12.0).strong().color(app.theme.text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(10.0);
            if icon_btn(app, ui, 18.0, &app.t(K::Reset), ico::undo).clicked() {
                app.set_transform_of_selection(|t| t.opacity = 1.0);
            }
        });
    });
    gradient_slider(ui, app.theme, &app.t(K::Opacity), c.transform.opacity, 0.0, 1.0,
        [app.theme.border2, app.theme.border2], |v| format!("{:.0}%", v * 100.0),
        &mut |nv| app.set_transform_of_selection(|t| t.opacity = nv));

    // ---- Transform (FCP-style) -------------------------------------
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        ui.label(egui::RichText::new(app.t(K::Transform)).size(12.0).strong().color(app.theme.text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(10.0);
            if icon_btn(app, ui, 18.0, &app.t(K::Reset), ico::undo).clicked() {
                app.set_transform_of_selection(|t| *t = crate::model::Transform::default());
            }
        });
    });
    let tf = c.transform;
    gradient_slider(ui, app.theme, &format!("{} X", app.t(K::Position)), tf.x, -1.0, 1.0,
        [app.theme.border2, app.theme.border2], |v| format!("{v:.2}"), &mut |nv| app.set_transform_of_selection(|t| t.x = nv));
    gradient_slider(ui, app.theme, &format!("{} Y", app.t(K::Position)), tf.y, -1.0, 1.0,
        [app.theme.border2, app.theme.border2], |v| format!("{v:.2}"), &mut |nv| app.set_transform_of_selection(|t| t.y = nv));
    gradient_slider(ui, app.theme, &app.t(K::Rotation), tf.rotation, -180.0, 180.0,
        [app.theme.border2, app.theme.border2], |v| format!("{v:.1}°"), &mut |nv| app.set_transform_of_selection(|t| t.rotation = nv));
    gradient_slider(ui, app.theme, &app.t(K::Scale), tf.scale, 0.05, 4.0,
        [app.theme.border2, app.theme.border2], |v| format!("{:.0}%", v * 100.0), &mut |nv| app.set_transform_of_selection(|t| t.scale = nv));

    // ---- Speed / Time ----------------------------------------------
    if c.is_visual() {
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            ui.label(egui::RichText::new("Time").size(12.0).strong().color(app.theme.text));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(10.0);
                if icon_btn(app, ui, 18.0, &app.t(K::Reset), ico::undo).clicked() {
                    if let Some(cl) = app.selected_clip_mut() {
                        let ratio = cl.speed / 1.0;
                        cl.src_dur *= ratio as f64;
                        cl.speed = 1.0;
                    }
                    app.invalidate_preview();
                }
            });
        });
        gradient_slider(ui, app.theme, &app.t(K::Speed), c.speed, 0.25, 4.0,
            [app.theme.border2, app.theme.border2], |v| format!("{v:.2}×"), &mut |nv| {
                if let Some(cl) = app.selected_clip_mut() {
                    let ratio = cl.speed / nv;
                    cl.src_dur *= ratio as f64;
                    cl.speed = nv;
                }
                app.invalidate_preview();
            });
    }

    // ---- Effects quick controls -------------------------------------
    ui.add_space(2.0);
    gradient_slider(ui, app.theme, &app.t(K::Blur), c.fx.blur, 0.0, 40.0,
        [app.theme.border2, app.theme.border2], |v| format!("{v:.0}"), &mut |nv| app.set_fx_of_selection(|f| f.blur = nv));
    gradient_slider(ui, app.theme, &app.t(K::FadeIn), c.fx.fade_in, 0.0, 5.0,
        [app.theme.border2, app.theme.border2], |v| format!("{v:.2}s"), &mut |nv| app.set_fx_of_selection(|f| f.fade_in = nv));
    gradient_slider(ui, app.theme, &app.t(K::FadeOut), c.fx.fade_out, 0.0, 5.0,
        [app.theme.border2, app.theme.border2], |v| format!("{v:.2}s"), &mut |nv| app.set_fx_of_selection(|f| f.fade_out = nv));

    // ---- Audio ------------------------------------------------------
    if c.is_audio() {
        gradient_slider(ui, app.theme, &app.t(K::GainC), c.gain_db, -48.0, 12.0,
            [app.theme.border2, app.theme.border2], |v| format!("{v:.1}dB"), &mut |nv| {
                if let Some(cl) = app.selected_clip_mut() { cl.gain_db = nv; }
            });
    }

    // ---- Title ------------------------------------------------------
    if let Some(mut td) = c.title.clone() {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            ui.label(egui::RichText::new(app.t(K::TitleText)).size(12.0).color(app.theme.dim));
        });
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            if ui.add_sized([ui.available_width() - 20.0, 22.0], egui::TextEdit::singleline(&mut td.text)).changed() {
                if let Some(cl) = app.selected_clip_mut() {
                    if let Some(t2) = cl.title.as_mut() { t2.text = td.text.clone(); }
                }
            }
        });
        gradient_slider(ui, app.theme, &app.t(K::FontSize), td.size, 20.0, 220.0,
            [app.theme.border2, app.theme.border2], |v| format!("{v:.0}"), &mut |nv| {
                if let Some(cl) = app.selected_clip_mut() {
                    if let Some(t2) = cl.title.as_mut() { t2.size = nv; }
                }
            });
        app.invalidate_preview();
    }
}

// ==================================================================== audio
pub fn show_audio(app: &mut App, ui: &mut egui::Ui) {
    ui.add_space(4.0);
    section_header(app, ui, "Audio", None);
    hline(app, ui);
    let Some(c) = app.selected_clip().cloned() else {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            ui.label(egui::RichText::new("Select a clip to adjust its audio").size(11.0).color(app.theme.faint));
        });
        return;
    };
    gradient_slider(ui, app.theme, &app.t(K::GainC), c.gain_db, -48.0, 12.0,
        [app.theme.border2, app.theme.border2], |v| format!("{v:.1}dB"), &mut |nv| {
            if let Some(cl) = app.selected_clip_mut() { cl.gain_db = nv; }
        });
    gradient_slider(ui, app.theme, &app.t(K::FadeIn), c.fx.fade_in, 0.0, 5.0,
        [app.theme.border2, app.theme.border2], |v| format!("{v:.2}s"), &mut |nv| app.set_fx_of_selection(|f| f.fade_in = nv));
    gradient_slider(ui, app.theme, &app.t(K::FadeOut), c.fx.fade_out, 0.0, 5.0,
        [app.theme.border2, app.theme.border2], |v| format!("{v:.2}s"), &mut |nv| app.set_fx_of_selection(|f| f.fade_out = nv));

    ui.add_space(8.0);
    hline(app, ui);
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        ui.label(egui::RichText::new("Tracks").size(12.0).strong().color(app.theme.text));
    });
    // track mute / solo controls (mirror the timeline headers)
    egui::Grid::new("audio_tracks").num_columns(4).spacing([8.0, 3.0]).show(ui, |ui| {
        for tr in app.project.tracks.clone() {
            ui.add_space(8.0);
            ui.label(egui::RichText::new(&tr.name).size(11.5).color(app.theme.text).monospace());
            let is_audio = tr.kind == crate::model::TrackKind::Audio;
            let mid = egui::Id::new(("amix", tr.id, "m"));
            let sid = egui::Id::new(("amix", tr.id, "s"));
            if is_audio {
                if crate::ui_common::icon_toggle_shared(app, ui, Rect::from_min_size(ui.cursor().left_top(), Vec2::splat(18.0)), mid, tr.mute, "mute").clicked() {
                    if let Some(t) = app.project.track_mut(tr.id) { t.mute = !t.mute; }
                }
                ui.add_space(2.0);
                if crate::ui_common::icon_toggle_shared(app, ui, Rect::from_min_size(ui.cursor().left_top(), Vec2::splat(18.0)), sid, tr.solo, "solo").clicked() {
                    if let Some(t) = app.project.track_mut(tr.id) { t.solo = !t.solo; }
                }
            } else {
                let vid = egui::Id::new(("amix", tr.id, "v"));
                if crate::ui_common::icon_toggle_shared(app, ui, Rect::from_min_size(ui.cursor().left_top(), Vec2::splat(18.0)), vid, !tr.hidden, "eye").clicked() {
                    if let Some(t) = app.project.track_mut(tr.id) { t.hidden = !t.hidden; }
                    app.invalidate_preview();
                }
            }
            ui.end_row();
        }
    });
}

// ================================================================== effects
pub fn show_fx(app: &mut App, ui: &mut egui::Ui) {
    ui.add_space(4.0);
    section_header(app, ui, &app.t(K::WsFx), None);
    hline(app, ui);
    let Some(c) = app.selected_clip().cloned() else {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            ui.label(egui::RichText::new("Select a clip, then click a look to apply it").size(11.0).color(app.theme.faint));
        });
        return;
    };

    // ---- one-click looks (all set REAL grade/fx through the setters) --
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        ui.label(egui::RichText::new("Looks").size(12.0).strong().color(app.theme.text));
    });
    ui.add_space(2.0);
    let looks: [(&str, fn(&mut crate::model::Grade), fn(&mut crate::model::Fx)); 5] = [
        ("Cinematic", |g| { g.contrast = 18.0; g.saturation = -12.0; g.lift = [0.06, 0.02, 0.10]; g.gain = [0.10, 0.07, 0.0]; }, |_| {}),
        ("Warm Sun", |g| { g.temp = 24.0; g.gain = [0.12, 0.08, 0.0]; g.vibrance = 18.0; }, |_| {}),
        ("Teal Night", |g| { g.temp = -22.0; g.lift = [0.0, 0.05, 0.10]; g.gamma = [0.0, 0.03, 0.08]; }, |_| {}),
        ("B&W", |g| { g.saturation = -100.0; g.contrast = 12.0; }, |_| {}),
        ("Vlog Pop", |g| { g.vibrance = 35.0; g.contrast = 10.0; g.offset = 3.0; }, |f| { f.fade_in = 0.3; }),
    ];
    ui.horizontal_wrapped(|ui| {
        for (i, (name, grade_fn, fx_fn)) in looks.iter().enumerate() {
            let b = egui::Button::new(egui::RichText::new(*name).size(11.5))
                .fill(app.theme.panel3).rounding(4)
                .stroke(egui::Stroke::new(1.0, app.theme.border2));
            if ui.add(b).clicked() {
                app.set_grade_of_selection(grade_fn);
                app.set_fx_of_selection(fx_fn);
                app.toast(format!("✓ {name}"), 1);
            }
            let _ = i;
        }
    });
    ui.add_space(6.0);
    hline(app, ui);
    ui.add_space(4.0);

    gradient_slider(ui, app.theme, &app.t(K::Blur), c.fx.blur, 0.0, 40.0,
        [app.theme.border2, app.theme.border2], |v| format!("{v:.0}"), &mut |nv| app.set_fx_of_selection(|f| f.blur = nv));
    gradient_slider(ui, app.theme, &app.t(K::FadeIn), c.fx.fade_in, 0.0, 5.0,
        [app.theme.border2, app.theme.border2], |v| format!("{v:.2}s"), &mut |nv| app.set_fx_of_selection(|f| f.fade_in = nv));
    gradient_slider(ui, app.theme, &app.t(K::FadeOut), c.fx.fade_out, 0.0, 5.0,
        [app.theme.border2, app.theme.border2], |v| format!("{v:.2}s"), &mut |nv| app.set_fx_of_selection(|f| f.fade_out = nv));
    if c.is_visual() {
        gradient_slider(ui, app.theme, &app.t(K::Speed), c.speed, 0.25, 4.0,
            [app.theme.border2, app.theme.border2], |v| format!("{v:.2}×"), &mut |nv| {
                if let Some(cl) = app.selected_clip_mut() {
                    let ratio = cl.speed / nv;
                    cl.src_dur *= ratio as f64;
                    cl.speed = nv;
                }
                app.invalidate_preview();
            });
    }
    // LUT browser shortcut
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        let b = egui::Button::new(egui::RichText::new(format!("{} (.cube)", app.t(K::Browse))).size(12.0))
            .fill(app.theme.panel3).rounding(4)
            .stroke(egui::Stroke::new(1.0, app.theme.border2));
        if ui.add(b).clicked() {
            app.dialog = Some(crate::app::Dialog::Fs(crate::app::FsState {
                dir: app.project_dir.clone(),
                mode: crate::app::FsMode::PickLut,
                name: String::new(),
            }));
        }
        if c.fx.lut.is_some() && ui.button(egui::RichText::new(app.t(K::LutRemove)).size(12.0)).clicked() {
            app.set_fx_of_selection(|f| f.lut = None);
        }
    });
}

// =================================================================== scopes
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

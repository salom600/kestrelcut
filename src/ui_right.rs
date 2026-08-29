//! Right column panels, per workspace:
//!   Edit    → Inspector (Compositing/blend, Transform + keyframes, Speed,
//!             Reverse, Transition In, Effects, Titles)
//!   Color   → Lumetri-style primaries + Lift/Gamma/Gain wheels, interactive
//!             Curves, HSL Secondary, White Balance picker
//!   Audio   → clip gain/fades + real processing rack (EQ/comp/limiter/NR/
//!             reverb/de-esser/voice) + ducking + beat detection
//!   Effects → one-click looks, effect set, Chroma Key, Masks, LUT, Stabilize
//!   Scopes  → luma waveform, vectorscope, RGB parade, histogram (live)

use crate::app::App;
use crate::i18n::K;
use crate::ui_common::{animated_section, gradient_slider, hline, icon_btn, kf_button, section_header, tab_btn};
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
            if icon_btn(app, ui, 20.0, &app.t(K::PickGray), ico::wand).clicked() {
                app.wb_pick = !app.wb_pick;
                app.toast(app.t(K::PickGray), 0);
            }
        });
    });
    hline(app, ui);

    let g = app.selected_clip().map(|c| c.grade.clone()).unwrap_or_default();

    // ---- real interactive color wheels (Resolve-style, above fold)
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

    {
    if animated_section(app, ui, "curves", &app.t(K::Curves), ico::wand, |app, ui| {
        curves_editor(app, ui);
    }).is_some() {} }
    hline(app, ui);

    {
    if animated_section(app, ui, "hsl", &app.t(K::HslSecondary), ico::dropper, |app, ui| {
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            ui.label(egui::RichText::new(app.t(K::Band)).size(12.0).color(app.theme.dim));
            let bands: Vec<String> = vec![app.t(K::Reds), app.t(K::Yellows), app.t(K::Greens), app.t(K::Cyans), app.t(K::Blues), app.t(K::Magentas)];
            egui::ComboBox::from_id_source("hslband")
                .selected_text(egui::RichText::new(bands[g.hsl_band as usize % 6].clone()).size(12.0))
                .width(ui.available_width() - 20.0)
                .show_ui(ui, |ui| {
                    for (i, b) in bands.iter().enumerate() {
                        if ui.selectable_label(g.hsl_band as usize == i, egui::RichText::new(b).size(12.0)).clicked() {
                            app.set_grade_of_selection(|gr| gr.hsl_band = i as u8);
                        }
                    }
                });
        });
        gradient_slider(ui, app.theme, &app.t(K::Saturation2), g.hsl_sat, -1.0, 1.0,
            [app.theme.border2, app.theme.border2], |v| format!("{v:+.2}"),
            &mut |v| app.set_grade_of_selection(|gr| gr.hsl_sat = v));
        gradient_slider(ui, app.theme, &app.t(K::Lightness), g.hsl_light, -1.0, 1.0,
            [app.theme.border2, app.theme.border2], |v| format!("{v:+.2}"),
            &mut |v| app.set_grade_of_selection(|gr| gr.hsl_light = v));
        ui.add_space(4.0);
    }).is_some() {} }
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

/// Interactive RGB master curve → real ffmpeg `curves` points. Drag points;
/// double-click a point to remove; double-click empty space to add.
fn curves_editor(app: &mut App, ui: &mut egui::Ui) {
    let sz = ((ui.available_width() - 24.0).min(210.0)).max(120.0);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(sz + 12.0, sz + 12.0), Sense::click_and_drag());
    let plot = rect.shrink(6.0);
    let p = ui.painter();
    p.rect_filled(plot, 3.0, Color32::from_rgb(16, 16, 20));
    // grid
    for i in 0..=4 {
        let t = i as f32 / 4.0;
        let x = plot.left() + t * plot.width();
        let y = plot.bottom() - t * plot.height();
        p.line_segment([Pos2::new(x, plot.top()), Pos2::new(x, plot.bottom())], Stroke::new(0.5, app.theme.border.gamma_multiply(0.5)));
        p.line_segment([Pos2::new(plot.left(), y), Pos2::new(plot.right(), y)], Stroke::new(0.5, app.theme.border.gamma_multiply(0.5)));
    }
    p.line_segment([Pos2::new(plot.left(), plot.bottom()), Pos2::new(plot.right(), plot.top())],
        Stroke::new(1.0, app.theme.border2.gamma_multiply(0.8)));

    let mut pts = app.selected_clip().map(|c| c.grade.curves.clone()).unwrap_or_default();
    if pts.is_empty() { pts.push((0.25, 0.25)); pts.push((0.75, 0.75)); }

    let px_of = |v: (f32, f32)| Pos2::new(plot.left() + v.0 * plot.width(), plot.bottom() - v.1 * plot.height());
    let val_of = |pos: Pos2| ((pos.x - plot.left()) / plot.width()).clamp(0.0, 1.0);

    // spline (monotone piecewise, mirrors ffmpeg's monotone interpolation closely enough)
    let mut sorted = pts.clone();
    sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let n = 48;
    let mut prev = px_of((0.0, interp_y(&sorted, 0.0)));
    for i in 1..=n {
        let x = i as f32 / n as f32;
        let cur = px_of((x, interp_y(&sorted, x)));
        p.line_segment([prev, cur], Stroke::new(1.8, app.theme.accent_text));
        prev = cur;
    }
    // points
    for (i, pt) in sorted.iter().enumerate() {
        let pos = px_of(*pt);
        let hovered = resp.hovered() && resp.interact_pointer_pos().map(|m| m.distance(pos) < 8.0).unwrap_or(false);
        p.circle_filled(pos, if hovered { 6.0 } else { 4.5 }, app.theme.accent);
        p.circle_stroke(pos, 4.5, Stroke::new(1.0, Color32::WHITE));
        let _ = i;
    }

    // interaction
    if resp.dragged() {
        if let Some(m) = resp.interact_pointer_pos() {
            // find nearest point
            if let Some(nearest) = sorted.iter().enumerate()
                .min_by(|a, b| px_of(*a.1).distance(m).partial_cmp(&px_of(*b.1).distance(m)).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(i, _)| i) {
                if px_of(sorted[nearest]).distance(m) < 14.0 {
                    let v = val_of(m);
                    sorted[nearest] = (v, ((plot.bottom() - m.y) / plot.height()).clamp(0.0, 1.0));
                    app.set_grade_of_selection(|gr| gr.curves = sorted.clone());
                }
            }
        }
    }
    if resp.secondary_clicked() {
        if let Some(m) = resp.interact_pointer_pos() {
            // remove nearest point
            if let Some(nearest) = sorted.iter().enumerate()
                .min_by(|a, b| px_of(*a.1).distance(m).partial_cmp(&px_of(*b.1).distance(m)).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(i, _)| i) {
                if px_of(sorted[nearest]).distance(m) < 10.0 && sorted.len() > 1 {
                    sorted.remove(nearest);
                    app.set_grade_of_selection(|gr| gr.curves = sorted.clone());
                }
            }
        }
    }
    if resp.double_clicked() {
        if let Some(m) = resp.interact_pointer_pos() {
            let v = (val_of(m), ((plot.bottom() - m.y) / plot.height()).clamp(0.0, 1.0));
            sorted.push(v);
            app.set_grade_of_selection(|gr| gr.curves = sorted.clone());
        }
    }
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        if ui.small_button(egui::RichText::new(app.t(K::Reset)).size(11.0)).clicked() {
            app.set_grade_of_selection(|gr| gr.curves = Vec::new());
        }
        ui.label(egui::RichText::new("drag points · dbl-click add · right-click remove").size(9.5).color(app.theme.faint));
    });
    ui.add_space(4.0);
}

fn interp_y(pts: &[(f32, f32)], x: f32) -> f32 {
    let mut pts: Vec<(f32, f32)> = pts.to_vec();
    if pts.is_empty() { return x; }
    pts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    if x <= pts[0].0 { return pts[0].1; }
    for w in pts.windows(2) {
        if x >= w[0].0 && x <= w[1].0 {
            let t = ((x - w[0].0) / (w[1].0 - w[0].0).max(1e-4)).clamp(0.0, 1.0);
            // smoothstep for a spline-like feel
            let ts = t * t * (3.0 - 2.0 * t);
            return w[0].1 + (w[1].1 - w[0].1) * ts;
        }
    }
    pts.last().map(|p| p.1).unwrap_or(x)
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

fn wheel_to_rgb(ang: f32, m: f32) -> [f32; 3] {
    [m * ang.cos(), m * (ang - TAU / 3.0).cos(), m * (ang - 2.0 * TAU / 3.0).cos()]
}

fn rgb_to_wheel(v: [f32; 3]) -> (f32, f32) {
    let [r, g, b] = v;
    let ang = (3.0f32.sqrt() * (g - b)).atan2(2.0 * r - g - b);
    let len = (r * r + g * g + b * b).sqrt();
    let mag = (len / 1.224745).clamp(0.0, 1.0);
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

    let t_play = app.player.clock;
    let clip_rel = (t_play - c.tl_start).clamp(0.0, c.src_dur);
    let tf_now = c.transform_at(t_play);
    let kf_here = |ch: &crate::model::Chan| ch.iter().any(|(t, _, _)| (t - clip_rel).abs() < 0.25);

    // ---- Compositing (blend + opacity, FCP-style) --------------------
    if animated_section(app, ui, "comp", &app.t(K::Compositing2), ico::arrow_select, |app, ui| {
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            ui.label(egui::RichText::new(app.t(K::BlendMode)).size(12.0).color(app.theme.dim));
            egui::ComboBox::from_id_source("blend")
                .selected_text(egui::RichText::new(c.blend.label()).size(12.0))
                .width((ui.available_width() - 20.0).max(100.0))
                .show_ui(ui, |ui| {
                    for bm in crate::model::BlendMode::ALL {
                        if ui.selectable_label(c.blend == bm, egui::RichText::new(bm.label()).size(12.0)).clicked() {
                            if let Some(cl) = app.selected_clip_mut() { cl.blend = bm; }
                            app.invalidate_preview();
                        }
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            ui.label(egui::RichText::new(app.t(K::Opacity)).size(12.0).color(app.theme.dim));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                {
                    let has = kf_here(&c.anim.opacity);
                    let tip = app.t(K::AddKey).to_string();
                    kf_button(app, ui, &tip, has, |app| app.add_keyframe(4));
                }
                if icon_btn(app, ui, 18.0, &app.t(K::ClearKey), ico::trash).clicked() {
                    app.clear_keyframes(4);
                }
                if icon_btn(app, ui, 18.0, &app.t(K::Reset), ico::undo).clicked() {
                    app.set_transform_of_selection(|t| t.opacity = 1.0);
                    app.clear_keyframes(4);
                }
            });
        });
        gradient_slider(ui, app.theme, "", tf_now.opacity, 0.0, 1.0,
            [app.theme.border2, app.theme.border2], |v| format!("{:.0}%", v * 100.0),
            &mut |nv| {
                if let Some(cl) = app.selected_clip_mut() {
                    if cl.anim.opacity.is_empty() { cl.transform.opacity = nv; }
                }
            });
        if !c.anim.opacity.is_empty() {
            ui.label(egui::RichText::new(format!("{} {} · static {:.0}%",
                app.t(K::Keyframes), c.anim.opacity.len(), c.transform.opacity * 100.0)).size(9.5).color(app.theme.faint));
        }
        ui.add_space(4.0);
    }).is_some() {}
    hline(app, ui);

    // ---- Transform (keyframable) ------------------------------------
    if animated_section(app, ui, "transform", &app.t(K::Transform), ico::arrow_select, |app, ui| {
        let rows: [(u8, &str, f32, f32, f32, String, &crate::model::Chan); 5] = [
            (0, "X", -1.0, 1.0, tf_now.x, format!("{:+.2}", tf_now.x), &c.anim.pos_x),
            (1, "Y", -1.0, 1.0, tf_now.y, format!("{:+.2}", tf_now.y), &c.anim.pos_y),
            (2, &app.t(K::Scale), 0.05, 4.0, tf_now.scale, format!("{:.0}%", tf_now.scale * 100.0), &c.anim.scale),
            (3, &app.t(K::Rotation), -180.0, 180.0, tf_now.rotation, format!("{:.1}°", tf_now.rotation), &c.anim.rotation),
            (4, "KF", 0.0, 1.0, 0.0, String::new(), &c.anim.pos_x), // placeholder row unused
        ];
        for (chan, label, mn, mx, val, fmt, ch) in rows.iter().take(4) {
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                ui.label(egui::RichText::new(*label).size(12.0).color(app.theme.dim).monospace());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    {
                    let has = kf_here(ch);
                    let tip = app.t(K::AddKey).to_string();
                    kf_button(app, ui, &tip, has, |app| app.add_keyframe(*chan));
                }
                });
            });
            let static_v = match chan { 0 => c.transform.x, 1 => c.transform.y, 2 => c.transform.scale, _ => c.transform.rotation };
            gradient_slider(ui, app.theme, "", *val, *mn, *mx,
                [app.theme.border2, app.theme.border2], |_| fmt.clone(), &mut |nv| {
                    if let Some(cl) = app.selected_clip_mut() {
                        if cl.anim.is_channel_empty(*chan) {
                            match chan { 0 => cl.transform.x = nv, 1 => cl.transform.y = nv, 2 => cl.transform.scale = nv, _ => cl.transform.rotation = nv }
                        }
                    }
                    let _ = static_v;
                });
        }
        // ease selection for the last edited channel is available via kf list below
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            if icon_btn(app, ui, 18.0, &app.t(K::Reset), ico::undo).clicked() {
                app.set_transform_of_selection(|t| *t = crate::model::Transform::default());
                for ch in 0..5 { app.clear_keyframes(ch); }
            }
        });
        ui.add_space(4.0);
    }).is_some() {}
    hline(app, ui);

    // ---- Keyframes list + easing ------------------------------------
    if !c.anim.is_empty() {
        if animated_section(app, ui, "kfs", &format!("{} ({})", app.t(K::Keyframes), count_kf(&c.anim)), ico::keyframe, |app, ui| {
            kf_channel_row(app, ui, "X", 0, &c.anim.pos_x, clip_rel);
            kf_channel_row(app, ui, "Y", 1, &c.anim.pos_y, clip_rel);
            kf_channel_row(app, ui, "Scale", 2, &c.anim.scale, clip_rel);
            kf_channel_row(app, ui, "Rotation", 3, &c.anim.rotation, clip_rel);
            kf_channel_row(app, ui, "Opacity", 4, &c.anim.opacity, clip_rel);
            ui.add_space(4.0);
        }).is_some() {}
        hline(app, ui);
    }

    // ---- Speed / Time / Reverse --------------------------------------
    if c.is_visual() {
        if animated_section(app, ui, "time", &app.t(K::Speed), ico::speed_icon, |app, ui| {
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                ui.label(egui::RichText::new(format!("{:.2}×  →  {:.2}s", c.speed, c.src_dur)).size(12.0).color(app.theme.text));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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
            gradient_slider(ui, app.theme, "", c.speed, 0.25, 4.0,
                [app.theme.border2, app.theme.border2], |v| format!("{v:.2}×"), &mut |nv| {
                    if let Some(cl) = app.selected_clip_mut() {
                        let ratio = cl.speed / nv;
                        cl.src_dur *= ratio as f64;
                        cl.speed = nv;
                    }
                    app.invalidate_preview();
                });
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                let rev = c.reverse;
                let b = egui::Button::new(egui::RichText::new(if rev { "⏪ Reverse ON" } else { "▶ Reverse OFF" }).size(11.5))
                    .fill(if rev { app.theme.accent_dim } else { app.theme.panel3 })
                    .rounding(4).stroke(Stroke::new(1.0, app.theme.border2));
                if ui.add(b).clicked() { app.toggle_reverse(); }
                if rev {
                    ui.label(egui::RichText::new(app.t(K::RevPreviewNote)).size(9.0).color(app.theme.faint));
                }
            });
            ui.add_space(4.0);
        }).is_some() {}
        hline(app, ui);
    }

    // ---- Transition In ------------------------------------------------
    if animated_section(app, ui, "trans", &app.t(K::TransIn), ico::loop_icon, |app, ui| {
        let cur = app.selection_transition();
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            ui.label(egui::RichText::new(cur.as_ref().map(|t| t.kind.label().to_string())
                .unwrap_or_else(|| app.t(K::TransNone))).size(12.0).color(app.theme.text));
        });
        ui.add_space(2.0);
        ui.horizontal_wrapped(|ui| {
            for kind in crate::model::TransKind::ALL {
                let b = egui::Button::new(egui::RichText::new(kind.label().replace(['←', '→'], "").trim().to_string()).size(10.5))
                    .fill(app.theme.panel3).rounding(4)
                    .stroke(Stroke::new(1.0, if cur.map(|t| t.kind) == Some(kind) { app.theme.accent } else { app.theme.border2 }));
                if ui.add(b).clicked() {
                    let d = cur.map(|t| t.dur).unwrap_or(0.5);
                    app.set_transition_on_selection(kind, d);
                }
            }
        });
        if let Some(t) = &cur {
            gradient_slider(ui, app.theme, "Duration", t.dur as f32, 0.1, 2.0,
                [app.theme.border2, app.theme.border2], |v| format!("{v:.2}s"), &mut |nv| {
                    app.set_transition_on_selection(t.kind, nv as f64);
                });
            if ui.small_button(egui::RichText::new(app.t(K::TransNone)).size(11.0)).clicked() {
                app.remove_transition();
            }
        }
        ui.add_space(4.0);
    }).is_some() {}
    hline(app, ui);

    // ---- Effects quick -------------------------------------------------
    if animated_section(app, ui, "insp_fx", &app.t(K::FxPresets), ico::wand, |app, ui| {
        gradient_slider(ui, app.theme, &app.t(K::Blur), c.fx.blur, 0.0, 40.0,
            [app.theme.border2, app.theme.border2], |v| format!("{v:.0}"), &mut |nv| app.set_fx_of_selection(|f| f.blur = nv));
        gradient_slider(ui, app.theme, &app.t(K::FadeIn), c.fx.fade_in, 0.0, 5.0,
            [app.theme.border2, app.theme.border2], |v| format!("{v:.2}s"), &mut |nv| app.set_fx_of_selection(|f| f.fade_in = nv));
        gradient_slider(ui, app.theme, &app.t(K::FadeOut), c.fx.fade_out, 0.0, 5.0,
            [app.theme.border2, app.theme.border2], |v| format!("{v:.2}s"), &mut |nv| app.set_fx_of_selection(|f| f.fade_out = nv));
        ui.add_space(4.0);
    }).is_some() {}
    hline(app, ui);

    // ---- Audio gain for audio clips ------------------------------------
    if c.is_audio() {
        gradient_slider(ui, app.theme, &app.t(K::GainC), c.gain_db, -48.0, 12.0,
            [app.theme.border2, app.theme.border2], |v| format!("{v:.1}dB"), &mut |nv| {
                if let Some(cl) = app.selected_clip_mut() { cl.gain_db = nv; }
            });
    }

    // ---- Title ------------------------------------------------------
    if let Some(mut td) = c.title.clone() {
        if animated_section(app, ui, "title", &app.t(K::CreateTitles), |p, r, c| ico::letter(p, r, c, 'T'), |app, ui| {
            // presets
            ui.horizontal_wrapped(|ui| {
                let preset_labels = [app.t(K::MainTitle), app.t(K::LowerThird), app.t(K::TopCaption), app.t(K::SubtitlePreset), app.t(K::BigDark)];
                for (i, label) in preset_labels.iter().enumerate() { let label = label.clone();
                    let b = egui::Button::new(egui::RichText::new(label).size(10.5))
                        .fill(app.theme.panel3).rounding(4).stroke(Stroke::new(1.0, app.theme.border2));
                    if ui.add(b).clicked() {
                        let keep_text = td.text.clone();
                        if let Some(cl) = app.selected_clip_mut() {
                            if let Some(t2) = cl.title.as_mut() {
                                let text = t2.text.clone();
                                *t2 = crate::model::TitleData::preset(i as u8, &text);
                            }
                        }
                        let _ = keep_text;
                        app.invalidate_preview();
                    }
                }
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
            // position X/Y
            gradient_slider(ui, app.theme, "Pos X", td.pos[0], 0.0, 1.0,
                [app.theme.border2, app.theme.border2], |v| format!("{v:.2}"), &mut |nv| {
                    if let Some(cl) = app.selected_clip_mut() {
                        if let Some(t2) = cl.title.as_mut() { t2.pos[0] = nv; }
                    }
                    app.invalidate_preview();
                });
            gradient_slider(ui, app.theme, "Pos Y", td.pos[1], 0.0, 1.0,
                [app.theme.border2, app.theme.border2], |v| format!("{v:.2}"), &mut |nv| {
                    if let Some(cl) = app.selected_clip_mut() {
                        if let Some(t2) = cl.title.as_mut() { t2.pos[1] = nv; }
                    }
                    app.invalidate_preview();
                });
            gradient_slider(ui, app.theme, "Bar", td.bar, 0.0, 1.0,
                [app.theme.border2, app.theme.border2], |v| format!("{v:.2}"), &mut |nv| {
                    if let Some(cl) = app.selected_clip_mut() {
                        if let Some(t2) = cl.title.as_mut() { t2.bar = nv; }
                    }
                    app.invalidate_preview();
                });
            gradient_slider(ui, app.theme, "Shadow", td.shadow, 0.0, 1.0,
                [app.theme.border2, app.theme.border2], |v| format!("{v:.2}"), &mut |nv| {
                    if let Some(cl) = app.selected_clip_mut() {
                        if let Some(t2) = cl.title.as_mut() { t2.shadow = nv; }
                    }
                    app.invalidate_preview();
                });
            // color
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                ui.label(egui::RichText::new("Color").size(12.0).color(app.theme.dim));
                let mut col = [td.color[0], td.color[1], td.color[2]];
                if egui::color_picker::color_edit_button_srgb(ui, &mut col).changed() {
                    if let Some(cl) = app.selected_clip_mut() {
                        if let Some(t2) = cl.title.as_mut() { t2.color = col; }
                    }
                    app.invalidate_preview();
                }
            });
            ui.add_space(4.0);
        }).is_some() {}
        app.invalidate_preview();
    }
}

fn count_kf(a: &crate::model::Anim) -> usize {
    a.pos_x.len() + a.pos_y.len() + a.scale.len() + a.rotation.len() + a.opacity.len()
}

fn kf_channel_row(app: &mut App, ui: &mut egui::Ui, label: &str, chan: u8, ch: &crate::model::Chan, clip_rel: f64) {
    if ch.is_empty() { return; }
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        ui.label(egui::RichText::new(label).size(11.0).color(app.theme.dim).monospace());
        ui.label(egui::RichText::new(format!("{} kf", ch.len())).size(10.5).color(app.theme.faint));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // ease selector for the segment starting at the kf nearest the playhead
            let nearest = ch.iter().enumerate()
                .min_by(|a, b| (a.1.0 - clip_rel).abs().partial_cmp(&(b.1.0 - clip_rel).abs()).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(i, _)| i);
            if let Some(i) = nearest {
                let ease = ch[i].2;
                egui::ComboBox::from_id_source(("ease", chan))
                    .selected_text(egui::RichText::new(ease.label()).size(10.5))
                    .width(96.0)
                    .show_ui(ui, |ui| {
                        for e in [crate::model::Ease::Linear, crate::model::Ease::EaseIn, crate::model::Ease::EaseOut, crate::model::Ease::EaseInOut] {
                            if ui.selectable_label(ease == e, egui::RichText::new(e.label()).size(10.5)).clicked() {
                                let id = app.sel;
                                app.commit();
                                if let Some(cl) = id.and_then(|id| app.project.clip_mut(id)) {
                                    let target = match chan {
                                        0 => &mut cl.anim.pos_x, 1 => &mut cl.anim.pos_y, 2 => &mut cl.anim.scale,
                                        3 => &mut cl.anim.rotation, _ => &mut cl.anim.opacity,
                                    };
                                    if let Some(k) = target.get_mut(i) { k.2 = e; }
                                }
                                app.commit();
                                app.invalidate_preview();
                            }
                        }
                    });
            }
            if icon_btn(app, ui, 16.0, &app.t(K::ClearKey), ico::trash).clicked() {
                app.clear_keyframes(chan);
            }
        });
    });
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

    // gain + fades
    gradient_slider(ui, app.theme, &app.t(K::GainC), c.gain_db, -48.0, 12.0,
        [app.theme.border2, app.theme.border2], |v| format!("{v:.1}dB"), &mut |nv| {
            if let Some(cl) = app.selected_clip_mut() { cl.gain_db = nv; }
        });
    gradient_slider(ui, app.theme, &app.t(K::FadeIn), c.fx.fade_in, 0.0, 5.0,
        [app.theme.border2, app.theme.border2], |v| format!("{v:.2}s"), &mut |nv| app.set_fx_of_selection(|f| f.fade_in = nv));
    gradient_slider(ui, app.theme, &app.t(K::FadeOut), c.fx.fade_out, 0.0, 5.0,
        [app.theme.border2, app.theme.border2], |v| format!("{v:.2}s"), &mut |nv| app.set_fx_of_selection(|f| f.fade_out = nv));

    // ---- processing rack (real ffmpeg filters, identical at export) ----
    ui.add_space(6.0);
    hline(app, ui);
    if animated_section(app, ui, "rack", &app.t(K::EqLow), ico::note, |app, ui| {
        gradient_slider(ui, app.theme, &app.t(K::EqLow), c.afx.eq_low, -18.0, 18.0,
            [app.theme.border2, app.theme.border2], |v| format!("{v:+.1}dB"), &mut |nv| {
                if let Some(cl) = app.selected_clip_mut() { cl.afx.eq_low = nv; }
            });
        gradient_slider(ui, app.theme, &app.t(K::EqMid), c.afx.eq_mid, -18.0, 18.0,
            [app.theme.border2, app.theme.border2], |v| format!("{v:+.1}dB"), &mut |nv| {
                if let Some(cl) = app.selected_clip_mut() { cl.afx.eq_mid = nv; }
            });
        gradient_slider(ui, app.theme, &app.t(K::EqHigh), c.afx.eq_high, -18.0, 18.0,
            [app.theme.border2, app.theme.border2], |v| format!("{v:+.1}dB"), &mut |nv| {
                if let Some(cl) = app.selected_clip_mut() { cl.afx.eq_high = nv; }
            });
        ui.add_space(4.0);
    }).is_some() {}
    hline(app, ui);

    if animated_section(app, ui, "rack2", &app.t(K::NoiseReduction), ico::mic, |app, ui| {
        fx_toggle_row(app, ui, &app.t(K::Compressor), c.afx.compressor, |cl| cl.afx.compressor = true, |cl| cl.afx.compressor = false);
        fx_toggle_row(app, ui, &app.t(K::Limiter), c.afx.limiter, |cl| cl.afx.limiter = true, |cl| cl.afx.limiter = false);
        fx_toggle_row(app, ui, &app.t(K::DeEsser), c.afx.deess, |cl| cl.afx.deess = true, |cl| cl.afx.deess = false);
        fx_toggle_row(app, ui, &app.t(K::VoiceClarity), c.afx.voice, |cl| cl.afx.voice = true, |cl| cl.afx.voice = false);
        let nr_label = app.t(K::NoiseReduction).to_string();
        let mut nr_on = c.afx.nr > 0.5;
        toggle_ui(app, ui, &nr_label, nr_on, |on| { nr_on = on; });
        if nr_on != (c.afx.nr > 0.5) {
            if let Some(cl) = app.selected_clip_mut() { cl.afx.nr = if nr_on { 40.0 } else { 0.0 }; }
        }
        gradient_slider(ui, app.theme, &app.t(K::NoiseReduction), c.afx.nr, 0.0, 100.0,
            [app.theme.border2, app.theme.border2], |v| format!("{v:.0}"), &mut |nv| {
                if let Some(cl) = app.selected_clip_mut() { cl.afx.nr = nv; }
            });
        gradient_slider(ui, app.theme, &app.t(K::Reverb), c.afx.reverb, 0.0, 100.0,
            [app.theme.border2, app.theme.border2], |v| format!("{v:.0}"), &mut |nv| {
                if let Some(cl) = app.selected_clip_mut() { cl.afx.reverb = nv; }
            });
        ui.add_space(4.0);
    }).is_some() {}
    hline(app, ui);

    // ---- Ducking + beats -----------------------------------------------
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        let b = egui::Button::new(egui::RichText::new(format!("duck {}", app.t(K::AutoDuck))).size(11.0))
            .fill(app.theme.panel3).rounding(4).stroke(Stroke::new(1.0, app.theme.border2));
        if ui.add(b).clicked() { app.auto_duck_under_selection(); }
        let b2 = egui::Button::new(egui::RichText::new(app.t(K::BeatDetect)).size(11.0))
            .fill(app.theme.panel3).rounding(4).stroke(Stroke::new(1.0, app.theme.border2));
        if ui.add(b2).clicked() { app.detect_beats(); }
    });

    ui.add_space(8.0);
    hline(app, ui);
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        ui.label(egui::RichText::new("Tracks").size(12.0).strong().color(app.theme.text));
    });
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

fn fx_toggle_row(app: &mut App, ui: &mut egui::Ui, label: &str, on: bool,
                 set_on: impl Fn(&mut crate::model::Clip), set_off: impl Fn(&mut crate::model::Clip)) {
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        let b = egui::Button::new(egui::RichText::new(if on { format!("✓ {label}") } else { label.to_string() }).size(11.5))
            .fill(if on { app.theme.accent_dim } else { app.theme.panel3 })
            .rounding(4).stroke(Stroke::new(1.0, app.theme.border2));
        if ui.add(b).clicked() {
            if let Some(cl) = app.selected_clip_mut() { if on { set_off(cl) } else { set_on(cl) } }
        }
    });
}

fn toggle_ui(app: &App, ui: &mut egui::Ui, label: &str, on: bool, mut set: impl FnMut(bool)) {
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        ui.label(egui::RichText::new(label).size(12.0).color(app.theme.dim));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(10.0);
            if ui.add(egui::Checkbox::without_text(&mut { on })).clicked() {
                set(!on);
            }
        });
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
        ("B&W", |g| { g.saturation = -100.0; g.contrast = 12.0; }, |f| { f.grayscale = true; }),
        ("Vlog Pop", |g| { g.vibrance = 35.0; g.contrast = 10.0; g.offset = 3.0; }, |f| { f.fade_in = 0.3; f.sharpen = 20.0; }),
    ];
    ui.horizontal_wrapped(|ui| {
        for (i, (name, grade_fn, fx_fn)) in looks.iter().enumerate() {
            let b = egui::Button::new(egui::RichText::new(*name).size(11.5))
                .fill(app.theme.panel3).rounding(4)
                .stroke(Stroke::new(1.0, app.theme.border2));
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

    // ---- effect set (all real ffmpeg filters) --------------------------
    if animated_section(app, ui, "fxset", &app.t(K::FxPresets), ico::wand, |app, ui| {
        gradient_slider(ui, app.theme, &app.t(K::Blur), c.fx.blur, 0.0, 40.0,
            [app.theme.border2, app.theme.border2], |v| format!("{v:.0}"), &mut |nv| app.set_fx_of_selection(|f| f.blur = nv));
        gradient_slider(ui, app.theme, &app.t(K::Sharpen), c.fx.sharpen, 0.0, 100.0,
            [app.theme.border2, app.theme.border2], |v| format!("{v:.0}"), &mut |nv| app.set_fx_of_selection(|f| f.sharpen = nv));
        gradient_slider(ui, app.theme, &app.t(K::Denoise), c.fx.denoise, 0.0, 100.0,
            [app.theme.border2, app.theme.border2], |v| format!("{v:.0}"), &mut |nv| app.set_fx_of_selection(|f| f.denoise = nv));
        gradient_slider(ui, app.theme, &app.t(K::Glow), c.fx.glow, 0.0, 100.0,
            [app.theme.border2, app.theme.border2], |v| format!("{v:.0}"), &mut |nv| app.set_fx_of_selection(|f| f.glow = nv));
        gradient_slider(ui, app.theme, &app.t(K::Vignette), c.fx.vignette, 0.0, 100.0,
            [app.theme.border2, app.theme.border2], |v| format!("{v:.0}"), &mut |nv| app.set_fx_of_selection(|f| f.vignette = nv));
        gradient_slider(ui, app.theme, &app.t(K::Hue), c.fx.hue, -180.0, 180.0,
            [Color32::from_rgb(180, 90, 60), Color32::from_rgb(90, 60, 200)], |v| format!("{v:.0}°"), &mut |nv| app.set_fx_of_selection(|f| f.hue = nv));
        gradient_slider(ui, app.theme, &app.t(K::Deband), c.fx.deband, 0.0, 100.0,
            [app.theme.border2, app.theme.border2], |v| format!("{v:.0}"), &mut |nv| app.set_fx_of_selection(|f| f.deband = nv));
        gradient_slider(ui, app.theme, &app.t(K::LensCorrection), c.fx.lens_k1, -0.5, 0.5,
            [app.theme.border2, app.theme.border2], |v| format!("{v:+.2}"), &mut |nv| app.set_fx_of_selection(|f| f.lens_k1 = nv));
        gradient_slider(ui, app.theme, "K2", c.fx.lens_k2, -0.5, 0.5,
            [app.theme.border2, app.theme.border2], |v| format!("{v:+.2}"), &mut |nv| app.set_fx_of_selection(|f| f.lens_k2 = nv));
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            let mut g = c.fx.grayscale;
            if ui.checkbox(&mut g, app.t(K::Grayscale)).changed() {
                app.set_fx_of_selection(|f| f.grayscale = g);
            }
            let mut sp = c.fx.sepia;
            if ui.checkbox(&mut sp, app.t(K::Sepia)).changed() {
                app.set_fx_of_selection(|f| f.sepia = sp);
            }
        });
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
        ui.add_space(4.0);
    }).is_some() {}
    hline(app, ui);

    // ---- Chroma Key (green screen) --------------------------------------
    if animated_section(app, ui, "chroma", &app.t(K::ChromaKey), ico::dropper, |app, ui| {
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            let mut on = c.fx.chroma.enabled;
            if ui.checkbox(&mut on, app.t(K::ChromaKey)).changed() {
                app.set_fx_of_selection(|f| f.chroma.enabled = on);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(10.0);
                let mut col = c.fx.chroma.color;
                if egui::color_picker::color_edit_button_srgb(ui, &mut col).changed() {
                    app.set_fx_of_selection(|f| f.chroma.color = col);
                }
                if ui.small_button(egui::RichText::new("Green").size(10.5)).clicked() {
                    app.set_fx_of_selection(|f| f.chroma = crate::model::ChromaKey::classic());
                }
                if ui.small_button(egui::RichText::new("Blue").size(10.5)).clicked() {
                    app.set_fx_of_selection(|f| {
                        f.chroma = crate::model::ChromaKey::classic();
                        f.chroma.color = [0, 70, 255];
                    });
                }
            });
        });
        if c.fx.chroma.enabled {
            gradient_slider(ui, app.theme, &app.t(K::Similarity), c.fx.chroma.similarity, 0.01, 1.0,
                [app.theme.border2, app.theme.border2], |v| format!("{v:.2}"), &mut |nv| app.set_fx_of_selection(|f| f.chroma.similarity = nv));
            gradient_slider(ui, app.theme, &app.t(K::BlendMode), c.fx.chroma.blend, 0.0, 1.0,
                [app.theme.border2, app.theme.border2], |v| format!("{v:.2}"), &mut |nv| app.set_fx_of_selection(|f| f.chroma.blend = nv));
            gradient_slider(ui, app.theme, &app.t(K::Spill), c.fx.chroma.spill, 0.0, 1.0,
                [app.theme.border2, app.theme.border2], |v| format!("{v:.2}"), &mut |nv| app.set_fx_of_selection(|f| f.chroma.spill = nv));
        }
        ui.add_space(4.0);
    }).is_some() {}
    hline(app, ui);

    // ---- Masks -----------------------------------------------------------
    if animated_section(app, ui, "mask", &app.t(K::Masks), ico::grid_all, |app, ui| {
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            let mut on = c.fx.mask.enabled;
            if ui.checkbox(&mut on, app.t(K::Masks)).changed() {
                app.set_fx_of_selection(|f| f.mask.enabled = on);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(10.0);
                let shape_label = if c.fx.mask.ellipse { app.t(K::MaskEllipse) } else { app.t(K::MaskRect) };
                if ui.small_button(egui::RichText::new(shape_label).size(10.5)).clicked() {
                    let e = !c.fx.mask.ellipse;
                    app.set_fx_of_selection(|f| f.mask.ellipse = e);
                }
                let mut inv = c.fx.mask.invert;
                if ui.checkbox(&mut inv, "Invert").changed() {
                    app.set_fx_of_selection(|f| f.mask.invert = inv);
                }
            });
        });
        if c.fx.mask.enabled {
            if c.fx.mask.hw <= 0.01 {
                app.set_fx_of_selection(|f| {
                    f.mask.cx = 0.5; f.mask.cy = 0.5; f.mask.hw = 0.3; f.mask.hh = 0.3;
                });
            }
            gradient_slider(ui, app.theme, "Center X", c.fx.mask.cx, 0.0, 1.0,
                [app.theme.border2, app.theme.border2], |v| format!("{v:.2}"), &mut |nv| app.set_fx_of_selection(|f| f.mask.cx = nv));
            gradient_slider(ui, app.theme, "Center Y", c.fx.mask.cy, 0.0, 1.0,
                [app.theme.border2, app.theme.border2], |v| format!("{v:.2}"), &mut |nv| app.set_fx_of_selection(|f| f.mask.cy = nv));
            gradient_slider(ui, app.theme, "Width", c.fx.mask.hw, 0.02, 0.7,
                [app.theme.border2, app.theme.border2], |v| format!("{v:.2}"), &mut |nv| app.set_fx_of_selection(|f| f.mask.hw = nv));
            gradient_slider(ui, app.theme, "Height", c.fx.mask.hh, 0.02, 0.7,
                [app.theme.border2, app.theme.border2], |v| format!("{v:.2}"), &mut |nv| app.set_fx_of_selection(|f| f.mask.hh = nv));
            gradient_slider(ui, app.theme, "Feather", c.fx.mask.feather, 0.0, 0.49,
                [app.theme.border2, app.theme.border2], |v| format!("{v:.2}"), &mut |nv| app.set_fx_of_selection(|f| f.mask.feather = nv));
        }
        ui.add_space(4.0);
    }).is_some() {}
    hline(app, ui);

    // ---- Stabilize (only when the bundled ffmpeg really has vidstab) ------
    if crate::media::has_stabilizer() {
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            let b = egui::Button::new(egui::RichText::new(app.t(K::Stabilize)).size(11.5))
                .fill(app.theme.panel3).rounding(4).stroke(Stroke::new(1.0, app.theme.border2));
            if ui.add(b).clicked() {
                app.toast("vidstab: use proxy pipeline — generating stabilized copy…", 0);
                app.stabilize_selected();
            }
        });
    } else if app.sel.is_some() {
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            ui.label(egui::RichText::new(app.t(K::StabUnavailable)).size(9.5).color(app.theme.faint));
        });
    }

    // LUT browser shortcut
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        let b = egui::Button::new(egui::RichText::new(format!("{} (.cube)", app.t(K::Browse))).size(12.0))
            .fill(app.theme.panel3).rounding(4)
            .stroke(Stroke::new(1.0, app.theme.border2));
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
    let each_h = ((avail.y - 62.0) / 4.0).max(48.0);
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
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        scope_box(app, ui, &app.t(K::Histogram), app.scopes.hist.as_ref(), Vec2::new(w, each_h));
    });
}

fn scope_box(app: &App, ui: &mut egui::Ui, title: &str, tex: Option<&egui::TextureHandle>, size: Vec2) {
    let (r, _) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect_filled(r, 3.0, app.theme.scope_bg);
    ui.painter().rect_stroke(r, 3.0, Stroke::new(1.0, app.theme.border), egui::StrokeKind::Inside);
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

    // ---- luma waveform
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

    // ---- RGB histogram (3 channel histograms overlaid)
    let mut hist = image::RgbaImage::new(256, 100);
    let mut hr = [0u32; 64]; let mut hg = [0u32; 64]; let mut hb = [0u32; 64];
    for p in px.iter() {
        hr[(p[0] >> 2) as usize] += 1;
        hg[(p[1] >> 2) as usize] += 1;
        hb[(p[2] >> 2) as usize] += 1;
    }
    let mx = hr.iter().chain(hg.iter()).chain(hb.iter()).copied().fold(1u32, u32::max);
    for i in 0..64 {
        let x = (i * 4) as u32;
        for (data, col) in [(&hr, [255, 80, 80]), (&hg, [80, 255, 110]), (&hb, [100, 130, 255])] {
            let v = ((data[i] as f32 / mx as f32) * 96.0) as u32;
            for dy in 0..v.min(100) {
                let y = 99 - dy;
                let cur = hist.get_pixel(x + 1, y).0;
                let nc = image::Rgba([col[0], col[1], col[2], 160]);
                let mixed = image::Rgba([
                    (cur[0] as u16 + nc[0] as u16).min(255) as u8,
                    (cur[1] as u16 + nc[1] as u16).min(255) as u8,
                    (cur[2] as u16 + nc[2] as u16).min(255) as u8,
                    255]);
                hist.put_pixel(x + 1, y, mixed);
            }
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
    app.scopes.hist = to_tex(&hist);
}

fn rand_suffix() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.subsec_nanos() as u64).unwrap_or(0)
}

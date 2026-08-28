//! Modal dialogs: in-app file browser (works cross-platform without native
//! portals), export dialog with live progress, proxy manager, learn/about.

use crate::app::{App, Dialog, FsMode, FsState};
use crate::i18n::K;
use crate::ui_common::gradient_slider;
use egui::{Align2, FontId, Pos2, Rect, RichText, Sense, Vec2};

pub fn show(app: &mut App, ctx: &egui::Context) {
    match app.dialog.clone() {
        Some(Dialog::Fs(st)) => file_browser(app, ctx, st),
        Some(Dialog::Learn) => learn(app, ctx),
        Some(Dialog::Proxies) => proxies(app, ctx),
        None => {}
    }
    if app.export_state.open {
        export_dialog(app, ctx);
    }
}

// ---------------------------------------------------------------- browser
fn file_browser(app: &mut App, ctx: &egui::Context, st: FsState) {
    let title = st.mode.title(app.lang);
    let mut open = true;
    egui::Window::new(RichText::new(title).size(14.0))
        .open(&mut open)
        .default_width(560.0)
        .default_height(420.0)
        .collapsible(false)
        .resizable(true)
        .show(ctx, |ui| {
            let mut enter_dir: Option<std::path::PathBuf> = None;
            let mut pick: Option<std::path::PathBuf> = None;
            // toolbar
            ui.horizontal(|ui| {
                if ui.button("⌂").clicked() {
                    if let Some(h) = crate::app::dirs_home() { enter_dir = Some(h); }
                }
                if ui.button("↑").clicked() {
                    if let Some(p) = st.dir.parent() { enter_dir = Some(p.to_path_buf()); }
                }
                ui.label(RichText::new(st.dir.display().to_string()).size(11.0).color(app.theme.dim).monospace());
            });
            ui.separator();
            // listing
            egui::ScrollArea::vertical().max_height(ui.available_height() - 66.0).show(ui, |ui| {
                let mut entries: Vec<(String, bool, std::path::PathBuf)> = Vec::new();
                if let Ok(rd) = std::fs::read_dir(&st.dir) {
                    for e in rd.flatten() {
                        let name = e.file_name().to_string_lossy().to_string();
                        if name.starts_with('.') { continue; }
                        let is_dir = e.path().is_dir();
                        entries.push((name, is_dir, e.path()));
                    }
                }
                entries.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
                let filter = st.mode.filter();
                for (name, is_dir, path) in entries {
                    if !is_dir {
                        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                        if !filter.is_empty() && !filter.contains(&ext.as_str()) { continue; }
                    }
                    let icon = if is_dir { "▸ " } else { "  " };
                    let label = format!("{icon}{name}");
                    if ui.add(egui::Button::new(RichText::new(label).size(12.5))
                        .fill(egui::Color32::TRANSPARENT)
                        .stroke(egui::Stroke::NONE))
                        .clicked()
                    {
                        if is_dir { enter_dir = Some(path); }
                        else { pick = Some(path); }
                    }
                }
            });
            ui.separator();
            // name field (save modes)
            if matches!(st.mode, FsMode::SaveProject | FsMode::SaveExport) {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(app.t(K::FileName)).size(12.0));
                    let mut name = st.name.clone();
                    if ui.add_sized([ui.available_width() - 8.0, 22.0], egui::TextEdit::singleline(&mut name)).changed() {
                        app.dialog = Some(Dialog::Fs(FsState { dir: st.dir.clone(), mode: st.mode, name }));
                    }
                });
            }
            ui.horizontal(|ui| {
                if ui.button(RichText::new(if matches!(st.mode, FsMode::SaveProject | FsMode::SaveExport) { app.t(K::SaveBtn) } else { app.t(K::Open) }).size(12.5)).clicked() {
                    let final_pick = pick.clone().or_else(|| {
                        if st.name.trim().is_empty() { None } else { Some(st.dir.join(&st.name)) }
                    });
                    if let Some(p) = final_pick { fs_done(app, p, st.mode); }
                }
                if ui.button(RichText::new(app.t(K::CancelBtn)).size(12.5)).clicked() {
                    app.dialog = None;
                }
                if let Some(p) = &pick {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(p.display().to_string()).size(10.0).color(app.theme.faint).monospace());
                    });
                }
            });
            if let Some(d) = enter_dir {
                app.dialog = Some(Dialog::Fs(FsState { dir: d, mode: st.mode, name: st.name.clone() }));
            } else if let Some(p) = pick {
                if matches!(st.mode, FsMode::OpenMedia | FsMode::OpenProject | FsMode::PickLut) {
                    fs_done(app, p, st.mode);
                } else {
                    // save modes: fill name
                    let name = p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                    app.dialog = Some(Dialog::Fs(FsState { dir: st.dir.clone(), mode: st.mode, name }));
                }
            }
        });
    if !open { app.dialog = None; }
}

fn fs_done(app: &mut App, p: std::path::PathBuf, mode: FsMode) {
    app.dialog = None;
    match mode {
        FsMode::OpenMedia => app.import_files(vec![p]),
        FsMode::OpenProject => app.load_project(p),
        FsMode::SaveProject => app.save_project(p),
        FsMode::PickLut => {
            app.set_fx_of_selection(|fx| fx.lut = Some(p));
        }
        FsMode::SaveExport => {
            if let Some(name) = p.file_name() {
                app.export_state.name = name.to_string_lossy().to_string();
            }
            if let Some(d) = p.parent() {
                app.export_state.dir = d.to_path_buf();
            }
            app.start_export();
        }
    }
}

// ---------------------------------------------------------------- export
pub fn export_dialog(app: &mut App, ctx: &egui::Context) {
    let mut open = true;
    egui::Window::new(RichText::new(app.t(K::ExportTitle)).size(14.0))
        .open(&mut open)
        .default_width(480.0)
        .collapsible(false)
        .show(ctx, |ui| {
            export_fields(app, ui);
        });
    app.export_state.open = open && app.export_state.open;
    if !open { app.export_state.open = false; }
}

/// Export page body (used by dialog AND the Deliver workspace).
pub fn export_fields(app: &mut App, ui: &mut egui::Ui) {
    // pre-compute labels to avoid holding a mutable borrow across ui calls
    let t_preset = app.t(K::Preset);
    let t_codec = app.t(K::Codec);
    let t_hw = app.t(K::HwAccel);
    let t_res = app.t(K::Resolution);
    let t_src = app.t(K::SourceRes);
    let t_fps = app.t(K::FrameRate);
    let t_range = app.t(K::Range);
    let t_entire = app.t(K::EntireSeq);
    let t_inout = app.t(K::InOutOnly);
    let t_out = app.t(K::OutputFile);
    let t_browse = app.t(K::Browse);
    let t_cancel = app.t(K::Cancel);
    let t_start = app.t(K::StartExport);
    let t_done = app.t(K::Done);
    let t_folder = app.t(K::OpenFolder);
    let t_running = app.t(K::ExportRunning);
    let seq_fps = app.project.fps;
    let has_io = app.project.in_mark.is_some() && app.project.out_mark.is_some();

    let res_choice = app.export_state.res_choice;
    let fps_choice = app.export_state.fps_choice;
    let range_inout = app.export_state.range_inout;

    // presets
    ui.horizontal(|ui| {
        ui.label(RichText::new(t_preset).size(12.0).color(app.theme.dim));
        let b = |txt: &str, sel: bool| egui::Button::new(RichText::new(txt).size(11.5))
            .fill(if sel { app.theme.accent_dim } else { app.theme.panel3 })
            .rounding(4);
        if ui.add(b(&app.t(K::Yt1080), res_choice == 1 && fps_choice == 0)).clicked() {
            app.export_state.res_choice = 1; app.export_state.fps_choice = 0;
        }
        if ui.add(b(&app.t(K::Yt720), res_choice == 2 && fps_choice == 0)).clicked() {
            app.export_state.res_choice = 2; app.export_state.fps_choice = 0;
        }
        if ui.add(b(&app.t(K::SrcPreset), res_choice == 0 && fps_choice == 0)).clicked() {
            app.export_state.res_choice = 0; app.export_state.fps_choice = 0;
        }
    });
    ui.add_space(4.0);
    // codec
    ui.horizontal(|ui| {
        ui.label(RichText::new(t_codec).size(12.0).color(app.theme.dim));
        let all: Vec<(String, String)> = app.export_state.hw.iter().cloned()
            .chain(app.export_state.sw.iter().cloned()).collect();
        let cur_label = all.iter().find(|(id, _)| *id == app.export_state.vcodec)
            .map(|(_, l)| l.clone()).unwrap_or_else(|| app.export_state.vcodec.clone());
        egui::ComboBox::from_id_source("vcodec")
            .selected_text(RichText::new(cur_label).size(12.0))
            .width(240.0)
            .show_ui(ui, |ui| {
                if !app.export_state.hw.is_empty() {
                    ui.label(RichText::new(t_hw).size(10.5).color(app.theme.faint));
                    for (id, label) in &all {
                        let is_hw = app.export_state.hw.iter().any(|(h, _)| h == id);
                        if !is_hw && id == &all.first().map(|x| x.0.clone()).unwrap_or_default() {
                            ui.separator();
                        }
                        if ui.selectable_label(*id == app.export_state.vcodec, RichText::new(label).size(12.0)).clicked() {
                            app.export_state.vcodec = id.clone();
                        }
                    }
                } else {
                    for (id, label) in &all {
                        if ui.selectable_label(*id == app.export_state.vcodec, RichText::new(label).size(12.0)).clicked() {
                            app.export_state.vcodec = id.clone();
                        }
                    }
                }
            });
    });
    // quality
    let q = app.export_state.quality;
    gradient_slider(ui, app.theme, &app.t(K::QualityCrf), q as f32, 14.0, 32.0,
        [app.theme.ok, app.theme.err], |v| format!("CRF {v:.0}"), &mut |v| app.export_state.quality = v.round() as u32);
    // resolution + fps
    ui.horizontal(|ui| {
        ui.label(RichText::new(t_res).size(12.0).color(app.theme.dim));
        let res_names = [t_src, "1920×1080".to_string(), "1280×720".to_string()];
        for (i, n) in res_names.iter().enumerate() {
            if ui.selectable_label(res_choice == i, RichText::new(n).size(12.0)).clicked() {
                app.export_state.res_choice = i;
            }
        }
        ui.add_space(8.0);
        ui.label(RichText::new(t_fps).size(12.0).color(app.theme.dim));
        let fps_names = [format!("{seq_fps:.0}"), "60".into(), "30".into(), "24".into()];
        for (i, n) in fps_names.iter().enumerate() {
            if ui.selectable_label(fps_choice == i, RichText::new(n).size(12.0)).clicked() {
                app.export_state.fps_choice = i;
            }
        }
    });
    // range
    ui.horizontal(|ui| {
        ui.label(RichText::new(t_range).size(12.0).color(app.theme.dim));
        if ui.selectable_label(!range_inout, RichText::new(t_entire).size(12.0)).clicked() {
            app.export_state.range_inout = false;
        }
        if ui.selectable_label(range_inout, RichText::new(t_inout).size(12.0)).clicked() {
            app.export_state.range_inout = true;
        }
        if range_inout && !has_io {
            ui.label(RichText::new("(!)").size(11.0).color(app.theme.warn));
        }
    });
    // output name
    ui.horizontal(|ui| {
        ui.label(RichText::new(t_out).size(12.0).color(app.theme.dim));
        ui.add_sized([ui.available_width() - 84.0, 22.0], egui::TextEdit::singleline(&mut app.export_state.name));
        if ui.button(RichText::new(t_browse).size(12.0)).clicked() {
            app.dialog = Some(Dialog::Fs(FsState { dir: app.project_dir.join("exports"), mode: FsMode::SaveExport, name: app.export_state.name.clone() }));
        }
    });

    ui.add_space(6.0);
    ui.separator();
    // progress / result / start
    if let Some((_, frac, out_time)) = app.export_state.running {
        ui.add_space(4.0);
        let pct = (frac * 100.0) as u32;
        ui.label(RichText::new(format!("{} … {}%  ({})", t_running, pct,
            crate::util::timecode(out_time, app.project.fps))).size(12.5).color(app.theme.accent_text));
        let top = ui.cursor().top();
        let left = ui.cursor().left();
        let w = ui.available_width();
        let bar = Rect::from_min_max(Pos2::new(left, top + 2.0), Pos2::new(left + w, top + 12.0));
        ui.allocate_rect(bar, Sense::hover());
        ui.painter().rect_filled(bar, 4, app.theme.panel3);
        ui.painter().rect_filled(Rect::from_min_max(bar.min, Pos2::new(bar.left() + bar.width() * frac.clamp(0.02, 1.0), bar.max.y)), 4, app.theme.accent);
        ui.add_space(18.0);
        if ui.button(RichText::new(t_cancel).size(12.5)).clicked() {
            crate::exporter::cancel_export();
            app.export_state.running = None;
            app.toast(app.t(K::Cancel), 2);
        }
    } else {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let start = egui::Button::new(RichText::new(format!("▶ {t_start}")).size(13.0))
                .fill(app.theme.accent)
                .rounding(5);
            if ui.add(start).clicked() { app.start_export(); }
            if let Some(res) = &app.export_state.last_result {
                match res {
                    Ok(p) => {
                        ui.label(RichText::new(format!("✓ {t_done}")).size(12.5).color(app.theme.ok));
                        if ui.button(RichText::new(t_folder).size(11.5)).clicked() {
                            let folder = p.parent().map(|d| d.to_path_buf()).unwrap_or_else(|| app.project_dir.clone());
                            open_folder(&folder);
                        }
                    }
                    Err(e) => { ui.label(RichText::new(format!("✗ {e}")).size(12.0).color(app.theme.err)); }
                }
            }
        });
    }
}

pub fn open_folder(p: &std::path::Path) {
    #[cfg(target_os = "windows")]
    { let _ = std::process::Command::new("explorer").arg(p).spawn(); }
    #[cfg(target_os = "macos")]
    { let _ = std::process::Command::new("open").arg(p).spawn(); }
    #[cfg(all(unix, not(target_os = "macos")))]
    { let _ = std::process::Command::new("xdg-open").arg(p).spawn(); }
}

// ---------------------------------------------------------------- proxies
fn proxies(app: &mut App, ctx: &egui::Context) {
    let mut open = true;
    egui::Window::new(RichText::new(app.t(K::ProxiesTitle)).size(14.0))
        .open(&mut open).default_width(520.0).collapsible(false)
        .show(ctx, |ui| {
            ui.label(RichText::new(app.t(K::ProxyDesc)).size(12.0).color(app.theme.dim));
            ui.add_space(4.0);
            let t_proxies = app.t(K::UseProxies);
            ui.checkbox(&mut app.proxy_enabled, RichText::new(t_proxies).size(12.5));
            ui.separator();
            let mut gen: Vec<u64> = Vec::new();
            egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                for a in &app.assets {
                    ui.horizontal(|ui| {
                        let done = a.proxy.as_ref().map(|p| p.exists()).unwrap_or(false);
                        if done {
                            ui.label(RichText::new("✓").size(13.0).color(app.theme.ok));
                        }
                        ui.label(RichText::new(a.label()).size(12.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if !done && ui.small_button(RichText::new(app.t(K::Generate)).size(11.0)).clicked() {
                                gen.push(a.id);
                            }
                        });
                    });
                }
            });
            for id in gen { app.create_proxy_for(id); }
            if app.assets.is_empty() {
                ui.label(RichText::new(app.t(K::NoMedia)).size(12.0).color(app.theme.faint));
            }
        });
    if !open { app.dialog = None; }
}

// ---------------------------------------------------------------- learn
fn learn(app: &mut App, ctx: &egui::Context) {
    let mut open = true;
    egui::Window::new(RichText::new(app.t(K::LearnTitle)).size(14.0))
        .open(&mut open).default_width(520.0).collapsible(false)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("KestrelCut").size(17.0).strong().color(app.theme.accent_text));
                ui.label(RichText::new(app.t(K::AppTagline)).size(12.0).color(app.theme.dim));
            });
            ui.add_space(6.0);
            ui.label(RichText::new(app.t(K::LearnBody)).size(12.5));
            ui.add_space(6.0);
            ui.label(RichText::new(app.t(K::Stabilized)).size(12.0).color(app.theme.ok));
            ui.label(RichText::new(app.t(K::Deterministic)).size(12.0).color(app.theme.ok));
            ui.separator();
            ui.label(RichText::new(app.t(K::Shortcuts)).size(12.5).strong());
            ui.label(RichText::new(app.t(K::ShotcutsBody)).size(11.5).color(app.theme.dim));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(app.t(K::Language)).size(12.0).color(app.theme.dim));
                if ui.button(RichText::new(app.lang.label()).size(12.0)).clicked() {
                    app.lang = app.lang.toggle();
                }
            });
        });
    if !open { app.dialog = None; }
}

//! Media pool panel (left column): project header, tabs, search, thumbnail
//! grid, import bar — mirrors the reference layout.

use crate::app::{App, Dialog, FsMode, FsState};
use crate::i18n::K;
use crate::model::AssetKind;
use crate::ui_common::{hline, icon_btn, icon_toggle, section_header, tab_btn};
use crate::ui_icons as ico;
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Vec2};

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    ui.add_space(4.0);
    // header: "Project: <name>"
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        let title = format!("{}: {}", app.t(K::ProjectScene), app.project.name);
        ui.label(egui::RichText::new(title).size(13.0).strong().color(app.theme.text));
    });
    ui.add_space(4.0);
    // tabs
    ui.horizontal(|ui| {
        ui.add_space(4.0);
        let tabs = [(0, K::Scene), (1, K::MediaTab), (2, K::Folders), (3, K::Markers)];
        for (idx, k) in tabs {
            if tab_btn(app, ui, &app.t(k), app.pool_tab == idx).clicked() {
                app.pool_tab = idx;
            }
        }
    });
    hline(app, ui);

    // search row
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        let sw = ui.available_width() - 70.0;
        let search = app.t(K::SearchPh);
        let resp = ui.add_sized(
            [sw, 20.0],
            egui::TextEdit::singleline(&mut app.search)
                .hint_text(egui::RichText::new(search).size(12.0).color(app.theme.faint))
                .font(FontId::proportional(12.0)),
        );
        let _ = resp;
        icon_btn(app, ui, 22.0, "Search", ico::search);
        ui.add_space(6.0);
    });
    ui.add_space(4.0);

    match app.pool_tab {
        0 | 1 => grid(app, ui),
        2 => folders_tab(app, ui),
        _ => markers_tab(app, ui),
    }

    // bottom import bar
    ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            let btn = egui::Button::new(egui::RichText::new(format!("＋ {}", app.t(K::ImportMedia))).size(12.0))
                .fill(app.theme.panel3)
                .stroke(egui::Stroke::new(1.0, app.theme.border2))
                .rounding(4.0);
            if ui.add(btn).clicked() {
                app.dialog = Some(Dialog::Fs(FsState {
                    dir: app.project_dir.clone(),
                    mode: FsMode::OpenMedia,
                    name: String::new(),
                }));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);
                let proxies_on = app.proxy_enabled;
                if icon_toggle(app, ui, 20.0, proxies_on, &app.t(K::UseProxies), ico::film).clicked() {
                    app.proxy_enabled = !app.proxy_enabled;
                    app.invalidate_preview();
                }
                if icon_btn(app, ui, 20.0, &app.t(K::ProxiesTitle), ico::folder).clicked() {
                    app.dialog = Some(Dialog::Proxies);
                }
                for k in [ico::note, ico::image_icon, ico::film].map(|f: fn(&egui::Painter, egui::Rect, Color32)| f) {
                    icon_btn(app, ui, 20.0, "Media", k);
                }
            });
        });
        ui.add_space(4.0);
    });
}

fn grid(app: &mut App, ui: &mut egui::Ui) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        let cell_w = 122.0;
        let cols = ((ui.available_width() - 16.0) / (cell_w + 8.0)).max(1.0) as usize;
        let q: Vec<_> = app.assets.iter()
            .filter(|a| app.search.is_empty() || a.label().to_lowercase().contains(&app.search.to_lowercase()))
            .map(|a| (a.id, a.label(), a.duration, a.kind, a.proxy.clone()))
            .collect();
        if q.is_empty() {
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new(app.t(K::NoMedia)).size(12.0).color(app.theme.faint));
                ui.label(egui::RichText::new(app.t(K::DropHint)).size(11.0).color(app.theme.faint));
            });
            return;
        }
        let mut add_req: Option<u64> = None;
        let mut proxy_req: Option<u64> = None;
        let mut remove_req: Option<u64> = None;
        egui::Grid::new("pool_grid").min_col_width(cell_w).max_col_width(cell_w).spacing([8.0, 8.0]).show(ui, |ui| {
            for (i, (id, name, dur, kind, proxy)) in q.iter().enumerate() {
                if i % cols == 0 && i > 0 { ui.end_row(); }
                ui.vertical(|ui| {
                    let thumb_h = cell_w * 9.0 / 16.0;
                    let (r, resp) = ui.allocate_exact_size(Vec2::new(cell_w, thumb_h), Sense::click_and_drag());
                    // background
                    ui.painter().rect_filled(r, 4.0, app.theme.panel3);
                    // thumbnail
                    let path = app.assets.iter().find(|a| a.id == *id).map(|a| a.path.clone());
                    let tex = path.clone().and_then(|p| app.thumbs.get(&p).cloned().flatten());
                    if let Some(tex) = tex {
                        let aspect = tex.size()[0] as f32 / tex.size()[1].max(1) as f32;
                        let (dw, dh) = if aspect > cell_w / thumb_h {
                            (cell_w, cell_w / aspect)
                        } else {
                            (thumb_h * aspect, thumb_h)
                        };
                        let dr = Rect::from_center_size(r.center(), Vec2::new(dw, dh));
                        ui.painter().image(tex.id(), dr, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                    } else {
                        let kind_icon = match kind { AssetKind::Video => ico::film, AssetKind::Audio => ico::note, AssetKind::Image => ico::image_icon };
                        kind_icon(ui.painter(), r.shrink(cell_w * 0.3), app.theme.dim);
                        // request thumbnail once
                        if let Some(p) = path.clone() { app.ensure_thumb(p); }
                    }
                    if resp.hovered() {
                        ui.painter().rect_stroke(r, 4.0, egui::Stroke::new(1.0, app.theme.accent), egui::StrokeKind::Inside);
                    }
                    // duration chip
                    ui.painter().rect_filled(
                        Rect::from_min_max(Pos2::new(r.right() - 38.0, r.bottom() - 15.0), Pos2::new(r.right() - 2.0, r.bottom() - 2.0)),
                        2.0, Color32::from_black_alpha(170));
                    ui.painter().text(Pos2::new(r.right() - 20.0, r.bottom() - 8.5), Align2::CENTER_CENTER,
                        crate::util::short_dur(*dur), FontId::monospace(9.0), app.theme.text);
                    // proxy badge
                    if proxy.is_some() {
                        ui.painter().rect_filled(
                            Rect::from_min_max(Pos2::new(r.left() + 2.0, r.top() + 2.0), Pos2::new(r.left() + 40.0, r.top() + 14.0)),
                            2.0, app.theme.ok.gamma_multiply(0.85));
                        ui.painter().text(Pos2::new(r.left() + 21.0, r.top() + 8.0), Align2::CENTER_CENTER,
                            "PROXY", FontId::proportional(8.0), Color32::BLACK);
                    }
                    if resp.double_clicked() { add_req = Some(*id); }
                    if resp.clicked() { app.sel = None; }
                    resp.context_menu(|ui| {
                        if ui.button(app.t(K::ClsTimeline)).clicked() { add_req = Some(*id); ui.close_menu(); }
                        if ui.button(app.t(K::CreateProxy)).clicked() { proxy_req = Some(*id); ui.close_menu(); }
                        if ui.button(app.t(K::Properties)).clicked() {
                            if let Some(a) = app.assets.iter().find(|a| a.id == *id) {
                                let msg = format!("{}×{} @ {:.2}fps · {} · {}", a.width, a.height, a.fps, a.codec, crate::util::fmt_bytes(a.size));
                                app.toast(msg, 0);
                            }
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button(app.t(K::RemoveAsset)).clicked() { remove_req = Some(*id); ui.close_menu(); }
                    });
                    // name row
                    ui.horizontal(|ui| {
                        ui.add_space(2.0);
                        ui.label(egui::RichText::new(truncate(name, 16)).size(10.5).color(app.theme.text));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add_space(2.0);
                            ui.label(egui::RichText::new(crate::util::short_dur(*dur)).size(10.0).color(app.theme.faint));
                        });
                    });
                });
            }
        });
        if let Some(id) = add_req { app.add_asset_to_timeline(id); }
        if let Some(id) = proxy_req { app.create_proxy_for(id); }
        if let Some(id) = remove_req {
            app.assets.retain(|a| a.id != id);
        }
    });
}

fn folders_tab(app: &App, ui: &mut egui::Ui) {
    ui.add_space(16.0);
    for d in ["demo", "exports", "proxies"] {
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            ico::folder(ui.painter(), Rect::from_min_size(ui.cursor().left_top(), Vec2::splat(16.0)), app.theme.dim);
            ui.label(egui::RichText::new(d).size(12.0).color(app.theme.text));
        });
        ui.add_space(6.0);
    }
}

fn markers_tab(app: &App, ui: &mut egui::Ui) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        if app.project.markers.is_empty() {
            ui.add_space(16.0);
            ui.label(egui::RichText::new("—").size(12.0).color(app.theme.faint));
        }
        for (t, name) in &app.project.markers {
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.label(egui::RichText::new(crate::util::timecode(*t, app.project.fps)).size(12.0).color(app.theme.accent_text).monospace());
                ui.label(egui::RichText::new(if name.is_empty() { "M" } else { name }).size(12.0).color(app.theme.text));
            });
            ui.add_space(4.0);
        }
        // in/out marks
        if let Some(i) = app.project.in_mark {
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.label(egui::RichText::new(format!("IN  {}", crate::util::timecode(i, app.project.fps))).size(12.0).color(app.theme.warn).monospace());
            });
        }
        if let Some(o) = app.project.out_mark {
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.label(egui::RichText::new(format!("OUT {}", crate::util::timecode(o, app.project.fps))).size(12.0).color(app.theme.warn).monospace());
            });
        }
    });
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.to_string() } else {
        format!("{}…", s.chars().take(n - 1).collect::<String>())
    }
}

pub fn section(app: &App, ui: &mut egui::Ui, title: &str) {
    section_header(app, ui, title, None);
}

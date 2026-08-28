//! Root layout: custom titlebar, menus, workspace tabs, panel arrangement,
//! toasts, frame pacing — mirrors the reference window structure.

use crate::app::{App, Dialog, Drag, FsMode, FsState, Workspace};
use crate::i18n::K;
use crate::ui_common::{draw_toast, icon_btn, tab_btn};
use crate::ui_icons as ico;
use eframe::App as _;
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, ViewportCommand};
use std::time::Instant;

impl eframe::App for App {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        let c = self.theme.bg;
        [c.r() as f32 / 255.0, c.g() as f32 / 255.0, c.b() as f32 / 255.0, 1.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let t0 = Instant::now();
        self.poll_events(ctx);
        self.handle_shortcuts(ctx);

        let dt = ctx.input(|i| i.stable_dt) as f64;
        let dur = self.project.duration();
        if self.player.tick(dt, dur, self.project.in_mark, self.project.out_mark) {
            self.toggle_play();
        }
        self.update_player(ctx);
        if self.demo_build_pending {
            self.try_build_demo_timeline();
        }
        if self.selftest.is_some() {
            let mut st = self.selftest.take();
            if let Some(test) = st.as_mut() {
                test.step(self, ctx);
                if test.finished { self.exit_requested = true; }
            }
            self.selftest = st;
        }

        // dropped files → import
        let dropped: Vec<std::path::PathBuf> = ctx.input(|i| {
            i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect()
        });
        if !dropped.is_empty() { self.import_files(dropped); }

        self.draw_titlebar(ctx);
        self.draw_tabs(ctx);
        self.draw_timeline_panel(ctx);
        self.draw_left_panel(ctx);
        self.draw_right_panel(ctx);
        self.draw_center(ctx);
        self.draw_overlays(ctx);

        if self.exit_requested {
            // Bypass eframe/Mesa GL teardown (segfaults under llvmpipe);
            // state is already persisted by the caller when needed.
            std::process::exit(0);
        }

        // fps counter
        let el = t0.elapsed().as_secs_f64();
        self.frame_times.push_back(el + dt);
        while self.frame_times.len() > 60 { self.frame_times.pop_front(); }
        if self.frame_times.len() >= 2 {
            let span = self.frame_times.back().unwrap() - self.frame_times.front().unwrap();
            if span > 0.0 { self.ui_fps = (self.frame_times.len() - 1) as f64 / span; }
        }

        // repaint policy: smooth playback, calm idle
        if self.player.playing || self.export_state.running.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(8));
        } else if !self.player.slots.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(160));
        }
    }
}

impl App {
    // ------------------------------------------------------------ titlebar
    fn draw_titlebar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("titlebar").exact_height(34.0)
            .frame(egui::Frame::none().fill(self.theme.panel).stroke(egui::Stroke::new(1.0, self.theme.border)))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    // app identity + menus (left)
                    ui.add_space(10.0);
                    let icon_r = Rect::from_center_size(Pos2::new(ui.cursor().left() + 9.0, ui.cursor().center().y), egui::vec2(18.0, 18.0));
                    ui.allocate_rect(icon_r, Sense::hover());
                    let kcol = self.theme.accent;
                    let p = ui.painter();
                    p.rect_filled(icon_r, 4.0, kcol.gamma_multiply(0.9));
                    ico::play(&p, icon_r.shrink(4.0), Color32::WHITE);
                    ui.add_space(24.0);

                    // menus — every item performs a real action
                    egui::menu::bar(ui, |ui| {
                        let menus = [
                            (K::File, 0u8), (K::EditMenu, 1), (K::ClipMenu, 2),
                            (K::TimelineMenu, 3), (K::ViewMenu, 4), (K::PlaybackMenu, 5), (K::HelpMenu, 6),
                        ];
                        for (k, idx) in menus {
                            ui.menu_button(self.t(k), |ui| {
                                match idx {
                                    0 => { // File
                                        if ui.button(self.t(K::NewProject)).clicked() { self.new_project(); ui.close_menu(); }
                                        if ui.button(self.t(K::OpenProject)).clicked() { self.dialog = Some(Dialog::Fs(FsState { dir: self.project_dir.clone(), mode: FsMode::OpenProject, name: String::new() })); ui.close_menu(); }
                                        if ui.button(self.t(K::Save)).clicked() { self.save_project(self.project_dir.join(format!("{}.kcproj", self.project.name))); ui.close_menu(); }
                                        if ui.button(self.t(K::SaveAs)).clicked() { self.dialog = Some(Dialog::Fs(FsState { dir: self.project_dir.clone(), mode: FsMode::SaveProject, name: format!("{}.kcproj", self.project.name) })); ui.close_menu(); }
                                        ui.separator();
                                        if ui.button(self.t(K::ImportMedia)).clicked() { self.dialog = Some(Dialog::Fs(FsState { dir: self.project_dir.clone(), mode: FsMode::OpenMedia, name: String::new() })); ui.close_menu(); }
                                        if ui.button(self.t(K::ExportMenu)).clicked() { self.export_state.open = true; self.workspace = Workspace::Export; ui.close_menu(); }
                                        ui.separator();
                                        if ui.button(self.t(K::Exit)).clicked() { self.exit_requested = true; ui.close_menu(); }
                                    }
                                    1 => { // Edit
                                        if ui.add_enabled(self.hist.can_undo(), egui::Button::new(self.t(K::Undo))).clicked() { self.do_undo(); ui.close_menu(); }
                                        if ui.add_enabled(self.hist.can_redo(), egui::Button::new(self.t(K::Redo))).clicked() { self.do_redo(); ui.close_menu(); }
                                        ui.separator();
                                        if ui.button(self.t(K::SplitPlayhead)).clicked() { self.split_at_playhead(); ui.close_menu(); }
                                        if ui.button(self.t(K::RippleDelete)).clicked() { self.delete_selection(true); ui.close_menu(); }
                                        if ui.button(self.t(K::Delete)).clicked() { self.delete_selection(false); ui.close_menu(); }
                                    }
                                    2 => { // Clip
                                        if ui.button(self.t(K::AddTitleClip)).clicked() { self.add_title_at_playhead(); ui.close_menu(); }
                                        if ui.button(self.t(K::CreateProxy)).clicked() {
                                            if let Some(c) = self.selected_clip().and_then(|c| c.source.clone()) {
                                                if let Some(a) = self.assets.iter().find(|a| a.path == c) { self.create_proxy_for(a.id); }
                                            }
                                            ui.close_menu();
                                        }
                                        if ui.button(self.t(K::Properties)).clicked() {
                                            if let Some(c) = self.selected_clip() {
                                                self.toast(format!("{} · {:.2}s · in {:.2}s", c.name, c.src_dur, c.src_in), 0);
                                            }
                                            ui.close_menu();
                                        }
                                    }
                                    3 => { // Timeline
                                        if ui.button(self.t(K::AddVideoTrack)).clicked() { self.add_video_track(); ui.close_menu(); }
                                        if ui.button(self.t(K::AddAudioTrack)).clicked() { self.add_audio_track(); ui.close_menu(); }
                                        ui.separator();
                                        let snap_label = format!("{} ✓", self.t(K::Snap));
                                        if ui.button(if self.snap { snap_label } else { self.t(K::Snap) }).clicked() { self.snap = !self.snap; ui.close_menu(); }
                                        ui.separator();
                                        if ui.button(self.t(K::ZoomIn)).clicked() { self.zoom = (self.zoom * 1.25).min(4000.0); ui.close_menu(); }
                                        if ui.button(self.t(K::ZoomOut)).clicked() { self.zoom = (self.zoom / 1.25).max(4.0); ui.close_menu(); }
                                    }
                                    4 => { // View
                                        for (ws, k) in [(Workspace::Edit, K::WsEdit), (Workspace::Color, K::WsColor), (Workspace::Audio, K::WsAudio), (Workspace::Fx, K::WsFx), (Workspace::Export, K::WsExport)] {
                                            if ui.button(self.t(k)).clicked() { self.workspace = ws; ui.close_menu(); }
                                        }
                                        ui.separator();
                                        let sc_label = if self.scopes_visible { format!("{} ✓", self.t(K::Scopes)) } else { self.t(K::Scopes) };
                                        if ui.button(sc_label).clicked() { self.scopes_visible = !self.scopes_visible; ui.close_menu(); }
                                    }
                                    5 => { // Playback
                                        if ui.button(if self.player.playing { self.t(K::Pause) } else { self.t(K::Play) }).clicked() { self.toggle_play(); ui.close_menu(); }
                                        let loop_label = if self.player.loop_play { format!("{} ✓", self.t(K::Loop)) } else { self.t(K::Loop) };
                                        if ui.button(loop_label).clicked() { self.player.loop_play = !self.player.loop_play; ui.close_menu(); }
                                        ui.separator();
                                        if ui.button(self.t(K::GoStart)).clicked() { self.player.seek(self.project.in_mark.unwrap_or(0.0)); self.player.slots.clear(); ui.close_menu(); }
                                        if ui.button(self.t(K::GoEnd)).clicked() { self.player.seek(self.project.out_mark.unwrap_or(self.project.duration())); self.player.slots.clear(); ui.close_menu(); }
                                        ui.separator();
                                        if ui.button(self.t(K::MarkIn)).clicked() { self.project.in_mark = Some(self.player.clock); ui.close_menu(); }
                                        if ui.button(self.t(K::MarkOut)).clicked() { self.project.out_mark = Some(self.player.clock); ui.close_menu(); }
                                    }
                                    _ => { // Help
                                        if ui.button(self.t(K::About2)).clicked() { self.dialog = Some(Dialog::Learn); ui.close_menu(); }
                                        if ui.button(self.t(K::Shortcuts)).clicked() { self.dialog = Some(Dialog::Learn); ui.close_menu(); }
                                    }
                                }
                            });
                        }
                    });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(8.0);
                        // window buttons
                        if icon_btn(self, ui, 24.0, "Close", ico::x).clicked() { self.exit_requested = true; }
                        if icon_btn(self, ui, 24.0, "Maximize", if self.is_maximized(ctx) { ico::restore } else { ico::maximize }).clicked() {
                            ctx.send_viewport_cmd(ViewportCommand::Maximized(!ctx.input(|i| i.viewport().maximized.unwrap_or(false))));
                        }
                        if icon_btn(self, ui, 24.0, "Minimize", ico::minimize).clicked() {
                            ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
                        }
                        ui.add_space(6.0);
                        // project title (center-ish)
                        ui.label(egui::RichText::new(&self.project.name).size(12.5).strong().color(self.theme.text));
                    });
                });

                // drag region (whole bar)
                let bar = ui.max_rect();
                let drag_id = egui::Id::new("titlebar_drag");
                let dr = ui.interact(bar, drag_id, Sense::click_and_drag());
                if dr.dragged_by(egui::PointerButton::Primary)
                    && ui.input(|i| i.pointer.button_down(egui::PointerButton::Primary))
                    && ctx.input(|i| !i.pointer.button_down(egui::PointerButton::Secondary))
                {
                    let over_widget = ui.input(|i| i.pointer.press_origin().map(|o| bar.contains(o)).unwrap_or(false));
                    if over_widget {
                        ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                    }
                }
                if dr.double_clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Maximized(!ctx.input(|i| i.viewport().maximized.unwrap_or(false))));
                }
            });
    }

    fn is_maximized(&self, ctx: &egui::Context) -> bool {
        ctx.input(|i| i.viewport().maximized.unwrap_or(false))
    }

    // ------------------------------------------------------------ tabs
    fn draw_tabs(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("ws_tabs").exact_height(40.0)
            .frame(egui::Frame::none().fill(self.theme.bg))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.add_space(8.0);
                    // every tab switches the real panel arrangement
                    let tabs = [
                        (Workspace::Edit, K::WsEdit),
                        (Workspace::Color, K::WsColor),
                        (Workspace::Audio, K::WsAudio),
                        (Workspace::Fx, K::WsFx),
                        (Workspace::Export, K::WsExport),
                    ];
                    for (ws, k) in tabs {
                        if tab_btn(self, ui, &self.t(k), self.workspace == ws).clicked() {
                            self.workspace = ws;
                            if ws == Workspace::Export { self.export_state.open = false; } // page, not dialog
                        }
                    }
                });
            });
    }

    // ------------------------------------------------------------ timeline
    fn draw_timeline_panel(&mut self, ctx: &egui::Context) {
        let h = (ctx.screen_rect().height() * 0.42).clamp(280.0, 470.0);
        egui::TopBottomPanel::bottom("timeline").exact_height(h)
            .frame(egui::Frame::none().fill(self.theme.panel).stroke(egui::Stroke::new(1.0, self.theme.border)))
            .show(ctx, |ui| {
                crate::ui_timeline::show(self, ui);
            });
        let _ = &mut self.drag;
    }

    // ------------------------------------------------------------ left pool
    fn draw_left_panel(&mut self, ctx: &egui::Context) {
        let w = (ctx.screen_rect().width() * 0.27).clamp(300.0, 460.0);
        egui::SidePanel::left("pool").exact_width(w)
            .frame(egui::Frame::none().fill(self.theme.panel).stroke(egui::Stroke::new(1.0, self.theme.border)))
            .resizable(false)
            .show(ctx, |ui| {
                crate::ui_pool::show(self, ui);
            });
    }

    // ------------------------------------------------------------ right column
    fn draw_right_panel(&mut self, ctx: &egui::Context) {
        let w = (ctx.screen_rect().width() * 0.25).clamp(280.0, 430.0);
        egui::SidePanel::right("inspector").exact_width(w)
            .frame(egui::Frame::none().fill(self.theme.panel).stroke(egui::Stroke::new(1.0, self.theme.border)))
            .resizable(false)
            .show(ctx, |ui| {
                // scopes occupy the bottom; proportion depends on workspace
                let scopes_frac = match self.workspace {
                    Workspace::Color => 0.42,
                    Workspace::Fx => 0.30,
                    _ => 0.44,
                };
                if self.scopes_visible {
                    egui::TopBottomPanel::bottom("scopes_area").exact_height(ui.available_height() * scopes_frac)
                        .frame(egui::Frame::none().fill(self.theme.panel))
                        .show_inside(ui, |ui| {
                            crate::ui_right::show_scopes(self, ui);
                        });
                }
                egui::CentralPanel::default()
                    .frame(egui::Frame::none().fill(self.theme.panel))
                    .show_inside(ui, |ui| {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            match self.workspace {
                                Workspace::Color => crate::ui_right::show_color(self, ui),
                                Workspace::Audio => crate::ui_right::show_audio(self, ui),
                                Workspace::Fx => crate::ui_right::show_fx(self, ui),
                                _ => crate::ui_right::show_inspector(self, ui),
                            }
                        });
                    });
            });
    }

    // ------------------------------------------------------------ center
    fn draw_center(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(self.theme.bg))
            .show(ctx, |ui| {
                if self.workspace == Workspace::Export {
                    ui.add_space(8.0);
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.add_space(8.0);
                        crate::ui_dialogs::export_fields(self, ui);
                        ui.add_space(20.0);
                    });
                } else {
                    crate::ui_preview::show(self, ui);
                }
            });
    }

    // ------------------------------------------------------------ overlays
    fn draw_overlays(&mut self, ctx: &egui::Context) {
        // toasts
        let screen = ctx.screen_rect();
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("toasts")));
        let now = Instant::now();
        self.toasts.retain(|t| now.duration_since(t.at).as_secs_f32() < 3.5);
        for (i, t) in self.toasts.iter().enumerate().rev() {
            let age = now.duration_since(t.at).as_secs_f32();
            let alpha = (1.0 - age / 3.5).clamp(0.0, 1.0);
            let mut yoff = (self.toasts.len() - 1 - i) as f32 * 38.0;
            let _ = &mut yoff;
            let mut msg_rect = screen;
            msg_rect.min.y = screen.bottom() - 30.0 - yoff;
            draw_toast(self, &painter, msg_rect, &t.msg, t.kind, alpha);
            yoff += 38.0;
        }
        // dialogs (skip export dialog while the Deliver page is open)
        if self.workspace != Workspace::Export {
            crate::ui_dialogs::show(self, ctx);
        } else {
            match &self.dialog {
                None => {}
                Some(Dialog::Proxies) | Some(Dialog::Learn) => crate::ui_dialogs::show(self, ctx),
                Some(Dialog::Fs(_)) => crate::ui_dialogs::show(self, ctx),
            }
        }
        // ffmpeg missing banner
        if !crate::media::ffmpeg_ok() {
            let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("ffmpegwarn")));
            let r = Rect::from_min_size(Pos2::new(screen.center().x - 160.0, screen.top() + 90.0), egui::vec2(320.0, 40.0));
            p.rect_filled(r, 6.0, self.theme.err.gamma_multiply(0.25));
            p.rect_stroke(r, 6.0, egui::Stroke::new(1.0, self.theme.err), egui::StrokeKind::Inside);
            p.text(r.center(), Align2::CENTER_CENTER, &self.t(K::MsgNoFfmpeg), FontId::proportional(13.0), self.theme.text);
        }
    }
}

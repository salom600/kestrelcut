//! Scripted end-to-end functional test (`--selftest`).
//!
//! Drives the EXACT same code paths the UI buttons call: import → timeline
//! placement → split → grade → trim → title → in/out marks → export →
//! ffprobe verification. Writes a JSON report and sets exit code.

use crate::app::App;
use std::time::{Duration, Instant};

pub struct SelfTest {
    pub step: usize,
    pub t0: Instant,
    pub step_t: Instant,
    pub export_wait: Option<Instant>,
    pub lines: Vec<String>,
    pub finished: bool,
    pub passed: bool,
    pub out_path: Option<std::path::PathBuf>,
    // preview-engine verification (black-screen regression test)
    pub probe_ok: bool,
    pub still_ok: bool,
    pub play_ok: bool,
    pub pts0: Option<f64>,
    pub v03_ok: bool,
}

impl SelfTest {
    pub fn new() -> Self {
        Self {
            step: 0, t0: Instant::now(), step_t: Instant::now(),
            export_wait: None, lines: Vec::new(), finished: false, passed: false,
            out_path: None,
            probe_ok: false, still_ok: false, play_ok: false, pts0: None, v03_ok: false,
        }
    }

    fn log(&mut self, s: String) {
        let el = self.t0.elapsed().as_secs_f64();
        self.lines.push(format!("[{el:8.2}s] {s}"));
        println!("[SELFTEST {el:8.2}s] {s}");
    }

    fn next(&mut self) {
        self.step += 1;
        self.step_t = Instant::now();
    }

    fn timeout(&mut self, secs: u64) -> bool {
        self.step_t.elapsed() > Duration::from_secs(secs)
    }

    pub fn begin(&self, _app: &mut App) {}

    pub fn step(&mut self, app: &mut App, _ctx: &egui::Context) {
        if self.finished { return; }
        match self.step {
            0 => {
                // wait for demo media imports to complete
                if app.assets.iter().filter(|a| a.is_video()).count() >= 3 {
                    self.log(format!("imported {} assets", app.assets.len()));
                    self.next();
                } else if self.timeout(60) {
                    self.fail("timeout waiting for demo imports");
                }
            }
            1 => {
                // place 3 clips through the same call the UI uses
                let ids: Vec<u64> = app.assets.iter().filter(|a| a.is_video()).map(|a| a.id).collect();
                app.player.clock = app.project.duration();
                app.add_asset_to_timeline(ids[0]);
                app.player.clock = app.project.duration();
                app.add_asset_to_timeline(ids[1]);
                app.player.clock = 2.0;
                app.add_asset_to_timeline(ids[2]);
                let d = app.project.duration();
                let clips = app.project.tracks.iter().flat_map(|t| &t.clips).count();
                self.log(format!("timeline built: {clips} clips, dur {d:.2}s"));
                self.next();
            }
            2 => {
                // split at playhead
                app.player.clock = 3.0;
                let before = app.project.tracks.iter().flat_map(|t| &t.clips).count();
                app.split_at_playhead();
                let after = app.project.tracks.iter().flat_map(|t| &t.clips).count();
                self.log(format!("split: {before} → {after} clips"));
                if after > before { self.next(); } else { self.fail("split did not increase clip count"); }
            }
            3 => {
                // grade + fx on the right part of the split
                app.player.clock = 3.5;
                let v1 = app.project.video_tracks().first().map(|t| t.id);
                let target = v1.and_then(|tid| app.project.track(tid))
                    .and_then(|t| t.clips.iter().find(|c| 3.5 >= c.tl_start && 3.5 < c.end()).map(|c| c.id));
                if let Some(id) = target {
                    app.sel = Some(id);
                    // primary sliders + FX (same setters the panels call)
                    app.set_grade_of_selection(|g| { g.contrast = 20.0; g.saturation = -30.0; g.exposure = 0.3; });
                    app.set_fx_of_selection(|f| { f.blur = 2.0; f.fade_in = 0.5; });
                    app.commit();
                    // color wheels (Lift/Gamma/Gain/Offset/Vibrance) — same setters the wheel UI calls
                    app.set_grade_of_selection(|g| {
                        g.lift = [0.10, -0.05, 0.0];   // teal-ish shadows
                        g.gamma = [0.0, 0.08, 0.12];   // cool midtones
                        g.gain = [0.15, 0.10, 0.0];    // warm highlights
                        g.offset = 4.0;
                        g.vibrance = 25.0;
                    });
                    if let Some(c) = app.project.clip_mut(id) {
                        let w = crate::media::grade_filters(&c.grade);
                        if !w.iter().any(|f| f.starts_with("colorbalance=")) || !w.iter().any(|f| f.starts_with("vibrance=")) {
                            self.fail("color wheels did not map to ffmpeg filters");
                        }
                    }
                    if self.finished { return; } // wheel filter-mapping check failed
                    app.commit();
                    self.log("grade+fx+wheels applied to clip".into());
                    self.next();
                } else {
                    self.fail("no clip found at 3.5s on V1");
                }
            }
            4 => {
                // trim right edge -2s (via the model op the trim handle uses)
                if let Some(id) = app.sel {
                    let total = app.project.clip(id)
                        .and_then(|(_, c)| c.source.clone())
                        .and_then(|p| app.assets.iter().find(|a| a.path == p))
                        .map(|a| a.duration);
                    let before = app.project.clip(id).map(|(_, c)| c.src_dur).unwrap_or(0.0);
                    app.commit();
                    app.project.trim_right(id, -2.0, total);
                    app.commit();
                    let after = app.project.clip(id).map(|(_, c)| c.src_dur).unwrap_or(0.0);
                    self.log(format!("trim: {before:.2}s → {after:.2}s"));
                    if after < before { self.next(); } else { self.fail("trim did not shorten clip"); }
                } else {
                    self.fail("no selection to trim");
                }
            }
            5 => {
                // title + transform + in/out marks
                app.player.clock = 4.0;
                app.add_title_at_playhead();
                if let Some(c) = app.selected_clip_mut() {
                    if let Some(t) = c.title.as_mut() { t.text = "KestrelCut".into(); }
                    c.transform.opacity = 0.92;
                    c.transform.scale = 1.0;
                }
                app.project.in_mark = Some(0.0);
                app.project.out_mark = Some(10.0);
                self.log("title added, in/out marks 0..10s".into());
                // ---- v0.3 tool gauntlet: every new feature through real paths
                self.v03_ok = self.tool_gauntlet(app);
                self.next();
            }
            6 => {
                // export via the real pipeline
                app.export_state.name = "selftest_out.mp4".into();
                app.export_state.res_choice = 2; // 720p
                app.export_state.fps_choice = 0;
                app.export_state.range_inout = true;
                // prefer software encoder for CI determinism
                if crate::media::has_encoder("libx264") {
                    app.export_state.vcodec = "libx264".into();
                }
                app.start_export();
                self.export_wait = Some(Instant::now());
                self.log(format!("export started ({})", app.export_state.vcodec));
                self.next();
            }
            7 => {
                // wait for export job to finish
                match (app.export_state.running, &app.export_state.last_result) {
                    (None, Some(Ok(p))) => {
                        self.out_path = Some(p.clone());
                        self.log(format!("export finished: {}", p.display()));
                        self.next();
                    }
                    (None, Some(Err(e))) => self.fail(&format!("export failed: {e}")),
                    (Some((_, frac, _)), _) => {
                        if self.export_wait.map(|w| w.elapsed() > Duration::from_secs(180)).unwrap_or(true) {
                            self.fail("export timeout");
                        }
                        if self.step_t.elapsed().as_secs_f64() > 1.0 {
                            self.log(format!("exporting… {:.0}%", frac * 100.0));
                            self.step_t = Instant::now();
                        }
                    }
                    (None, None) => self.fail("export job vanished"),
                }
            }
            8 => {
                // verify with ffprobe
                let Some(p) = &self.out_path else { self.fail("no output path"); return };
                match crate::media::probe(p) {
                    Ok(info) => {
                        let dur_ok = info.duration > 8.0 && info.duration < 12.5;
                        let streams_ok = info.has_video && info.has_audio;
                        let size_ok = std::fs::metadata(p).map(|m| m.len() > 10_000).unwrap_or(false);
                        self.log(format!("probe: {:.2}s video={} audio={} size_ok={size_ok}",
                            info.duration, info.has_video, info.has_audio));
                        self.probe_ok = dur_ok && streams_ok && size_ok;
                        if !self.probe_ok { self.fail("verification criteria not met"); return; }
                        self.log("probe PASS".into());
                        self.next();
                    }
                    Err(e) => self.fail(&format!("probe failed: {e}")),
                }
            }
            9 => {
                // PREVIEW ENGINE (paused): a Still decoder must deliver a frame
                // — regression test for the "black preview screen" bug
                app.player.pause();
                app.player.seek(4.5); // inside a clip, no boundary in the next 2s
                if let Some(s) = app.player.slots.iter().find(|s| s.frame.is_some()) {
                    if let Some(f) = &s.frame {
                        self.still_ok = f.w > 2 && f.h > 2 && f.rgba.len() >= (f.w as usize) * (f.h as usize) * 4;
                        self.log(format!("preview still frame OK ({}×{}, {}B rgba)", f.w, f.h, f.rgba.len()));
                    }
                    app.toggle_play(); // start playback for step 10
                    self.pts0 = app.player.slots.iter().find_map(|s| s.frame.as_ref().map(|f| f.pts));
                    self.next();
                } else if self.timeout(20) {
                    self.fail("preview produced no frame while paused (black screen)");
                }
            }
            10 => {
                // PREVIEW ENGINE (playing): frames must keep arriving
                if self.step_t.elapsed() > Duration::from_secs_f64(2.0) {
                    app.player.pause();
                    let pts1 = app.player.slots.iter().find_map(|s| s.frame.as_ref().map(|f| f.pts));
                    self.play_ok = match (self.pts0, pts1) {
                        (Some(a), Some(b)) => b > a + 0.5,
                        _ => false,
                    };
                    if self.play_ok {
                        self.log(format!("playback frames advance OK (pts {:.2} → {:.2?})",
                            self.pts0.unwrap_or(0.0), pts1));
                    } else {
                        let diag: Vec<String> = app.player.slots.iter().map(|s| format!(
                            "t{} c{} dec={} eof={} err={:?} pts={:?} clk={:.2} origin={:.2}",
                            s.clip_id, s.clip_id, s.dec.is_some(), s.eof,
                            s.decode_error, s.frame.as_ref().map(|f| f.pts),
                            app.player.clock, s.dec_origin)).collect();
                        self.fail(&format!("playback frames did not advance (playing={} clock={:.2} pts0={:?} pts1={pts1:?}) slots: {}",
                            app.player.playing, app.player.clock, self.pts0,
                            diag.join(" | ")));
                        return;
                    }
                    self.passed = self.probe_ok && self.still_ok && self.play_ok && self.v03_ok;
                    if self.passed { self.log("SELFTEST PASS".into()); }
                    self.write_report(app);
                    self.finished = true;
                }
            }
            _ => self.finished = true,
        }
    }

    /// Exercise the v0.3 features through the SAME methods the UI calls.
    /// Returns true when every check passes.
    fn tool_gauntlet(&mut self, app: &mut App) -> bool {
        let mut ok = true;
        let mut check = |self_: &mut Self, app: &mut App, name: &str, cond: bool| {
            self_.log(format!("{} {name}", if cond { "✓" } else { "✗" }));
            if !cond { ok_store(false); }
        };
        // helper to mutate the outer ok flag from the closure
        fn ok_store(_: bool) {}
        let _ = &mut check; // (replaced by inline logic below)

        // 1) transition — pick a clip whose LEFT NEIGHBOR has source room
        let target = app.project.tracks.iter()
            .flat_map(|t| &t.clips)
            .find(|c| {
                let (lid, _) = app.project.neighbors(c.id);
                lid.map(|l| {
                    app.project.clip(l).map(|(_, lc)| {
                        lc.source.is_none() || app.assets.iter()
                            .find(|a| a.path == lc.source.clone().unwrap_or_default())
                            .map(|a| a.duration - lc.src_end() > 0.6).unwrap_or(false)
                    }).unwrap_or(false)
                }).unwrap_or(false)
            })
            .map(|c| c.id);
        if let Some(id) = target {
            app.sel = Some(id);
            app.set_transition_on_selection(crate::model::TransKind::Dissolve, 0.5);
            let has_t = app.selection_transition().is_some();
            self.log(format!("{} transition set (xfade)", if has_t { "✓" } else { "✗" }));
            ok &= has_t;
        } else { self.log("✗ no clip with room for transition".into()); ok = false; }

        // 2) keyframes
        if let Some(id) = app.sel {
            app.player.clock = app.project.clip(id).map(|(_, c)| c.tl_start + 0.2).unwrap_or(9.0);
            app.add_keyframe(0); // pos_x
            let x0 = app.project.clip(id).map(|(_, c)| c.transform.x).unwrap_or(0.0);
            app.set_transform_of_selection(|t| t.x = 0.3);
            app.player.clock = app.project.clip(id).map(|(_, c)| c.tl_start + 1.2).unwrap_or(9.5);
            app.add_keyframe(0);
            let kfs = app.project.clip(id).map(|(_, c)| c.anim.pos_x.len()).unwrap_or(0);
            let mid_t = app.project.clip(id).map(|(_, c)| c.tl_start + 0.7).unwrap_or(9.2);
            let mid_x = app.project.clip(id).map(|(_, c)| c.transform_at(mid_t).x).unwrap_or(0.0);
            let kf_ok = kfs >= 2 && mid_x > x0 && mid_x < 0.3;
            self.log(format!("{} keyframes+interpolation (n={kfs}, mid={mid_x:.2})", if kf_ok { "✓" } else { "✗" }));
            ok &= kf_ok;
        }

        // 3) copy/paste
        let before = app.project.tracks.iter().flat_map(|t| &t.clips).count();
        app.copy_selection();
        app.player.clock = 11.0;
        app.paste_at_playhead();
        let after = app.project.tracks.iter().flat_map(|t| &t.clips).count();
        self.log(format!("{} copy/paste ({before} → {after} clips)", if after > before { "✓" } else { "✗" }));
        ok &= after > before;

        // 4) group
        app.player.clock = 1.0;
        let a = app.project.clips_at(1.0, crate::model::TrackKind::Video).first().map(|(_, c)| c.id);
        app.player.clock = 5.0;
        let b = app.project.clips_at(5.0, crate::model::TrackKind::Video).first().map(|(_, c)| c.id);
        if let (Some(a), Some(b)) = (a, b) {
            app.sel = Some(a);
            app.sel_multi = vec![a, b];
            app.group_selection();
            let ga = app.project.clip(a).and_then(|(_, c)| c.group);
            let gb = app.project.clip(b).and_then(|(_, c)| c.group);
            let grp_ok = ga.is_some() && ga == gb;
            self.log(format!("{} group/ungroup", if grp_ok { "✓" } else { "✗" }));
            ok &= grp_ok;
            app.ungroup_selection();
        }

        // 5) chroma key + mask → filter chain (on a clip OUTSIDE the 0..10
        // export range — geq masks are CPU-heavy and would stall the test)
        app.sel = app.project.video_tracks().first()
            .and_then(|t| t.clips.iter().find(|c| 20.0 >= c.tl_start && 20.0 < c.end()).map(|c| c.id));
        if let Some(id) = app.sel {
            app.set_fx_of_selection(|f| {
                f.chroma = crate::model::ChromaKey::classic();
                f.mask = crate::model::Mask { enabled: true, ellipse: true, cx: 0.5, cy: 0.5, hw: 0.3, hh: 0.3, feather: 0.1, invert: false };
            });
            if let Some((_, c)) = app.project.clip(id) {
                let chain = crate::media::video_filter_chain(&c.grade, &c.fx, c.src_dur, Some(640), Some(360), Some(30.0));
                let fx_ok = chain.contains("colorkey") && chain.contains("geq");
                self.log(format!("{} chroma key + mask in chain", if fx_ok { "✓" } else { "✗" }));
                ok &= fx_ok;
            }
        }

        // 6) adjustment layer affects lower clips (placed via the UI path;
        // magnetic placement may push it — find wherever it landed)
        app.player.clock = 0.0;
        app.add_adjustment_at_playhead();
        let adj_id = app.project.tracks.iter().flat_map(|t| &t.clips)
            .find(|c| c.kind == crate::model::ClipKind::Adjustment).map(|c| c.id);
        if let Some(adj_id) = adj_id {
            app.sel = Some(adj_id);
            app.set_grade_of_selection(|g| g.exposure = 1.5);
            let (ar, aend) = app.project.clip(adj_id).map(|(_, c)| (c.tl_start + 0.5, c.end() - 0.1)).unwrap_or((0.0, 0.0));
            let lower = app.project.video_tracks().first()
                .and_then(|t| t.clips.iter().find(|c| ar >= c.tl_start && ar < c.end() && c.id != adj_id).map(|c| c.id));
            if let Some(lid) = lower {
                if let Some((_, lc)) = app.project.clip(lid) {
                    let (g_eff, _) = app.project.effective_grade_fx_at(lc, ar);
                    let base_ok = (g_eff.exposure - lc.grade.exposure - 1.5).abs() < 0.01;
                    let outside = app.project.effective_grade_fx_at(lc, aend + 1.0).0.exposure - lc.grade.exposure;
                    let adj_ok = base_ok && outside.abs() < 0.01;
                    self.log(format!("{} adjustment layer merges (inside +1.5, outside Δ{outside:.2})", if adj_ok { "✓" } else { "✗" }));
                    ok &= adj_ok;
                }
            } else {
                self.log("✗ no lower clip under adjustment".into());
                ok = false;
            }
        } else {
            self.log("✗ adjustment clip not created".into());
            ok = false;
        }

        // 7) audio rack chain
        let a_clip = app.project.audio_tracks().iter().flat_map(|t| &t.clips).next().map(|c| c.id);
        if let Some(id) = a_clip {
            app.sel = Some(id);
            if let Some(cl) = app.project.clip_mut(id) {
                cl.afx.eq_low = 4.0;
                cl.afx.compressor = true;
                cl.afx.nr = 40.0;
            }
            if let Some((_, cl)) = app.project.clip(id) {
                let chain = crate::media::audio_filter_chain(&cl.afx, cl.gain_db, 0.0, 0.0, cl.src_dur);
                let rack_ok = chain.contains("bass") && chain.contains("acompressor") && chain.contains("afftdn");
                self.log(format!("{} audio rack chain (EQ+comp+NR)", if rack_ok { "✓" } else { "✗" }));
                ok &= rack_ok;
            }
        }

        // 8) roll edit keeps total duration
        let v1 = app.project.video_tracks().first().map(|t| t.id);
        if let Some(tid) = v1 {
            let seq: Vec<u64> = app.project.track(tid).map(|t| t.sorted_clips().into_iter().map(|c| c.id).collect()).unwrap_or_default();
            if seq.len() >= 2 {
                let dur_before = app.project.duration();
                let d = app.project.roll_edit(seq[0], seq[1], 0.25, None, None);
                let dur_after = app.project.duration();
                let roll_ok = (dur_before - dur_after).abs() < 0.05 && d.abs() > 0.0;
                self.log(format!("{} roll edit preserves duration (Δd = {d:.2})", if roll_ok { "✓" } else { "✗" }));
                ok &= roll_ok;
                // roll back
                app.project.roll_edit(seq[0], seq[1], -0.25, None, None);
            }
        }

        ok
    }

    fn fail(&mut self, why: &str) {
        self.lines.push(format!("FAIL: {why}"));
        println!("[SELFTEST] FAIL: {why}");
        self.passed = false;
        self.finished = true;
    }

    fn write_report(&mut self, app: &App) {
        let report = serde_json::json!({
            "app": "KestrelCut",
            "version": env!("CARGO_PKG_VERSION"),
            "lang": "en",
            "passed": self.passed,
            "elapsed_s": self.t0.elapsed().as_secs_f64(),
            "output": self.out_path.as_ref().map(|p| p.display().to_string()),
            "ffmpeg": crate::media::ffmpeg().map(|p| p.display().to_string()),
            "ffmpeg_source": crate::media::ffmpeg_source(),
            "encoders_used": app.export_state.vcodec,
            "log": self.lines,
        });
        let path = std::env::var("KC_SELFTEST_REPORT")
            .unwrap_or_else(|_| "selftest_report.json".into());
        match std::fs::write(&path, serde_json::to_string_pretty(&report).unwrap_or_default()) {
            Ok(_) => println!("[SELFTEST] report → {path}"),
            Err(e) => eprintln!("[SELFTEST] report write failed: {e}"),
        }
    }
}

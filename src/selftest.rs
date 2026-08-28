//! Scripted end-to-end functional test (`--selftest`).
//!
//! Drives the EXACT same code paths the UI buttons call: import → timeline
//! placement → split → grade → trim → title → in/out marks → export →
//! ffprobe verification. Writes a JSON report and sets exit code.

use crate::app::App;
use crate::i18n::Lang;
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
}

impl SelfTest {
    pub fn new() -> Self {
        Self {
            step: 0, t0: Instant::now(), step_t: Instant::now(),
            export_wait: None, lines: Vec::new(), finished: false, passed: false,
            out_path: None,
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
                    app.set_grade_of_selection(|g| { g.contrast = 20.0; g.saturation = -30.0; g.exposure = 0.3; });
                    app.set_fx_of_selection(|f| { f.blur = 2.0; f.fade_in = 0.5; });
                    app.commit();
                    self.log(format!("grade+fx applied to clip {id}"));
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
                        self.passed = dur_ok && streams_ok && size_ok;
                        if self.passed { self.log("SELFTEST PASS".into()); }
                        else { self.fail("verification criteria not met"); }
                        self.write_report(app);
                        self.finished = true;
                    }
                    Err(e) => self.fail(&format!("probe failed: {e}")),
                }
            }
            _ => self.finished = true,
        }
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
            "lang": match app.lang { Lang::En => "en", _ => "ar" },
            "passed": self.passed,
            "elapsed_s": self.t0.elapsed().as_secs_f64(),
            "output": self.out_path.as_ref().map(|p| p.display().to_string()),
            "ffmpeg": crate::media::ffmpeg().map(|p| p.display().to_string()),
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

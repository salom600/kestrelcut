//! Application core: state, media orchestration, decode slots, persistence.

use crate::decoder::{DecodeMode, Decoder, DecoderReq};
use crate::exporter::{self, ExportSpec};
use crate::i18n::K;
use crate::media::{self, MediaEvent, MediaInfo};
use crate::model::{Clip, History, MediaAsset, Project, TrackKind};
use crate::player::{bucket, hash_key, Player, Quality, Slot, Tool};
use crate::util::Theme;
use egui::TextureHandle;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};

// ------------------------------------------------------------------ enums
/// Real workspaces — every tab switches the panel arrangement.
#[derive(Clone, Copy, PartialEq)]
pub enum Workspace { Edit, Color, Audio, Fx, Export }

#[derive(Clone)]
pub struct FsState {
    pub dir: PathBuf,
    pub mode: FsMode,
    pub name: String,
}

#[derive(Clone, Copy, PartialEq)]
pub enum FsMode { OpenMedia, OpenProject, SaveProject, SaveExport, PickLut }

impl FsMode {
    pub fn filter(&self) -> Vec<&'static str> {
        match self {
            FsMode::OpenMedia => vec!["mp4", "mov", "mkv", "avi", "webm", "m4v", "mpg", "mpeg", "ts", "wmv",
                "mp3", "wav", "aac", "flac", "ogg", "m4a", "opus", "png", "jpg", "jpeg"],
            FsMode::OpenProject | FsMode::SaveProject => vec!["kcproj"],
            FsMode::SaveExport => vec!["mp4"],
            FsMode::PickLut => vec!["cube"],
        }
    }
    pub fn title(&self) -> String {
        match self {
            FsMode::OpenMedia => crate::i18n::tr(K::OpenMedia).to_string(),
            FsMode::OpenProject => crate::i18n::tr(K::OpenProject).to_string(),
            FsMode::SaveProject => crate::i18n::tr(K::SaveProject).to_string(),
            FsMode::SaveExport => crate::i18n::tr(K::OutputFile).to_string(),
            FsMode::PickLut => "LUT (.cube)".into(),
        }
    }
}

#[derive(Clone)]
pub enum Drag {
    ClipMove { id: u64, grab_off: f64, moved: bool },
    TrimL { id: u64 },
    TrimR { id: u64 },
    Slip { id: u64, grab_src: f64, grab_x: f32 },
    Kf { clip: u64, idx: usize },
    HScroll { grab_t: f64, grab_x: f32 },
}

#[derive(Clone)]
pub struct Toast { pub msg: String, pub kind: u8, pub at: Instant }

#[derive(Clone)]
pub struct ExportState {
    pub vcodec: String,
    pub quality: u32,
    pub res_choice: usize, // 0 seq, 1 1080, 2 720
    pub fps_choice: usize, // 0 seq, 1 60, 2 30, 3 24
    pub range_inout: bool,
    pub name: String,
    pub dir: PathBuf,
    pub running: Option<(u64, f32, f64)>, // job, frac, out_time
    pub last_result: Option<Result<PathBuf, String>>,
    pub hw: Vec<(String, String)>,
    pub sw: Vec<(String, String)>,
    pub open: bool,
}
impl Default for ExportState {
    fn default() -> Self {
        let hw = exporter::hw_encoders();
        let sw = exporter::sw_encoders();
        let vcodec = hw.first().map(|h| h.0.clone()).unwrap_or_else(|| "libx264".into());
        Self {
            vcodec, quality: 21, res_choice: 0, fps_choice: 0, range_inout: false,
            name: "export.mp4".into(),
            dir: dirs_home().unwrap_or_else(|| PathBuf::from(".")),
            running: None, last_result: None, hw, sw, open: false,
        }
    }
}

pub fn dirs_home() -> Option<PathBuf> {
    std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).map(PathBuf::from).ok()
}

#[derive(Default, Clone)]
pub struct ScopeImages {
    pub wave: Option<TextureHandle>,
    pub vector: Option<TextureHandle>,
    pub parade: Option<TextureHandle>,
    pub stamp: Option<Instant>,
}

#[derive(Clone)]
pub enum Dialog {
    Fs(FsState),
    Learn,
    Proxies,
}

// ------------------------------------------------------------------ App
pub struct App {
    pub theme: Theme,
    pub project: Project,
    pub hist: History,
    pub assets: Vec<MediaAsset>,
    pub project_dir: PathBuf,
    pub proxy_enabled: bool,
    pub proxy_jobs: HashMap<u64, u64>, // job → asset id

    pub ev_tx: Sender<MediaEvent>,
    pub ev_rx: Receiver<MediaEvent>,

    pub player: Player,
    pub tool: Tool,
    pub snap: bool,
    pub sel: Option<u64>,
    pub zoom: f64,
    pub scroll_t: f64,
    pub drag: Option<Drag>,
    pub workspace: Workspace,
    pub scopes_visible: bool,

    pub thumbs: HashMap<PathBuf, Option<TextureHandle>>,
    pub big_imgs: HashMap<PathBuf, TextureHandle>,
    pub waves: HashMap<PathBuf, std::sync::Arc<Vec<(i8, i8)>>>,
    pub tex_cache: std::collections::HashMap<u64, TextureHandle>,
    pub title_tex: HashMap<u64, (String, TextureHandle)>,
    pub probe_meta: HashMap<PathBuf, MediaInfo>,
    pub scopes: ScopeImages,

    pub dialog: Option<Dialog>,
    pub toasts: Vec<Toast>,
    pub export_state: ExportState,

    pub track_h_video: f32,
    pub track_h_audio: f32,

    pub preview_dirty: Option<Instant>,
    pub audio_warned: bool,
    pub timeline_rect: egui::Rect,

    pub selftest: Option<crate::selftest::SelfTest>,
    pub demo: bool,
    pub demo_build_pending: bool,
    pub exit_requested: bool,
    pub frame_times: std::collections::VecDeque<f64>,
    pub ui_fps: f64,
    pub settings_path: PathBuf,
    pub search: String,
    pub pool_tab: usize,
    pub pool_filter: u8, // 0 all · 1 video · 2 audio · 3 image
    pub pending_seek: Option<f64>,
    /// Wall-clock timestamp of the previous UI frame — real-time playback
    /// clock source (stable_dt drifts under slow software rendering, which
    /// desynced the decoders and caused constant reseek restarts).
    pub last_real: Option<Instant>,
    /// `--play` flag: start playback automatically once the demo timeline
    /// is built (used by CI screenshots and hands-free verification).
    pub autoplay: bool,
}

// ------------------------------------------------------------------ init
impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, demo: bool, selftest: Option<crate::selftest::SelfTest>) -> Self {
        crate::fonts::install(&cc.egui_ctx);
        let (tx, rx) = channel();
        let project = Project::default();
        let project_dir = dirs_home().unwrap_or_else(|| PathBuf::from(".")).join("KestrelCut");
        let _ = std::fs::create_dir_all(project_dir.join("exports"));
        let _ = std::fs::create_dir_all(project_dir.join("proxies"));
        let is_test = selftest.is_some();
        let mut app = Self {
            theme: Theme::default(), project,
            hist: History::new(Project::default()),
            assets: Vec::new(),
            project_dir,
            proxy_enabled: true,
            proxy_jobs: HashMap::new(),
            ev_tx: tx, ev_rx: rx,
            player: Player::new(),
            tool: Tool::Select,
            snap: true,
            sel: None,
            zoom: 110.0,
            scroll_t: 0.0,
            drag: None,
            workspace: Workspace::Edit,
            scopes_visible: true,
            thumbs: HashMap::new(), big_imgs: HashMap::new(), waves: HashMap::new(),
            tex_cache: HashMap::new(), title_tex: HashMap::new(),
            probe_meta: HashMap::new(), scopes: ScopeImages::default(),
            dialog: None, toasts: Vec::new(),
            export_state: ExportState::default(),
            track_h_video: 46.0, track_h_audio: 40.0,
            preview_dirty: None, audio_warned: false,
            timeline_rect: egui::Rect::NOTHING,
            selftest, demo,
            demo_build_pending: false,
            exit_requested: false,
            frame_times: std::collections::VecDeque::new(),
            ui_fps: 0.0,
            settings_path: dirs_home().unwrap_or_else(|| PathBuf::from(".")).join(".config/kestrelcut.json"),
            search: String::new(),
            pool_tab: 0,
            pool_filter: 0,
            pending_seek: None,
            last_real: None,
            autoplay: false,
        };
        app.player.quality = Some(Quality::Half);
        if !media::ffmpeg_ok() {
            std::thread::spawn(|| { let _ = media::ffmpeg(); });
        }
        if demo || is_test {
            app.setup_demo_media();
        }
        app
    }

    // ------------------------------------------------------------ helpers
    #[inline]
    pub fn t(&self, k: K) -> String { crate::i18n::tr(k).to_string() }

    pub fn commit(&mut self) { self.hist.commit(self.project.clone()); }

    pub fn do_undo(&mut self) {
        if self.hist.undo() { self.project = self.hist.current().clone(); self.invalidate_preview(); }
    }

    pub fn do_redo(&mut self) {
        if self.hist.redo() { self.project = self.hist.current().clone(); self.invalidate_preview(); }
    }

    pub fn new_project(&mut self) {
        self.project = crate::model::Project::default();
        self.hist = crate::model::History::new(self.project.clone());
        self.assets.clear();
        self.sel = None;
        self.player.pause();
        self.player.seek(0.0);
        self.player.slots.clear();
        self.invalidate_preview();
        self.toast(self.t(K::NewProject), 1);
    }

    pub fn add_video_track(&mut self) {
        self.commit();
        let n = self.project.video_tracks().len() + 1;
        self.project.tracks.push(crate::model::Track {
            id: crate::model::next_id(),
            kind: crate::model::TrackKind::Video,
            name: format!("V{n}"),
            locked: false, hidden: false, mute: false, solo: false, arm: false,
            clips: Vec::new(),
        });
        self.commit();
        self.toast(format!("✓ V{n}"), 1);
    }

    pub fn add_audio_track(&mut self) {
        self.commit();
        let n = self.project.audio_tracks().len() + 1;
        self.project.tracks.push(crate::model::Track {
            id: crate::model::next_id(),
            kind: crate::model::TrackKind::Audio,
            name: format!("A{n}"),
            locked: false, hidden: false, mute: false, solo: false, arm: false,
            clips: Vec::new(),
        });
        self.commit();
        self.toast(format!("✓ A{n}"), 1);
    }

    pub fn toast(&mut self, msg: impl Into<String>, kind: u8) {
        self.toasts.push(Toast { msg: msg.into(), kind, at: Instant::now() });
        if self.toasts.len() > 5 { self.toasts.remove(0); }
    }

    pub fn selected_clip(&self) -> Option<&Clip> {
        self.sel.and_then(|id| self.project.clip(id).map(|(_, c)| c))
    }

    pub fn selected_clip_mut(&mut self) -> Option<&mut Clip> {
        let sel = self.sel?;
        self.project.clip_mut(sel)
    }

    pub fn invalidate_preview(&mut self) {
        self.preview_dirty = Some(Instant::now() + Duration::from_millis(200));
    }

    fn apply_preview_dirty(&mut self) {
        for s in self.player.slots.iter_mut() { s.key = 0; }
    }

    // ------------------------------------------------------------ import
    pub fn import_files(&mut self, paths: Vec<PathBuf>) {
        let tx = self.ev_tx.clone();
        std::thread::spawn(move || {
            for p in paths {
                match media::import(&p) {
                    Ok(asset) => { let _ = tx.send(MediaEvent::Imported(asset)); }
                    Err(e) => { let _ = tx.send(MediaEvent::ImportFailed { path: p, err: e }); }
                }
            }
        });
    }

    pub fn ensure_thumb(&mut self, path: PathBuf) {
        if self.thumbs.contains_key(&path) { return; }
        self.thumbs.insert(path.clone(), None);
        media::spawn_thumb(path.clone(), 1.0, 160, self.ev_tx.clone());
    }

    pub fn ensure_wave(&mut self, path: PathBuf) {
        if self.waves.contains_key(&path) { return; }
        self.waves.insert(path.clone(), std::sync::Arc::new(Vec::new()));
        media::spawn_wave(path.clone(), self.ev_tx.clone());
    }

    fn asset_by(&self, id: u64) -> Option<&MediaAsset> { self.assets.iter().find(|a| a.id == id) }

    pub fn add_asset_to_timeline(&mut self, asset_id: u64) {
        let Some(a) = self.asset_by(asset_id).cloned() else { return };
        let t = self.player.clock;
        let (v_track, a_track) = self.free_tracks_for(&a, t);
        self.commit(); // pre-edit marker
        match a.kind {
            crate::model::AssetKind::Video => {
                let vclip = crate::model::clip_from_asset(&a, t, None);
                let vid = vclip.id;
                let mut aclip = crate::model::clip_from_asset(&a, t, Some(vid));
                aclip.kind = crate::model::ClipKind::Audio;
                let aid = aclip.id;
                let mut vclip = vclip;
                vclip.link = Some(aid);
                if let Some(vt) = v_track { self.project.place_clip(vclip, vt); }
                if let Some(at) = a_track { self.project.place_clip(aclip, at); }
                self.sel = Some(vid);
            }
            crate::model::AssetKind::Audio => {
                if let Some(at) = a_track {
                    self.project.place_clip(crate::model::clip_from_asset(&a, t, None), at);
                }
            }
            crate::model::AssetKind::Image => {
                if let Some(vt) = v_track {
                    self.project.place_clip(crate::model::clip_from_asset(&a, t, None), vt);
                }
            }
        }
        self.commit();
        self.ensure_wave_for_all();
        self.invalidate_preview();
    }

    pub fn pick_source(&self, a: &MediaAsset) -> PathBuf {
        if self.proxy_enabled {
            if let Some(p) = &a.proxy { if p.exists() { return p.clone(); } }
        }
        a.path.clone()
    }

    fn free_tracks_for(&self, a: &MediaAsset, t: f64) -> (Option<u64>, Option<u64>) {
        let need_v = matches!(a.kind, crate::model::AssetKind::Video | crate::model::AssetKind::Image);
        let need_a = a.has_audio || a.kind == crate::model::AssetKind::Audio;
        let v_free = self.project.video_tracks().iter()
            .find(|tr| !tr.locked && tr.clips.iter().all(|c| t < c.tl_start || t >= c.end()))
            .map(|t| t.id);
        let a_free = self.project.audio_tracks().iter()
            .find(|tr| !tr.locked && tr.clips.iter().all(|c| t < c.tl_start || t >= c.end()))
            .map(|t| t.id);
        (need_v.then_some(v_free).flatten(), need_a.then_some(a_free).flatten())
    }

    pub fn ensure_wave_for_all(&mut self) {
        let paths: Vec<PathBuf> = self.assets.iter().filter(|a| a.has_audio).map(|a| a.path.clone()).collect();
        for p in paths { self.ensure_wave(p); }
    }

    pub fn add_title_at_playhead(&mut self) {
        let t = self.player.clock;
        let track = self.project.video_tracks().iter().rev()
            .find(|tr| !tr.locked && tr.clips.iter().all(|c| t < c.tl_start || t >= c.end()))
            .map(|tr| tr.id);
        if let Some(tid) = track {
            self.commit();
            let c = crate::model::title_clip("Title", t, 4.0);
            self.sel = Some(c.id);
            self.project.place_clip(c, tid);
            self.commit();
            self.invalidate_preview();
        }
    }

    // ------------------------------------------------------------ edit ops
    pub fn split_at_playhead(&mut self) {
        let t = self.player.clock;
        let ids: Vec<u64> = self.project.tracks.iter()
            .flat_map(|tr| tr.clips.iter())
            .filter(|c| t > c.tl_start + 0.05 && t < c.end() - 0.05)
            .map(|c| c.id).collect();
        if ids.is_empty() { return; }
        self.commit();
        for id in ids { self.project.split_clip(id, t); }
        self.commit();
        self.toast(self.t(K::SplitHere), 0);
        self.invalidate_preview();
    }

    pub fn delete_selection(&mut self, ripple: bool) {
        if let Some(id) = self.sel {
            self.commit();
            self.project.delete_clip(id, ripple);
            self.sel = None;
            self.commit();
            self.invalidate_preview();
        }
    }

    pub fn nudge_selection(&mut self, frames: f64) {
        if let Some(id) = self.sel {
            let Some((tr, c)) = self.project.clip(id) else { return };
            let (tr_id, new_start) = (tr.id, c.tl_start + frames / self.project.fps);
            self.project.move_clip(id, tr_id, new_start);
            self.invalidate_preview();
        }
    }

    pub fn set_grade_of_selection(&mut self, f: impl FnOnce(&mut crate::model::Grade)) {
        if let Some(c) = self.selected_clip_mut() { f(&mut c.grade); }
        self.invalidate_preview();
    }
    pub fn set_fx_of_selection(&mut self, f: impl FnOnce(&mut crate::model::Fx)) {
        if let Some(c) = self.selected_clip_mut() { f(&mut c.fx); }
        self.invalidate_preview();
    }
    pub fn set_transform_of_selection(&mut self, f: impl FnOnce(&mut crate::model::Transform)) {
        if let Some(c) = self.selected_clip_mut() { f(&mut c.transform); }
        self.invalidate_preview();
    }

    pub fn auto_color(&mut self) {
        let Some((w, h, buf)) = &self.player.last_frame_for_scopes else { return };
        let mut sum = 0.0f64;
        let mut mn = 255.0f64;
        let mut mx = 0.0f64;
        let step = (((w * h) as usize) / 4096).max(1) * 4;
        let mut i = 0;
        let mut n = 0;
        while i + 3 < buf.len() {
            let l = 0.299 * buf[i] as f64 + 0.587 * buf[i + 1] as f64 + 0.114 * buf[i + 2] as f64;
            sum += l; mn = mn.min(l); mx = mx.max(l); n += 1;
            i += step;
        }
        if n == 0 { return; }
        let mean = sum / n as f64;
        let range = (mx - mn).max(1.0);
        let exposure = ((128.0 - mean) / 128.0 * 4.0).clamp(-4.0, 4.0) as f32;
        let contrast = ((255.0 - range) / 255.0 * 100.0).clamp(-100.0, 100.0) as f32;
        self.set_grade_of_selection(|g| { g.exposure = exposure; g.contrast = contrast; });
        self.toast(self.t(K::ColorAuto), 1);
    }

    // ------------------------------------------------------------ persistence
    pub fn save_project(&mut self, path: PathBuf) {
        let pf = ProjectFile { project: self.project.clone(), assets: self.assets.clone() };
        match serde_json::to_string_pretty(&pf) {
            Ok(s) => match std::fs::write(&path, s) {
                Ok(_) => {
                    self.toast(self.t(K::MsgProjectSaved), 1);
                    if let Some(stem) = path.file_stem() {
                        self.project.name = stem.to_string_lossy().to_string();
                    }
                }
                Err(e) => self.toast(format!("write: {e}"), 2),
            },
            Err(e) => self.toast(format!("serialize: {e}"), 2),
        }
    }

    pub fn load_project(&mut self, path: PathBuf) {
        let read = std::fs::read(&path).map_err(|e| e.to_string())
            .and_then(|b| serde_json::from_slice::<ProjectFile>(&b).map_err(|e| e.to_string()));
        match read {
            Ok(pf) => {
                self.project = pf.project.clone();
                self.assets = pf.assets;
                self.hist = History::new(pf.project);
                self.player.pause();
                self.player.seek(0.0);
                self.player.slots.clear();
                let paths: Vec<PathBuf> = self.assets.iter().map(|a| a.path.clone()).collect();
                for p in paths { self.ensure_thumb(p); }
                self.ensure_wave_for_all();
                self.invalidate_preview();
                self.toast(self.t(K::MsgLoaded), 1);
            }
            Err(e) => self.toast(format!("open: {e}"), 2),
        }
    }

    // ------------------------------------------------------------ export
    pub fn start_export(&mut self) {
        if self.export_state.running.is_some() { return; }
        if !media::ffmpeg_ok() { self.toast(self.t(K::MsgNoFfmpeg), 2); return; }
        let seq_dur = self.project.duration();
        let range = if self.export_state.range_inout
            && self.project.in_mark.is_some() && self.project.out_mark.is_some() {
            (self.project.in_mark.unwrap(), self.project.out_mark.unwrap())
        } else { (0.0, seq_dur.max(0.5)) };
        let (w, h) = match self.export_state.res_choice {
            1 => (1920, 1080),
            2 => (1280, 720),
            _ => (self.project.width, self.project.height),
        };
        let fps = match self.export_state.fps_choice { 1 => 60.0, 2 => 30.0, 3 => 24.0, _ => self.project.fps };
        let out = self.project_dir.join("exports").join(&self.export_state.name);
        let spec = ExportSpec {
            out, width: w, height: h, fps, range,
            vcodec: self.export_state.vcodec.clone(),
            acodec: "aac".into(),
            quality: self.export_state.quality,
            preset: "veryfast".into(),
        };
        let job = crate::model::next_id();
        self.export_state.running = Some((job, 0.0, 0.0));
        let proj = self.project.clone();
        let assets = self.assets.clone();
        exporter::run_export(job, spec, proj, assets, self.ev_tx.clone());
        self.toast(self.t(K::MsgExporting), 0);
    }

    pub fn create_proxy_for(&mut self, asset_id: u64) {
        let Some(a) = self.asset_by(asset_id).cloned() else { return };
        let dest = self.proxy_path_for(&a.path);
        if let Some(px) = a.proxy.as_ref() { if px.exists() { return; } }
        let job = crate::model::next_id();
        self.proxy_jobs.insert(job, asset_id);
        media::spawn_proxy(job, a.path.clone(), dest, self.ev_tx.clone());
        self.toast(self.t(K::CreateProxy), 0);
    }

    // ------------------------------------------------------------ player
    /// Playback engine state machine.
    ///
    /// - Paused  → one `Still` decoder per active video track grabs a single
    ///   frame at the current position (fast seek); the previous frame stays
    ///   visible until the fresh one arrives, so scrubbing never goes black.
    /// - Playing → one `Run` decoder per active video track streams frames
    ///   paced at (fps × clip-speed × playback-speed); drift vs the playback
    ///   clock triggers a reseek only when it exceeds 0.6 s. Decoders are NOT
    ///   restarted per frame — that was the cause of the old black preview.
    /// - Decode failures are captured and surfaced in the preview overlay.
    pub fn update_player(&mut self, _ctx: &egui::Context) {
        let fps = self.project.fps;
        let q = self.player.quality.unwrap_or(Quality::Half);
        let playing = self.player.playing;
        let mode = if playing { DecodeMode::Run } else { DecodeMode::Still };
        let gen = self.player.seek_gen;
        let speed = self.player.speed as f64;
        if let Some(ts) = self.preview_dirty {
            if Instant::now() >= ts { self.preview_dirty = None; self.apply_preview_dirty(); }
        }

        struct Want {
            track: u64,
            clip_id: u64,
            key: u64,
            src_in: f64,
            req: Option<DecoderReq>,
        }
        let mut wants: Vec<Want> = Vec::new();
        let t = self.player.clock;
        let bucket_now = bucket(t, fps);
        let aspect = self.project.width as f32 / self.project.height.max(1) as f32;
        let mut ph = (self.project.height as f32 * q.factor()).clamp(90.0, 720.0);
        let mut pw = ph * aspect;
        if pw > 1280.0 { pw = 1280.0; ph = pw / aspect; }
        let (pw, ph) = ((pw as u32) & !1, (ph as u32) & !1);

        for tr in self.project.tracks.iter().filter(|tr| tr.kind == TrackKind::Video && !tr.hidden) {
            let Some(c) = tr.clips.iter().find(|c| t >= c.tl_start && t < c.end()) else { continue };
            if c.kind != crate::model::ClipKind::Video { continue; }
            let rel = (t - c.tl_start) * c.speed as f64;
            let src_t = c.src_in + rel;
            let src_path = c.source.clone().unwrap_or_default();
            let filters = media::video_filter_chain(&c.grade, &c.fx, c.src_dur, Some(pw), Some(ph), Some(fps));
            let key = hash_key(&[c.id, fnv(&filters), q as u64, mode as u64]);
            // pacing rate: frames per wall-second — project fps scaled by clip
            // speed (source consumed faster/slower) and playback speed
            let rate = (fps * (c.speed as f64).max(0.01) * speed).max(1.0);
            wants.push(Want {
                track: tr.id, clip_id: c.id, key, src_in: src_t,
                req: Some(DecoderReq { path: src_path, src_in: src_t, filters, w: pw, h: ph, fps: rate, mode }),
            });
        }

        let want_tracks: Vec<u64> = wants.iter().map(|w| w.track).collect();
        // Freeze at the end: paused at/after the sequence end with no active
        // clip, real NLEs hold the last decoded frame instead of going black.
        let at_end = t >= self.project.duration() - 1e-6;
        let freeze_end = !playing && at_end && want_tracks.is_empty() && !self.player.slots.is_empty();
        if !freeze_end {
            self.player.slots.retain(|s| want_tracks.contains(&s.track_id));
            for w in wants {
            if let Some(slot) = self.player.slots.iter_mut().find(|s| s.track_id == w.track) {
                let clip_changed = slot.clip_id != w.clip_id;
                let key_changed = slot.key != w.key;
                let seeked = slot.seek_gen != gen;
                let mut need_restart = key_changed || seeked;
                if !need_restart {
                    if playing {
                        // Playback is decode-limited best-effort (like dropped
                        // frames in real NLEs): a heavy filter chain may lag
                        // the wall clock, which must NOT trigger restarts.
                        // Restart only on a genuine stall (no frames from the
                        // current decoder) or a huge seek-induced drift.
                        let stalled = match slot.last_frame_at {
                            Some(lf) => lf.elapsed() > Duration::from_secs(2),
                            None => slot.started_at.elapsed() > Duration::from_secs(4),
                        };
                        let big_drift = slot.frame_current && slot.eof != true
                            && slot.frame.as_ref()
                                .map(|f| (t - slot.origin_clock - f.pts * speed).abs() > 3.0)
                                .unwrap_or(false);
                        if (stalled && !slot.eof) || big_drift { need_restart = true; }
                    } else if slot.still_bucket != bucket_now {
                        need_restart = true;
                    }
                }
                if need_restart {
                    if std::env::var("KC_TRACE").is_ok() {
                        eprintln!("[trace] restart track={} clip={} reason={}{}{} clk={t:.3} src_in={:.3} key={:x}→{:x} gen {}→{}",
                            w.track, w.clip_id,
                            if key_changed { "KEY " } else { "" },
                            if seeked { "SEEK " } else { "" },
                            if !key_changed && !seeked { "DRIFT" } else { "" },
                            w.src_in, slot.key, w.key, slot.seek_gen, gen);
                    }
                    match w.req.clone().map(Decoder::start) {
                        Some(Ok(d)) => {
                            slot.dec = Some(d);
                            slot.decode_error = None;
                            if clip_changed { slot.frame = None; } // new content
                        }
                        Some(Err(e)) => { slot.dec = None; slot.decode_error = Some(e); }
                        None => { slot.dec = None; }
                    }
                    slot.clip_id = w.clip_id;
                    slot.key = w.key;
                    slot.eof = false;
                    slot.origin_clock = t;
                    slot.seek_gen = gen;
                    slot.still_bucket = bucket_now;
                    slot.frame_current = false; // carried frame is from the old gen
                    slot.last_frame_at = None;
                    slot.started_at = Instant::now();
                }
            } else {
                let (dec, err) = match w.req.clone().map(Decoder::start) {
                    Some(Ok(d)) => (Some(d), None),
                    Some(Err(e)) => (None, Some(e)),
                    None => (None, None),
                };
                self.player.slots.push(Slot {
                    track_id: w.track, clip_id: w.clip_id, key: w.key, dec, frame: None, eof: false,
                    origin_clock: t, seek_gen: gen, still_bucket: bucket_now, decode_error: err,
                    frame_current: false, last_frame_at: None, started_at: Instant::now(),
                });
            }
            }
        } // !freeze_end

        for s in self.player.slots.iter_mut() {
            if let Some(dec) = s.dec.as_mut() {
                if let Some(f) = dec.poll() {
                    s.frame = Some(f); s.frame_current = true; s.decode_error = None;
                    s.last_frame_at = Some(Instant::now());
                }
                if let Some(e) = &dec.last_error { s.decode_error = Some(e.clone()); }
                if dec.is_eof() { s.eof = true; }
            }
        }
        if self.player.slots.iter().any(|s| s.frame.is_some()) {
            self.player.ever_had_frame = true;
        }

        #[cfg(feature = "audio")]
        self.update_audio();

        if let Some(s) = self.player.slots.iter().filter(|s| s.frame.is_some()).next_back() {
            if let Some(f) = &s.frame {
                self.player.last_frame_for_scopes = Some((f.w, f.h, f.rgba.clone()));
            }
        }
    }

    #[cfg(feature = "audio")]
    fn update_audio(&mut self) {
        if !self.player.playing {
            if self.player.audio.is_some() { self.player.audio = None; self.player.audio_clip = None; }
            return;
        }
        let t = self.player.clock;
        let any_solo = self.project.audio_tracks().iter().any(|tr| tr.solo);
        let mut chosen: Option<(u64, PathBuf, f64, f64)> = None;
        for tr in self.project.audio_tracks() {
            if tr.mute || (any_solo && !tr.solo) { continue; }
            if let Some(c) = tr.clips.iter().find(|c| t >= c.tl_start && t < c.end()) {
                if let Some(src) = c.source.clone() {
                    let src_in = c.src_in + (t - c.tl_start) * c.speed as f64;
                    let dur = (c.end() - t) * c.speed as f64;
                    chosen = Some((c.id, src, src_in, dur));
                }
            }
        }
        match chosen {
            Some((cid, src, src_in, dur)) if self.player.audio_clip != Some(cid) => {
                self.player.audio = crate::decoder::audio::Monitor::start(src, src_in, dur.max(0.2));
                if self.player.audio.is_some() {
                    self.player.audio_clip = Some(cid);
                } else if !self.audio_warned {
                    self.audio_warned = true;
                    self.toast(self.t(K::MsgNoAudio), 2);
                }
            }
            None => { self.player.audio = None; self.player.audio_clip = None; }
            _ => {}
        }
    }

    // ------------------------------------------------------------ events
    pub fn poll_events(&mut self, ctx: &egui::Context) {
        while let Ok(ev) = self.ev_rx.try_recv() {
            match ev {
                MediaEvent::Imported(asset) => {
                    let p = asset.path.clone();
                    let has_audio = asset.has_audio;
                    self.assets.push(asset);
                    self.ensure_thumb(p.clone());
                    if has_audio { self.ensure_wave(p); }
                    self.toast("✓ import ok", 1);
                }
                MediaEvent::ImportFailed { path, err } => {
                    self.toast(format!("✗ {}: {}", path.display(), err), 2);
                }
                MediaEvent::Thumb { path, png } => {
                    if let Some(img) = image::load_from_memory(&png).ok() {
                        let rgba = img.to_rgba8();
                        let (w, h) = (rgba.width(), rgba.height());
                        let tex = ctx.load_texture(
                            format!("thumb:{}", path.display()),
                            egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], rgba.as_raw()),
                            egui::TextureOptions::LINEAR);
                        self.thumbs.insert(path, Some(tex));
                    }
                }
                MediaEvent::Wave { path, peaks } => { self.waves.insert(path, peaks); }
                MediaEvent::Progress { job, frac, out_time, .. } => {
                    if let Some((j, f, t)) = self.export_state.running.as_mut() {
                        if *j == job { *f = frac; *t = out_time; }
                    }
                }
                MediaEvent::JobDone { job, result } => {
                    if self.export_state.running.map(|(j, _, _)| j == job).unwrap_or(false) {
                        self.export_state.running = None;
                        match &result {
                            Ok(p) => {
                                self.export_state.last_result = Some(Ok(PathBuf::from(p)));
                                self.toast(format!("✓ {} — {p}", self.t(K::MsgExportDone)), 1);
                            }
                            Err(e) => {
                                self.export_state.last_result = Some(Err(e.clone()));
                                self.toast(format!("✗ {}: {}", self.t(K::MsgExportFail), e), 2);
                            }
                        }
                    } else if let Some(asset_id) = self.proxy_jobs.remove(&job) {
                        match result {
                            Ok(p) => {
                                let pp = PathBuf::from(&p);
                                if let Some(a) = self.assets.iter_mut().find(|a| a.id == asset_id) {
                                    a.proxy = Some(pp);
                                }
                                self.toast(format!("✓ {}", self.t(K::MsgProxyDone)), 1);
                            }
                            Err(e) => self.toast(format!("✗ {}: {}", self.t(K::MsgProxyFail), e), 2),
                        }
                    }
                }
            }
        }
    }

    pub fn proxy_path_for(&self, src: &Path) -> PathBuf {
        let stem = src.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "media".to_string());
        self.project_dir.join("proxies").join(format!("{stem}_proxy.mp4"))
    }

    // ------------------------------------------------------------ transport
    pub fn toggle_play(&mut self) {
        if self.player.playing {
            self.player.pause();
            self.player.audio = None;
            self.player.audio_clip = None;
            // decoders switch to Still mode on the next tick; the last frame
            // stays visible so pause never blanks the preview
        } else {
            // play pressed at/after the end → restart from the in-point
            let dur = self.project.duration();
            if dur > 0.0 && self.player.clock >= dur - 1e-6 {
                let t0 = self.project.in_mark.unwrap_or(0.0);
                self.player.seek(if t0 < dur - 1e-6 { t0 } else { 0.0 });
            }
            self.player.playing = true;
            // decoders switch to Run mode on the next tick via key change
        }
    }

    // ------------------------------------------------------------ shortcuts
    pub fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.wants_keyboard_input() { return; }
        let mut act: Option<u8> = None;
        ctx.input(|i| {
            use egui::Key;
            let m = i.modifiers;
            if i.key_pressed(Key::Space) { act = Some(1); }
            else if i.key_pressed(Key::S) && !m.ctrl && !m.shift { act = Some(2); }
            else if i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace) { act = Some(3); }
            else if i.key_pressed(Key::Z) && m.ctrl && !m.shift { act = Some(4); }
            else if (i.key_pressed(Key::Z) && m.ctrl && m.shift) || (i.key_pressed(Key::Y) && m.ctrl) { act = Some(5); }
            else if i.key_pressed(Key::I) { act = Some(6); }
            else if i.key_pressed(Key::O) { act = Some(7); }
            else if i.key_pressed(Key::ArrowRight) { act = Some(if m.shift { 8 } else { 9 }); }
            else if i.key_pressed(Key::ArrowLeft) { act = Some(if m.shift { 10 } else { 11 }); }
            else if i.key_pressed(Key::Home) { act = Some(12); }
            else if i.key_pressed(Key::End) { act = Some(13); }
            else if i.key_pressed(Key::L) && !m.ctrl { act = Some(14); }
            else if i.key_pressed(Key::A) && !m.ctrl { act = Some(15); }
            else if i.key_pressed(Key::C) && !m.ctrl { act = Some(16); }
            else if i.key_pressed(Key::P) && !m.ctrl { act = Some(17); }
            else if i.key_pressed(Key::G) && !m.ctrl { act = Some(18); }
            else if i.key_pressed(Key::H) && !m.ctrl { act = Some(19); }
            else if i.key_pressed(Key::Z) && !m.ctrl { act = Some(20); }
            else if i.key_pressed(Key::T) && !m.ctrl { act = Some(21); }
            else if i.key_pressed(Key::Equals) && m.ctrl { act = Some(22); }
            else if i.key_pressed(Key::Minus) && m.ctrl { act = Some(23); }
            else if i.key_pressed(Key::E) && m.ctrl { act = Some(24); }
            else if i.key_pressed(Key::Escape) { act = Some(25); }
        });
        match act {
            Some(1) => self.toggle_play(),
            Some(2) => self.split_at_playhead(),
            Some(3) => self.delete_selection(false),
            Some(4) => { if self.hist.undo() { self.project = self.hist.current().clone(); self.invalidate_preview(); } }
            Some(5) => { if self.hist.redo() { self.project = self.hist.current().clone(); self.invalidate_preview(); } }
            Some(6) => { self.project.in_mark = Some(self.player.clock); }
            Some(7) => { self.project.out_mark = Some(self.player.clock); }
            Some(8) => self.player.seek(self.player.clock + 1.0),
            Some(9) => self.player.seek(self.player.clock + 1.0 / self.project.fps),
            Some(10) => self.player.seek(self.player.clock - 1.0),
            Some(11) => self.player.seek(self.player.clock - 1.0 / self.project.fps),
            Some(12) => self.player.seek(self.project.in_mark.unwrap_or(0.0)),
            Some(13) => self.player.seek(self.project.out_mark.unwrap_or(self.project.duration())),
            Some(14) => self.player.loop_play = !self.player.loop_play,
            Some(15) => self.tool = Tool::Select,
            Some(16) => self.tool = Tool::Razor,
            Some(17) => self.tool = Tool::Slip,
            Some(18) => self.tool = Tool::Pen,
            Some(19) => self.tool = Tool::Hand,
            Some(20) => self.tool = Tool::Zoom,
            Some(21) => self.tool = Tool::Text,
            Some(22) => self.zoom = (self.zoom * 1.25).min(4000.0),
            Some(23) => self.zoom = (self.zoom / 1.25).max(4.0),
            Some(24) => { self.export_state.open = true; }
            Some(25) => { self.sel = None; }
            _ => {}
        }
    }

    // ------------------------------------------------------------ demo
    pub fn setup_demo_media(&mut self) {
        let dir = self.project_dir.join("demo");
        let _ = std::fs::create_dir_all(&dir);
        let clips: Vec<(String, &str)> = vec![
            ("C0001.MP4".into(), "testsrc2=size=1280x720:rate=30:duration=8"),
            ("C0002.MP4".into(), "smptebars=size=1280x720:rate=30:duration=8"),
            ("C0003.MP4".into(), "gradients=size=1280x720:rate=30:duration=8"),
        ];
        let mut paths = Vec::new();
        for (name, src) in clips {
            let p = dir.join(&name);
            if !p.exists() {
                let ffmpeg = media::ffmpeg().ok_or_else(|| "ffmpeg not found".to_string());
                let Ok(bin) = ffmpeg else { continue };
                let _ = std::process::Command::new(bin).args([
                    "-v", "quiet", "-y", "-f", "lavfi", "-i", src,
                    "-f", "lavfi", "-i", "sine=frequency=440:duration=8",
                    "-c:v", "libx264", "-preset", "ultrafast", "-crf", "30",
                    "-c:a", "aac", "-shortest", &p.to_string_lossy(),
                ]).output();
            }
            paths.push(p);
        }
        self.import_files(paths);
        self.demo_build_pending = true;
    }

    /// Once demo assets are imported, build a populated timeline (used by
    /// --demo and selftest) through the SAME code paths as the UI.
    pub fn try_build_demo_timeline(&mut self) {
        if !self.demo_build_pending { return; }
        let vids: Vec<u64> = self.assets.iter()
            .filter(|a| a.is_video()).map(|a| a.id).take(3).collect();
        if vids.len() < 3 { return; }
        self.demo_build_pending = false;
        // deterministic placement (same code path the UI buttons use)
        self.player.clock = 0.0;
        self.add_asset_to_timeline(vids[0]);
        self.player.clock = 8.0;
        self.add_asset_to_timeline(vids[1]);
        self.player.clock = 2.0;
        self.add_asset_to_timeline(vids[2]);
        if let Some(tid) = self.project.video_tracks().last().map(|t| t.id) {
            let c = crate::model::title_clip("KestrelCut", 1.0, 3.0);
            self.project.place_clip(c, tid);
        }
        self.commit();
        self.player.clock = self.pending_seek.take().unwrap_or(0.0);
        self.player.slots.clear();
        self.ensure_wave_for_all();
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct ProjectFile { pub project: Project, pub assets: Vec<MediaAsset> }

pub fn fnv(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

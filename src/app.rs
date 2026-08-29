//! Application core: state, media orchestration, decode slots, persistence.

use crate::decoder::{DecodeMode, Decoder, DecoderReq};
use crate::exporter::{self, ExportSpec};
use crate::i18n::K;
use crate::media::{self, MediaEvent, MediaInfo};
use crate::model::{Clip, History, MediaAsset, Project, TrackKind};
use crate::player::{bucket, hash_key, Player, Quality, Slot, Tool};
use crate::util::Theme;
use egui::{Pos2, Rect, TextureHandle};
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
pub enum FsMode { OpenMedia, OpenProject, SaveProject, SaveExport, PickLut, PickSrt }

impl FsMode {
    pub fn filter(&self) -> Vec<&'static str> {
        match self {
            FsMode::OpenMedia => vec!["mp4", "mov", "mkv", "avi", "webm", "m4v", "mpg", "mpeg", "ts", "wmv",
                "mp3", "wav", "aac", "flac", "ogg", "m4a", "opus", "png", "jpg", "jpeg"],
            FsMode::OpenProject | FsMode::SaveProject => vec!["kcproj"],
            FsMode::SaveExport => vec!["mp4"],
            FsMode::PickLut => vec!["cube"],
            FsMode::PickSrt => vec!["srt"],
        }
    }
    pub fn title(&self) -> String {
        match self {
            FsMode::OpenMedia => crate::i18n::tr(K::OpenMedia).to_string(),
            FsMode::OpenProject => crate::i18n::tr(K::OpenProject).to_string(),
            FsMode::SaveProject => crate::i18n::tr(K::SaveProject).to_string(),
            FsMode::SaveExport => crate::i18n::tr(K::OutputFile).to_string(),
            FsMode::PickLut => "LUT (.cube)".into(),
            FsMode::PickSrt => "Subtitles (.srt)".into(),
        }
    }
}

#[derive(Clone)]
pub enum Drag {
    ClipMove { id: u64, grab_off: f64, moved: bool },
    TrimL { id: u64 },
    TrimR { id: u64 },
    Slip { id: u64, grab_src: f64, grab_x: f32 },
    /// Roll edit: move the cut point between two adjacent clips.
    Roll { left_id: u64, right_id: u64 },
    /// Slide edit: move clip between its neighbors (content unchanged).
    Slide { id: u64, grab_t: f64, grab_x: f32 },
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
    pub hist: Option<TextureHandle>,
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
    /// Multi-selection (group edits, shift-click). Always contains `sel` when non-empty.
    pub sel_multi: Vec<u64>,
    /// Copy/paste clipboard (full clip snapshots).
    pub clipboard: Vec<Clip>,
    pub zoom: f64,
    pub scroll_t: f64,
    pub drag: Option<Drag>,
    pub workspace: Workspace,
    pub scopes_visible: bool,
    /// Title/action-safe overlay on the preview.
    pub safe_margins: bool,
    /// White-balance eyedropper armed — next click on the preview samples gray.
    pub wb_pick: bool,
    /// Timeline snap indicator line (x time) while dragging.
    pub snap_line: Option<f64>,
    /// Input modifiers cache (refreshed each frame; used by timeline tools).
    pub mod_cache: Option<egui::Modifiers>,

    pub thumbs: HashMap<PathBuf, Option<TextureHandle>>,
    pub big_imgs: HashMap<PathBuf, TextureHandle>,
    /// CPU-side RGBA copies for images (software blend compositor input).
    pub big_imgs_cpu: HashMap<PathBuf, std::sync::Arc<Vec<u8>>>,
    pub waves: HashMap<PathBuf, std::sync::Arc<Vec<(i8, i8)>>>,
    pub tex_cache: std::collections::HashMap<u64, TextureHandle>,
    pub title_tex: HashMap<u64, (String, TextureHandle, std::sync::Arc<Vec<u8>>)>,
    /// Scratch RGBA canvas for the software blend compositor.
    pub soft_canvas: (u32, u32, Vec<u8>),
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
            sel_multi: Vec::new(),
            clipboard: Vec::new(),
            zoom: 110.0,
            scroll_t: 0.0,
            drag: None,
            workspace: Workspace::Edit,
            scopes_visible: true,
            safe_margins: false,
            wb_pick: false,
            snap_line: None,
            mod_cache: None,
            thumbs: HashMap::new(), big_imgs: HashMap::new(), waves: HashMap::new(),
            big_imgs_cpu: HashMap::new(),
            soft_canvas: (0, 0, Vec::new()),
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
        // coalesce: do NOT push the deadline forward on every mouse move —
        // otherwise continuous drags would starve the preview refresh
        if self.preview_dirty.is_none() {
            self.preview_dirty = Some(Instant::now() + Duration::from_millis(180));
        }
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

    fn _mod(&self, f: impl FnOnce(egui::Modifiers) -> bool) -> bool {
        self.mod_cache.map(f).unwrap_or(false)
    }

    pub fn shift_down(&self) -> bool { self._mod(|m| m.shift) }
    pub fn ctrl_down(&self) -> bool { self._mod(|m| m.ctrl || m.command) }
    pub fn alt_down(&self) -> bool { self._mod(|m| m.alt) }

    pub fn asset_kind_matches_track(&self, asset_id: u64, kind: TrackKind) -> bool {
        let Some(a) = self.assets.iter().find(|a| a.id == asset_id) else { return false };
        match kind {
            TrackKind::Video => a.kind != crate::model::AssetKind::Audio,
            TrackKind::Audio => a.kind == crate::model::AssetKind::Audio,
        }
    }
    pub fn asset_kind_dur(&self, asset_id: u64) -> (crate::model::AssetKind, f64) {
        self.assets.iter().find(|a| a.id == asset_id)
            .map(|a| (a.kind, a.duration)).unwrap_or((crate::model::AssetKind::Video, 4.0))
    }
    pub fn asset_label(&self, asset_id: u64) -> String {
        self.assets.iter().find(|a| a.id == asset_id).map(|a| a.label()).unwrap_or_default()
    }

    /// Place an asset at an explicit timeline time (drag&drop target).
    pub fn add_asset_to_timeline_at(&mut self, asset_id: u64, at: f64) {
        let saved = self.player.clock;
        self.player.clock = at.max(0.0);
        self.add_asset_to_timeline(asset_id);
        self.player.clock = saved;
    }

    /// Commit a media-pool drop at `pt` onto the compatible track under it.
    /// Places ON the hovered track (magnetic push on overlap) — never refuses.
    pub fn drop_media(&mut self, canvas: Rect, rows: &[(u64, TrackKind)], pt: Option<Pos2>, asset_id: u64, t0: f64, zoom: f32) {
        let Some(pt) = pt else { return };
        let Some(a) = self.assets.iter().find(|a| a.id == asset_id).cloned() else { return };
        let want_video = a.kind != crate::model::AssetKind::Audio;
        let mut ry = canvas.top() + 22.0;
        for (tid, tkind) in rows {
            let h = match tkind { TrackKind::Video => self.track_h_video, TrackKind::Audio => self.track_h_audio };
            if pt.y >= ry && pt.y < ry + h {
                let ok = match tkind { TrackKind::Video => want_video, TrackKind::Audio => !want_video };
                if !ok { return; }
                let t = t0 + ((pt.x - canvas.left()) / zoom).max(0.0) as f64;
                self.commit();
                match a.kind {
                    crate::model::AssetKind::Video => {
                        let vclip = crate::model::clip_from_asset(&a, t, None);
                        let vid = vclip.id;
                        if a.has_audio {
                            let mut aclip = crate::model::clip_from_asset(&a, t, Some(vid));
                            aclip.kind = crate::model::ClipKind::Audio;
                            let aid = aclip.id;
                            let mut vclip = vclip;
                            vclip.link = Some(aid);
                            self.project.place_clip(vclip, *tid);
                            if let Some(at) = self.project.audio_tracks().first().map(|tr| tr.id) {
                                self.project.place_clip(aclip, at);
                            }
                        } else {
                            self.project.place_clip(vclip, *tid);
                        }
                        self.sel = Some(vid);
                    }
                    crate::model::AssetKind::Image => {
                        let c = crate::model::clip_from_asset(&a, t, None);
                        self.project.place_clip(c, *tid);
                    }
                    crate::model::AssetKind::Audio => {
                        let c = crate::model::clip_from_asset(&a, t, None);
                        self.project.place_clip(c, *tid);
                    }
                }
                self.commit();
                self.ensure_wave_for_all();
                self.invalidate_preview();
                self.toast(format!("✓ {}", a.label()), 1);
                return;
            }
            ry += h;
        }
    }

    /// Adjustment layer on the topmost free video track at the playhead.
    pub fn add_adjustment_at_playhead(&mut self) {
        let t = self.player.clock;
        let track = self.project.video_tracks().iter().rev()
            .find(|tr| !tr.locked && tr.clips.iter().all(|c| t < c.tl_start || t >= c.end()))
            .map(|tr| tr.id);
        if let Some(tid) = track {
            self.commit();
            let c = crate::model::adjustment_clip(t, 4.0);
            self.sel = Some(c.id);
            self.project.place_clip(c, tid);
            self.commit();
            self.invalidate_preview();
            self.toast("✓ Adjustment layer", 1);
        }
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
    /// Ids the current edit applies to: selection + group members.
    pub fn edit_targets(&self) -> Vec<u64> {
        let mut ids = self.sel_multi.clone();
        if let Some(s) = self.sel {
            if !ids.contains(&s) { ids.push(s); }
            if let Some((_, c)) = self.project.clip(s) {
                if let Some(g) = c.group {
                    for m in self.project.group_members(g) {
                        if !ids.contains(&m) { ids.push(m); }
                    }
                }
            }
        }
        ids
    }

    pub fn copy_selection(&mut self) {
        let ids = self.edit_targets();
        self.clipboard = ids.iter().filter_map(|id| self.project.clip(*id).map(|(_, c)| c.clone())).collect();
        if !self.clipboard.is_empty() {
            self.toast(format!("✓ {} clip(s) copied", self.clipboard.len()), 1);
        }
    }

    pub fn cut_selection(&mut self) {
        self.copy_selection();
        self.delete_selection(false);
    }

    pub fn paste_at_playhead(&mut self) {
        if self.clipboard.is_empty() { return; }
        self.commit();
        let t0 = self.player.clock;
        let min_start = self.clipboard.iter().map(|c| c.tl_start).fold(f64::INFINITY, f64::min);
        for c in &self.clipboard {
            let mut nc = c.clone();
            nc.id = crate::model::next_id();
            nc.group = None;
            nc.link = None;
            nc.tl_start = t0 + (c.tl_start - min_start);
            nc.vol_kf = c.vol_kf.clone();
            let kind = nc.kind;
            let tid = self.project.tracks.iter()
                .find(|tr| tr.kind == if kind == crate::model::ClipKind::Audio { TrackKind::Audio } else { TrackKind::Video })
                .map(|tr| tr.id);
            if let Some(tid) = tid {
                self.project.place_clip(nc, tid);
            }
        }
        self.commit();
        self.invalidate_preview();
        self.toast(format!("✓ {} clip(s) pasted", self.clipboard.len()), 1);
    }

    pub fn group_selection(&mut self) {
        let ids = self.sel_multi.clone();
        let mut all = ids.clone();
        if let Some(s) = self.sel { if !all.contains(&s) { all.push(s); } }
        if all.len() < 2 { self.toast("Select several clips to group (Shift+Click)", 0); return; }
        self.commit();
        let g = self.project.group_clips(&all);
        self.toast(format!("✓ grouped {} clips", self.project.group_members(g).len()), 1);
    }

    pub fn ungroup_selection(&mut self) {
        let ids = self.edit_targets();
        if ids.is_empty() { return; }
        self.commit();
        self.project.ungroup_clips(&ids);
        self.toast(self.t(K::MsgUngrouped), 1);
    }

    /// Freeze Frame: snapshot the composited frame at the playhead into an
    /// Image clip placed right after the playhead (real PNG → real clip).
    pub fn freeze_frame_at_playhead(&mut self) {
        let Some((w, h, buf)) = self.player.last_frame_for_scopes.clone() else {
            self.toast(self.t(K::MsgNoFrame), 2);
            return;
        };
        let dir = self.project_dir.join("snapshots");
        let _ = std::fs::create_dir_all(&dir);
        let name = format!("freeze_{}.png", crate::util::timecode(self.player.clock, self.project.fps).replace(':', "-"));
        let path = dir.join(&name);
        if let Err(e) = exporter::save_frame_png(&buf, w, h, &path) {
            self.toast(e, 2);
            return;
        }
        let t = self.player.clock;
        let track = self.project.video_tracks().iter().rev()
            .find(|tr| !tr.locked && tr.clips.iter().all(|c| t < c.tl_start || t >= c.end()))
            .map(|tr| tr.id);
        if let Some(tid) = track {
            self.commit();
            let c = crate::model::still_clip(path, &format!("Freeze {}", name.trim_end_matches(".png").trim_start_matches("freeze_")), t, 2.0);
            self.sel = Some(c.id);
            self.project.place_clip(c, tid);
            self.commit();
            self.invalidate_preview();
            self.toast(self.t(K::MsgFreeze), 1);
        }
    }

    /// Toggle Reverse on the selected clip (+linked peer). Source must be
    /// ≤ 60 s — the ffmpeg reverse filter buffers the segment in RAM.
    pub fn toggle_reverse(&mut self) {
        let Some(id) = self.sel else { return };
        let (rev, len, name, peer) = match self.project.clip(id) {
            Some((_, c)) => (c.reverse, c.src_len(), c.name.clone(), c.link),
            None => return,
        };
        if !rev && len > 60.0 {
            self.toast(format!("{} ({name}: {len:.0}s > 60s)", self.t(K::MsgRevTooLong)), 2);
            return;
        }
        self.commit();
        if let Some(c) = self.project.clip_mut(id) { c.reverse = !c.reverse; }
        if let Some(p) = peer {
            if let Some(c) = self.project.clip_mut(p) { c.reverse = !c.reverse; }
        }
        self.commit();
        self.invalidate_preview();
        self.toast(if !rev { "⏪ reverse ON" } else { "▶ reverse OFF" }, 1);
    }

    /// Auto-Duck: dip the volume of every OTHER audio clip overlapping the
    /// selected (voice) clip wherever the voice waveform is loud.
    pub fn auto_duck_under_selection(&mut self) {
        let Some(vid) = self.sel else { return };
        let Some((vtr, vc)) = self.project.clip(vid) else { return };
        if !vc.is_audio() { self.toast(self.t(K::MsgDuckNeedVoice), 0); return; }
        let (vtr_id, vc) = (vtr.id, vc.clone());
        let Some(peaks) = vc.source.as_ref().and_then(|p| self.waves.get(p)).cloned() else {
            self.toast(self.t(K::MsgNoWave), 2);
            return;
        };
        if peaks.is_empty() { self.toast(self.t(K::MsgNoWave), 2); return; }
        const PEAK_RATE: f64 = 50.0;
        let mut regions: Vec<(f64, f64)> = Vec::new();
        let mut open: Option<f64> = None;
        let win = (vc.src_dur * PEAK_RATE) as usize;
        for i in 0..win {
            let src_i = ((vc.src_in + i as f64 / PEAK_RATE) * PEAK_RATE) as usize;
            let loud = peaks.get(src_i).map(|&(mn, mx)| mx.max(-mn) > 45).unwrap_or(false);
            let tl = vc.tl_start + i as f64 / PEAK_RATE;
            if loud && open.is_none() { open = Some(tl); }
            if !loud {
                if let Some(o) = open.take() {
                    if tl - o > 0.15 { regions.push((o, tl)); }
                }
            }
        }
        if let Some(o) = open.take() { regions.push((o, vc.end())); }
        if regions.is_empty() { self.toast(self.t(K::MsgDuckQuiet), 0); return; }
        self.commit();
        let mut dipped = 0;
        let others: Vec<crate::model::Clip> = self.project.tracks.iter()
            .filter(|tr| tr.kind == TrackKind::Audio && tr.id != vtr_id)
            .flat_map(|tr| tr.clips.iter().filter(|c| c.tl_start < vc.end() && c.end() > vc.tl_start).cloned())
            .collect();
        for c in others {
            let mut kfs: Vec<(f64, f32)> = Vec::new();
            for (a, b) in &regions {
                let (a, b) = (*a - c.tl_start, *b - c.tl_start);
                let (a, b) = (a.max(0.02), b.min(c.src_dur - 0.02));
                if b <= a { continue; }
                kfs.push((a - 0.1, 1.0));
                kfs.push((a + 0.12, 0.28));
                kfs.push((b - 0.12, 0.28));
                kfs.push((b + 0.1, 1.0));
            }
            if kfs.is_empty() { continue; }
            dipped += 1;
            if let Some(cc) = self.project.clip_mut(c.id) {
                cc.vol_kf = kfs;
            }
        }
        self.commit();
        self.toast(format!("✓ ducked {dipped} clip(s) under voice"), 1);
    }

    /// Beat detection (approximate, energy-onset based) → timeline markers.
    pub fn detect_beats(&mut self) {
        let Some(id) = self.sel else { self.toast(self.t(K::MsgPickAudio), 0); return; };
        let Some((_, c)) = self.project.clip(id) else { return };
        if !c.is_audio() { self.toast(self.t(K::MsgPickAudio), 0); return; }
        let Some(peaks) = c.source.as_ref().and_then(|p| self.waves.get(p)).cloned() else { return };
        if peaks.is_empty() { self.toast(self.t(K::MsgNoWave), 2); return; }
        const PEAK_RATE: f64 = 50.0;
        let start_i = (c.src_in * PEAK_RATE) as usize;
        let count = (c.src_len() * PEAK_RATE) as usize;
        let mut marks: Vec<f64> = Vec::new();
        let mut run_avg = 10.0f32;
        let mut last_t = -1.0f64;
        for i in 0..count {
            let e = peaks.get(start_i + i).map(|&(mn, mx)| mx.max(-mn) as f32).unwrap_or(0.0);
            let t_src = i as f64 / PEAK_RATE;
            let t_tl = c.tl_start + if c.reverse { c.src_len() - t_src } else { t_src } / c.speed as f64;
            if e > run_avg * 1.55 && e > 40.0 && t_tl - last_t > 0.22 {
                marks.push(t_tl);
                last_t = t_tl;
            }
            run_avg = run_avg * 0.96 + e * 0.04;
        }
        marks.truncate(300);
        let n = marks.len();
        if n == 0 { self.toast(self.t(K::MsgNoBeats), 0); return; }
        self.commit();
        for m in marks { self.project.markers.push((m, "♪".into())); }
        self.commit();
        self.toast(format!("✓ {} beats → markers", n), 1);
    }

    /// Import an .srt subtitle file as Title clips on the top video track.
    pub fn import_srt(&mut self, path: PathBuf) {
        match crate::subs::parse_srt_file(&path) {
            Ok(cues) if !cues.is_empty() => {
                self.commit();
                let track = self.project.video_tracks().last().map(|t| t.id);
                if let Some(tid) = track {
                    for (a, b, text) in &cues {
                        let mut c = crate::model::title_clip(text, *a, (b - a).max(0.3));
                        if let Some(td) = c.title.as_mut() {
                            *td = crate::model::TitleData::preset(3, text);
                        }
                        c.name = format!("SUB {}", text.chars().take(14).collect::<String>());
                        self.project.place_clip(c, tid);
                    }
                    self.commit();
                    self.invalidate_preview();
                    self.toast(format!("✓ {} subtitles imported", cues.len()), 1);
                }
            }
            Ok(_) => self.toast("empty SRT", 2),
            Err(e) => self.toast(format!("srt: {e}"), 2),
        }
    }

    /// Set/replace the transition INTO the selected clip (clamped to the
    /// available source room on the left clip).
    pub fn set_transition_on_selection(&mut self, kind: crate::model::TransKind, dur: f64) {
        let Some(id) = self.sel else { return };
        let (left, room) = {
            let (lid, _) = self.project.neighbors(id);
            let room = lid.and_then(|l| self.project.clip(l).and_then(|(_, lc)| {
                match lc.source.clone() {
                    Some(p) => self.assets.iter().find(|a| a.path == p)
                        .map(|a| ((a.duration - lc.src_end()) / lc.speed as f64).max(0.0)),
                    None => Some(f64::INFINITY),
                }
            })).unwrap_or(0.0);
            (lid, room)
        };
        if left.is_none() { self.toast("no previous clip on this track", 0); return; }
        let clip_dur = self.project.clip(id).map(|(_, c)| c.src_dur).unwrap_or(0.0);
        let d = dur.clamp(0.1, room.min(clip_dur - 0.1).min(4.0).max(0.1));
        self.commit();
        if let Some(c) = self.project.clip_mut(id) {
            c.trans_in = Some(crate::model::Transition { kind, dur: d });
        }
        self.commit();
        self.invalidate_preview();
        self.toast(format!("✓ {} {d:.2}s", self.t(K::AddTransition)), 1);
    }

    pub fn remove_transition(&mut self) {
        let Some(id) = self.sel else { return };
        self.commit();
        if let Some(c) = self.project.clip_mut(id) { c.trans_in = None; }
        self.commit();
        self.invalidate_preview();
    }

    pub fn selection_transition(&self) -> Option<crate::model::Transition> {
        self.sel.and_then(|id| self.project.clip(id)).and_then(|(_, c)| c.trans_in)
    }

    /// White-balance: sample a neutral gray point from the composited frame.
    pub fn wb_pick_at(&mut self, u: f32, v: f32) {
        let Some((w, h, buf)) = self.player.last_frame_for_scopes.clone() else { return };
        let x = ((u * w as f32) as usize).min(w as usize - 1);
        let y = ((v * h as f32) as usize).min(h as usize - 1);
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
        let temp = ((r - lum) / 255.0 * 300.0 - (b - lum) / 255.0 * 900.0).clamp(-100.0, 100.0);
        let tint = (-(g - lum) / 255.0 * 500.0).clamp(-100.0, 100.0);
        self.set_grade_of_selection(|gr| { gr.temp = temp; gr.tint = tint; });
        self.toast(format!("{}  temp {temp:+.0}  tint {tint:+.0}", self.t(K::WhiteBalance)), 1);
    }

    /// Two-pass vidstab stabilization into a stabilized intermediate.
    pub fn stabilize_selected(&mut self) {
        let Some(id) = self.sel else { return };
        let Some((_, c)) = self.project.clip(id) else { return };
        let Some(src) = c.source.clone() else { return };
        if !media::has_stabilizer() {
            self.toast(self.t(K::StabUnavailable), 2);
            return;
        }
        let dest = self.project_dir.join("proxies")
            .join(format!("{}_stab.mp4", src.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()));
        let job = crate::model::next_id();
        self.proxy_jobs.insert(job, self.assets.iter().find(|a| a.path == src).map(|a| a.id).unwrap_or(0));
        media::spawn_stabilize(job, src.clone(), dest, self.ev_tx.clone());
        self.toast("Stabilizing… (vidstab two-pass)", 0);
    }

    // ---- keyframe helpers ------------------------------------------------
    /// Add a keyframe on `chan` for the selected clip at the playhead.
    pub fn add_keyframe(&mut self, chan: u8) {
        let Some(id) = self.sel else { return };
        let Some((_, c)) = self.project.clip(id) else { return };
        let rel = (self.player.clock - c.tl_start).max(0.0).min(c.src_dur);
        let base = c.transform;
        let v = match chan {
            0 => base.x, 1 => base.y, 2 => base.scale, 3 => base.rotation, _ => base.opacity,
        };
        self.commit();
        if let Some(c) = self.project.clip_mut(id) {
            let ch: &mut Vec<(f64, f32, crate::model::Ease)> = match chan {
                0 => &mut c.anim.pos_x, 1 => &mut c.anim.pos_y, 2 => &mut c.anim.scale,
                3 => &mut c.anim.rotation, _ => &mut c.anim.opacity,
            };
            ch.retain(|(t, _, _)| (t - rel).abs() > 0.02);
            ch.push((rel, v, crate::model::Ease::Linear));
            ch.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        }
        self.commit();
        self.invalidate_preview();
    }

    pub fn clear_keyframes(&mut self, chan: u8) {
        let Some(id) = self.sel else { return };
        self.commit();
        if let Some(c) = self.project.clip_mut(id) {
            match chan {
                0 => c.anim.pos_x.clear(), 1 => c.anim.pos_y.clear(), 2 => c.anim.scale.clear(),
                3 => c.anim.rotation.clear(), _ => c.anim.opacity.clear(),
            }
        }
        self.commit();
        self.invalidate_preview();
    }

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
        let ids = self.edit_targets();
        if ids.is_empty() { return; }
        self.commit();
        let first = ids[0];
        for id in ids {
            self.project.delete_clip(id, ripple && id == first);
        }
        self.sel = None;
        self.sel_multi.clear();
        self.commit();
        self.invalidate_preview();
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
    /// Playback engine (v0.3 streaming design).
    ///
    /// Every active clip owns ONE streaming decoder for its whole lifetime —
    /// there is no Still/Run mode switching and no per-bucket restarts:
    ///   • Pacing comes from backpressure: the decoder thread blocks on the
    ///     bounded (4-frame) channel whenever the UI stops draining.
    ///   • Playing: the wall clock advances the target position; each tick
    ///     drains frames up to the target (a few per tick).
    ///   • Scrubbing/paused: same path — forward seeks drain (skip) frames
    ///     at decode speed; the shown frame never goes black.
    ///   • Backward seeks past 0.7 s (and clip/filter changes) restart the
    ///     process at the new position — the previous frame stays visible
    ///     until the fresh one arrives.
    ///   • Reverse clips: one-frame grabs per (throttled) frame bucket.
    ///   • Transition windows decode BOTH the outgoing and incoming clip.
    /// Decode failures surface in the preview overlay instead of blackness.
    pub fn update_player(&mut self, _ctx: &egui::Context) {
        let fps = self.project.fps;
        // per-pixel filters (geq masks, glow…) cost ~10× in the decoder —
        // drop preview quality automatically so scrubbing stays responsive
        let heavy = self.project.tracks.iter().flat_map(|t| &t.clips).any(|c|
            (c.fx.mask.is_active() || c.fx.glow > 0.5 || c.fx.denoise > 50.0)
            && self.player.clock >= c.tl_start && self.player.clock < c.end());
        let base_q = self.player.quality.unwrap_or(Quality::Half);
        let q = if heavy { Quality::Quarter } else { base_q };
        let gen = self.player.seek_gen;
        let playing = self.player.playing;
        if let Some(ts) = self.preview_dirty {
            if Instant::now() >= ts { self.preview_dirty = None; self.apply_preview_dirty(); }
        }

        struct Want {
            clip_id: u64,
            key: u64,
            src_t: f64,
            req: Option<crate::decoder::DecoderReq>,
            reverse: bool,
        }
        let mut wants: Vec<Want> = Vec::new();
        let t = self.player.clock;
        let bucket_now = bucket(t, fps);
        let aspect = self.project.width as f32 / self.project.height.max(1) as f32;
        let mut ph = (self.project.height as f32 * q.factor()).clamp(90.0, 720.0);
        let mut pw = ph * aspect;
        if pw > 1280.0 { pw = 1280.0; ph = pw / aspect; }
        let (pw, ph) = ((pw as u32) & !1, (ph as u32) & !1);

        // ---- collect active clips (video tracks, bottom-up storage order) --
        for tr in self.project.tracks.iter().filter(|tr| tr.kind == TrackKind::Video && !tr.hidden) {
            let Some(c) = tr.clips.iter().find(|c| t >= c.tl_start && t < c.end()) else { continue };
            if c.kind != crate::model::ClipKind::Video { continue; }
            let src_t = c.src_t_at(t);
            let src_path = c.source.clone().unwrap_or_default();
            let filters = media::video_filter_chain(&c.grade, &c.fx, c.src_dur, Some(pw), Some(ph), Some(fps));
            let stream_rate = (fps * (c.speed as f64).max(0.01)).max(1.0);
            let reverse = c.reverse;
            let key = hash_key(&[c.id, fnv(&filters), q as u64]);
            wants.push(Want {
                clip_id: c.id, key, src_t, reverse,
                req: Some(crate::decoder::DecoderReq {
                    path: src_path, src_in: src_t.max(0.0), filters, w: pw, h: ph,
                    fps: stream_rate, mode: if reverse { crate::decoder::DecodeMode::Still } else { crate::decoder::DecodeMode::Stream },
                }),
            });
            // ---- transition window: also decode the OUTGOING left clip ----
            if let (Some(trans), Some(left_id)) = (c.trans_in, self.project.seam_for(c.id)) {
                let tw0 = c.tl_start;
                let tw1 = c.tl_start + trans.dur.min(c.src_dur);
                if t >= tw0 && t < tw1 {
                    if let Some((_, l)) = self.project.clip(left_id) {
                        if l.is_visual() && l.kind == crate::model::ClipKind::Video {
                            // left keeps playing past its cut (needs source room)
                            let rel = t - tw0;
                            let l_src_t = l.src_end() + rel * l.speed as f64;
                            let l_filters = media::video_filter_chain(&l.grade, &l.fx, l.src_dur, Some(pw), Some(ph), Some(fps));
                            let l_key = hash_key(&[l.id, fnv(&l_filters), q as u64]);
                            wants.push(Want {
                                clip_id: l.id, key: l_key, src_t: l_src_t, reverse: false,
                                req: Some(crate::decoder::DecoderReq {
                                    path: l.source.clone().unwrap_or_default(), src_in: l_src_t.max(0.0),
                                    filters: l_filters, w: pw, h: ph,
                                    fps: (fps * (l.speed as f64).max(0.01)).max(1.0),
                                    mode: crate::decoder::DecodeMode::Stream,
                                }),
                            });
                        }
                    }
                }
            }
        }

        let want_clips: Vec<u64> = wants.iter().map(|w| w.clip_id).collect();
        // Freeze at the end: paused at/after the sequence end with no active
        // clip — hold the last decoded frame instead of going black.
        let at_end = t >= self.project.duration() - 1e-6;
        let freeze_end = !playing && at_end && want_clips.is_empty() && !self.player.slots.is_empty();
        if !freeze_end {
            self.player.slots.retain(|s| want_clips.contains(&s.clip_id));

            for w in &wants {
                let existing = self.player.slots.iter().find(|s| s.clip_id == w.clip_id);
                let mut need_restart = match existing {
                    None => true,
                    Some(s) => s.key != w.key,
                };
                // reverse clips: regrab on (throttled) bucket change
                if !need_restart && w.reverse {
                    if let Some(s) = existing {
                        need_restart = s.rev_bucket != bucket_now / 3 || (!s.frame_current && s.eof);
                    }
                }
                // stream slots: restart only on backward jumps / stalls
                if !need_restart && !w.reverse {
                    if let Some(s) = existing {
                        let backward = match (s.dec.as_ref().and_then(|d| d.head_pts()), s.frame.as_ref()) {
                            // compare against the position we actually display
                            (_, Some(f)) => {
                                let shown_src = s.dec_origin + f.pts;
                                w.src_t < shown_src - 0.7
                            }
                            _ => false,
                        };
                        let stalled = playing && !s.eof && match s.last_frame_at {
                            Some(lf) => lf.elapsed() > Duration::from_secs(2),
                            None => s.started_at.elapsed() > Duration::from_secs(4),
                        };
                        need_restart = backward || stalled;
                    }
                }
                if need_restart {
                    // fresh decoder starts 0.2 s before the target so the
                    // drain always has a frame at/before the wanted position
                    let start_at = (w.src_t - 0.2).max(0.0);
                    match w.req.clone().map(|mut r| { r.src_in = start_at; crate::decoder::Decoder::start(r) }) {
                        Some(Ok(d)) => {
                            let slot = self.player.slots.iter_mut().find(|s| s.clip_id == w.clip_id);
                            match slot {
                                Some(s) => {
                                    s.dec = Some(d);
                                    s.decode_error = None;
                                    s.key = w.key;
                                    s.dec_origin = start_at;
                                    s.eof = false;
                                    s.frame_current = false; // old frame is from the old gen
                                    s.last_frame_at = None;
                                    s.started_at = Instant::now();
                                    s.rev_bucket = bucket_now / 3;
                                }
                                None => {
                                    self.player.slots.push(crate::player::Slot {
                                        clip_id: w.clip_id, key: w.key, dec: Some(d), frame: None,
                                        dec_origin: start_at, eof: false, decode_error: None,
                                        frame_current: false, last_frame_at: None,
                                        started_at: Instant::now(), rev_bucket: bucket_now / 3,
                                    });
                                }
                            }
                        }
                        Some(Err(e)) => {
                            let slot = self.player.slots.iter_mut().find(|s| s.clip_id == w.clip_id);
                            match slot {
                                Some(s) => { s.dec = None; s.decode_error = Some(e); s.key = w.key; s.dec_origin = start_at; }
                                None => {
                                    self.player.slots.push(crate::player::Slot {
                                        clip_id: w.clip_id, key: w.key, dec: None, frame: None,
                                        dec_origin: start_at, eof: true, decode_error: Some(e),
                                        frame_current: false, last_frame_at: None,
                                        started_at: Instant::now(), rev_bucket: bucket_now / 3,
                                    });
                                }
                            }
                        }
                        None => {}
                    }
                    if std::env::var("KC_TRACE").is_ok() {
                        eprintln!("[trace] decoder start clip={} src_t={:.3} reverse={} key={:x} gen={}",
                            w.clip_id, w.src_t, w.reverse, w.key, gen);
                    }
                }
            }
        } // !freeze_end

        // ---- drain every live decoder toward the wanted positions --------
        for s in self.player.slots.iter_mut() {
            let want_src = wants.iter().find(|w| w.clip_id == s.clip_id).map(|w| w.src_t);
            let Some(dec) = s.dec.as_mut() else { continue };
            let until = want_src.map(|st| (st - s.dec_origin).max(0.0));
            let before_head = dec.head_pts();
            if let Some(f) = dec.poll(until.filter(|_| !s.eof || true)) {
                let is_new = before_head.map(|h| f.pts > h).unwrap_or(true);
                if is_new || !s.frame_current {
                    s.frame = Some(f);
                    s.frame_current = true;
                    s.decode_error = None;
                    if is_new { s.last_frame_at = Some(Instant::now()); }
                }
            }
            if let Some(e) = &dec.last_error { s.decode_error = Some(e.clone()); }
            if dec.is_eof() { s.eof = true; }
            // playing past EOF: hold the last frame (no restart churn)
            let _ = want_src;
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
        let mut chosen: Option<(u64, PathBuf, f64, f64, String)> = None;
        for tr in self.project.audio_tracks() {
            if tr.mute || (any_solo && !tr.solo) { continue; }
            if let Some(c) = tr.clips.iter().find(|c| t >= c.tl_start && t < c.end()) {
                if let Some(src) = c.source.clone() {
                    let src_in = c.src_in + (t - c.tl_start) * c.speed as f64;
                    let dur = (c.end() - t) * c.speed as f64;
                    // per-clip processing chain — identical to the export mix
                    let chain = media::audio_filter_chain(&c.afx, c.gain_db, c.fx.fade_in, c.fx.fade_out, c.src_dur);
                    chosen = Some((c.id, src, src_in, dur, chain));
                }
            }
        }
        match chosen {
            Some((cid, src, src_in, dur, chain)) if self.player.audio_clip != Some(cid) => {
                self.player.audio = crate::decoder::audio::Monitor::start(src, src_in, dur.max(0.2), &chain);
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
            else if i.key_pressed(Key::C) && m.ctrl { act = Some(26); }
            else if i.key_pressed(Key::X) && m.ctrl { act = Some(27); }
            else if i.key_pressed(Key::V) && m.ctrl { act = Some(28); }
            else if i.key_pressed(Key::G) && m.ctrl && !m.shift { act = Some(29); }
            else if i.key_pressed(Key::G) && m.ctrl && m.shift { act = Some(30); }
            else if i.key_pressed(Key::M) && !m.ctrl { act = Some(31); }
            else if i.key_pressed(Key::F) && !m.ctrl { act = Some(32); }
            else if i.key_pressed(Key::R) && !m.ctrl { act = Some(33); }
            else if i.key_pressed(Key::S) && m.ctrl { act = Some(34); }
            else if i.key_pressed(Key::J) && !m.ctrl { act = Some(35); }
            else if i.key_pressed(Key::U) && !m.ctrl { act = Some(36); }
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
            Some(25) => { self.sel = None; self.sel_multi.clear(); }
            Some(26) => self.copy_selection(),
            Some(27) => self.cut_selection(),
            Some(28) => self.paste_at_playhead(),
            Some(29) => self.group_selection(),
            Some(30) => self.ungroup_selection(),
            Some(31) => { self.project.add_marker(self.player.clock); self.commit(); }
            Some(32) => self.freeze_frame_at_playhead(),
            Some(33) => self.toggle_reverse(),
            Some(34) => self.save_project(self.project_dir.join(format!("{}.kcproj", self.project.name))),
            Some(35) => self.tool = Tool::Slide,
            Some(36) => self.tool = Tool::Roll,
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

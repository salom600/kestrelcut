//! Project model: media assets, tracks, clips, edits (split/trim/slide/snap),
//! magnetic timeline logic and undo history.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ------------------------------------------------------------------ ids
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
pub fn next_id() -> u64 { NEXT_ID.fetch_add(1, Ordering::Relaxed) }

// ------------------------------------------------------------------ assets
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq)]
pub enum AssetKind { Video, Audio, Image }

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct MediaAsset {
    pub id: u64,
    pub path: PathBuf,
    pub kind: AssetKind,
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub codec: String,
    pub has_audio: bool,
    pub audio_codec: Option<String>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u16>,
    pub proxy: Option<PathBuf>,
    pub size: u64,
}

impl MediaAsset {
    pub fn label(&self) -> String {
        self.path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
    }
    pub fn is_video(&self) -> bool { self.kind == AssetKind::Video }
}

// ------------------------------------------------------------------ clip data
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq)]
pub struct Transform {
    pub x: f32,        // -1..1 fraction of frame width
    pub y: f32,
    pub scale: f32,    // 1 = fit
    pub rotation: f32, // degrees
    pub opacity: f32,  // 0..1
}
impl Default for Transform {
    fn default() -> Self { Self { x: 0.0, y: 0.0, scale: 1.0, rotation: 0.0, opacity: 1.0 } }
}

#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq)]
pub struct Grade {
    pub temp: f32,
    pub tint: f32,
    pub exposure: f32,
    pub contrast: f32,
    pub saturation: f32,
    pub highlights: f32,
    pub whites: f32,
    pub blacks: f32,
    /// Vibrance -100..100 (saturates low-saturation colors first).
    #[serde(default)]
    pub vibrance: f32,
    /// Lift color wheel (shadows), per-channel -1..1.
    #[serde(default)]
    pub lift: [f32; 3],
    /// Gamma color wheel (midtones), per-channel -1..1.
    #[serde(default)]
    pub gamma: [f32; 3],
    /// Gain color wheel (highlights), per-channel -1..1.
    #[serde(default)]
    pub gain: [f32; 3],
    /// Master offset (brightness) -100..100.
    #[serde(default)]
    pub offset: f32,
}
impl Default for Grade {
    fn default() -> Self {
        Self { temp: 0.0, tint: 0.0, exposure: 0.0, contrast: 0.0, saturation: 0.0,
               highlights: 0.0, whites: 0.0, blacks: 0.0,
               vibrance: 0.0, lift: [0.0; 3], gamma: [0.0; 3], gain: [0.0; 3], offset: 0.0 }
    }
}
impl Grade {
    pub fn is_default(&self) -> bool { *self == Grade::default() }
    /// True when any color wheel deviates from neutral.
    pub fn wheels_active(&self) -> bool {
        self.lift.iter().any(|v| v.abs() > 0.005)
            || self.gamma.iter().any(|v| v.abs() > 0.005)
            || self.gain.iter().any(|v| v.abs() > 0.005)
            || self.offset.abs() > 0.5
            || self.vibrance.abs() > 0.5
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
pub struct Fx {
    pub blur: f32, // px radius
    pub fade_in: f32,
    pub fade_out: f32,
    pub lut: Option<PathBuf>,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct TitleData {
    pub text: String,
    pub size: f32,
    pub color: [u8; 3],
}
impl Default for TitleData {
    fn default() -> Self { Self { text: "Title".into(), size: 72.0, color: [255, 255, 255] } }
}

#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq)]
pub enum ClipKind { Video, Audio, Image, Title }

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Clip {
    pub id: u64,
    pub kind: ClipKind,
    pub name: String,
    pub source: Option<PathBuf>,
    pub src_in: f64,
    pub src_dur: f64, // timeline duration (already / speed)
    pub tl_start: f64,
    pub speed: f32,
    pub transform: Transform,
    pub grade: Grade,
    pub fx: Fx,
    pub gain_db: f32,
    #[serde(default)]
    pub vol_kf: Vec<(f64, f32)>,
    pub title: Option<TitleData>,
    #[serde(default)]
    pub link: Option<u64>,
}

impl Clip {
    pub fn end(&self) -> f64 { self.tl_start + self.src_dur }
    pub fn src_len(&self) -> f64 { self.src_dur * self.speed as f64 }
    pub fn src_end(&self) -> f64 { self.src_in + self.src_len() }
    pub fn is_visual(&self) -> bool { matches!(self.kind, ClipKind::Video | ClipKind::Image | ClipKind::Title) }
    pub fn is_audio(&self) -> bool { self.kind == ClipKind::Audio }
}

pub fn clip_from_asset(a: &MediaAsset, tl_start: f64, link: Option<u64>) -> Clip {
    let (kind, dur) = match a.kind {
        AssetKind::Video => (ClipKind::Video, a.duration),
        AssetKind::Audio => (ClipKind::Audio, a.duration),
        AssetKind::Image => (ClipKind::Image, 4.0),
    };
    Clip {
        id: next_id(),
        kind,
        name: a.label(),
        source: Some(a.path.clone()),
        src_in: 0.0,
        src_dur: dur,
        tl_start,
        speed: 1.0,
        transform: Transform::default(),
        grade: Grade::default(),
        fx: Fx::default(),
        gain_db: 0.0,
        vol_kf: Vec::new(),
        title: None,
        link,
    }
}

pub fn title_clip(text: &str, tl_start: f64, dur: f64) -> Clip {
    Clip {
        id: next_id(),
        kind: ClipKind::Title,
        name: text.chars().take(24).collect(),
        source: None,
        src_in: 0.0,
        src_dur: dur,
        tl_start,
        speed: 1.0,
        transform: Transform::default(),
        grade: Grade::default(),
        fx: Fx::default(),
        gain_db: 0.0,
        vol_kf: Vec::new(),
        title: Some(TitleData { text: text.to_string(), ..Default::default() }),
        link: None,
    }
}

// ------------------------------------------------------------------ tracks
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq)]
pub enum TrackKind { Video, Audio }

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Track {
    pub id: u64,
    pub kind: TrackKind,
    pub name: String,
    pub locked: bool,
    pub hidden: bool, // video
    pub mute: bool,   // audio
    pub solo: bool,   // audio
    pub arm: bool,
    pub clips: Vec<Clip>,
}
impl Track {
    pub fn sorted_clips(&self) -> Vec<&Clip> {
        let mut v: Vec<&Clip> = self.clips.iter().collect();
        v.sort_by(|a, b| a.tl_start.partial_cmp(&b.tl_start).unwrap_or(std::cmp::Ordering::Equal));
        v
    }
}

// ------------------------------------------------------------------ project
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Project {
    pub name: String,
    pub seq_name: String,
    pub fps: f64,
    pub width: u32,
    pub height: u32,
    pub tracks: Vec<Track>,
    pub markers: Vec<(f64, String)>,
    #[serde(default)]
    pub in_mark: Option<f64>,
    #[serde(default)]
    pub out_mark: Option<f64>,
}

impl Default for Project {
    fn default() -> Self {
        let mk = |kind: TrackKind, name: &str| Track {
            id: next_id(), kind, name: name.into(),
            locked: false, hidden: false, mute: false, solo: false, arm: false,
            clips: Vec::new(),
        };
        Self {
            name: "Untitled Project".into(),
            seq_name: "Sequence 01".into(),
            fps: 30.0,
            width: 1920,
            height: 1080,
            tracks: vec![
                mk(TrackKind::Video, "V1"), mk(TrackKind::Video, "V2"), mk(TrackKind::Video, "V3"),
                mk(TrackKind::Audio, "A1"), mk(TrackKind::Audio, "A2"), mk(TrackKind::Audio, "A3"),
            ],
            markers: Vec::new(),
            in_mark: None,
            out_mark: None,
        }
    }
}

pub const MIN_CLIP_DUR: f64 = 0.1;

impl Project {
    pub fn track(&self, id: u64) -> Option<&Track> { self.tracks.iter().find(|t| t.id == id) }
    pub fn track_mut(&mut self, id: u64) -> Option<&mut Track> { self.tracks.iter_mut().find(|t| t.id == id) }

    pub fn video_tracks(&self) -> Vec<&Track> { self.tracks.iter().filter(|t| t.kind == TrackKind::Video).collect() }
    pub fn audio_tracks(&self) -> Vec<&Track> { self.tracks.iter().filter(|t| t.kind == TrackKind::Audio).collect() }

    /// Compositing order bottom -> top (V1 first).
    pub fn video_tracks_bottom_up(&self) -> Vec<&Track> {
        let mut v: Vec<&Track> = self.video_tracks();
        v.reverse();
        v
    }

    pub fn clip(&self, id: u64) -> Option<(&Track, &Clip)> {
        for t in &self.tracks {
            if let Some(c) = t.clips.iter().find(|c| c.id == id) { return Some((t, c)); }
        }
        None
    }
    pub fn clip_mut(&mut self, id: u64) -> Option<&mut Clip> {
        for t in &mut self.tracks {
            if let Some(c) = t.clips.iter_mut().find(|c| c.id == id) { return Some(c); }
        }
        None
    }

    pub fn duration(&self) -> f64 {
        self.tracks.iter().flat_map(|t| &t.clips).map(|c| c.end()).fold(0.0, f64::max)
    }

    /// Clips visible at time `t`, in track order (bottom-up index order as stored).
    pub fn clips_at(&self, t: f64, kind: TrackKind) -> Vec<(usize, &Clip)> {
        let mut out = Vec::new();
        for (i, tr) in self.tracks.iter().enumerate() {
            if tr.kind != kind { continue; }
            if let Some(c) = tr.clips.iter().find(|c| t >= c.tl_start && t < c.end()) {
                out.push((i, c));
            }
        }
        out
    }

    /// Snapping candidates: clip edges, playhead, markers, in/out marks, 0, end.
    pub fn snap_candidates(&self, exclude_clip: u64, playhead: f64) -> Vec<f64> {
        let mut v = vec![0.0, playhead, self.duration()];
        if let Some(i) = self.in_mark { v.push(i); }
        if let Some(o) = self.out_mark { v.push(o); }
        for m in &self.markers { v.push(m.0); }
        for t in &self.tracks {
            for c in &t.clips {
                if c.id == exclude_clip { continue; }
                v.push(c.tl_start);
                v.push(c.end());
            }
        }
        v
    }

    /// Insert a clip, magnetically pushing right on overlap (no overwrite).
    pub fn place_clip(&mut self, mut clip: Clip, track_id: u64) -> u64 {
        let id = clip.id;
        let others: Vec<(f64, f64)> = self.track(track_id)
            .map(|t| t.clips.iter().map(|c| (c.tl_start, c.end())).collect())
            .unwrap_or_default();
        let mut guard = 0;
        while guard < 10_000 {
            guard += 1;
            let end = clip.tl_start + clip.src_dur;
            match others.iter().find(|&&(s, e)| clip.tl_start < e && end > s) {
                Some(&(_s, e)) => clip.tl_start = e,
                None => break,
            }
        }
        if let Some(t) = self.track_mut(track_id) { t.clips.push(clip); }
        id
    }

    /// Split `clip_id` at timeline time `t` (also splits linked peer).
    pub fn split_clip(&mut self, clip_id: u64, t: f64) -> Option<u64> {
        let (track_id, left, link) = {
            let (tr, c) = self.clip(clip_id)?;
            (tr.id, c.clone(), c.link)
        };
        if t <= left.tl_start + MIN_CLIP_DUR || t >= left.end() - MIN_CLIP_DUR { return None; }
        let off = t - left.tl_start;
        let mut right = left.clone();
        right.id = next_id();
        right.tl_start = t;
        right.src_dur = left.src_dur - off;
        right.src_in = left.src_in + off * left.speed as f64;
        right.link = None;
        right.vol_kf = left.vol_kf.iter().filter(|(x, _)| *x >= off)
            .map(|(x, g)| (x - off, *g)).collect();
        let mut left2 = left.clone();
        left2.src_dur = off;
        left2.link = None;
        left2.vol_kf = left.vol_kf.iter().filter(|(x, _)| *x < off)
            .map(|(x, g)| (*x, *g)).collect();
        let right_id = right.id;
        let tr = self.track_mut(track_id)?;
        if let Some(c) = tr.clips.iter_mut().find(|c| c.id == clip_id) { *c = left2; }
        tr.clips.push(right);
        if let Some(peer) = link { self.split_clip(peer, t); }
        Some(right_id)
    }

    /// Move clip to (track, start); linked A/V peer follows.
    pub fn move_clip(&mut self, clip_id: u64, new_track: u64, new_start: f64) {
        let (old_track, mut c) = match self.clip(clip_id) {
            Some((t, c)) => (t.id, c.clone()),
            None => return,
        };
        if old_track == new_track && (new_start - c.tl_start).abs() < 1e-9 { return; }
        let Some(dest) = self.track(new_track) else { return };
        if dest.locked { return; }
        let kind_ok = match dest.kind {
            TrackKind::Video => c.is_visual(),
            TrackKind::Audio => c.is_audio(),
        };
        if !kind_ok { return; }
        let delta = new_start - c.tl_start;
        c.tl_start = new_start.max(0.0);
        if let Some(t) = self.track_mut(old_track) { t.clips.retain(|x| x.id != clip_id); }
        if let Some(t) = self.track_mut(new_track) { t.clips.push(c.clone()); }
        if let Some(peer) = c.link {
            if peer != clip_id {
                let pt = self.clip(peer).map(|(t, _)| t.id);
                if let Some(pt) = pt {
                    if pt != new_track {
                        if let Some(pc) = self.clip_mut(peer) {
                            pc.tl_start = (pc.tl_start + delta).max(0.0);
                        }
                    }
                }
            }
        }
    }

    fn neighbor_bounds(&self, clip_id: u64, tl_start: f64, end: f64) -> (f64, f64) {
        let track_id = self.clip(clip_id).map(|(t, _)| t.id).unwrap_or(0);
        self.track(track_id).map(|t| {
            let le = t.clips.iter().filter(|x| x.id != clip_id && x.end() <= tl_start + 1e-6)
                .map(|x| x.end()).fold(0.0_f64, f64::max);
            let ns = t.clips.iter().filter(|x| x.id != clip_id && x.tl_start >= end - 1e-6)
                .map(|x| x.tl_start).fold(f64::INFINITY, f64::min);
            (le, ns)
        }).unwrap_or((0.0, f64::INFINITY))
    }

    /// Trim left edge by `delta` (+ = start later). Returns applied delta.
    pub fn trim_left(&mut self, clip_id: u64, delta: f64) -> f64 {
        let c = match self.clip(clip_id) { Some((_, c)) => c.clone(), None => return 0.0 };
        let mut d = delta;
        let room_left = if c.source.is_some() { c.src_in / c.speed as f64 } else { f64::INFINITY };
        d = d.max(-room_left);
        d = d.min(c.src_dur - MIN_CLIP_DUR);
        let (le, _ns) = self.neighbor_bounds(clip_id, c.tl_start, c.end());
        if c.tl_start + d < le { d = le - c.tl_start; }
        if let Some(peer) = c.link { self.trim_left(peer, d); }
        if let Some(c) = self.clip_mut(clip_id) {
            c.tl_start += d;
            c.src_in += d * c.speed as f64;
            c.src_dur -= d;
            for kf in c.vol_kf.iter_mut() { kf.0 -= d; }
            c.vol_kf.retain(|(x, _)| *x >= 0.0);
        }
        d
    }

    /// Trim right edge by `delta` (+ = longer). `src_total` = asset duration.
    pub fn trim_right(&mut self, clip_id: u64, delta: f64, src_total: Option<f64>) -> f64 {
        let c = match self.clip(clip_id) { Some((_, c)) => c.clone(), None => return 0.0 };
        let mut d = delta;
        if let Some(total) = src_total {
            let room = (total - c.src_end()) / c.speed as f64;
            d = d.min(room.max(0.0));
        }
        d = d.max(MIN_CLIP_DUR - c.src_dur);
        let (_le, ns) = self.neighbor_bounds(clip_id, c.tl_start, c.end());
        if c.end() + d > ns { d = ns - c.end(); }
        if let Some(peer) = c.link {
            let peer_total = src_total; // peer shares the same asset
            self.trim_right(peer, d, peer_total);
        }
        if let Some(c) = self.clip_mut(clip_id) { c.src_dur += d; }
        d
    }

    /// Slip: shift source in/out without changing timeline position.
    pub fn slip(&mut self, clip_id: u64, delta_src: f64) {
        let c = match self.clip(clip_id) { Some((_, c)) => c.clone(), None => return };
        if c.source.is_none() { return; }
        // total length is injected by the caller through trim clamp here:
        let total = self.src_len_of(clip_id).unwrap_or(c.src_end());
        let new_in = (c.src_in + delta_src).max(0.0).min((total - c.src_len()).max(0.0));
        if let Some(c) = self.clip_mut(clip_id) { c.src_in = new_in; }
    }

    fn src_len_of(&self, _id: u64) -> Option<f64> { None } // UI passes totals via slip_with

    pub fn delete_clip(&mut self, clip_id: u64, ripple: bool) {
        let c = match self.clip(clip_id) { Some((_, c)) => c.clone(), None => return };
        let peer = c.link;
        let (track_id, dur, start) = match self.clip(clip_id) {
            Some((t, _)) => (t.id, c.src_dur, c.tl_start),
            None => return,
        };
        if let Some(t) = self.track_mut(track_id) {
            t.clips.retain(|x| x.id != clip_id);
            if ripple {
                for x in t.clips.iter_mut() {
                    if x.tl_start >= start - 1e-6 { x.tl_start -= dur; }
                }
            }
        }
        if let Some(p) = peer { self.delete_clip(p, false); }
    }

    pub fn add_marker(&mut self, t: f64) { self.markers.push((t, String::new())); }
}

// ------------------------------------------------------------------ history
pub struct History {
    stack: Vec<Project>,
    pos: usize,
    cap: usize,
}

impl History {
    pub fn new(p: Project) -> Self { Self { stack: vec![p], pos: 0, cap: 100 } }
    pub fn current(&self) -> &Project { &self.stack[self.pos] }
    pub fn commit(&mut self, p: Project) {
        self.stack.truncate(self.pos + 1);
        self.stack.push(p);
        if self.stack.len() > self.cap { self.stack.remove(0); }
        self.pos = self.stack.len() - 1;
    }
    pub fn undo(&mut self) -> bool { if self.pos > 0 { self.pos -= 1; true } else { false } }
    pub fn redo(&mut self) -> bool { if self.pos + 1 < self.stack.len() { self.pos += 1; true } else { false } }
    pub fn can_undo(&self) -> bool { self.pos > 0 }
    pub fn can_redo(&self) -> bool { self.pos + 1 < self.stack.len() }
}

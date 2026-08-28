//! Playback transport: clock, in/out loop, speed, decode slots, audio monitor.

use crate::decoder::audio::Monitor;
use crate::decoder::{Decoder, Frame};
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq)]
pub enum Quality { Full, Half, Quarter }
impl Quality {
    pub fn factor(self) -> f32 { match self { Quality::Full => 1.0, Quality::Half => 0.5, Quality::Quarter => 0.25 } }
    pub fn label(self) -> &'static str { match self { Quality::Full => "Full", Quality::Half => "Half", Quality::Quarter => "Quarter" } }
    pub fn all() -> [Quality; 3] { [Quality::Full, Quality::Half, Quality::Quarter] }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Tool { Select, Razor, Slip, Pen, Hand, Zoom, Text }
impl Tool {
    pub fn all() -> [Tool; 7] { [Tool::Select, Tool::Razor, Tool::Slip, Tool::Pen, Tool::Hand, Tool::Zoom, Tool::Text] }
}

/// One active decode slot (per video track).
pub struct Slot {
    pub track_id: u64,
    pub clip_id: u64,
    pub key: u64, // (clip, src-time-bucket, filters, quality)
    pub dec: Option<Decoder>,
    pub frame: Option<Frame>,
    pub eof: bool,
}

#[derive(Default)]
pub struct Player {
    pub playing: bool,
    pub clock: f64,
    pub speed: f32,
    pub loop_play: bool,
    pub quality: Option<Quality>, // None until set by app (defaults Half)
    pub slots: Vec<Slot>,
    pub audio: Option<Monitor>,
    pub audio_clip: Option<u64>,
    pub last_frame_for_scopes: Option<(u32, u32, std::sync::Arc<Vec<u8>>)>,
}

impl Player {
    pub fn new() -> Self {
        Self { playing: false, clock: 0.0, speed: 1.0, loop_play: false, quality: None,
               slots: Vec::new(), audio: None, audio_clip: None, last_frame_for_scopes: None }
    }

    /// Advance the playback clock. Returns true when playback ended.
    pub fn tick(&mut self, dt: f64, seq_dur: f64, in_mark: Option<f64>, out_mark: Option<f64>) -> bool {
        if !self.playing { return false; }
        self.clock += dt * self.speed as f64;
        let end = out_mark.unwrap_or(seq_dur);
        if self.clock >= end {
            if self.loop_play {
                self.clock = in_mark.unwrap_or(0.0).max(if self.clock > end + 30.0 { end } else { in_mark.unwrap_or(0.0) });
            } else {
                self.clock = end.max(0.0);
                self.playing = false;
                return true;
            }
        }
        false
    }

    pub fn toggle_play(&mut self) { self.playing = !self.playing; }
    pub fn pause(&mut self) { self.playing = false; }
    pub fn seek(&mut self, t: f64) { self.clock = t.max(0.0); }

    pub fn slot_for(&mut self, track_id: u64) -> Option<&mut Slot> {
        self.slots.iter_mut().find(|s| s.track_id == track_id)
    }

    pub fn active_frames(&self) -> impl Iterator<Item = (&Slot, &Frame)> {
        self.slots.iter().filter_map(|s| s.frame.as_ref().map(|f| (s, f)))
    }

    /// Memory bookkeeping: drop frame buffers for slots no longer active.
    pub fn gc(&mut self, valid_track_ids: &[u64]) {
        self.slots.retain(|s| valid_track_ids.contains(&s.track_id));
    }

    pub fn fps_effective(&self, project_fps: f64) -> f64 { project_fps * self.speed as f64 }
}

/// Hash helper for decoder restart keys.
pub fn hash_key(vals: &[u64]) -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    // deterministic-enough local key: FNV-1a
    let mut hash: u64 = 0xcbf29ce484222325;
    for v in vals {
        let mut b = *v;
        for _ in 0..8 {
            hash ^= b & 0xff;
            hash = hash.wrapping_mul(0x100000001b3);
            b >>= 8;
        }
    }
    h.write_u64(hash);
    hash
}

/// Frame-time bucket for seek keys.
pub fn bucket(t: f64, fps: f64) -> u64 { (t.max(0.0) * fps.max(1.0)).floor() as u64 }

pub type TexCache = HashMap<u64, egui::TextureHandle>;

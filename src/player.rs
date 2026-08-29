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
pub enum Tool { Select, Razor, Slip, Pen, Hand, Zoom, Text, Roll, Slide }
impl Tool {
    pub fn all() -> [Tool; 9] { [Tool::Select, Tool::Razor, Tool::Roll, Tool::Slide, Tool::Slip, Tool::Pen, Tool::Hand, Tool::Zoom, Tool::Text] }
}

/// One active decode slot, keyed by CLIP id (a transition window keeps two
/// live slots on the same track — one for the outgoing clip, one incoming).
pub struct Slot {
    pub clip_id: u64,
    /// (clip, filters, quality) hash — changes restart the decoder.
    pub key: u64,
    pub dec: Option<Decoder>,
    /// Last frame chosen for display.
    pub frame: Option<Frame>,
    /// Source position the current decoder's pts 0 corresponds to.
    pub dec_origin: f64,
    pub eof: bool,
    /// Last decode failure message (shown in the preview overlay).
    pub decode_error: Option<String>,
    /// True when `frame` came from the CURRENT decoder generation.
    pub frame_current: bool,
    /// Wall time of the last new frame from the decoder (stall detection).
    pub last_frame_at: Option<std::time::Instant>,
    /// Wall time when the current decoder was started.
    pub started_at: std::time::Instant,
    /// Reverse-clip grab bucket (throttled still mode).
    pub rev_bucket: u64,
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
    /// Bumped on every seek; lets decoders detect user seeks.
    pub seek_gen: u64,
    /// True once the engine produced at least one preview frame.
    pub ever_had_frame: bool,
}

impl Player {
    pub fn new() -> Self {
        Self { playing: false, clock: 0.0, speed: 1.0, loop_play: false, quality: None,
               slots: Vec::new(), audio: None, audio_clip: None, last_frame_for_scopes: None,
               seek_gen: 0, ever_had_frame: false }
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
    pub fn seek(&mut self, t: f64) {
        let nt = t.max(0.0);
        if nt != self.clock { self.seek_gen = self.seek_gen.wrapping_add(1); }
        self.clock = nt;
    }

    pub fn slot_for_clip_mut(&mut self, clip_id: u64) -> Option<&mut Slot> {
        self.slots.iter_mut().find(|s| s.clip_id == clip_id)
    }

    pub fn active_frames(&self) -> impl Iterator<Item = (&Slot, &Frame)> {
        self.slots.iter().filter_map(|s| s.frame.as_ref().map(|f| (s, f)))
    }
}

/// Hash helper for decoder restart keys.
pub fn hash_key(vals: &[u64]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for v in vals {
        let mut b = *v;
        for _ in 0..8 {
            hash ^= b & 0xff;
            hash = hash.wrapping_mul(0x100000001b3);
            b >>= 8;
        }
    }
    hash
}

/// Frame-time bucket for seek keys / reverse-clip throttling.
pub fn bucket(t: f64, fps: f64) -> u64 { (t.max(0.0) * fps.max(1.0)).floor() as u64 }

pub type TexCache = HashMap<u64, egui::TextureHandle>;

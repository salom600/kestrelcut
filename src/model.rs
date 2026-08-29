//! Project model: media assets, tracks, clips, edits (split/trim/slide/snap),
//! magnetic timeline logic and undo history.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ------------------------------------------------------------------ ids
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
pub fn next_id() -> u64 { NEXT_ID.fetch_add(1, Ordering::Relaxed) }

// ------------------------------------------------------------------ compositing
/// Layer blend mode (Resolve/FCP "Compositing" inspector). `Normal` is plain
/// alpha compositing; the rest map to real ffmpeg `blend` modes at export and
/// to the software compositor in the preview.
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq, Default)]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    SoftLight,
    HardLight,
    Darken,
    Lighten,
    Difference,
}
impl BlendMode {
    pub const ALL: [BlendMode; 9] = [
        BlendMode::Normal, BlendMode::Multiply, BlendMode::Screen, BlendMode::Overlay,
        BlendMode::SoftLight, BlendMode::HardLight, BlendMode::Darken, BlendMode::Lighten,
        BlendMode::Difference,
    ];
    pub fn label(&self) -> &'static str {
        match self {
            BlendMode::Normal => "Normal", BlendMode::Multiply => "Multiply",
            BlendMode::Screen => "Screen", BlendMode::Overlay => "Overlay",
            BlendMode::SoftLight => "Soft Light", BlendMode::HardLight => "Hard Light",
            BlendMode::Darken => "Darken", BlendMode::Lighten => "Lighten",
            BlendMode::Difference => "Difference",
        }
    }
}

/// Clip-to-clip transition (attached to the RIGHT clip, plays over the seam).
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum TransKind { Dissolve, DipToBlack, WipeLeft, WipeRight, SlideLeft, SlideRight, Zoom }
impl TransKind {
    pub const ALL: [TransKind; 7] = [
        TransKind::Dissolve, TransKind::DipToBlack, TransKind::WipeLeft, TransKind::WipeRight,
        TransKind::SlideLeft, TransKind::SlideRight, TransKind::Zoom,
    ];
    pub fn label(&self) -> &'static str {
        match self {
            TransKind::Dissolve => "Cross Dissolve", TransKind::DipToBlack => "Dip to Black",
            TransKind::WipeLeft => "Wipe ←", TransKind::WipeRight => "Wipe →",
            TransKind::SlideLeft => "Slide ←", TransKind::SlideRight => "Slide →",
            TransKind::Zoom => "Zoom",
        }
    }
    /// ffmpeg xfade transition name (export backend).
    pub fn xfade_name(&self) -> &'static str {
        match self {
            TransKind::Dissolve => "fade", TransKind::DipToBlack => "fadeblack",
            TransKind::WipeLeft => "wipeleft", TransKind::WipeRight => "wiperight",
            TransKind::SlideLeft => "slideleft", TransKind::SlideRight => "slideright",
            TransKind::Zoom => "zoomin",
        }
    }
}

#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq)]
pub struct Transition {
    pub kind: TransKind,
    pub dur: f64,
}
impl Default for Transition {
    fn default() -> Self { Self { kind: TransKind::Dissolve, dur: 0.5 } }
}

/// Shape mask isolating a region of the clip (real `geq` alpha at export AND
/// preview — both share the same filter chain).
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Default)]
pub struct Mask {
    pub enabled: bool,
    pub ellipse: bool,          // false = rectangle
    /// center + half-size as 0..1 fractions of the frame
    pub cx: f32, pub cy: f32, pub hw: f32, pub hh: f32,
    /// soft edge 0..1 (0 = hard edge)
    pub feather: f32,
    pub invert: bool,
}
impl Mask {
    pub fn is_active(&self) -> bool { self.enabled && self.hw > 0.01 && self.hh > 0.01 }
}

/// Chroma key (green screen) — real ffmpeg `colorkey` + `despill`.
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Default)]
pub struct ChromaKey {
    pub enabled: bool,
    pub color: [u8; 3],   // key color
    /// 0.01..1.0 color similarity
    pub similarity: f32,
    /// 0..1 edge blend
    pub blend: f32,
    /// 0..1 green spill suppression
    pub spill: f32,
}
impl ChromaKey {
    /// Preset for classic green screens.
    pub fn classic() -> Self {
        Self { enabled: true, color: [0, 255, 0], similarity: 0.28, blend: 0.10, spill: 0.2 }
    }
}

// ------------------------------------------------------------------ keyframes
/// Easing applied on the segment STARTING at a keyframe.
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq, Default)]
pub enum Ease { #[default] Linear, EaseIn, EaseOut, EaseInOut }
impl Ease {
    pub fn label(&self) -> &'static str {
        match self { Ease::Linear => "Linear", Ease::EaseIn => "Ease In",
                     Ease::EaseOut => "Ease Out", Ease::EaseInOut => "Ease In-Out" }
    }
    /// Map raw 0..1 progress through the easing curve.
    pub fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Ease::Linear => t,
            Ease::EaseIn => t * t,
            Ease::EaseOut => 1.0 - (1.0 - t) * (1.0 - t),
            Ease::EaseInOut => if t < 0.5 { 2.0 * t * t } else { 1.0 - 2.0 * (1.0 - t) * (1.0 - t) },
        }
    }
}

/// One animated scalar channel: sorted keyframes (clip-relative seconds).
pub type Chan = Vec<(f64, f32, Ease)>;

pub fn chan_eval(ch: &Chan, t: f64, static_v: f32) -> f32 {
    if ch.is_empty() { return static_v; }
    if t <= ch[0].0 { return ch[0].1; }
    for w in ch.windows(2) {
        if t >= w[0].0 && t < w[1].0 {
            let span = (w[1].0 - w[0].0).max(1e-6);
            let p = ((t - w[0].0) / span) as f32;
            return w[0].1 + (w[1].1 - w[0].1) * w[0].2.apply(p);
        }
    }
    ch.last().map(|k| k.1).unwrap_or(static_v)
}

/// Per-clip animation channels (clip-relative seconds). Empty channel = use
/// the static Transform value. Position/Scale/Rotation/Opacity are animated
/// in the preview through the same code path as the static transform, and
/// exported as ffmpeg expressions.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
pub struct Anim {
    #[serde(default)] pub pos_x: Chan,
    #[serde(default)] pub pos_y: Chan,
    #[serde(default)] pub scale: Chan,
    #[serde(default)] pub rotation: Chan,
    #[serde(default)] pub opacity: Chan,
}
impl Anim {
    pub fn is_channel_empty(&self, chan: u8) -> bool {
        match chan {
            0 => self.pos_x.is_empty(), 1 => self.pos_y.is_empty(), 2 => self.scale.is_empty(),
            3 => self.rotation.is_empty(), _ => self.opacity.is_empty(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.pos_x.is_empty() && self.pos_y.is_empty() && self.scale.is_empty()
            && self.rotation.is_empty() && self.opacity.is_empty()
    }
    /// Effective transform at clip-relative time `t` (seconds on the timeline
    /// clip, not source seconds).
    pub fn eval(&self, t: f64, base: &Transform) -> Transform {
        if self.is_empty() { return *base; }
        Transform {
            x: chan_eval(&self.pos_x, t, base.x),
            y: chan_eval(&self.pos_y, t, base.y),
            scale: chan_eval(&self.scale, t, base.scale),
            rotation: chan_eval(&self.rotation, t, base.rotation),
            opacity: chan_eval(&self.opacity, t, base.opacity),
        }
    }
}

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

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
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
    /// RGB master curve control points (normalized 0..1, sorted by x).
    /// Rendered through the real ffmpeg `curves` filter (natural spline).
    #[serde(default)]
    pub curves: Vec<(f32, f32)>,
    /// HSL secondary: target band (0=reds .. 5=magentas), saturation and
    /// lightness adjustments -1..1 (real `huesaturation` filter).
    #[serde(default)] pub hsl_band: u8,
    #[serde(default)] pub hsl_sat: f32,
    #[serde(default)] pub hsl_light: f32,
}
impl Default for Grade {
    fn default() -> Self {
        Self { temp: 0.0, tint: 0.0, exposure: 0.0, contrast: 0.0, saturation: 0.0,
               highlights: 0.0, whites: 0.0, blacks: 0.0,
               vibrance: 0.0, lift: [0.0; 3], gamma: [0.0; 3], gain: [0.0; 3], offset: 0.0,
               curves: Vec::new(), hsl_band: 0, hsl_sat: 0.0, hsl_light: 0.0 }
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
            || !self.curves.is_empty()
            || self.hsl_sat.abs() > 0.01 || self.hsl_light.abs() > 0.01
    }
    /// Merge an adjustment layer's grade ON TOP of this one (additive).
    pub fn merge_adjustment(&mut self, adj: &Grade) {
        self.temp += adj.temp; self.tint += adj.tint;
        self.exposure += adj.exposure; self.contrast += adj.contrast;
        self.saturation += adj.saturation; self.highlights += adj.highlights;
        self.whites += adj.whites; self.blacks += adj.blacks;
        self.vibrance += adj.vibrance; self.offset += adj.offset;
        for i in 0..3 {
            self.lift[i] += adj.lift[i]; self.gamma[i] += adj.gamma[i]; self.gain[i] += adj.gain[i];
        }
        if !adj.curves.is_empty() { self.curves = adj.curves.clone(); }
        if adj.hsl_sat.abs() > 0.01 || adj.hsl_light.abs() > 0.01 {
            self.hsl_band = adj.hsl_band; self.hsl_sat = adj.hsl_sat; self.hsl_light = adj.hsl_light;
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
pub struct Fx {
    pub blur: f32, // px radius
    pub fade_in: f32,
    pub fade_out: f32,
    pub lut: Option<PathBuf>,
    // ---- extended effect set (all real ffmpeg filters) ----
    /// Unsharp mask amount 0..100
    #[serde(default)] pub sharpen: f32,
    /// Spatial video denoise 0..100 (hqdn3d)
    #[serde(default)] pub denoise: f32,
    /// Vignette 0..100
    #[serde(default)] pub vignette: f32,
    /// Hue rotation -180..180 degrees
    #[serde(default)] pub hue: f32,
    /// Bloom/glow 0..100 (split+gblur+blend=screen)
    #[serde(default)] pub glow: f32,
    #[serde(default)] pub grayscale: bool,
    #[serde(default)] pub sepia: bool,
    /// Debanding 0..100 (gradfun)
    #[serde(default)] pub deband: f32,
    /// Lens distortion correction k1/k2 (lenscorrection)
    #[serde(default)] pub lens_k1: f32,
    #[serde(default)] pub lens_k2: f32,
    /// Chroma key (green screen)
    #[serde(default)] pub chroma: ChromaKey,
    /// Shape mask
    #[serde(default)] pub mask: Mask,
}
impl Fx {
    pub fn is_default(&self) -> bool {
        let d = Fx::default();
        self.blur == d.blur && self.fade_in == d.fade_in && self.fade_out == d.fade_out
            && self.lut.is_none() && self.sharpen == 0.0 && self.denoise == 0.0
            && self.vignette == 0.0 && self.hue == 0.0 && self.glow == 0.0
            && !self.grayscale && !self.sepia && self.deband == 0.0
            && self.lens_k1 == 0.0 && self.lens_k2 == 0.0
            && !self.chroma.enabled && !self.mask.enabled
    }
}

/// Per-clip audio processing rack (real ffmpeg filters, applied identically
/// in the preview monitor and the export mixdown).
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Default)]
pub struct AudioFx {
    /// 3-band EQ, dB -18..18
    #[serde(default)] pub eq_low: f32,
    #[serde(default)] pub eq_mid: f32,
    #[serde(default)] pub eq_high: f32,
    #[serde(default)] pub compressor: bool,
    #[serde(default)] pub limiter: bool,
    /// Noise reduction 0..100 (afftdn)
    #[serde(default)] pub nr: f32,
    /// Reverb/echo 0..100 (aecho)
    #[serde(default)] pub reverb: f32,
    #[serde(default)] pub deess: bool,
    /// Speech clarity boost (highpass + presence EQ + denoise)
    #[serde(default)] pub voice: bool,
}
impl AudioFx {
    pub fn is_default(&self) -> bool { *self == AudioFx::default() }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct TitleData {
    pub text: String,
    pub size: f32,
    pub color: [u8; 3],
    /// Text block anchor, fractions of the frame (0.5, 0.5 = centered).
    #[serde(default)]
    pub pos: [f32; 2],
    /// Background bar behind the text, 0..1 opacity (lower thirds).
    #[serde(default)]
    pub bar: f32,
    /// Bar tint.
    #[serde(default = "default_bar_color")]
    pub bar_color: [u8; 3],
    /// Optional shadow strength 0..1 for readability.
    #[serde(default)]
    pub shadow: f32,
}
fn default_bar_color() -> [u8; 3] { [12, 12, 16] }
impl Default for TitleData {
    fn default() -> Self {
        Self { text: "Title".into(), size: 72.0, color: [255, 255, 255],
               pos: [0.5, 0.5], bar: 0.0, bar_color: [12, 12, 16], shadow: 0.35 }
    }
}
impl TitleData {
    /// Title style presets — one click in the Text panel.
    pub fn preset(kind: u8, text: &str) -> Self {
        let mut t = TitleData { text: text.into(), ..Default::default() };
        match kind {
            0 => { t.pos = [0.5, 0.42]; t.size = 96.0; }                          // Main Title
            1 => { t.pos = [0.24, 0.78]; t.size = 48.0; t.bar = 0.72; }           // Lower Third
            2 => { t.pos = [0.5, 0.12]; t.size = 44.0; t.bar = 0.55; }            // Top Caption
            3 => { t.pos = [0.5, 0.86]; t.size = 38.0; t.bar = 0.5; }             // Subtitle
            4 => { t.pos = [0.5, 0.5]; t.size = 140.0; t.color = [15, 15, 18]; }  // Big Dark
            _ => { t.pos = [0.5, 0.5]; }
        }
        t
    }
}

#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq)]
pub enum ClipKind { Video, Audio, Image, Title, Adjustment }
impl ClipKind {
    pub fn label(&self) -> &'static str {
        match self {
            ClipKind::Video => "Video", ClipKind::Audio => "Audio",
            ClipKind::Image => "Image", ClipKind::Title => "Title",
            ClipKind::Adjustment => "Adjustment",
        }
    }
}

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
    /// Transition INTO this clip from the previous clip on the same track.
    #[serde(default)]
    pub trans_in: Option<Transition>,
    /// Play source backwards (preview throttled, export via ffmpeg `reverse`).
    #[serde(default)]
    pub reverse: bool,
    /// Group id — clips sharing a group move/delete together.
    #[serde(default)]
    pub group: Option<u64>,
    /// Transform/opacity keyframe channels.
    #[serde(default)]
    pub anim: Anim,
    /// Audio processing rack.
    #[serde(default)]
    pub afx: AudioFx,
    /// Layer blend mode (Compositing inspector).
    #[serde(default)]
    pub blend: BlendMode,
}

impl Clip {
    /// Effective transform at timeline position `t` (absolute time).
    pub fn transform_at(&self, t: f64) -> Transform {
        self.anim.eval(t - self.tl_start, &self.transform)
    }
    /// Source position shown at timeline time `t` (handles speed + reverse).
    pub fn src_t_at(&self, t: f64) -> f64 {
        let rel = (t - self.tl_start) * self.speed as f64;
        if self.reverse { self.src_in + self.src_len() - rel } else { self.src_in + rel }
    }
}

impl Clip {
    pub fn end(&self) -> f64 { self.tl_start + self.src_dur }
    pub fn src_len(&self) -> f64 { self.src_dur * self.speed as f64 }
    pub fn src_end(&self) -> f64 { self.src_in + self.src_len() }
    pub fn is_visual(&self) -> bool { matches!(self.kind, ClipKind::Video | ClipKind::Image | ClipKind::Title | ClipKind::Adjustment) }
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
        trans_in: None,
        reverse: false,
        group: None,
        anim: Anim::default(),
        afx: AudioFx::default(),
        blend: BlendMode::default(),
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
        trans_in: None,
        reverse: false,
        group: None,
        anim: Anim::default(),
        afx: AudioFx::default(),
        blend: BlendMode::default(),
    }
}

/// A frozen frame (snapshot PNG) placed on the timeline as an Image clip.
pub fn still_clip(png_path: PathBuf, name: &str, tl_start: f64, dur: f64) -> Clip {
    Clip {
        id: next_id(),
        kind: ClipKind::Image,
        name: name.chars().take(24).collect(),
        source: Some(png_path),
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
        link: None,
        trans_in: None,
        reverse: false,
        group: None,
        anim: Anim::default(),
        afx: AudioFx::default(),
        blend: BlendMode::default(),
    }
}

/// Adjustment layer clip: its grade+fx apply to every visual clip BELOW it
/// (lower tracks) during its time range.
pub fn adjustment_clip(tl_start: f64, dur: f64) -> Clip {
    let mut c = title_clip("Adjustment", tl_start, dur);
    c.kind = ClipKind::Adjustment;
    c.title = None;
    c.name = "Adjustment".into();
    c
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

    /// Neighbors on the same track: (left clip id, right clip id).
    pub fn neighbors(&self, clip_id: u64) -> (Option<u64>, Option<u64>) {
        let Some((tr, c)) = self.clip(clip_id) else { return (None, None) };
        let (mut l, mut r) = (None, None);
        for x in &tr.clips {
            if x.id == clip_id { continue; }
            if (x.end() - c.tl_start).abs() < 1e-4 { l = Some(x.id); }
            if (x.tl_start - c.end()).abs() < 1e-4 { r = Some(x.id); }
        }
        (l, r)
    }

    /// How much source room exists to extend each side of a clip.
    /// (left_ext: seconds the clip can grow earlier at its head,
    ///  right_ext: seconds it can grow later at its tail)
    pub fn src_room(&self, clip_id: u64, total: Option<f64>) -> (f64, f64) {
        let Some((_, c)) = self.clip(clip_id) else { return (0.0, 0.0) };
        let head = if c.source.is_some() { c.src_in / c.speed as f64 } else { f64::INFINITY };
        let tail = match total {
            Some(t) => ((t - c.src_end()) / c.speed as f64).max(0.0),
            None => f64::INFINITY,
        };
        (head.max(0.0), tail)
    }

    /// ROLL EDIT: move the cut point between two ADJACENT clips. The left
    /// clip's out point and the right clip's in point change together, so the
    /// total duration stays constant. `right_id` is the clip AFTER the seam.
    /// Returns the applied delta.
    pub fn roll_edit(&mut self, left_id: u64, right_id: u64, delta: f64, left_total: Option<f64>, right_total: Option<f64>) -> f64 {
        // clamp by both clips' source rooms
        let (l_head, l_tail) = self.src_room(left_id, left_total);
        let (r_head, r_tail) = self.src_room(right_id, right_total);
        let _ = (l_head, r_tail);
        let mut d = delta;
        // left grows on its tail by d (d > 0): needs l_tail
        d = d.min(l_tail);
        // left shrinks: keep MIN_CLIP_DUR
        if let Some((_, lc)) = self.clip(left_id) { d = d.max(MIN_CLIP_DUR - lc.src_dur); }
        // right grows on its head by d (needs r_head for d > 0) / shrinks by |d| (needs r_tail... no: shrinking keeps room)
        if d > 0.0 { d = d.min(r_head); }
        if d < 0.0 { if let Some((_, rc)) = self.clip(right_id) { d = d.max(rc.src_dur - MIN_CLIP_DUR).max(-(r_tail)); } }
        // apply
        if let Some((_, lc)) = self.clip(left_id) {
            let lc = lc.clone();
            if let Some(c) = self.clip_mut(left_id) { c.src_dur += d; }
            if let Some(peer) = lc.link { self.trim_right_extend(peer, d, left_total); }
        }
        if let Some((_, rc)) = self.clip(right_id) {
            let rc = rc.clone();
            if let Some(c) = self.clip_mut(right_id) {
                c.tl_start += d;
                c.src_in += d * c.speed as f64;
                c.src_dur -= d;
                for kf in c.vol_kf.iter_mut() { kf.0 += d; }
            }
            if let Some(peer) = rc.link { self.trim_left(peer, d); }
        }
        d
    }

    /// Extend a linked peer's tail when the master rolls (keeps A/V in sync).
    fn trim_right_extend(&mut self, clip_id: u64, delta: f64, total: Option<f64>) {
        let (head, tail) = self.src_room(clip_id, total);
        let d = delta.min(tail);
        let _ = head;
        if let Some(c) = self.clip_mut(clip_id) { c.src_dur += d; }
    }

    /// SLIDE EDIT: move a clip along the timeline between its neighbors; the
    /// clip's own content is unchanged, the neighbors' cut points move.
    /// Returns the applied delta.
    pub fn slide_edit(&mut self, clip_id: u64, delta: f64, left_total: Option<f64>, right_total: Option<f64>) -> f64 {
        if self.clip(clip_id).is_none() { return 0.0; }
        let (lid, rid) = self.neighbors(clip_id);
        let mut d = delta;
        // left neighbor tail room limits moving earlier (delta < 0):
        // it must shrink by |d|, so |d| <= min(its len - MIN, its tail source room)
        if d < 0.0 {
            let shrink = match lid.and_then(|id| self.clip(id).map(|(_, c)| c.clone())) {
                Some(l) => {
                    let room = match left_total {
                        Some(t) => ((t - l.src_end()) / l.speed as f64).max(0.0),
                        None => f64::INFINITY,
                    };
                    (l.src_dur - MIN_CLIP_DUR).min(room)
                }
                None => 0.0, // no left neighbor: cannot move earlier
            };
            d = d.max(-shrink.max(0.0));
        }
        // right neighbor head room limits moving later (delta > 0):
        // it must shrink from its head, so d <= min(its len - MIN, its head room)
        if d > 0.0 {
            let shrink = match rid.and_then(|id| self.clip(id).map(|(_, c)| c.clone())) {
                Some(r) => {
                    let room = match right_total {
                        Some(t) => r.src_in / r.speed as f64,
                        None => f64::INFINITY,
                    };
                    (r.src_dur - MIN_CLIP_DUR).min(room.max(0.0))
                }
                None => 0.0, // no right neighbor: cannot move later
            };
            d = d.min(shrink.max(0.0));
        }
        if d.abs() < 1e-9 { return 0.0; }
        // apply: shift the clip; trim left neighbor tail / right neighbor head
        if let Some(c) = self.clip_mut(clip_id) { c.tl_start += d; }
        if let Some(l) = lid {
            if d < 0.0 {
                // moving earlier: left neighbor shrinks on its tail
                if let Some(lc) = self.clip_mut(l) { lc.src_dur += d; }
            } else {
                // moving later: left neighbor GROWS back into freed space
                if let Some(lc) = self.clip_mut(l) {
                    if let Some(t) = left_total {
                        let room = ((t - lc.src_end()) / lc.speed as f64).max(0.0);
                        lc.src_dur += d.min(room);
                    } else {
                        lc.src_dur += d;
                    }
                }
            }
        }
        if let Some(r) = rid {
            if d < 0.0 {
                // moving earlier: right neighbor GROWS into freed space
                if let Some(rc) = self.clip_mut(r) {
                    if let Some(t) = right_total {
                        let room = rc.src_in / rc.speed as f64;
                        let grow = (-d).min(room.max(0.0));
                        rc.tl_start -= grow;
                        rc.src_in -= grow * rc.speed as f64;
                        rc.src_dur += grow;
                    } else {
                        rc.tl_start -= -d;
                        rc.src_in -= -d * rc.speed as f64;
                        rc.src_dur += -d;
                    }
                }
            } else if let Some(rc) = self.clip_mut(r) {
                rc.tl_start += d;
                rc.src_in += d * rc.speed as f64;
                rc.src_dur -= d;
            }
        }
        d
    }

    /// All clips sharing `group_id`.
    pub fn group_members(&self, group_id: u64) -> Vec<u64> {
        self.tracks.iter().flat_map(|t| &t.clips)
            .filter(|c| c.group == Some(group_id))
            .map(|c| c.id).collect()
    }

    /// Assign a new group to the given clips (they then edit together).
    pub fn group_clips(&mut self, ids: &[u64]) -> u64 {
        let g = next_id();
        for id in ids {
            if let Some(c) = self.clip_mut(*id) { c.group = Some(g); }
        }
        g
    }

    pub fn ungroup_clips(&mut self, ids: &[u64]) {
        for id in ids {
            if let Some(c) = self.clip_mut(*id) { c.group = None; }
        }
    }

    /// Seam between two adjacent clips on the same track (left.end == right.start).
    pub fn seam_for(&self, right_id: u64) -> Option<u64> {
        self.neighbors(right_id).0
    }

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

    /// Effective grade/fx for `clip` at timeline time `t`, with Adjustment
    /// layers on HIGHER video tracks merged in. Used identically by the
    /// preview engine and the exporter, so both honor adjustment layers.
    pub fn effective_grade_fx_at(&self, clip: &Clip, t: f64) -> (Grade, Fx) {
        let mut g = clip.grade.clone();
        let mut f = clip.fx.clone();
        let my_track = self.tracks.iter().position(|tr| tr.clips.iter().any(|c| c.id == clip.id));
        let Some(ti) = my_track else { return (g, f) };
        for (j, tr) in self.tracks.iter().enumerate() {
            if tr.kind != TrackKind::Video || tr.hidden || j <= ti { continue; }
            for a in &tr.clips {
                if a.kind == ClipKind::Adjustment && t >= a.tl_start && t < a.end() {
                    g.merge_adjustment(&a.grade);
                    merge_fx(&mut f, &a.fx);
                }
            }
        }
        (g, f)
    }
}

/// Combine an adjustment layer's fx into a base clip's fx (additive where it
/// makes sense, max where stacking two of the same effect is meaningless).
pub fn merge_fx(base: &mut Fx, adj: &Fx) {
    base.blur += adj.blur;
    base.sharpen = base.sharpen.max(adj.sharpen);
    base.denoise = base.denoise.max(adj.denoise);
    base.vignette = base.vignette.max(adj.vignette);
    base.hue += adj.hue;
    base.glow = base.glow.max(adj.glow);
    base.grayscale |= adj.grayscale;
    base.sepia |= adj.sepia;
    base.deband = base.deband.max(adj.deband);
    base.lens_k1 += adj.lens_k1;
    base.lens_k2 += adj.lens_k2;
    if base.lut.is_none() { base.lut = adj.lut.clone(); }
    // chroma/mask/fades stay with the base clip (masking a mask is confusing)
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

//! Shared utilities: theme palette, timecode math, easing.

use egui::{Color32, Rounding, Stroke};

#[derive(Clone, Copy)]
pub struct Theme {
    pub bg: Color32,
    pub panel: Color32,
    pub panel2: Color32,
    pub panel3: Color32,
    pub border: Color32,
    pub border2: Color32,
    pub text: Color32,
    pub dim: Color32,
    pub faint: Color32,
    pub accent: Color32,
    pub accent_dim: Color32,
    pub accent_text: Color32,
    pub clip_video: Color32,
    pub clip_video_edge: Color32,
    pub clip_audio: Color32,
    pub clip_audio_edge: Color32,
    pub clip_title: Color32,
    pub clip_title_edge: Color32,
    pub clip_image: Color32,
    pub clip_image_edge: Color32,
    pub playhead: Color32,
    pub io_band: Color32,
    pub ruler_bg: Color32,
    pub lane: Color32,
    pub lane_alt: Color32,
    pub warn: Color32,
    pub ok: Color32,
    pub err: Color32,
    pub scope_bg: Color32,
    pub scope_trace: Color32,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            bg: Color32::from_rgb(13, 13, 16),
            panel: Color32::from_rgb(22, 22, 26),
            panel2: Color32::from_rgb(28, 28, 33),
            panel3: Color32::from_rgb(35, 35, 41),
            border: Color32::from_rgb(42, 42, 49),
            border2: Color32::from_rgb(56, 56, 64),
            text: Color32::from_rgb(205, 207, 214),
            dim: Color32::from_rgb(140, 143, 152),
            faint: Color32::from_rgb(96, 99, 108),
            accent: Color32::from_rgb(47, 129, 247),
            accent_dim: Color32::from_rgb(31, 78, 140),
            accent_text: Color32::from_rgb(120, 180, 255),
            clip_video: Color32::from_rgb(61, 111, 168),
            clip_video_edge: Color32::from_rgb(125, 170, 220),
            clip_audio: Color32::from_rgb(43, 118, 76),
            clip_audio_edge: Color32::from_rgb(96, 175, 130),
            clip_title: Color32::from_rgb(183, 76, 165),
            clip_title_edge: Color32::from_rgb(226, 145, 210),
            clip_image: Color32::from_rgb(140, 110, 62),
            clip_image_edge: Color32::from_rgb(196, 165, 110),
            playhead: Color32::from_rgb(64, 140, 255),
            io_band: Color32::from_rgba_unmultiplied(212, 177, 6, 60),
            ruler_bg: Color32::from_rgb(18, 18, 22),
            lane: Color32::from_rgb(19, 19, 23),
            lane_alt: Color32::from_rgb(16, 16, 20),
            warn: Color32::from_rgb(226, 168, 43),
            ok: Color32::from_rgb(67, 181, 129),
            err: Color32::from_rgb(228, 88, 88),
            scope_bg: Color32::from_rgb(8, 8, 10),
            scope_trace: Color32::from_rgb(205, 205, 205),
        }
    }
}

pub fn round_small() -> Rounding { Rounding::same(3) }
pub fn round_med() -> Rounding { Rounding::same(5) }
pub fn border_stroke(t: &Theme) -> Stroke { Stroke::new(1.0, t.border) }

/// Format seconds as SMPTE-like timecode HH:MM:SS:FF at `fps`.
pub fn timecode(secs: f64, fps: f64) -> String {
    let secs = secs.max(0.0);
    let fps = fps.max(1.0);
    let total_frames = (secs * fps).round() as u64;
    let fr = total_frames % fps.round() as u64;
    let total_secs = total_frames / fps.round() as u64;
    format!(
        "{:02}:{:02}:{:02}:{:02}",
        total_secs / 3600,
        (total_secs % 3600) / 60,
        total_secs % 60,
        fr
    )
}

/// Short mm:ss duration label (as used in the media pool).
pub fn short_dur(secs: f64) -> String {
    let s = secs.max(0.0).round() as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

pub fn clampf(v: f64, lo: f64, hi: f64) -> f64 { v.max(lo).min(hi) }

/// Snap `t` to the nearest candidate within `thresh`; returns None if none.
pub fn snap_to(t: f64, candidates: &[f64], thresh: f64) -> Option<f64> {
    let mut best: Option<(f64, f64)> = None;
    for &c in candidates {
        let d = (t - c).abs();
        if d <= thresh && best.map(|(bd, _)| d < bd).unwrap_or(true) {
            best = Some((d, c));
        }
    }
    best.map(|(_, c)| c)
}

pub fn nice_step(seconds_visible: f64) -> f64 {
    let raw = seconds_visible / 6.0;
    let steps = [1.0 / 30.0, 0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0, 120.0, 300.0, 600.0, 1800.0, 3600.0];
    steps.iter().copied().find(|&s| s >= raw).unwrap_or(3600.0)
}

pub fn fmt_bytes(b: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < 4 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 { format!("{b} B") } else { format!("{v:.1} {}", U[i]) }
}

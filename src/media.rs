//! Media engine: FFmpeg/ffprobe process management, probing, thumbnails,
//! waveform peaks, proxy generation.
//!
//! Design note (stability): all decoding/encoding happens in *separate
//! processes* piped over stdio. A malformed file or a decoder fault can never
//! take the editor down — the UI thread only ever reads bounded channels.

use crate::model::{Fx, Grade, MediaAsset};
use serde_json::Value;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, OnceLock};

// ------------------------------------------------------------------ events
#[derive(Clone, Debug)]
pub enum MediaEvent {
    Imported(MediaAsset),
    ImportFailed { path: PathBuf, err: String },
    Thumb { path: PathBuf, png: Vec<u8> },
    Wave { path: PathBuf, peaks: Arc<Vec<(i8, i8)>> },
    Progress { job: u64, frac: f32, fps: f32, speed: f32, out_time: f64 },
    JobDone { job: u64, result: Result<String, String> },
}

// ------------------------------------------------------------------ binaries
fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let p = dir.join(name);
        if p.is_file() { return Some(p); }
        #[cfg(windows)]
        {
            let p = dir.join(format!("{name}.exe"));
            if p.is_file() { return Some(p); }
        }
    }
    None
}

static FFMPEG: OnceLock<Option<PathBuf>> = OnceLock::new();
static FFPROBE: OnceLock<Option<PathBuf>> = OnceLock::new();
static FFMPEG_SRC: OnceLock<&'static str> = OnceLock::new();

/// Candidate directories that carry BUNDLED ffmpeg binaries shipped with the
/// app (portable zip, AppImage mount, MSI/deb install layouts).
fn bundled_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            v.push(dir.to_path_buf());          // ffmpeg next to the binary
            v.push(dir.join("bin"));            // ffmpeg/bin layout
            v.push(dir.join("ffmpeg"));         // ffmpeg/ffmpeg.exe layout
            // staged/extracted .deb layout: <root>/usr/bin -> <root>/usr/lib/kestrelcut/bin
            if let Some(root) = dir.parent() {
                v.push(root.join("usr/lib/kestrelcut/bin"));
                v.push(root.join("lib/kestrelcut/bin"));
            }
        }
    }
    // Linux distro packages must not collide with system ffmpeg in /usr/bin
    v.push(PathBuf::from("/usr/lib/kestrelcut/bin"));
    v.push(PathBuf::from("/usr/local/lib/kestrelcut/bin"));
    v
}

fn find_bundled(name: &str) -> Option<PathBuf> {
    for dir in bundled_dirs() {
        let mut cands = vec![dir.join(name)];
        if cfg!(windows) {
            cands.push(dir.join(format!("{name}.exe")));
        }
        for c in cands {
            if c.is_file() { return Some(c); }
        }
    }
    None
}

/// Resolve ffmpeg binary. Order (offline-first — the app NEVER downloads):
///   1. KESTRELCUT_FFMPEG env override
///   2. Bundled copy shipped with the app (portable/AppImage/deb/MSI)
///   3. System PATH
pub fn ffmpeg() -> Option<PathBuf> {
    FFMPEG.get_or_init(|| {
        if let Ok(p) = std::env::var("KESTRELCUT_FFMPEG") {
            let p = PathBuf::from(p);
            if p.is_file() {
                let _ = FFMPEG_SRC.set("env override");
                return Some(p);
            }
        }
        if let Some(p) = find_bundled("ffmpeg") {
            let _ = FFMPEG_SRC.set("bundled with KestrelCut");
            return Some(p);
        }
        if let Some(p) = which("ffmpeg") {
            let _ = FFMPEG_SRC.set("system PATH");
            return Some(p);
        }
        None
    }).clone()
}

pub fn ffprobe() -> Option<PathBuf> {
    FFPROBE.get_or_init(|| {
        if let Ok(p) = std::env::var("KESTRELCUT_FFPROBE") {
            let p = PathBuf::from(p);
            if p.is_file() { return Some(p); }
        }
        if let Some(p) = find_bundled("ffprobe") { return Some(p); }
        if let Some(p) = which("ffprobe") { return Some(p); }
        // static builds ship ffprobe next to ffmpeg
        ffmpeg().as_ref().map(|f| f.with_file_name("ffprobe")
            .with_extension(if cfg!(windows) { "exe" } else { "" }))
            .filter(|p| p.exists())
    }).clone()
}

pub fn ffmpeg_ok() -> bool { ffmpeg().is_some() }

/// Where the resolved binaries came from (for About / diagnostics).
pub fn ffmpeg_source() -> &'static str {
    FFMPEG_SRC.get().copied().unwrap_or("not found")
}

/// Diagnostic printout used by `kestrelcut --where`.
pub fn where_report() -> String {
    let f = ffmpeg();
    let p = ffprobe();
    format!(
        "ffmpeg : {}\nffprobe: {}\nsource : {}",
        f.as_ref().map(|x| x.display().to_string()).unwrap_or_else(|| "NOT FOUND".into()),
        p.as_ref().map(|x| x.display().to_string()).unwrap_or_else(|| "NOT FOUND".into()),
        ffmpeg_source(),
    )
}

// ------------------------------------------------------------------ probe
#[derive(Clone, Debug, Default)]
pub struct MediaInfo {
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub codec: String,
    pub has_video: bool,
    pub has_audio: bool,
    pub audio_codec: Option<String>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u16>,
    pub size: u64,
}

fn probe_with(bin: &Path, args: &[&str]) -> Option<Value> {
    let out = Command::new(bin).args(args).output().ok()?;
    if !out.status.success() { return None; }
    serde_json::from_slice(&out.stdout).ok()
}

/// Analyze a media file with ffprobe.
pub fn probe(path: &Path) -> Result<MediaInfo, String> {
    let bin = ffprobe().ok_or_else(|| "ffprobe not found".to_string())?;
    let json = probe_with(&bin, &[
        "-v", "quiet", "-print_format", "json", "-show_format", "-show_streams",
        &path.to_string_lossy(),
    ]).ok_or_else(|| "ffprobe failed".to_string())?;

    let mut info = MediaInfo::default();
    if let Some(fmt) = json.get("format") {
        info.duration = fmt.get("duration").and_then(|d| d.as_str()).and_then(|d| d.parse().ok()).unwrap_or(0.0);
        info.size = fmt.get("size").and_then(|d| d.as_str()).and_then(|d| d.parse().ok()).unwrap_or(0);
    }
    if let Some(streams) = json.get("streams").and_then(|s| s.as_array()) {
        for s in streams {
            match s.get("codec_type").and_then(|c| c.as_str()) {
                Some("video") if info.codec.is_empty() || !info.has_video => {
                    // pick the first video stream
                    if !info.has_video {
                        info.has_video = true;
                        info.codec = s.get("codec_name").and_then(|c| c.as_str()).unwrap_or("?").into();
                        info.width = s.get("width").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                        info.height = s.get("height").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                        let rate = s.get("avg_frame_rate").and_then(|v| v.as_str()).unwrap_or("30/1");
                        info.fps = parse_ratio(rate).unwrap_or(30.0);
                        if info.duration <= 0.0 {
                            info.duration = s.get("duration").and_then(|d| d.as_str())
                                .and_then(|d| d.parse().ok()).unwrap_or(0.0);
                        }
                    }
                }
                Some("audio") if !info.has_audio => {
                    info.has_audio = true;
                    info.audio_codec = s.get("codec_name").and_then(|c| c.as_str()).map(String::from);
                    info.sample_rate = s.get("sample_rate").and_then(|v| v.as_str()).and_then(|v| v.parse().ok());
                    info.channels = s.get("channels").and_then(|v| v.as_u64()).map(|v| v as u16);
                }
                _ => {}
            }
        }
    }
    if !info.has_video && !info.has_audio {
        return Err("no streams found".into());
    }
    Ok(info)
}

fn parse_ratio(s: &str) -> Option<f64> {
    let mut it = s.split('/');
    let a: f64 = it.next()?.parse().ok()?;
    let b: f64 = it.next().and_then(|b| b.parse().ok()).unwrap_or(1.0);
    if b == 0.0 { return None; }
    Some(a / b)
}

/// Classify a path into an asset kind by extension.
pub fn classify_kind(path: &Path) -> crate::model::AssetKind {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "mp3" | "wav" | "aac" | "flac" | "ogg" | "m4a" | "opus" | "wma" => crate::model::AssetKind::Audio,
        "png" | "jpg" | "jpeg" | "bmp" | "gif" | "webp" | "tiff" => crate::model::AssetKind::Image,
        _ => crate::model::AssetKind::Video,
    }
}

/// Full import: probe + build asset.
pub fn import(path: &Path) -> Result<MediaAsset, String> {
    let info = probe(path)?;
    let kind = classify_kind(path);
    Ok(MediaAsset {
        id: crate::model::next_id(),
        path: path.to_path_buf(),
        kind,
        duration: if info.duration > 0.0 { info.duration } else { 4.0 },
        width: info.width,
        height: info.height,
        fps: info.fps,
        codec: if info.has_video { info.codec.clone() } else { info.audio_codec.clone().unwrap_or_default() },
        has_audio: info.has_audio,
        audio_codec: info.audio_codec,
        sample_rate: info.sample_rate,
        channels: info.channels,
        proxy: None,
        size: info.size,
    })
}

// ------------------------------------------------------------------ filter chain
pub fn esc_filter_path(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/").replace('\'', "\\\\'").replace(':', "\\:")
}

/// Grade → ffmpeg filter args. Single source of truth shared by the preview
/// decoder and the exporter so WYSIWYG holds.
pub fn grade_filters(g: &Grade) -> Vec<String> {
    let mut v = Vec::new();
    if g.temp.abs() > 0.5 {
        let t = (6500.0 + g.temp as f64 * 40.0).clamp(2000.0, 11000.0) as i32;
        v.push(format!("colortemperature=temperature={t}"));
    }
    // ---- colorbalance: tint + Lift/Gamma/Gain wheels (single filter) ----
    // colorbalance ranges are -1..1 per channel per tonal band.
    let mut cb: Vec<String> = Vec::new();
    if g.tint.abs() > 0.5 {
        let m = ((g.tint.abs() / 100.0) * 0.5).min(0.5);
        if g.tint > 0.0 {
            cb.push(format!("rm={m:.3}")); cb.push(format!("gm={:.3}", m * 0.3));
        } else {
            cb.push(format!("bm={m:.3}"));
        }
    }
    for (band, wheel) in [("s", &g.lift), ("m", &g.gamma), ("h", &g.gain)] {
        for (ch, val) in wheel.iter().enumerate() {
            if val.abs() > 0.005 {
                // colorbalance keys are channel-first: rs/gs/bs, rm/gm/bm, rh/gh/bh
                let key = format!("{}{}", ["r", "g", "b"][ch], band);
                cb.push(format!("{key}={:.3}", val.clamp(-1.0, 1.0)));
            }
        }
    }
    if !cb.is_empty() { v.push(format!("colorbalance={}", cb.join(":"))); }

    let mut eq = Vec::new();
    // exposure + offset master wheel fold into one brightness value
    let brightness = (g.exposure / 10.0).clamp(-1.0, 1.0) + (g.offset / 100.0).clamp(-1.0, 1.0);
    if brightness.abs() > 0.01 { eq.push(format!("brightness={brightness:.3}")); }
    if g.contrast.abs() > 0.5 { eq.push(format!("contrast={:.3}", 1.0 + g.contrast as f64 / 100.0)); }
    if g.saturation.abs() > 0.5 { eq.push(format!("saturation={:.3}", (1.0 + g.saturation as f64 / 100.0).max(0.0))); }
    if !eq.is_empty() { v.push(format!("eq={}", eq.join(":"))); }
    // Vibrance: saturates low-sat colors first (real ffmpeg vibrance filter).
    if g.vibrance.abs() > 0.5 {
        let i = (g.vibrance as f64 / 100.0).clamp(-1.0, 1.0);
        v.push(format!("vibrance=intensity={i:.3}"));
    }
    if g.blacks.abs() > 0.5 {
        let b = (g.blacks as f64 / 100.0 * 0.05).abs();
        let p = if g.blacks > 0.0 { format!("0/{:.3}", b) } else { format!("0/-{:.3}", b) };
        v.push(format!("curves=all='{p} 1/1'"));
    }
    if g.whites.abs() > 0.5 {
        let w = (g.whites as f64 / 100.0 * 0.05).abs();
        let p = if g.whites > 0.0 { format!("1/{:.3}", 1.0 - w) } else { format!("1/{:.3}", 1.0 + w) };
        v.push(format!("curves=all='0/0 {p}'"));
    }
    if g.highlights.abs() > 0.5 {
        let h = g.highlights as f64 / 100.0 * 0.08;
        v.push(format!("curves=all='0/0 0.75/{:.3} 1/1'", (0.75 + h).clamp(0.0, 1.0)));
    }
    // ---- master RGB curve (ffmpeg natural spline through the control pts) --
    if !g.curves.is_empty() {
        let mut pts: Vec<(f32, f32)> = g.curves.clone();
        pts.retain(|(x, y)| *x >= 0.0 && *x <= 1.0);
        pts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        pts.dedup_by(|a, b| (a.0 - b.0).abs() < 0.01);
        if !pts.is_empty() {
            let s = pts.iter().map(|(x, y)| format!("{:.3}/{:.3}", x * 255.0, y * 255.0)).collect::<Vec<_>>().join(" ");
            v.push(format!("curves=all='{s}'"));
        }
    }
    // ---- HSL secondary (per color band saturation/lightness) --------------
    if g.hsl_sat.abs() > 0.01 || g.hsl_light.abs() > 0.01 {
        let band = ["reds", "yellows", "greens", "cyans", "blues", "magentas"]
            .get(g.hsl_band as usize % 6).copied().unwrap_or("reds");
        v.push(format!("huesaturation={band}={:.2}", g.hsl_sat.clamp(-1.0, 1.0)));
        if g.hsl_light.abs() > 0.01 {
            // lightness adjustment via selective levels on the band is not a
            // single filter; apply a gentle eq brightness scaled by band weight
            v.push(format!("eq=brightness={:.3}", (g.hsl_light * 0.25).clamp(-0.5, 0.5)));
        }
    }
    v
}

/// Full preview/segment filter chain. `dur` enables fades.
pub fn video_filter_chain(
    grade: &Grade, fx: &Fx, dur: f64,
    w: Option<u32>, h: Option<u32>, fps: Option<f64>,
) -> String {
    let mut parts: Vec<String> = vec!["setpts=PTS-STARTPTS".into()];
    parts.extend(grade_filters(grade));
    parts.extend(fx_filters(fx, dur));
    if let (Some(w), Some(h)) = (w, h) {
        parts.push(format!("scale={w}:{h}:flags=fast_bilinear"));
    }
    if let Some(fps) = fps { parts.push(format!("fps={}", fps.max(1.0))); }
    parts.push("format=rgba".into());
    parts.join(",")
}

/// Effect (Fx) portion of the chain — shared by preview and export so the
/// two never disagree. Every entry is a real ffmpeg filter.
pub fn fx_filters(fx: &Fx, dur: f64) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    if fx.denoise > 0.5 {
        let d = (fx.denoise / 100.0 * 8.0).clamp(0.5, 8.0);
        parts.push(format!("hqdn3d={:.1}:{:.1}:6:6", d, d * 0.7));
    }
    if fx.deband > 0.5 {
        let r = (fx.deband / 100.0 * 24.0).clamp(4.0, 24.0) as i32;
        parts.push(format!("gradfun={r}"));
    }
    if fx.lens_k1.abs() > 0.001 || fx.lens_k2.abs() > 0.001 {
        parts.push(format!("lenscorrection=k1={:.4}:k2={:.4}", fx.lens_k1, fx.lens_k2));
    }
    if fx.hue.abs() > 0.5 {
        parts.push(format!("hue=h={:.1}", fx.hue));
    }
    if fx.glow > 0.5 {
        // bloom: blur a copy and screen-blend it back (single-graph -vf)
        let r = (fx.glow / 100.0 * 30.0).clamp(3.0, 30.0) as i32;
        let op = (0.25 + fx.glow / 100.0 * 0.5).min(0.8);
        parts.push(format!(
            "split[a][b];[b]gblur={r}[bb];[a][bb]blend=all_mode=screen:all_opacity={op:.2}"));
    }
    if fx.grayscale { parts.push("hue=s=0".into()); }
    if fx.sepia {
        parts.push("colorchannelmixer=.393:.769:.189:0:.349:.686:.168:0:.272:.534:.131".into());
    }
    if fx.blur > 0.5 {
        let r = (fx.blur as i32).clamp(1, 40);
        parts.push(format!("boxblur={r}:1"));
    }
    if fx.sharpen > 0.5 {
        let amt = (fx.sharpen / 100.0 * 2.0).clamp(0.1, 2.0);
        parts.push(format!("unsharp=5:5:{amt:.2}"));
    }
    if fx.vignette > 0.5 {
        // default angle PI/5; stronger slider → tighter (smaller) angle
        let angle = (std::f64::consts::PI / 2.2) - (fx.vignette as f64 / 100.0) * (std::f64::consts::PI / 2.2 - std::f64::consts::PI / 12.0);
        parts.push(format!("vignette=angle=PI/{:.4}", std::f64::consts::PI / angle));
    }
    if let Some(lut) = &fx.lut {
        if lut.exists() { parts.push(format!("lut3d='{}'", esc_filter_path(lut))); }
    }
    // chroma key BEFORE fades; keeps alpha through the rest of the chain
    if fx.chroma.enabled {
        let hex = format!("0x{:02X}{:02X}{:02X}", fx.chroma.color[0], fx.chroma.color[1], fx.chroma.color[2]);
        parts.push(format!(
            "colorkey={}:similarity={:.3}:blend={:.3}",
            hex, fx.chroma.similarity.clamp(0.01, 1.0), fx.chroma.blend.clamp(0.0, 1.0)));
        if fx.chroma.spill > 0.01 {
            parts.push(format!("despill=type=green:mix={:.2}", fx.chroma.spill.clamp(0.0, 1.0)));
        }
    }
    // shape mask (geq alpha) — after color work, before fades
    if fx.mask.is_active() {
        parts.push(mask_filter(&fx.mask));
    }
    if fx.fade_in > 0.01 { parts.push(format!("fade=t=in:st=0:d={:.2}", fx.fade_in.min(dur as f32))); }
    if fx.fade_out > 0.01 {
        let st = (dur - fx.fade_out as f64).max(0.0);
        parts.push(format!("fade=t=out:st={st:.2}:d={:.2}", fx.fade_out.min(dur as f32)));
    }
    parts
}

/// Shape mask → `geq` alpha expression (fractions of frame → pixel space via
/// the geq W/H variables). Ellipse or rectangle with optional feather.
pub fn mask_filter(m: &crate::model::Mask) -> String {
    let f = (m.feather.clamp(0.0, 0.99) * 2.0).max(0.001); // transition width, fraction units
    let inside = if m.ellipse {
        format!("(1-pow((X/W-{cx:.4})/{hw:.4},2)-pow((Y/H-{cy:.4})/{hh:.4},2))",
            cx = m.cx, hw = m.hw.max(0.02), cy = m.cy, hh = m.hh.max(0.02))
    } else {
        format!("(1-max(abs(X/W-{cx:.4})/{hw:.4}, abs(Y/H-{cy:.4})/{hh:.4}))",
            cx = m.cx, hw = m.hw.max(0.02), cy = m.cy, hh = m.hh.max(0.02))
    };
    let alpha = if m.feather > 0.005 {
        format!("255*clip(({ins}-{edge:.4})/{f:.4},0,1)", ins = inside, edge = 1.0 - f, f = f)
    } else {
        format!("255*gt({ins},0)", ins = inside)
    };
    let a = if m.invert { format!("255-({alpha})") } else { alpha };
    format!("format=rgba,geq=r='r(X,Y)':g='g(X,Y)':b='b(X,Y)':a='{a}'")
}

/// Blend-mode neutral padding color (what a layer blends as "invisible").
pub fn blend_neutral(mode: &crate::model::BlendMode) -> [u8; 3] {
    use crate::model::BlendMode::*;
    match mode {
        Multiply | Darken => [255, 255, 255],
        Screen | Lighten | Difference => [0, 0, 0],
        Overlay | SoftLight | HardLight => [128, 128, 128],
        Normal => [0, 0, 0],
    }
}

// ------------------------------------------------------------------ audio chain
/// Per-clip audio processing chain — IDENTICAL in preview monitor and export.
pub fn audio_filter_chain(afx: &crate::model::AudioFx, gain_db: f32, fade_in: f32, fade_out: f32, dur: f64) -> String {
    let mut parts: Vec<String> = Vec::new();
    if fade_in > 0.01 { parts.push(format!("afade=t=in:st=0:d={:.2}", fade_in.min(dur as f32))); }
    if fade_out > 0.01 {
        let st = (dur - fade_out as f64).max(0.0);
        parts.push(format!("afade=t=out:st={st:.2}:d={:.2}", fade_out.min(dur as f32)));
    }
    if afx.voice {
        parts.push("highpass=f=180".into());
        parts.push("equalizer=g=4:f=3000:w=2:t=q".into());
    }
    if afx.nr > 0.5 {
        let n = (afx.nr / 100.0 * 90.0).clamp(5.0, 90.0);
        parts.push(format!("afftdn=nr={n:.0}:nf=-25"));
    }
    if afx.eq_low.abs() > 0.2 { parts.push(format!("bass=g={:.1}:f=120:w=1", afx.eq_low.clamp(-18.0, 18.0))); }
    if afx.eq_mid.abs() > 0.2 { parts.push(format!("equalizer=g={:.1}:f=1000:w=1.4:t=q", afx.eq_mid.clamp(-18.0, 18.0))); }
    if afx.eq_high.abs() > 0.2 { parts.push(format!("treble=g={:.1}:f=6000:w=1", afx.eq_high.clamp(-18.0, 18.0))); }
    if afx.deess { parts.push("deesser=i=0.4:m=0.5:f=0.4".into()); }
    if afx.compressor { parts.push("acompressor=threshold=0.08:ratio=4:attack=15:release=220:makeup=1.4".into()); }
    if afx.reverb > 0.5 {
        let d = (80.0 + afx.reverb / 100.0 * 320.0).round() as i32; // ms
        parts.push(format!("aecho=0.75:0.85:{d}:{:.2}", 0.25 + afx.reverb as f64 / 100.0 * 0.5));
    }
    if gain_db.abs() > 0.05 { parts.push(format!("volume={:.2}dB", gain_db.clamp(-48.0, 12.0))); }
    if afx.limiter { parts.push("alimiter=limit=0.891:level=false".into()); }
    parts.join(",")
}

// ------------------------------------------------------------------ thumbnails
pub fn spawn_thumb(path: PathBuf, at: f64, w: u32, tx: Sender<MediaEvent>) {
    std::thread::spawn(move || {
        let Some(bin) = ffmpeg() else { return; };
        let out = Command::new(bin)
            .args(["-v", "quiet", "-ss", &format!("{at:.3}"), "-i", &path.to_string_lossy(),
                   "-frames:v", "1", "-vf", &format!("scale={w}:-2:flags=fast_bilinear"),
                   "-f", "image2pipe", "-vcodec", "png", "pipe:1"])
            .output();
        if let Ok(o) = out {
            if o.status.success() && !o.stdout.is_empty() {
                let _ = tx.send(MediaEvent::Thumb { path, png: o.stdout });
            }
        }
    });
}

// ------------------------------------------------------------------ waveforms
pub fn spawn_wave(path: PathBuf, tx: Sender<MediaEvent>) {
    std::thread::spawn(move || {
        let Some(bin) = ffmpeg() else { return };
        let Ok(mut child) = Command::new(bin)
            .args(["-v", "quiet", "-i", &path.to_string_lossy(), "-ac", "1", "-ar", "8000",
                   "-f", "s16le", "pipe:1"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
        else { return };
        let mut pipe = std::io::BufReader::new(child.stdout.take().unwrap());
        const BUCKET: usize = 160; // 20ms at 8kHz
        const MAX_BUCKETS: usize = 60_000; // 20 minutes cap (bounded memory)
        let mut peaks: Vec<(i8, i8)> = Vec::with_capacity(4096);
        let mut carry: Vec<u8> = Vec::with_capacity(BUCKET * 2 * 16);
        let mut buf = [0u8; 8192];
        let mut eof = false;
        while !eof && peaks.len() < MAX_BUCKETS {
            while carry.len() < BUCKET * 2 {
                match pipe.read(&mut buf) {
                    Ok(0) => { eof = true; break; }
                    Ok(n) => carry.extend_from_slice(&buf[..n]),
                    Err(_) => { eof = true; break; }
                }
            }
            if carry.is_empty() { break; }
            let take = carry.len().min(BUCKET * 2);
            let chunk: Vec<u8> = carry.drain(..take).collect();
            let mut mn = i16::MAX;
            let mut mx = i16::MIN;
            for c in chunk.chunks_exact(2) {
                let s = i16::from_le_bytes([c[0], c[1]]);
                mn = mn.min(s);
                mx = mx.max(s);
            }
            if mn == i16::MAX { mn = 0; }
            if mx == i16::MIN { mx = 0; }
            peaks.push(((mn >> 8) as i8, (mx >> 8) as i8));
        }
        let _ = child.kill();
        let _ = child.wait();
        let _ = tx.send(MediaEvent::Wave { path, peaks: Arc::new(peaks) });
    });
}

// ------------------------------------------------------------------ proxies
/// Generate a 540p H.264 proxy next to the source (project proxy dir).
pub fn spawn_proxy(job: u64, src: PathBuf, dest: PathBuf, tx: Sender<MediaEvent>) {
    std::thread::spawn(move || {
        let Some(bin) = ffmpeg() else {
            let _ = tx.send(MediaEvent::JobDone { job, result: Err("ffmpeg not found".into()) });
            return;
        };
        let total = probe(&src).map(|i| i.duration).unwrap_or(0.0);
        let Ok(mut child) = Command::new(bin)
            .args(["-hide_banner", "-loglevel", "error", "-y", "-i", &src.to_string_lossy(),
                   "-vf", "scale=-2:540", "-c:v", "libx264", "-preset", "veryfast", "-crf", "26",
                   "-c:a", "aac", "-b:a", "128k", "-movflags", "+faststart",
                   "-progress", "pipe:1", "-nostats", &dest.to_string_lossy()])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        else {
            let _ = tx.send(MediaEvent::JobDone { job, result: Err("failed to start ffmpeg".into()) });
            return;
        };
        pump_progress(&mut child, job, total, &tx);
        let status = child.wait();
        if status.map(|s| s.success()).unwrap_or(false) {
            let _ = tx.send(MediaEvent::JobDone { job, result: Ok(dest.to_string_lossy().to_string()) });
        } else {
            let _ = tx.send(MediaEvent::JobDone { job, result: Err("ffmpeg exited with error".into()) });
        }
    });
}

/// Two-pass vidstab stabilization: vidstabdetect then vidstabtransform.
pub fn spawn_stabilize(job: u64, src: PathBuf, dest: PathBuf, tx: Sender<MediaEvent>) {
    std::thread::spawn(move || {
        let Some(bin) = ffmpeg() else {
            let _ = tx.send(MediaEvent::JobDone { job, result: Err("ffmpeg not found".into()) });
            return;
        };
        let total = probe(&src).map(|i| i.duration).unwrap_or(0.0);
        let trf = std::env::temp_dir().join(format!("kestrelcut_stab_{job}.trf"));
        // pass 1: analyze
        let ok1 = Command::new(&bin)
            .args(["-hide_banner", "-loglevel", "error", "-y", "-i", &src.to_string_lossy(),
                   "-vf", &format!("vidstabdetect=shakiness=6:accuracy=12:result={}", trf.to_string_lossy()),
                   "-f", "null", "-"])
            .stdout(Stdio::null()).stderr(Stdio::null()).status()
            .map(|s| s.success()).unwrap_or(false);
        if !ok1 {
            let _ = tx.send(MediaEvent::JobDone { job, result: Err("vidstabdetect failed".into()) });
            return;
        }
        // pass 2: apply
        let Ok(mut child) = Command::new(bin)
            .args(["-hide_banner", "-loglevel", "error", "-y", "-i", &src.to_string_lossy(),
                   "-vf", &format!("vidstabtransform=input={}:smoothing=20:optzoom=1", trf.to_string_lossy()),
                   "-c:v", "libx264", "-preset", "veryfast", "-crf", "20",
                   "-c:a", "copy", "-movflags", "+faststart",
                   "-progress", "pipe:1", "-nostats", &dest.to_string_lossy()])
            .stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()
        else {
            let _ = tx.send(MediaEvent::JobDone { job, result: Err("vidstabtransform failed to start".into()) });
            return;
        };
        pump_progress(&mut child, job, total, &tx);
        let status = child.wait();
        let _ = std::fs::remove_file(&trf);
        if status.map(|s| s.success()).unwrap_or(false) {
            let _ = tx.send(MediaEvent::JobDone { job, result: Ok(dest.to_string_lossy().to_string()) });
        } else {
            let _ = tx.send(MediaEvent::JobDone { job, result: Err("vidstabtransform failed".into()) });
        }
    });
}

/// Read `-progress pipe:1` output, forwarding progress events until exit.
pub fn pump_progress(child: &mut std::process::Child, job: u64, total: f64, tx: &Sender<MediaEvent>) {
    let Some(mut out) = child.stdout.take() else { return };
    let mut buf = [0u8; 4096];
    let mut acc = String::new();
    loop {
        match out.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                acc.push_str(&String::from_utf8_lossy(&buf[..n]));
                while let Some(pos) = acc.find('\n') {
                    let line: String = acc.drain(..=pos).collect();
                    let line = line.trim();
                    let mut kv = line.splitn(2, '=');
                    let (k, v) = (kv.next().unwrap_or(""), kv.next().unwrap_or(""));
                    if k == "out_time_ms" || k == "out_time_us" {
                        if let Ok(us) = v.parse::<f64>() {
                            let t = us / 1_000_000.0;
                            let frac = if total > 0.0 { (t / total).clamp(0.0, 1.0) as f32 } else { 0.0 };
                            let _ = tx.send(MediaEvent::Progress { job, frac, fps: 0.0, speed: 0.0, out_time: t });
                        }
                    }
                }
            }
        }
    }
}

// ------------------------------------------------------------------ encoders
static ENCODERS: OnceLock<Vec<String>> = OnceLock::new();

pub fn available_encoders() -> &'static [String] {
    ENCODERS.get_or_init(|| {
        let Some(bin) = ffmpeg() else { return Vec::new() };
        let Ok(out) = Command::new(bin).args(["-hide_banner", "-encoders"]).output() else { return Vec::new() };
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|l| l.split_whitespace().nth(1).map(String::from))
            .collect()
    })
}

pub fn has_encoder(name: &str) -> bool { available_encoders().iter().any(|e| e == name) }

// ------------------------------------------------------------------ filters
static FILTERS: OnceLock<Vec<String>> = OnceLock::new();

/// Filter names built into this ffmpeg build (real capability probe — used
/// to hide features the bundled binary cannot run, e.g. vidstab).
pub fn available_filters() -> &'static [String] {
    FILTERS.get_or_init(|| {
        let Some(bin) = ffmpeg() else { return Vec::new() };
        let Ok(out) = Command::new(bin).args(["-hide_banner", "-filters"]).output() else { return Vec::new() };
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|l| {
                // format: " T.. name123 description" — take the filter name column
                let cols: Vec<&str> = l.split_whitespace().collect();
                if cols.len() >= 3 { cols.get(2).map(|s| s.trim_end_matches('_')).map(String::from) } else { None }
            })
            .collect()
    })
}

pub fn has_filter(name: &str) -> bool { available_filters().iter().any(|f| f == name) }

/// True when the bundle can run the two-pass vidstab stabilizer.
pub fn has_stabilizer() -> bool { has_filter("vidstabtransform") && has_filter("vidstabdetect") }

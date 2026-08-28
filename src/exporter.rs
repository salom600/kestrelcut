//! Export pipeline: builds one deterministic ffmpeg command from the timeline
//! (multi-track overlay graph, per-clip grade/FX/transform, audio mixdown),
//! hardware-accelerated encoders with quality mapping, live progress, and a
//! title rasterizer (ab_glyph + Arabic presentation forms).

use crate::media::{self, MediaEvent};
use crate::model::{Clip, ClipKind, MediaAsset, Project, TrackKind};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ExportSpec {
    pub out: PathBuf,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub range: (f64, f64),
    pub vcodec: String,
    pub acodec: String,
    pub quality: u32, // CRF-ish 14..32
    pub preset: String,
}

impl Default for ExportSpec {
    fn default() -> Self {
        Self {
            out: PathBuf::from("export.mp4"),
            width: 1920, height: 1080, fps: 30.0,
            range: (0.0, 0.0),
            vcodec: "libx264".into(),
            acodec: "aac".into(),
            quality: 21,
            preset: "veryfast".into(),
        }
    }
}

static EXPORT_PID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub fn cancel_export() {
    use std::sync::atomic::Ordering;
    let pid = EXPORT_PID.swap(0, Ordering::SeqCst);
    if pid == 0 { return; }
    #[cfg(unix)]
    { let _ = Command::new("kill").arg("-9").arg(pid.to_string()).status(); }
    #[cfg(windows)]
    { let _ = Command::new("taskkill").args(["/PID", &pid.to_string(), "/F", "/T"]).status(); }
}

fn set_child_pid(pid: u32) {
    use std::sync::atomic::Ordering;
    EXPORT_PID.store(pid, Ordering::SeqCst);
}
fn take_child_pid() -> u32 {
    use std::sync::atomic::Ordering;
    EXPORT_PID.swap(0, Ordering::SeqCst)
}

// ------------------------------------------------------------------ encoders
fn encoder_quality_args(vcodec: &str, q: u32) -> Vec<String> {
    match vcodec {
        "libx264" | "libx265" => vec!["-crf".into(), q.to_string(), "-preset".into(), "veryfast".into()],
        "h264_nvenc" | "hevc_nvenc" | "av1_nvenc" => {
            vec!["-preset".into(), "p3".into(), "-rc".into(), "vbr".into(), "-cq".into(), q.to_string(), "-b:v".into(), "0".into()]
        }
        "h264_qsv" | "hevc_qsv" | "av1_qsv" => vec!["-global_quality".into(), q.to_string(), "-preset".into(), "veryfast".into()],
        "h264_amf" | "hevc_amf" => vec!["-quality".into(), "balanced".into(), "-rc".into(), "cqp".into(), "-qp_i".into(), q.to_string(), "-qp_p".into(), q.to_string()],
        "libsvtav1" => vec!["-crf".into(), q.to_string(), "-preset".into(), "8".into()],
        _ => vec!["-crf".into(), q.to_string()],
    }
}

/// Hardware-capable encoders present in this ffmpeg build (real detection).
pub fn hw_encoders() -> Vec<(String, String)> {
    let mut v = Vec::new();
    let table = [
        ("h264_nvenc", "H.264 NVENC (NVIDIA)"),
        ("hevc_nvenc", "H.265 NVENC (NVIDIA)"),
        ("av1_nvenc", "AV1 NVENC (NVIDIA)"),
        ("h264_qsv", "H.264 QuickSync (Intel)"),
        ("hevc_qsv", "H.265 QuickSync (Intel)"),
        ("av1_qsv", "AV1 QuickSync (Intel)"),
        ("h264_amf", "H.264 AMF (AMD VCN)"),
        ("hevc_amf", "H.265 AMF (AMD VCN)"),
    ];
    for (id, label) in table {
        if media::has_encoder(id) { v.push((id.into(), label.into())); }
    }
    v
}

pub fn sw_encoders() -> Vec<(String, String)> {
    let mut v = Vec::new();
    for (id, label) in [
        ("libx264", "H.264 (x264)"),
        ("libx265", "H.265 / HEVC (x265)"),
        ("libsvtav1", "AV1 (SVT-AV1)"),
    ] {
        if media::has_encoder(id) { v.push((id.into(), label.into())); }
    }
    v
}

// ------------------------------------------------------------------ titles
const FONT: &[u8] = include_bytes!("../assets/fonts/NotoNaskhArabic-Regular.ttf");

/// Rasterize (possibly Arabic) text into a transparent RGBA PNG of W×H.
pub fn render_text_png(text: &str, size_px: f32, color: [u8; 3], w: u32, h: u32) -> Result<Vec<u8>, String> {
    use ab_glyph::{Font as _, FontRef, PxScale, ScaleFont};
    let font = FontRef::try_from_slice(FONT).map_err(|e| e.to_string())?;
    let visual = crate::arabic::shape_if_arabic(text);
    let scale = PxScale { x: size_px, y: size_px };
    let sf = font.as_scaled(scale);

    // measure
    let glyph_ids: Vec<_> = visual.chars()
        .filter(|c| !c.is_control())
        .map(|c| font.glyph_id(c))
        .collect();
    let total_w: f32 = glyph_ids.iter().map(|&g| sf.h_advance(g)).sum();
    let ascent = sf.ascent();
    let descent = sf.descent();

    let mut img = image::RgbaImage::new(w, h);
    let mut pen_x = ((w as f32 - total_w) / 2.0).max(4.0);
    let baseline = ((h as f32 + ascent + descent) / 2.0).max(ascent + 2.0);

    for &g in &glyph_ids {
        if g.0 == 0 { continue; }
        let glyph = ab_glyph::Glyph { id: g, scale, position: ab_glyph::point(pen_x, baseline) };
        if let Some(og) = font.outline_glyph(glyph) {
            let bb = og.px_bounds();
            og.draw(|x, y, a| {
                let px = x as u32 + bb.min.x as u32;
                let py = y as u32 + bb.min.y as u32;
                if px < w && py < h {
                    let p = img.get_pixel_mut(px, py);
                    let cov = (a * 255.0) as u8;
                    let blend = |bg: u8| -> u8 { ((color[0] as u32 * cov as u32 + bg as u32 * (255 - cov) as u32) / 255) as u8 };
                    // text over transparent background: straight alpha
                    let _ = blend;
                    *p = image::Rgba([color[0], color[1], color[2], cov]);
                }
            });
        }
        pen_x += sf.h_advance(g);
    }

    let mut cursor = std::io::Cursor::new(Vec::new());
    img.write_to(&mut cursor, image::ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(cursor.into_inner())
}

fn write_temp_title(id: u64, clip: &Clip, w: u32, h: u32) -> Result<PathBuf, String> {
    let td = clip.title.clone().unwrap_or_default();
    let scale = (w as f32 / 1920.0).max(0.25);
    let png = render_text_png(&td.text, td.size * scale, td.color, w, h)?;
    let dir = std::env::temp_dir().join("kestrelcut");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let p = dir.join(format!("title_{id}.png"));
    std::fs::write(&p, png).map_err(|e| e.to_string())?;
    Ok(p)
}

// ------------------------------------------------------------------ graph
struct VClip {
    clip: Clip,
    input_idx: usize,
    start: f64, // timeline (absolute) start within export
    dur: f64,
}

/// Build + run the export. Blocking — run on a worker thread.
pub fn run_export(job: u64, spec: ExportSpec, project: Project, assets: Vec<MediaAsset>, tx: Sender<MediaEvent>) {
    std::thread::spawn(move || {
        let res = export_blocking(&spec, &project, &assets, job, &tx);
        let _ = tx.send(MediaEvent::JobDone {
            job,
            result: res.map_err(|e| e),
        });
    });
}

fn export_blocking(spec: &ExportSpec, project: &Project, assets: &[MediaAsset], job: u64, tx: &Sender<MediaEvent>) -> Result<String, String> {
    let bin = media::ffmpeg().ok_or_else(|| "ffmpeg not found".to_string())?;
    let (r0, r1) = spec.range;
    let total = (r1 - r0).max(0.1);
    let (w, h) = (spec.width, spec.height);

    let asset = |p: &Option<PathBuf>| assets.iter().find(|a| p.as_ref() == Some(&a.path));

    let mut inputs: Vec<String> = Vec::new();
    let mut filters: Vec<String> = Vec::new();
    let mut vclips: Vec<VClip> = Vec::new();

    let mut in_idx = 0usize;
    // ---- base stream: bottom-most visual track
    let base_track = project.tracks.iter()
        .filter(|t| t.kind == TrackKind::Video && !t.hidden)
        .next();
    let mut base_parts: Vec<String> = Vec::new();
    let mut cursor = r0;
    if let Some(bt) = base_track {
        for c in bt.sorted_clips() {
            let s = c.tl_start.max(r0);
            let e = c.end().min(r1);
            if e - s < 0.02 { continue; }
            if s > cursor + 0.02 {
                // black gap segment as lavfi input (keeps concat uniform)
                let d = s - cursor;
                inputs.push(format!("-f lavfi -t {d:.3} -i color=c=black:s={w}x{h}:r={}", spec.fps));
                filters.push(format!("[{in_idx}:v]format=rgba,setsar=1[bg{in_idx}]"));
                base_parts.push(format!("[bg{in_idx}]"));
                in_idx += 1;
            }
            let rel = s - c.tl_start;
            let src_in = c.src_in + rel * c.speed as f64;
            let dur = e - s;
            let src = c.source.clone().unwrap_or_default();
            match c.kind {
                ClipKind::Image | ClipKind::Title => {
                    let p = if c.kind == ClipKind::Title {
                        write_temp_title(c.id, c, w, h)?
                    } else { src };
                    inputs.push(format!("-loop 1 -framerate {} -t {dur:.3} -i '{}'", spec.fps, p.to_string_lossy()));
                }
                _ => {
                    inputs.push(format!("-ss {src_in:.3} -t {dur:.3} -i '{}'", src.to_string_lossy()));
                }
            }
            let chain = media::video_filter_chain(&c.grade, &c.fx, dur, Some(w), Some(h), Some(spec.fps));
            filters.push(format!("[{in_idx}:v]{chain},setsar=1[b{in_idx}]"));
            base_parts.push(format!("[b{in_idx}]"));
            vclips.push(VClip { clip: c.clone(), input_idx: in_idx, start: s, dur });
            cursor = e;
            in_idx += 1;
        }
    }
    if cursor < r1 - 0.02 {
        let d = r1 - cursor;
        inputs.push(format!("-f lavfi -t {d:.3} -i color=c=black:s={w}x{h}:r={}", spec.fps));
        filters.push(format!("[{in_idx}:v]format=rgba,setsar=1[bg{in_idx}]"));
        base_parts.push(format!("[bg{in_idx}]"));
        in_idx += 1;
    }
    if base_parts.is_empty() {
        filters.push(format!("color=c=black:s={w}x{h}:d={total:.3}[base]"));
    } else if base_parts.len() == 1 {
        filters.push(format!("{}null[base]", base_parts[0]));
    } else {
        filters.push(format!("{}concat=n={}:v=1:a=0[base]", base_parts.join(""), base_parts.len()));
    }

    // ---- overlays: remaining visual tracks (V2, V3, ...)
    let mut current = "base".to_string();
    let mut overlay_n = 0;
    for t in project.tracks.iter().filter(|t| t.kind == TrackKind::Video && !t.hidden).skip(1) {
        for c in t.sorted_clips() {
            let s = c.tl_start.max(r0);
            let e = c.end().min(r1);
            if e - s < 0.02 { continue; }
            let rel = s - c.tl_start;
            let src_in = c.src_in + rel * c.speed as f64;
            let dur = e - s;
            let src = c.source.clone().unwrap_or_default();
            match c.kind {
                ClipKind::Image | ClipKind::Title => {
                    let p = if c.kind == ClipKind::Title {
                        write_temp_title(c.id, c, w, h)?
                    } else { src };
                    inputs.push(format!("-loop 1 -framerate {} -t {dur:.3} -i '{}'", spec.fps, p.to_string_lossy()));
                }
                _ => {
                    inputs.push(format!("-ss {src_in:.3} -t {dur:.3} -i '{}'", src.to_string_lossy()));
                }
            }
            let tf = c.transform;
            let mut chain = media::video_filter_chain(&c.grade, &c.fx, dur, None, None, None);
            chain.push_str(&format!(",scale=iw*{:.4}:-2", tf.scale.max(0.01)));
            if (tf.rotation.abs()) > 0.01 {
                let rad = tf.rotation.to_radians();
                chain.push_str(&format!(
                    ",rotate={rad:.5}:c=black@0:ow='hypot(iw,ih)':oh='hypot(iw,ih)'"));
            }
            chain.push_str(",format=rgba,colorchannelmixer=aa=");
            chain.push_str(&format!("{:.3}", tf.opacity.clamp(0.0, 1.0)));
            filters.push(format!("[{in_idx}:v]{chain}[o{in_idx}]"));
            let cx = format!("(main_w-overlay_w)/2+{:.4}*main_w/2", tf.x);
            let cy = format!("(main_h-overlay_h)/2+{:.4}*main_h/2", tf.y);
            filters.push(format!(
                "[{current}][o{in_idx}]overlay=x='{cx}':y='{cy}':enable='between(t,{:.3},{:.3})':eof_action=pass[n{overlay_n}]",
                s - r0, e - r0));
            current = format!("n{overlay_n}");
            overlay_n += 1;
            in_idx += 1;
        }
    }
    filters.push(format!("[{current}]format=yuv420p[vout]"));

    // ---- audio mixdown
    let any_solo = project.audio_tracks().iter().any(|t| t.solo);
    let mut a_labels: Vec<String> = Vec::new();
    for t in project.tracks.iter().filter(|t| t.kind == TrackKind::Audio) {
        if t.mute || (any_solo && !t.solo) { continue; }
        for c in t.sorted_clips() {
            let s = c.tl_start.max(r0);
            let e = c.end().min(r1);
            if e - s < 0.02 { continue; }
            let Some(src) = c.source.clone() else { continue };
            let Some(a) = asset(&Some(src.clone())) else { continue };
            if !a.has_audio { continue; }
            let rel = s - c.tl_start;
            let src_in = c.src_in + rel * c.speed as f64;
            let dur = e - s;
            inputs.push(format!("-ss {src_in:.3} -t {dur:.3} -i '{}'", src.to_string_lossy()));
            let mut chain = format!("[{in_idx}:a]aformat=sample_rates=48000:channel_layouts=stereo,aresample=48000");
            chain.push_str(&format!(",volume={:.1}dB", c.gain_db));
            // piecewise volume keyframes (timeline-relative seconds)
            let mut kfs = c.vol_kf.clone();
            kfs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            for win in kfs.windows(2) {
                chain.push_str(&format!(
                    ",volume={:.3}:enable='between(t,{:.3},{:.3})'",
                    win[1].1, (win[0].0 + rel).max(0.0), (win[1].0 + rel).max(0.0)));
            }
            let delay_ms = ((s - r0) * 1000.0).max(0.0) as i64;
            chain.push_str(&format!(",adelay={delay_ms}:all=1[a{in_idx}]"));
            filters.push(chain);
            a_labels.push(format!("[a{in_idx}]"));
            in_idx += 1;
        }
    }
    let map_audio: String = if a_labels.is_empty() {
        "-an".to_string()
    } else if a_labels.len() == 1 {
        format!("-map {}", a_labels[0])
    } else {
        filters.push(format!("{}amix=inputs={}:normalize=0[aout]", a_labels.join(""), a_labels.len()));
        "-map [aout]".to_string()
    };

    // ---- assemble command (args vector, no shell)
    let mut args: Vec<String> = vec!["-hide_banner".into(), "-loglevel".into(), "error".into(), "-y".into()];
    for i in inputs {
        for a in split_args(&i) { args.push(a); }
    }
    let fc = filters.join(";");
    args.push("-filter_complex".into());
    args.push(fc);
    args.push("-map".into());
    args.push("[vout]".into());
    if !map_audio.is_empty() && map_audio != "-an" {
        for a in split_args(&map_audio) { args.push(a); }
    } else {
        args.push("-an".into());
    }
    args.push("-c:v".into()); args.push(spec.vcodec.clone());
    for q in encoder_quality_args(&spec.vcodec, spec.quality) { args.push(q); }
    args.push("-pix_fmt".into()); args.push("yuv420p".into());
    args.push("-r".into()); args.push(format!("{}", spec.fps));
    args.push("-c:a".into()); args.push(spec.acodec.clone());
    args.push("-b:a".into()); args.push("192k".into());
    args.push("-movflags".into()); args.push("+faststart".into());
    args.push("-progress".into()); args.push("pipe:1".into());
    args.push("-nostats".into());
    args.push(spec.out.to_string_lossy().to_string());

    if let Some(parent) = spec.out.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let child = Command::new(&bin)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn: {e}"))?;
    set_child_pid(child.id());

    // progress pump on this thread
    let mut child = child;
    if let Some(mut out) = child.stdout.take() {
        let mut buf = [0u8; 4096];
        let mut acc = String::new();
        loop {
            match out.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    acc.push_str(&String::from_utf8_lossy(&buf[..n]));
                    while let Some(pos) = acc.find('\n') {
                        let line: String = acc.drain(..=pos).collect();
                        let mut kv = line.trim().splitn(2, '=');
                        let (k, v) = (kv.next().unwrap_or(""), kv.next().unwrap_or(""));
                        if k == "out_time_ms" {
                            if let Ok(us) = v.parse::<f64>() {
                                let t = us / 1_000_000.0;
                                let _ = tx.send(MediaEvent::Progress {
                                    job, frac: (t / total).clamp(0.0, 1.0) as f32,
                                    fps: 0.0, speed: 0.0, out_time: t,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    let stderr = child.stderr.take().map(|mut s| {
        let mut out = String::new();
        let _ = s.read_to_string(&mut out);
        out
    }).unwrap_or_default();
    let status = child.wait().map_err(|e| e.to_string())?;
    let _ = take_child_pid();
    if status.success() {
        Ok(spec.out.to_string_lossy().to_string())
    } else {
        Err(format!("ffmpeg failed: {}", stderr.lines().last().unwrap_or("unknown error")))
    }
}

/// Split a pre-joined "-flag value -i path" string (quoted paths supported).
fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_q = false;
    for ch in s.chars() {
        match ch {
            '\'' => in_q = !in_q,
            ' ' if !in_q => { if !cur.is_empty() { out.push(std::mem::take(&mut cur)); } }
            _ => cur.push(ch),
        }
    }
    if !cur.is_empty() { out.push(cur); }
    out
}

/// Save current preview frame (RGBA) to PNG — the Snapshot button backend.
pub fn save_frame_png(rgba: &[u8], w: u32, h: u32, path: &Path) -> Result<(), String> {
    let img = image::RgbaImage::from_raw(w, h, rgba.to_vec()).ok_or("bad frame")?;
    img.save_with_format(path, image::ImageFormat::Png).map_err(|e| e.to_string())
}

/// App icon generator (--gen-icon): gradient rounded square + play glyph.
pub fn gen_icon(path: &Path, size: u32) -> Result<(), String> {
    let mut img = image::RgbaImage::new(size, size);
    let s = size as f32;
    for y in 0..size {
        for x in 0..size {
            let fx = x as f32 / s;
            let fy = y as f32 / s;
            // rounded-rect mask
            let r = 0.18;
            let (cx, cy) = ((fx - 0.5).abs(), (fy - 0.5).abs());
            let inside = cx.max(cy - 0.0) < 0.5 - r || {
                let dx = (cx - (0.5 - r)).max(0.0);
                let dy = (cy - (0.5 - r)).max(0.0);
                dx * dx + dy * dy <= r * r
            };
            let col = if inside {
                let t = (fx + fy) / 2.0;
                let c0 = [47u8, 129, 247];
                let c1 = [20u8, 30, 60];
                image::Rgba([
                    (c0[0] as f32 * (1.0 - t) + c1[0] as f32 * t) as u8,
                    (c0[1] as f32 * (1.0 - t) + c1[1] as f32 * t) as u8,
                    (c0[2] as f32 * (1.0 - t) + c1[2] as f32 * t) as u8,
                    255,
                ])
            } else {
                image::Rgba([0, 0, 0, 0])
            };
            img.put_pixel(x, y, col);
        }
    }
    // play triangle
    let tri = [(0.40f32, 0.32f32), (0.40, 0.68), (0.68, 0.5)];
    for y in (0.3 * s) as u32..(0.7 * s) as u32 {
        for x in (0.35 * s) as u32..(0.72 * s) as u32 {
            let (px, py) = (x as f32 / s, y as f32 / s);
            let (a, b, c) = (tri[0], tri[1], tri[2]);
            let sign = |p: (f32, f32), q: (f32, f32), r: (f32, f32)| (q.0 - p.0) * (r.1 - p.1) - (q.1 - p.1) * (r.0 - p.0);
            let d1 = sign((px, py), a, b);
            let d2 = sign((px, py), b, c);
            let d3 = sign((px, py), c, a);
            let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
            let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
            if !(neg && pos) {
                img.put_pixel(x, y, image::Rgba([255, 255, 255, 235]));
            }
        }
    }
    if path.extension().and_then(|e| e.to_str()) == Some("ico") {
        use image::{ColorType, ImageEncoder};
        let raw = img.into_raw();
        let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
        let enc = image::codecs::ico::IcoEncoder::new(file);
        enc.write_image(&raw, size, size, image::ExtendedColorType::Rgba8).map_err(|e| e.to_string())?;
        Ok(())
    } else {
        img.save_with_format(path, image::ImageFormat::Png).map_err(|e| e.to_string())
    }
}

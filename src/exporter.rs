//! Export pipeline: builds deterministic ffmpeg commands from the timeline
//! (per-track xfade transition chains, per-clip grade/FX/transform with
//! keyframe expressions, blend modes, adjustment layers, reverse, audio
//! rack + ducking mixdown), hardware-accelerated encoders, live progress,
//! and a shaped title rasterizer (rustybuzz — real Arabic RTL joining).

use crate::media::{self, MediaEvent};
use crate::model::{Clip, ClipKind, MediaAsset, Project, TrackKind, Transform, Anim};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::Sender;
use std::sync::{atomic::AtomicU32};

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

static EXPORT_PID: AtomicU32 = AtomicU32::new(0);

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
pub const FONT: &[u8] = include_bytes!("../assets/fonts/NotoNaskhArabic-Regular.ttf");

/// Rasterize (possibly Arabic/RTL, shaped via rustybuzz) text into a
/// transparent RGBA PNG of W×H at the TitleData anchor with optional bar.
pub fn render_text_png(text: &str, size_px: f32, color: [u8; 3], w: u32, h: u32,
                       pos: [f32; 2], bar: f32, bar_color: [u8; 3], shadow: f32) -> Result<Vec<u8>, String> {
    use ab_glyph::{Font as _, FontRef, Glyph, PxScale, ScaleFont};
    let font = FontRef::try_from_slice(FONT).map_err(|e| e.to_string())?;
    let sh = crate::textshape::shape_text(&font, text, size_px);
    let advance = sh.advance;
    let scale = PxScale { x: size_px, y: size_px };
    let sf = font.as_scaled(scale);
    let ascent = sf.ascent();
    let descent = sf.descent();
    let line_h = ascent - descent;

    // block position (anchor): pos is the CENTER of the text block
    let block_w = advance;
    let bx = (w as f32 * pos[0] - block_w / 2.0).clamp(2.0, (w as f32 - block_w - 2.0).max(2.0));
    let baseline = (h as f32 * pos[1] + line_h / 2.0 - descent).clamp(ascent + 2.0, h as f32 - 2.0);

    let mut img = image::RgbaImage::new(w, h);

    // background bar behind the text block
    if bar > 0.01 {
        let pad_x = size_px * 0.45;
        let pad_y = size_px * 0.22;
        let rx = (bx - pad_x).max(0.0);
        let ry = (baseline - ascent - pad_y).max(0.0);
        let rw = (block_w + pad_x * 2.0).min(w as f32 - rx);
        let rh = (line_h + pad_y * 2.0).min(h as f32 - ry);
        let a = (bar.clamp(0.0, 1.0) * 255.0) as u8;
        for y in (ry as u32)..((ry + rh) as u32).min(h) {
            for x in (rx as u32)..((rx + rw) as u32).min(w) {
                img.put_pixel(x, y, image::Rgba([bar_color[0], bar_color[1], bar_color[2], a]));
            }
        }
    }

    let draw_glyph = |img: &mut image::RgbaImage, g: Glyph, fill: [u8; 3], strength: f32| {
        if let Some(og) = font.outline_glyph(g) {
            let bb = og.px_bounds();
            og.draw(|x, y, a| {
                let px = x as u32 + bb.min.x as u32;
                let py = y as u32 + bb.min.y as u32;
                if px < img.width() && py < img.height() {
                    let p = img.get_pixel_mut(px, py);
                    let cov = ((a * strength * 255.0) as u8).max(p.0[3]);
                    if strength < 1.0 {
                        // shadow pass: darken alpha only
                        *p = image::Rgba([0, 0, 0, cov]);
                    } else {
                        *p = image::Rgba([fill[0], fill[1], fill[2], cov.max(p.0[3])]);
                    }
                }
            });
        }
    };

    // shadow pass (offset down-right), then fill pass
    if shadow > 0.01 {
        let off = (size_px * 0.04).max(1.5);
        for g in &sh.glyphs {
            if g.id == 0 { continue; }
            let glyph = Glyph { id: ab_glyph::GlyphId(g.id), scale,
                position: ab_glyph::point(bx + g.x + off, baseline + g.y + off) };
            draw_glyph(&mut img, glyph, color, shadow.min(1.0));
        }
    }
    for g in &sh.glyphs {
        if g.id == 0 { continue; }
        let glyph = Glyph { id: ab_glyph::GlyphId(g.id), scale,
            position: ab_glyph::point(bx + g.x, baseline + g.y) };
        draw_glyph(&mut img, glyph, color, 1.0);
    }
    let _ = descent;

    let mut cursor = std::io::Cursor::new(Vec::new());
    img.write_to(&mut cursor, image::ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(cursor.into_inner())
}

fn write_temp_title(id: u64, clip: &Clip, w: u32, h: u32) -> Result<PathBuf, String> {
    let td = clip.title.clone().unwrap_or_default();
    let scale = (w as f32 / 1920.0).max(0.25);
    let png = render_text_png(&td.text, td.size * scale, td.color, w, h,
                              td.pos, td.bar, td.bar_color, td.shadow)?;
    let dir = std::env::temp_dir().join("kestrelcut");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let p = dir.join(format!("title_{id}.png"));
    std::fs::write(&p, png).map_err(|e| e.to_string())?;
    Ok(p)
}

// ------------------------------------------------------------------ keyframe expressions
/// Build a piecewise-linear (with easing) ffmpeg expression for a channel.
/// `t0` = output time of clip start; `unit` scales the value into filter units.
fn chan_expr(ch: &[(f64, f32, crate::model::Ease)], t0: f64, base: f32, unit: &str) -> String {
    use crate::model::Ease;
    if ch.is_empty() { return format!("{:.4}", base); }
    let ease_f = |e: Ease| match e {
        Ease::Linear => "p".to_string(),
        Ease::EaseIn => "p*p".to_string(),
        Ease::EaseOut => "1-(1-p)*(1-p)".to_string(),
        Ease::EaseInOut => "if(lt(p,0.5),2*p*p,1-2*(1-p)*(1-p))".to_string(),
    };
    let seg = |i: usize, inner: String| {
        if i == 0 { inner } else { format!("if(gte(t,{a:.3}),{inner})", a = ch[i].0 + t0) }
    };
    // before first kf
    let mut expr = format!("{:.4}", ch[0].1);
    // each segment
    for i in 0..ch.len() {
        let (a, va, e) = ch[i];
        let expr_seg = if i + 1 < ch.len() {
            let (b, vb, _) = ch[i + 1];
            let span = (b - a).max(1e-4);
            let p = format!("((t-{ta:.3})/{span:.4})", ta = a + t0);
            let interp = format!("{va:.4}+({vb:.4}-{va:.4})*({ef})", ef = ease_f(e));
            let interp = interp.replace("p", &format!("clip({p},0,1)"));
            // replace eased p refs
            format!("if(lt(t,{tb:.3}),{interp},", tb = b + t0) // will close later
        } else {
            format!("{va:.4}")
        };
        expr = if i + 1 < ch.len() {
            // wrap: need proper nesting — build from the end instead
            expr_seg // placeholder, real nesting assembled below
        } else { expr };
    }
    // assemble right-to-left for correct nesting
    let mut inner = format!("{:.4}", ch[ch.len() - 1].1);
    for i in (0..ch.len() - 1).rev() {
        let (a, va, e) = ch[i];
        let (b, vb, _) = ch[i + 1];
        let span = (b - a).max(1e-4);
        let p = format!("clip((t-{ta:.3})/{span:.4},0,1)", ta = a + t0);
        let interp = format!("{va:.4}+({vb:.4}-{va:.4})*({ef})", ef = ease_f(e).replace("p", &p));
        inner = format!("if(lt(t,{tb:.3}),{interp},{inner})", tb = b + t0);
    }
    let _ = seg;
    let out = format!("if(lt(t,{:.3}),{:.4},{})", ch[0].0 + t0, ch[0].1, inner);
    // unit wrapping is applied by the caller for coordinates that need main_w
    let _ = unit;
    out
}

/// Expression for one transform channel at output time (t is main timeline).
fn tf_expr(anim: &Anim, t0: f64, which: u8, base: &Transform) -> String {
    let (ch, bv) = match which {
        0 => (&anim.pos_x, base.x),
        1 => (&anim.pos_y, base.y),
        2 => (&anim.scale, base.scale),
        3 => (&anim.rotation, base.rotation),
        _ => (&anim.opacity, base.opacity),
    };
    chan_expr(ch, t0, bv, "")
}

// ------------------------------------------------------------------ graph
struct VSeg {
    clip: Clip,
    /// index of the ffmpeg input holding this segment
    input_idx: usize,
    start: f64, // timeline start (absolute)
    dur: f64,
    /// extended input length (transition tail) — actual -t value
    input_len: f64,
}

/// Transition lookup: does `right` have a transition from `left`, and is
/// there source room on the left clip to extend by the transition duration?
fn transition_between(project: &Project, left: &Clip, right: &Clip, assets: &[MediaAsset]) -> Option<crate::model::Transition> {
    let trans = right.trans_in?;
    if trans.dur < 0.05 { return None; }
    // adjacency
    if (left.end() - right.tl_start).abs() > 1e-4 { return None; }
    // room on the left clip tail
    let need = trans.dur * left.speed as f64;
    match left.kind {
        ClipKind::Image | ClipKind::Title => Some(trans), // stills loop — infinite room
        _ => {
            let total = left.source.as_ref()
                .and_then(|p| assets.iter().find(|a| a.path == *p))
                .map(|a| a.duration)?;
            if total - left.src_end() >= need - 1e-3 { Some(trans) } else { None }
        }
    }
}

/// Build + run the export. Blocking — run on a worker thread.
pub fn run_export(job: u64, spec: ExportSpec, project: Project, assets: Vec<MediaAsset>, tx: Sender<MediaEvent>) {
    std::thread::spawn(move || {
        let res = export_blocking(&spec, &project, &assets, job, &tx);
        let _ = tx.send(MediaEvent::JobDone { job, result: res.map_err(|e| e) });
    });
}

fn asset_dur(assets: &[MediaAsset], src: &Option<PathBuf>) -> Option<f64> {
    assets.iter().find(|a| src.as_ref() == Some(&a.path)).map(|a| a.duration)
}
fn export_blocking(spec: &ExportSpec, project: &Project, assets: &[MediaAsset], job: u64, tx: &Sender<MediaEvent>) -> Result<String, String> {
    let bin = media::ffmpeg().ok_or_else(|| "ffmpeg not found".to_string())?;
    let (r0, r1) = spec.range;
    let total = (r1 - r0).max(0.1);
    let (w, h) = (spec.width, spec.height);

    let mut inputs: Vec<String> = Vec::new();
    let mut filters: Vec<String> = Vec::new();
    let mut in_idx = 0usize;
    let mut run_n = 0usize;

    let video_tracks: Vec<&crate::model::Track> = project.tracks.iter()
        .filter(|t| t.kind == TrackKind::Video && !t.hidden)
        .collect();

    // ------------------------------------------------------------------
    // helpers
    // ------------------------------------------------------------------
    fn clip_input(c: &Clip, dur: f64, ext: f64, w: u32, h: u32, fps: f64,
                  inputs: &mut Vec<String>, in_idx: &mut usize) -> usize {
        let idx = *in_idx;
        let src = c.source.clone().unwrap_or_default();
        match c.kind {
            ClipKind::Image | ClipKind::Title => {
                let p = if c.kind == ClipKind::Title { write_temp_title(c.id, c, w, h).unwrap_or(src) } else { src };
                inputs.push(format!("-loop 1 -framerate {fps} -t {:.3} -i '{}'", dur + ext, p.to_string_lossy()));
            }
            _ => {
                if c.reverse {
                    inputs.push(format!("-ss {:.3} -t {:.3} -i '{}'", c.src_in, c.src_len().min(60.0), src.to_string_lossy()));
                } else {
                    inputs.push(format!("-ss {:.3} -t {:.3} -i '{}'", c.src_in, dur + ext, src.to_string_lossy()));
                }
            }
        }
        *in_idx += 1;
        idx
    }

    fn black_input(dur: f64, w: u32, h: u32, fps: f64, inputs: &mut Vec<String>, in_idx: &mut usize) -> usize {
        let idx = *in_idx;
        inputs.push(format!("-f lavfi -t {dur:.3} -i color=c=black:s={w}x{h}:r={fps}"));
        *in_idx += 1;
        idx
    }

    /// Build the BOTTOM track as one full-length stream (black gaps + xfade
    /// transition runs + concat). Returns the label, or None when empty.
    fn chain_track(
        track: &crate::model::Track, project: &Project, assets: &[MediaAsset],
        inputs: &mut Vec<String>, filters: &mut Vec<String>, in_idx: &mut usize,
        ti: usize, spec: &ExportSpec, r0: f64, r1: f64, w: u32, h: u32,
    ) -> Option<String> {
        let clips: Vec<Clip> = track.sorted_clips().into_iter().cloned()
            .filter(|c| c.end() > r0 && c.tl_start < r1 && c.kind != ClipKind::Adjustment)
            .collect();
        if clips.is_empty() { return None; }

        struct Pz { clip: Clip, dur: f64, label: String }
        let mut pieces: Vec<Pz> = Vec::new();
        let mut cursor = r0;
        for c in &clips {
            let s = c.tl_start.max(r0);
            let e = c.end().min(r1);
            if e - s < 0.02 { continue; }
            let trans = pieces.last().and_then(|p| transition_between(project, &p.clip, &c, assets))
                .filter(|_| (c.tl_start - cursor).abs() < 1e-4);
            if trans.is_none() && s > cursor + 0.02 {
                let d = s - cursor;
                let idx = black_input(d, w, h, spec.fps, inputs, in_idx);
                let lbl = format!("g{ti}{}", pieces.len());
                filters.push(format!("[{idx}:v]format=yuv420p,setsar=1[{lbl}]"));
                pieces.push(Pz { clip: black_piece(cursor, d), dur: d, label: lbl });
            }
            let d = e - s;
            let rel = s - c.tl_start;
            let mut cc = c.clone();
            if c.reverse {
                cc.src_in = c.src_in;
            } else {
                cc.src_in = c.src_in + rel * c.speed as f64;
            }
            let ext = trans.as_ref().map(|t| t.dur * c.speed as f64).unwrap_or(0.0);
            let idx = clip_input(&cc, d, ext, w, h, spec.fps, inputs, in_idx);
            let lbl = format!("p{ti}{}", pieces.len());
            let (g_eff, fx_eff) = project.effective_grade_fx_at(c, s + d / 2.0);
            let chain = if c.reverse {
                format!("reverse,{}", media::video_filter_chain(&g_eff, &fx_eff, d, Some(w), Some(h), Some(spec.fps)))
            } else {
                media::video_filter_chain(&g_eff, &fx_eff, d, Some(w), Some(h), Some(spec.fps))
            };
            filters.push(format!("[{idx}:v]{chain},setsar=1[{lbl}]"));
            pieces.push(Pz { clip: c.clone(), dur: d, label: lbl });
            cursor = e;
        }
        if pieces.is_empty() { return None; }
        if cursor < r1 - 0.02 {
            let d = r1 - cursor;
            let idx = black_input(d, w, h, spec.fps, inputs, in_idx);
            let lbl = format!("g{ti}{}", pieces.len());
            filters.push(format!("[{idx}:v]format=yuv420p,setsar=1[{lbl}]"));
            pieces.push(Pz { clip: black_piece(cursor, d), dur: d, label: lbl });
        }

        // xfade runs + concat joins
        let mut runs: Vec<Vec<usize>> = Vec::new();
        let mut cur: Vec<usize> = Vec::new();
        for pi in 0..pieces.len() {
            if pi == 0 { cur.push(pi); continue; }
            let trans = transition_between(project, &pieces[pi - 1].clip, &pieces[pi].clip, assets)
                .filter(|_| pieces[pi - 1].clip.name != "__black__" && pieces[pi].clip.name != "__black__");
            if trans.is_some() { cur.push(pi); }
            else { runs.push(std::mem::take(&mut cur)); cur.push(pi); }
        }
        if !cur.is_empty() { runs.push(cur); }

        let mut run_labels: Vec<String> = Vec::new();
        let mut xf_n = 0;
        for (ri, run) in runs.iter().enumerate() {
            if run.len() == 1 {
                run_labels.push(pieces[run[0]].label.clone());
            } else {
                let mut lbl = pieces[run[0]].label.clone();
                // cumulative timeline position where each window starts
                let mut off = pieces[run[0]].dur;
                for k in 1..run.len() {
                    let prev_i = run[k - 1];
                    let pi = run[k];
                    let Some(trans) = transition_between(project, &pieces[prev_i].clip, &pieces[pi].clip, assets) else { continue };
                    let out = format!("x{ti}_{ri}_{xf_n}"); xf_n += 1;
                    filters.push(format!(
                        "[{lbl}][{}]xfade=transition={}:duration={:.3}:offset={:.3}[{out}]",
                        pieces[pi].label, trans.kind.xfade_name(),
                        trans.dur.min(pieces[pi].dur).min(pieces[prev_i].dur + 10.0), off));
                    lbl = out;
                    off += pieces[pi].dur;
                }
                run_labels.push(lbl);
            }
        }
        if run_labels.len() == 1 {
            Some(run_labels[0].clone())
        } else {
            let n = run_labels.len();
            let out = format!("tr{ti}");
            filters.push(format!("{}concat=n={n}:v=1:a=0[{out}]",
                run_labels.iter().map(|l| format!("[{l}]")).collect::<String>()));
            Some(out)
        }
    }

    fn black_piece(start: f64, dur: f64) -> Clip {
        let mut c = crate::model::title_clip("__black__", start, dur);
        c.kind = ClipKind::Adjustment;
        c
    }

    /// Overlay one UPPER-track run (1 clip, or a transitioned pair) onto
    /// `base`, with transform/blend. Updates `base` label.
    #[allow(clippy::too_many_arguments)]
    fn overlay_run(
        group: &[Clip], project: &Project, assets: &[MediaAsset],
        inputs: &mut Vec<String>, filters: &mut Vec<String>, in_idx: &mut usize,
        base: &mut String, ti: usize, rn: &mut usize, spec: &ExportSpec,
        r0: f64, r1: f64, w: u32, h: u32,
    ) -> Result<(), String> {
        let first = &group[0];
        let last = &group[group.len() - 1];
        let s = first.tl_start.max(r0);
        let e = last.end().min(r1);
        if e - s < 0.02 { return Ok(()); }
        let style_clip = if group.len() == 1 { first } else { &group[1] };
        let _ = style_clip;

        let run_in = if group.len() == 1 {
            let c = first;
            let rel = s - c.tl_start;
            let mut cc = c.clone();
            cc.src_in = c.src_in + rel * c.speed as f64;
            let d = e - s;
            let idx = clip_input(&cc, d, 0.0, w, h, spec.fps, inputs, in_idx);
            let (g_eff, fx_eff) = project.effective_grade_fx_at(c, s + d / 2.0);
            let chain = media::video_filter_chain(&g_eff, &fx_eff, d, Some(w), Some(h), Some(spec.fps));
            let lbl = format!("rl{ti}_{rn}");
            filters.push(format!("[{idx}:v]{chain},setsar=1[{lbl}]"));
            lbl
        } else {
            let lc = &group[0];
            let rc = &group[1];
            let Some(trans) = transition_between(project, lc, rc, assets) else { return Ok(()) };
            let le = lc.end().min(r1);
            let ls = lc.tl_start.max(r0);
            let rs = rc.tl_start.max(r0);
            let re = rc.end().min(r1);
            let mut cc0 = lc.clone();
            cc0.src_in = lc.src_in + (ls - lc.tl_start) * lc.speed as f64;
            let mut cc1 = rc.clone();
            cc1.src_in = rc.src_in + (rs - rc.tl_start) * rc.speed as f64;
            let i0 = clip_input(&cc0, le - ls, trans.dur * lc.speed as f64, w, h, spec.fps, inputs, in_idx);
            let i1 = clip_input(&cc1, re - rs, 0.0, w, h, spec.fps, inputs, in_idx);
            let (g0, f0) = project.effective_grade_fx_at(lc, ls + (le - ls) / 2.0);
            let (g1, f1) = project.effective_grade_fx_at(rc, rs + (re - rs) / 2.0);
            let c0 = media::video_filter_chain(&g0, &f0, le - ls, Some(w), Some(h), Some(spec.fps));
            let c1 = media::video_filter_chain(&g1, &f1, re - rs, Some(w), Some(h), Some(spec.fps));
            filters.push(format!("[{i0}:v]{c0},setsar=1[xa{ti}_{rn}]"));
            filters.push(format!("[{i1}:v]{c1},setsar=1[xb{ti}_{rn}]"));
            filters.push(format!(
                "[xa{ti}_{rn}][xb{ti}_{rn}]xfade=transition={}:duration={:.3}:offset={:.3}[rl{ti}_{rn}]",
                trans.kind.xfade_name(), trans.dur.min((le - ls).min(re - rs)), le - ls));
            format!("rl{ti}_{rn}")
        };

        let xf_raw = transform_suffix(style_clip, r0);
        // a chain must START with a filter name — strip the leading comma
        let xf = if xf_raw.is_empty() { "null".to_string() } else { xf_raw.trim_start_matches(',').to_string() };
        let overlaid = format!("oc{ti}_{rn}");
        let blend = style_clip.blend;
        let (ox, oy) = (pos_x_expr(style_clip, s - r0), pos_y_expr(style_clip, s - r0));
        if blend == crate::model::BlendMode::Normal {
            filters.push(format!(
                "[{run_in}]{xf}[of{ti}_{rn}];[{base}][of{ti}_{rn}]overlay=x='{ox}':y='{oy}':enable='between(t,{a:.3},{b:.3})':eof_action=pass[{overlaid}]",
                a = s - r0, b = e - r0));
        } else {
            // real blend: normalize to canvas size then blend
            filters.push(format!(
                "[{run_in}]{xf},scale={w}:{h}[bf{ti}_{rn}];[{base}][bf{ti}_{rn}]blend=all_mode={mode}:all_opacity={op:.3}:enable='between(t,{a:.3},{b:.3})'[{overlaid}]",
                mode = ffmpeg_blend_name(&blend), op = style_clip.transform.opacity.clamp(0.0, 1.0),
                a = s - r0, b = e - r0));
        }
        *base = overlaid;
        *rn += 1;
        Ok(())
    }

    // ---- compose ------------------------------------------------------------
    let bottom_label = video_tracks.first()
        .and_then(|t| chain_track(t, project, assets, &mut inputs, &mut filters, &mut in_idx, 0, spec, r0, r1, w, h));

    let mut cur = match bottom_label {
        Some(l) => l,
        None => {
            let idx = black_input(total, w, h, spec.fps, &mut inputs, &mut in_idx);
            filters.push(format!("[{idx}:v]format=yuv420p,setsar=1[bc0]"));
            "bc0".to_string()
        }
    };

    // upper tracks: overlay each transition-run with per-run transform/blend
    for (ti, track) in video_tracks.iter().enumerate().skip(1) {
        let clips: Vec<Clip> = track.sorted_clips().into_iter().cloned()
            .filter(|c| c.end() > r0 && c.tl_start < r1 && c.kind != ClipKind::Adjustment)
            .collect();
        let mut groups: Vec<Vec<Clip>> = Vec::new();
        for c in clips {
            let linked = groups.last_mut().and_then(|g| g.last())
                .map(|lc| transition_between(project, lc, &c, assets).is_some())
                .unwrap_or(false);
            if linked { groups.last_mut().unwrap().push(c); }
            else { groups.push(vec![c]); }
        }
        for group in groups {
            overlay_run(&group, project, assets, &mut inputs, &mut filters, &mut in_idx,
                        &mut cur, ti, &mut run_n, spec, r0, r1, w, h)?;
        }
    }

    filters.push(format!("[{cur}]format=yuv420p[voutf]"));
    let vout = "[voutf]".to_string();

    // ---- audio mixdown -------------------------------------------------------
    let audio_out = audio_graph(project, assets, spec, &mut inputs, &mut filters, &mut in_idx, r0, r1);

    // ---- assemble ------------------------------------------------------------
    let mut args: Vec<String> = vec!["-hide_banner".into(), "-loglevel".into(), "error".into(), "-y".into()];
    for i in inputs {
        for a in split_args(&i) { args.push(a); }
    }
    args.push("-filter_complex".into());
    args.push(filters.join(";"));
    args.push("-map".into());
    args.push(vout);
    match audio_out {
        AudioOut::None => { args.push("-an".into()); }
        AudioOut::Map(l) => { args.push("-map".into()); args.push(l); }
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
        let tail: Vec<&str> = stderr.lines().rev().take(6).collect();
        let mut tail: Vec<&str> = tail.into_iter().rev().collect();
        tail.dedup();
        Err(format!("ffmpeg failed: {}", tail.join(" | ")))
    }
}

enum AudioOut { None, Map(String) }

fn ffmpeg_blend_name(m: &crate::model::BlendMode) -> &'static str {
    use crate::model::BlendMode::*;
    match m {
        Normal => "normal", Multiply => "multiply", Screen => "screen",
        Overlay => "overlay", SoftLight => "softlight", HardLight => "hardlight",
        Darken => "darken", Lighten => "lighten", Difference => "difference",
    }
}

/// Static-or-keyframed transform suffix (scale/rotate/alpha) for an overlay
/// run stream. Position goes into overlay x/y separately.
fn transform_suffix(c: &Clip, r0: f64) -> String {
    let tf = &c.transform;
    let anim = &c.anim;
    let mut out = String::new();
    if anim.scale.is_empty() {
        if (tf.scale - 1.0).abs() > 0.005 {
            out.push_str(&format!(",scale=iw*{:.4}:-2", tf.scale.max(0.01)));
        }
    } else {
        let se = chan_expr(&anim.scale, c.tl_start - r0, tf.scale, "");
        out.push_str(&format!(",scale=w='max(2,trunc(iw*({se})))':h=-2:eval=frame"));
    }
    if anim.rotation.is_empty() {
        if tf.rotation.abs() > 0.01 {
            out.push_str(&format!(",rotate={:.5}:c=black@0:ow='hypot(iw,ih)':oh='hypot(iw,ih)'", tf.rotation.to_radians()));
        }
    } else {
        let re = chan_expr(&anim.rotation, c.tl_start - r0, tf.rotation, "");
        out.push_str(&format!(",rotate='({re})*PI/180':c=black@0:ow='hypot(iw,ih)':oh='hypot(iw,ih)'"));
    }
    out.push_str(",format=rgba");
    if anim.opacity.is_empty() {
        if tf.opacity < 0.999 {
            out.push_str(&format!(",colorchannelmixer=aa={:.3}", tf.opacity.clamp(0.0, 1.0)));
        }
    } else {
        let ch = &anim.opacity;
        let has_zero = ch.iter().any(|(_, v, _)| *v <= 0.01);
        if has_zero {
            for w2 in ch.windows(2) {
                let ((a, va, _), (b, vb, _)) = (w2[0], w2[1]);
                let t0 = c.tl_start - r0;
                if va > 0.9 && vb < 0.1 {
                    out.push_str(&format!(",fade=t=out:st={:.3}:d={:.3}:alpha=1", a + t0, (b - a).max(0.02)));
                } else if va < 0.1 && vb > 0.9 {
                    out.push_str(&format!(",fade=t=in:st={:.3}:d={:.3}:alpha=1", a + t0, (b - a).max(0.02)));
                }
            }
        } else {
            let mean: f32 = ch.iter().map(|(_, v, _)| v).sum::<f32>() / ch.len().max(1) as f32;
            out.push_str(&format!(",colorchannelmixer=aa={:.3}", mean.clamp(0.0, 1.0)));
        }
    }
    out
}

fn pos_x_expr(c: &Clip, t0: f64) -> String {
    if c.anim.pos_x.is_empty() {
        format!("(main_w-overlay_w)/2+{:.4}*main_w/2", c.transform.x)
    } else {
        let e = chan_expr(&c.anim.pos_x, t0, c.transform.x, "");
        format!("(main_w-overlay_w)/2+({e})*main_w/2")
    }
}

fn pos_y_expr(c: &Clip, t0: f64) -> String {
    if c.anim.pos_y.is_empty() {
        format!("(main_h-overlay_h)/2+{:.4}*main_h/2", c.transform.y)
    } else {
        let e = chan_expr(&c.anim.pos_y, t0, c.transform.y, "");
        format!("(main_h-overlay_h)/2+({e})*main_h/2")
    }
}

/// Build the full audio mixdown (rack filters, ducking keyframes as a linear
/// expression, delay placement). Returns the output label to map.
fn audio_graph(
    project: &Project, _assets: &[MediaAsset], spec: &ExportSpec,
    inputs: &mut Vec<String>, filters: &mut Vec<String>, in_idx: &mut usize,
    r0: f64, r1: f64,
) -> AudioOut {
    let any_solo = project.audio_tracks().iter().any(|t| t.solo);
    let mut a_labels: Vec<String> = Vec::new();
    for t in project.tracks.iter().filter(|t| t.kind == TrackKind::Audio) {
        if t.mute || (any_solo && !t.solo) { continue; }
        for c in t.sorted_clips() {
            let s = c.tl_start.max(r0);
            let e = c.end().min(r1);
            if e - s < 0.02 { continue; }
            let Some(src) = c.source.clone() else { continue };
            let rel = s - c.tl_start;
            let src_in = c.src_in + rel * c.speed as f64;
            let dur = e - s;
            inputs.push(format!("-ss {src_in:.3} -t {dur:.3} -i '{}'", src.to_string_lossy()));
            let idx = *in_idx; *in_idx += 1;
            let mut chain = format!("[{idx}:a]aformat=sample_rates=48000:channel_layouts=stereo,aresample=48000");
            if c.reverse { chain.push_str(",areverse"); }
            let base_chain = media::audio_filter_chain(&c.afx, 0.0, c.fx.fade_in, c.fx.fade_out, c.src_dur);
            if !base_chain.is_empty() { chain.push_str(&format!(",{base_chain}")); }
            let mut kfs = c.vol_kf.clone();
            kfs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            let vol_expr = vol_expression(&kfs, rel, c.gain_db);
            chain.push_str(&format!(",volume='{vol_expr}':eval=frame"));
            let delay_ms = ((s - r0) * 1000.0).max(0.0) as i64;
            chain.push_str(&format!(",adelay={delay_ms}:all=1[a{idx}]"));
            filters.push(chain);
            a_labels.push(format!("[a{idx}]"));
        }
    }
    if a_labels.is_empty() {
        AudioOut::None
    } else if a_labels.len() == 1 {
        AudioOut::Map(a_labels[0].clone())
    } else {
        let n = a_labels.len();
        filters.push(format!("{}amix=inputs={n}:normalize=0[aout]", a_labels.join("")));
        AudioOut::Map("[aout]".to_string())
    }
}

/// Piecewise-linear volume expression (incl. gain_db base and ducking kfs).
fn vol_expression(kfs: &[(f64, f32)], rel: f64, gain_db: f32) -> String {
    let base = 10f32.powf(gain_db / 20.0);
    if kfs.is_empty() {
        return format!("{base:.4}");
    }
    let mut inner = format!("{:.4}", kfs[kfs.len() - 1].1 * base);
    for i in (0..kfs.len().saturating_sub(1)).rev() {
        let (a, va) = kfs[i];
        let (b, vb) = kfs[i + 1];
        let span = (b - a).max(1e-4);
        let p = format!("clip((t-{ta:.3})/{span:.4},0,1)", ta = a + rel);
        inner = format!(
            "if(lt(t,{tb:.3}),{v0:.4}+({v1:.4}-{v0:.4})*({p}),{inner})",
            tb = b + rel, v0 = va * base, v1 = vb * base
        );
    }
    format!("if(lt(t,{:.3}),{:.4},{})", kfs[0].0 + rel, kfs[0].1 * base, inner)
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

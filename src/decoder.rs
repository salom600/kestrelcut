//! Live preview decoder: streams RGBA frames from an ffmpeg subprocess.
//!
//! Stability contract: bounded channel (4 frames) + process isolation + kill
//! on drop. A stuck/crashing decoder can never hang or crash the UI.
//!
//! v0.3 smoothness model: decoders ALWAYS stream (no per-frame restarts).
//! Pacing comes free from backpressure — when the UI stops draining, the
//! bounded channel fills, ffmpeg blocks writing to the pipe, decoding pauses.
//! Seeking forward drains (skips) frames as fast as the decoder produces
//! them; only BACKWARD seeks past the hysteresis restart the process.

use crate::media;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::mpsc::{sync_channel, Receiver, TryRecvError};

#[derive(Clone)]
pub struct DecoderReq {
    pub path: PathBuf,
    /// Source offset the stream starts at (decoder pts 0 == this position).
    pub src_in: f64,
    pub filters: String, // full chain incl. scale/fps/format=rgba
    pub w: u32,
    pub h: u32,
    /// Output frame rate (frames per stream-second). The fps filter emits CFR.
    pub fps: f64,
    /// Still = grab exactly one frame then stop (reverse-clip preview and
    /// thumbnails). Stream = continuous playback.
    pub mode: DecodeMode,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DecodeMode { Still, Stream }

#[derive(Clone)]
pub struct Frame {
    pub pts: f64, // seconds from decode start (== seconds after src_in)
    pub w: u32,
    pub h: u32,
    pub rgba: Arc<Vec<u8>>,
}

pub enum DecEvent {
    Frame(Frame),
    Eof,
    Failed(String),
}

pub struct Decoder {
    child: Option<Child>,
    rx: Receiver<DecEvent>,
    pub w: u32,
    pub h: u32,
    pub src_in: f64,
    /// Newest frame drained so far (kept so scrubbing never goes black).
    latest: Option<Frame>,
    eof: bool,
    pub last_error: Option<String>,
}

impl Decoder {
    pub fn start(req: DecoderReq) -> Result<Decoder, String> {
        let bin = media::ffmpeg().ok_or_else(|| "ffmpeg not found".to_string())?;
        let mut cmd = Command::new(bin);
        cmd.args([
            "-hide_banner", "-loglevel", "error", "-hwaccel", "auto",
            "-ss", &format!("{:.3}", req.src_in.max(0.0)),
            "-i", &req.path.to_string_lossy(),
            "-an",
            "-vf", &req.filters,
            "-f", "rawvideo",
            "-pix_fmt", "rgba",
            "pipe:1",
        ]);
        if req.mode == DecodeMode::Still {
            cmd.args(["-frames:v", "1"]);
        }
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = cmd.spawn().map_err(|e| format!("decoder spawn: {e}"))?;

        let stdout = child.stdout.take().ok_or("no stdout")?;
        let stderr = child.stderr.take();
        // capture the ffmpeg error tail so failures can be shown in the UI
        let err_tail: Arc<std::sync::Mutex<String>> = Arc::new(std::sync::Mutex::new(String::new()));
        if let Some(mut se) = stderr {
            let tail = err_tail.clone();
            std::thread::Builder::new().name("decerr".into()).spawn(move || {
                let mut buf = [0u8; 512];
                let mut acc = String::new();
                loop {
                    match se.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            acc.push_str(&String::from_utf8_lossy(&buf[..n]));
                            // keep only the tail
                            if acc.len() > 800 { let cut = acc.len() - 800; acc = acc.split_off(cut); }
                            if let Ok(mut t) = tail.lock() { *t = acc.clone(); }
                        }
                    }
                }
            }).ok();
        }

        let (tx, rx) = sync_channel::<DecEvent>(4);
        let (w, h) = (req.w.max(2), req.h.max(2));
        let still = req.mode == DecodeMode::Still;
        // NOTE: no pacing sleeps — backpressure through the bounded channel
        // (and the OS pipe) throttles the decoder to the UI's drain rate.
        std::thread::Builder::new().name("decoder".into()).spawn(move || {
            let mut pipe = stdout;
            let frame_len = (w as usize) * (h as usize) * 4;
            let mut buf = vec![0u8; frame_len];
            let mut n = 0u64;
            loop {
                match read_exact_or_eof(&mut pipe, &mut buf) {
                    Ok(true) => {}
                    Ok(false) => { break; }
                    Err(_) => {
                        let tail = err_tail.lock().ok().map(|m| m.trim().to_string()).unwrap_or_default();
                        let _ = tx.send(DecEvent::Failed(if tail.is_empty() { "pipe error".into() } else { tail }));
                        break;
                    }
                }
                let frame = Frame {
                    pts: n as f64 / req.fps.max(1.0),
                    w, h,
                    rgba: Arc::new(buf.clone()),
                };
                if tx.send(DecEvent::Frame(frame)).is_err() { break; }
                n += 1;
                if still { break; } // one frame is all we need
            }
            if n == 0 {
                let tail = err_tail.lock().ok().map(|m| m.trim().to_string()).unwrap_or_default();
                let msg = if tail.is_empty() {
                    "no video output (end of stream before first frame)".to_string()
                } else { tail };
                let _ = tx.send(DecEvent::Failed(msg));
            } else if !still {
                let _ = tx.send(DecEvent::Eof);
            }
        }).map_err(|e| e.to_string())?;

        Ok(Decoder { child: Some(child), rx, w, h, src_in: req.src_in, latest: None, eof: false, last_error: None })
    }

    /// Drain events. When `until_pts` is set, frames older than it are
    /// discarded (forward skip) and the newest frame at/before it is kept.
    /// Returns the frame to display.
    pub fn poll(&mut self, until_pts: Option<f64>) -> Option<Frame> {
        loop {
            match self.rx.try_recv() {
                Ok(DecEvent::Frame(f)) => {
                    match until_pts {
                        Some(tp) if f.pts <= tp + 1e-3 => {
                            // this frame is at/before the target — display it
                            self.latest = Some(f);
                        }
                        Some(tp) => {
                            // first frame BEYOND the target: if we have nothing
                            // at/before yet, take it (nearest available);
                            // otherwise keep the earlier one and push nothing
                            // back (bounded channel — dropping is fine, the
                            // decoder is ahead of the target now).
                            if self.latest.is_none() { self.latest = Some(f); }
                            else { break; }
                        }
                        None => { self.latest = Some(f); }
                    }
                }
                Ok(DecEvent::Eof) => { self.eof = true; break; }
                Ok(DecEvent::Failed(msg)) => { self.eof = true; self.last_error = Some(msg); break; }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => { self.eof = true; break; }
            }
        }
        self.latest.clone()
    }

    pub fn is_eof(&self) -> bool { self.eof }
    pub fn has_frame(&self) -> bool { self.latest.is_some() }
    /// pts of the newest drained frame (decoder position on its own timeline).
    pub fn head_pts(&self) -> Option<f64> { self.latest.as_ref().map(|f| f.pts) }
}

fn read_exact_or_eof<R: Read>(r: &mut R, buf: &mut [u8]) -> Result<bool, ()> {
    let mut off = 0;
    while off < buf.len() {
        match r.read(&mut buf[off..]) {
            Ok(0) => return Ok(off == 0), // clean EOF only if nothing read
            Ok(n) => off += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(()),
        }
    }
    Ok(true)
}

impl Drop for Decoder {
    fn drop(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

// ------------------------------------------------------------------ audio
#[cfg(feature = "audio")]
pub mod audio {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    struct Ring {
        q: VecDeque<i16>,
        cap: usize,
    }

    /// Single-clip audio monitor: pipes s16le stereo from ffmpeg into the
    /// default output device via cpal. Honors the per-clip processing chain
    /// so the preview matches the export. Degrades gracefully without a
    /// device.
    pub struct Monitor {
        child: Child,
        _handle: std::thread::JoinHandle<()>,
        stream: cpal::Stream,
    }

    impl Monitor {
        pub fn start(path: PathBuf, src_in: f64, dur: f64, filters: &str) -> Option<Monitor> {
            use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
            let device = cpal::default_host().default_output_device()?;
            let cfg = device.default_output_config().ok()?;
            let bin = media::ffmpeg()?;
            let mut cmd = Command::new(bin);
            cmd.args(["-hide_banner", "-loglevel", "error",
                      "-ss", &format!("{src_in:.3}"), "-t", &format!("{dur:.3}"),
                      "-i", &path.to_string_lossy(), "-ac", "2", "-ar", "48000"]);
            if !filters.is_empty() {
                cmd.args(["-af", filters]);
            }
            cmd.args(["-f", "s16le", "pipe:1"])
                .stdout(Stdio::piped())
                .stderr(Stdio::null());
            let mut child = cmd.spawn().ok()?;
            let stdout = child.stdout.take()?;
            let ring = Arc::new(Mutex::new(Ring { q: VecDeque::with_capacity(96_000), cap: 96_000 }));
            let ring_w = ring.clone();
            let handle = std::thread::spawn(move || {
                let mut pipe = stdout;
                let mut buf = [0u8; 8192];
                loop {
                    match pipe.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if let Ok(mut r) = ring_w.lock() {
                                let chunk = buf[..n].chunks_exact(2)
                                    .map(|c| i16::from_le_bytes([c[0], c[1]]));
                                for s in chunk {
                                    if r.q.len() >= r.cap { r.q.pop_front(); }
                                    r.q.push_back(s);
                                }
                            }
                        }
                    }
                }
            });

            let sample_format = cfg.sample_format();
            let ring_c = ring.clone();
            let err_fn = |e| eprintln!("audio: {e}");
            let stream = match sample_format {
                cpal::SampleFormat::I16 => device.build_output_stream(
                    &cfg.into(),
                    move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                        if let Ok(mut r) = ring_c.lock() {
                            for d in data.iter_mut() {
                                *d = r.q.pop_front().unwrap_or(0);
                            }
                        }
                    },
                    err_fn, None,
                ),
                cpal::SampleFormat::F32 => device.build_output_stream(
                    &cfg.into(),
                    move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                        if let Ok(mut r) = ring_c.lock() {
                            for d in data.iter_mut() {
                                *d = r.q.pop_front().map(|s| s as f32 / 32768.0).unwrap_or(0.0);
                            }
                        }
                    },
                    err_fn, None,
                ),
                _ => return None,
            }.ok()?;
            stream.play().ok()?;
            Some(Monitor { child, _handle: handle, stream })
        }
    }

    impl Drop for Monitor {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

// No-op audio monitor when built without the `audio` feature (keeps the
// player wiring identical; app degrades to silent preview).
#[cfg(not(feature = "audio"))]
pub mod audio {
    use std::path::PathBuf;
    pub struct Monitor;
    impl Monitor {
        pub fn start(_path: PathBuf, _src_in: f64, _dur: f64, _filters: &str) -> Option<Monitor> { None }
    }
}

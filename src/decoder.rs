//! Live preview decoder: streams RGBA frames from an ffmpeg subprocess.
//!
//! Stability contract: bounded channel (4 frames) + process isolation + kill
//! on drop. A stuck/crashing decoder can never hang or crash the UI.

use crate::media;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TryRecvError};
use std::time::{Duration, Instant};

pub struct DecoderReq {
    pub path: PathBuf,
    pub src_in: f64,
    pub filters: String, // full chain incl. scale/fps/format=rgba
    pub w: u32,
    pub h: u32,
    pub fps: f64,
}

#[derive(Clone)]
pub struct Frame {
    pub pts: f64, // seconds from decode start (clip-relative)
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
    latest: Option<Frame>,
    eof: bool,
}

impl Decoder {
    pub fn start(req: DecoderReq) -> Result<Decoder, String> {
        let bin = media::ffmpeg().ok_or_else(|| "ffmpeg not found".to_string())?;
        let mut child = Command::new(bin)
            .args([
                "-hide_banner", "-loglevel", "error", "-hwaccel", "auto",
                "-ss", &format!("{:.3}", req.src_in.max(0.0)),
                "-i", &req.path.to_string_lossy(),
                "-an",
                "-vf", &req.filters,
                "-f", "rawvideo",
                "-pix_fmt", "rgba",
                "pipe:1",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("decoder spawn: {e}"))?;

        let stdout = child.stdout.take().ok_or("no stdout")?;
        let (tx, rx) = sync_channel::<DecEvent>(4);
        let (w, h, fps) = (req.w.max(2), req.h.max(2), req.fps.max(1.0));
        std::thread::Builder::new().name("decoder".into()).spawn(move || {
            let mut pipe = stdout;
            let frame_len = (w as usize) * (h as usize) * 4;
            let mut buf = vec![0u8; frame_len];
            let t0 = Instant::now();
            let mut n = 0u64;
            loop {
                // pacing: deliver frames at the sequence rate, not faster
                let target = Duration::from_secs_f64(n as f64 / fps);
                let elapsed = t0.elapsed();
                if target > elapsed {
                    std::thread::sleep(target - elapsed);
                }
                match read_exact_or_eof(&mut pipe, &mut buf) {
                    Ok(true) => {}
                    Ok(false) => { let _ = tx.send(DecEvent::Eof); break; }
                    Err(_) => { let _ = tx.send(DecEvent::Failed("pipe error".into())); break; }
                }
                let frame = Frame {
                    pts: n as f64 / fps,
                    w, h,
                    rgba: Arc::new(buf.clone()),
                };
                if tx.send(DecEvent::Frame(frame)).is_err() { break; }
                n += 1;
            }
            let _ = tx.send(DecEvent::Eof);
        }).map_err(|e| e.to_string())?;

        Ok(Decoder { child: Some(child), rx, w, h, src_in: req.src_in, latest: None, eof: false })
    }

    /// Drain events; returns the newest frame (stale ones are dropped —
    /// bounded latency, bounded memory).
    pub fn poll(&mut self) -> Option<Frame> {
        loop {
            match self.rx.try_recv() {
                Ok(DecEvent::Frame(f)) => { self.latest = Some(f); }
                Ok(DecEvent::Eof) => { self.eof = true; break; }
                Ok(DecEvent::Failed(_)) => { self.eof = true; break; }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => { self.eof = true; break; }
            }
        }
        self.latest.clone()
    }

    pub fn is_eof(&self) -> bool { self.eof }
    pub fn has_frame(&self) -> bool { self.latest.is_some() }
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
    /// default output device via cpal. Degrades gracefully without a device.
    pub struct Monitor {
        child: Child,
        _handle: std::thread::JoinHandle<()>,
        stream: cpal::Stream,
    }

    impl Monitor {
        pub fn start(path: PathBuf, src_in: f64, dur: f64) -> Option<Monitor> {
            use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
            let device = cpal::default_host().default_output_device()?;
            let cfg = device.default_output_config().ok()?;
            let bin = media::ffmpeg()?;
            let mut child = Command::new(bin)
                .args(["-hide_banner", "-loglevel", "error",
                       "-ss", &format!("{src_in:.3}"), "-t", &format!("{dur:.3}"),
                       "-i", &path.to_string_lossy(),
                       "-ac", "2", "-ar", "48000", "-f", "s16le", "pipe:1"])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .ok()?;
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

        pub fn finished(&self) -> bool {
            // pipe ended and ring drained → monitor is done
            false // conservatively keep until stopped/restarted
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
        pub fn start(_path: PathBuf, _src_in: f64, _dur: f64) -> Option<Monitor> { None }
    }
}

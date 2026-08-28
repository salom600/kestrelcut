# KestrelCut — Architecture

## 1. Design goals (from the brief)

| Goal | Architectural answer |
|------|----------------------|
| Crash-free | Media work runs in **isolated FFmpeg processes**. A poisoned file kills a subprocess, never the editor. Job panics are confined to worker threads (the UI loop cannot observe them as crashes). |
| Memory-bounded | Decoded frames flow through a **bounded channel (4 frames)** with back-pressure into the ffmpeg pipe. Waveform analysis is capped (20 min). Undo history is capped (100 steps, model-only — no pixel data). |
| Thread-safe by construction | All cross-thread traffic is `mpsc`/`sync_channel` message passing. No `Mutex` guards around UI state; the UI thread owns `App`. |
| Low-spec friendly | Half-resolution preview decode by default, proxy generation (540p H.264) with transparent decode switching, pacing in the decode loop so idle CPU ≈ 0 when paused. |
| Familiar UX | Resolve/Premiere-style layout: media pool (L), preview (C), color+scopes (R), magnetic multi-track timeline (bottom), RTL-capable bilingual UI. |

## 2. Module map

```
src/
├── main.rs            args (--lang/--demo/--selftest/--gen-icon), window, icon
├── lib.rs             module wiring
├── app.rs             App state: import, edit ops, decode orchestration, events
├── app_ui.rs          root layout: titlebar/menus, workspace tabs, panels
├── model.rs           Project/Track/Clip + split/trim/slip/move/snap + History
├── media.rs           ffmpeg/ffprobe discovery, probe, filter chains, thumbs,
│                      waveforms, proxies, encoder detection
├── exporter.rs        export graph builder (overlay/concat/mix), HW encoders,
│                      title rasterizer (ab_glyph + Arabic shaping), icon gen
├── decoder.rs         live preview: paced rawvideo pipe, bounded channel,
│                      optional cpal audio monitor
├── player.rs          transport clock, decode slots, quality levels
├── selftest.rs        scripted E2E test through the same code paths as the UI
├── arabic.rs          Unicode presentation-forms shaper + simplified bidi
├── i18n.rs            EN/AR string table (shaping-aware)
├── fonts.rs           egui theme + Noto Naskh Arabic registration
├── ui_*.rs            pool / preview / color+scopes / timeline / dialogs
└── util.rs            theme palette, timecode, snapping helpers
```

## 3. The decode pipeline (preview)

```
UI thread                     decoder thread               ffmpeg -f rawvideo
─────────                     ──────────────               ──────────────────
clock tick ──► want(track, clip, src_t, filters, quality)
   │ key mismatch? ──► kill old proc, spawn new
   │                     │  read_exact(w*h*4)
   │◄── sync_channel(4) ─┤  pace: sleep to 1/fps
   │ poll() → latest frame (stale frames dropped)
   ▼
upload to GPU texture → painter mesh (transform/rotate/opacity)
```

- **Single source of truth for filters**: `media::video_filter_chain()`
  is used by BOTH preview and export — WYSIWYG by construction.
- Restart-on-change (clip, frame bucket, filter hash, quality) with 200 ms
  debounce while dragging sliders.

## 4. Export graph

One deterministic `ffmpeg` command per export:

- Inputs: per-clip `-ss/-t` (fast, frame-accurate with re-encode), images as
  `-loop 1`, titles pre-rasterized to PNG (ab_glyph, Arabic-correct).
- Base layer: bottom video track concat (with `color` gap segments).
- Upper tracks: per-clip `scale/rotate/colorchannelmixer` →
  `overlay enable=between(t,…)` chain.
- Audio: per-clip `aformat → volume(+dB, piecewise keyframes) → adelay`,
  `amix(normalize=0)`.
- Encoder: HW first (NVENC/QSV/AMF — detected via `ffmpeg -encoders`),
  quality mapped per-encoder (CRF / CQ / global_quality / QP).
- Progress: `-progress pipe:1` parsed on the worker thread.

## 5. Why Rust + egui (2026 evaluation)

- **Memory security**: ownership ⇒ no use-after-free/double-free in the editor
  core; `Arc`-shared frame buffers, no GC pauses.
- **Concurrency**: `Send`/`Sync` checked at compile time; data-race-free
  pipelines by construction.
- **GPU integration**: egui paints through glow (OpenGL 3.3+), mesh-level
  transforms give GPU-accelerated clip transforms; swrast fallback works on
  VMs, real GPUs get native speed.
- **Ecosystem maturity**: eframe, ffmpeg-sidecar, cpal, image, serde — all
  stable, MSRV-friendly, cross-compiled identically on Windows/Linux CI.

## 6. Stability guardrails

| Risk | Mitigation |
|------|-----------|
| Decoder hang | subprocess `Drop` ⇒ kill+wait; UI never blocks on pipes |
| Huge frames | decode size capped (≤1280 px wide, quality-scaled) |
| Job storm | one export / proxies queue per asset; cancel kills child proc |
| Malformed project JSON | load is total: any error → toast, state untouched |
| Slider storm | filter changes debounced (200 ms) before decoder restart |

# Verification — v0.3.0

## Automated end-to-end selftest (`--selftest`)

Drives the EXACT code paths the UI buttons call, under Xvfb, with real
FFmpeg-imported demo media, and writes a JSON report:

| Check | Result |
|---|---|
| Import 3 real media files (ffprobe) | PASS |
| Timeline build (magnetic placement, 12 clips) | PASS |
| Razor split at playhead (12 → 18 clips) | PASS |
| Grade + FX + Lift/Gamma/Gain/Offset/Vibrance → FFmpeg filter mapping | PASS |
| Trim handle (−2 s) | PASS |
| Title clip + in/out marks | PASS |
| **Transition set (xfade, source-room clamped)** | PASS |
| **Keyframes + eased interpolation (mid-point value)** | PASS |
| **Copy/paste clips** | PASS |
| **Group/ungroup (shared group id)** | PASS |
| **Chroma key + mask present in the render chain** | PASS |
| **Adjustment layer merges into lower clips (inside ±, outside Δ0.00)** | PASS |
| **Audio rack chain (EQ + compressor + noise reduction)** | PASS |
| **Roll edit preserves total duration** | PASS |
| Export (libx264 720p, in/out range) → valid MP4 | PASS |
| ffprobe verification (duration/streams/size) | PASS |
| Preview engine — paused frame (black-screen regression) | PASS |
| Preview engine — playback frames advance | PASS |

**Result: SELFTEST PASS** (all 18 checks).

## Manual screenshots (Xvfb + x11grab)

- **Edit workspace** — live program monitor compositing demo clips + title,
  media pool thumbnails, timeline with filmstrip clips / audio waveforms /
  title clip, inspector (Compositing blend-mode + keyframe diamonds,
  Transform), 4 live scopes (Waveform/Vectorscope/Parade/Histogram), tool
  strip, transport with speed control.
- **Color workspace** — Lift/Gamma/Gain wheels, Offset, Curves section, HSL
  Secondary, white-balance eyedropper.
- **Effects workspace** — one-click looks + Blur/Sharpen/Denoise/Glow/
  Vignette/Hue/Deband/Lens Correction + Chroma Key + Masks.

## Offline bundling

`kestrelcut --where` resolves the bundled FFmpeg first; CI verifies the
packaged artifacts run with an **empty PATH** (no system FFmpeg visible).

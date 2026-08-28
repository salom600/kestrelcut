# KestrelCut — Verification Report

**Date:** 2026-08-28 · **Commit:** main (v0.1.0) · **Verified by:** scripted E2E + headless GUI run

## 1. End-to-end functional test (`--selftest`)

Drives the exact code paths the UI buttons call, then verifies the output
file with ffprobe. Log (abridged):

```
[  0.51s] imported 3 assets
[  0.61s] timeline built: 6 clips, dur 16.00s
[  0.79s] split: 6 → 12 clips          (razor at playhead, A/V linked pairs split)
[  0.98s] grade+fx applied             (contrast/saturation/exposure/blur/fade)
[  1.16s] trim: 5.00s → 3.00s          (edge trim with source-bound clamping)
[  1.29s] title added, in/out 0..10s
[  1.47s] export started (libx264 720p) — UI stayed responsive, live progress
[ 19.49s] export finished: ~/KestrelCut/exports/selftest_out.mp4
[ 19.72s] probe: 10.00s video=true audio=true size_ok=true
[ 19.72s] SELFTEST PASS
```

Output re-verified with `ffprobe`: duration 10.00 s (exactly the In/Out range),
H.264 video + AAC audio present, >10 kB. **PASS**

## 2. GUI run under Xvfb (headless display)

- App launches, decodes demo clips through the isolated FFmpeg pipeline.
- Preview renders decoded frames with per-clip transforms; scopes
  (luma waveform / vectorscope / RGB parade) compute from the live frame.
- Screenshot vs. reference layout: panel structure correlation
  **0.85 (columns) / 0.73 (rows)** — media pool left, preview center,
  color+scopes right, timeline bottom, RTL Arabic UI with contextual shaping.

## 3. CI artifacts (GitHub Actions, both platforms green)

| Artifact | Validation |
|---|---|
| `kestrelcut_0.1.0_amd64.deb` | extracted; **binary executed** under Xvfb — decodes, renders, exports |
| `kestrelcut_0.1.0_amd64.AppImage` | ELF, linuxdeploy-validated desktop entry |
| `KestrelCut_0.1.0_x64.msi` | WiX candle/light ICE-clean |
| `KestrelCut_0.1.0_win64_portable.zip` | portable exe |

## 4. Known limitations (v0.1.0)

- Exit path bypasses GL teardown (Mesa llvmpipe shutdown segfault upstream);
  state is safe (project saved explicitly).
- Live audio monitoring requires a sound device; export mixdown is unaffected.
- On systems without FFmpeg, Windows/portable builds auto-download it on
  first launch (ffmpeg-sidecar).

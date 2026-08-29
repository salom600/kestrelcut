# KestrelCut 🪶

**Feather-light, crash-proof, cross-platform video editing — with FFmpeg bundled.**

![platforms](https://img.shields.io/badge/platform-Windows%20%7C%20Linux-blue) ![ffmpeg](https://img.shields.io/badge/FFmpeg-bundled%20%E2%80%94%20works%20offline-success)

KestrelCut is a professional non-linear video editor built around two ideas:

1. **The media engine can never take the editor down.** Decoding and encoding
   run in isolated FFmpeg processes; the GPU-accelerated UI stays responsive
   on low-spec machines and scales up to 8K proxy workflows on workstations.
2. **It must work the moment you install it.** Every release artifact ships
   with its own FFmpeg + ffprobe binaries. No system dependency, no downloads,
   no "FFmpeg not found" — ever.

## Highlights

- **FFmpeg bundled, works 100% offline** — the portable `.zip`, the `.msi`
  installer, the `.AppImage` and the `.deb` all carry real FFmpeg binaries.
  The app resolves them next to its own executable first (verified in CI by
  running the packaged app with an empty `PATH`).
- **Stability by architecture** — process-isolated media engine, bounded
  memory pipelines, no decoder crash can kill the editor.
- **Five real workspaces** (like Premiere/Resolve — every tab switches the
  actual panel layout): **Edit · Color · Audio · Effects · Deliver**.
- **Familiar professional layout** — media pool (thumbnail grid *and*
  Name/Duration/FPS/Codec list), program monitor with full transport,
  magnetic multi-track timeline (V1–V3 / A1–A3, lock / hide / mute / solo),
  live scopes (luma waveform / vectorscope / RGB parade).
- **Real color grading** — interactive **Lift / Gamma / Gain color wheels**
  (drag to tint a tonal band, right-click to reset), Offset, Temperature,
  Tint, Exposure, Contrast, Saturation, Vibrance, Highlights/Whites/Blacks,
  LUT (.cube) loader, one-click looks — all rendered through real FFmpeg
  filters in both preview *and* export (WYSIWYG).
- **Real tools** — select, razor, **roll**, **slide**, slip, pen (audio
  keyframes), hand, zoom, text — now with a visible tool strip. Cut / copy /
  paste, **multi-select + group/ungroup** (clips that move & delete together),
  **freeze frame**, **reverse** (≤ 60 s source), ripple delete, magnetic snap
  with visual snap indicators, in/out marks, markers, beat detection.
- **Transitions** — Cross Dissolve, Dip to Black, Wipe ←/→, Slide ←/→, Zoom:
  rendered live in the preview (real mesh math) and exported through FFmpeg
  `xfade` (identical math), with automatic source-room clamping.
- **Compositing** — 9 blend modes (Normal/Multiply/Screen/Overlay/Soft·Hard
  Light/Darken/Lighten/Difference): exact per-pixel software compositor in
  the preview, real FFmpeg `blend` at export. **Adjustment layers** apply
  their grade/FX to everything below. Shape **masks** (rect/ellipse with
  feather + invert) and **Chroma Key** (green screen with spill suppression).
- **Keyframe animation** — Position X/Y, Scale, Rotation, Opacity with
  per-keyframe easing (Linear / Ease In / Out / In-Out); interpolated live in
  the preview and exported as FFmpeg expressions.
- **Effects rack** — Sharpen, Denoise, Vignette, Hue Rotate, Glow, Deband,
  Lens Correction, Grayscale, Sepia, Blur + LUT (.cube) — every one a real
  FFmpeg filter, identical in preview and export.
- **Audio suite** — waveform displays, clip gain, fades, 3-band EQ,
  compressor, limiter, de-esser, noise reduction (afftdn), reverb/echo,
  voice-clarity boost, volume envelopes, **auto-ducking** (analyzes the voice
  waveform and dips the music), beat detection → markers. The rack runs
  identically in the preview monitor and the export mixdown.
- **Titles & subtitles** — shaped text via **rustybuzz + unicode-bidi**
  (real Arabic RTL with contextual glyph joining — مرحبا renders correctly),
  5 title presets (Main/Lower Third/Top Caption/Subtitle/Big Dark), position,
  background bar, shadow, safe-margin overlay, and **.srt import**.
- **Smooth by design** — streaming decoders with skip-ahead scrubbing
  (no per-frame process spawns), pipe-backpressure pacing, coalesced preview
  invalidation while dragging, drag & drop from the media pool onto any track
  with ghost preview + snap line + edge auto-scroll, resizable panels, and
  animated collapsible inspector sections.
- **Hardware-accelerated export** — auto-detects NVENC / Intel QSV / AMD AMF;
  H.264, H.265/HEVC and AV1 (SVT-AV1) with live progress; the UI never blocks.
- **Proxy workflow for low-spec PCs** — one click generates 540p proxies;
  decoding transparently switches to them.

## Download

Grab the latest artifact for your platform from
[Releases](https://github.com/salom600/kestrelcut/releases) or from any CI
run's artifact page:

| Platform | Artifact | FFmpeg |
|----------|----------|--------|
| Windows  | `KestrelCut_<ver>_win64_portable.zip` | `ffmpeg.exe` + `ffprobe.exe` next to the app |
| Windows  | `KestrelCut_<ver>_x64.msi`            | installed into the app folder |
| Linux    | `kestrelcut_<ver>_amd64.AppImage`     | inside the image (`/usr/bin/ffmpeg`) |
| Linux    | `kestrelcut_<ver>_amd64.deb`          | `/usr/lib/kestrelcut/bin/ffmpeg` |

Advanced: set `KESTRELCUT_FFMPEG` to point at your own FFmpeg build.

## Build

```bash
cargo build --release
./target/release/kestrelcut --demo              # populated demo timeline
./target/release/kestrelcut --ws color          # start in the Color workspace
./target/release/kestrelcut --where             # print resolved FFmpeg paths
# scripted end-to-end functional test (imports, splits, wheels, exports, verifies)
KC_SELFTEST_REPORT=report.json ./target/release/kestrelcut --selftest
```

## Keyboard shortcuts

`Space` play/pause · `S` split · `A` select · `C` razor · `P` slip ·
`G` pen · `H` hand · `Z` zoom · `T` text · `I`/`O` mark in/out ·
`Del` delete · `Ctrl+Z`/`Ctrl+Y` undo/redo · `Ctrl+wheel` zoom timeline ·
`←`/`→` step frame (`Shift` = 1 s)

## License

MIT — see [LICENSE](LICENSE).

## Scope honesty (no fake buttons)

Every control in KestrelCut performs real work through the bundled FFmpeg or
in-app engines. Features from "big NLE" checklists that would require faking
are deliberately **not** shipped:

- **Multicam / nested sequences / compound clips** — not implemented (would
  require a nested timeline renderer to be honest; groups cover basic
  multi-clip workflows).
- **Motion tracking, camera tracking, mask tracking, rotoscope, puppet pin,
  particle systems, pen/vector shapes, motion paths** — not implemented
  (would need OpenCV-class CV; a hidden or dead button would be dishonest).
- **Third-party plugins** (Red Giant / Boris FX / Sapphire) — impossible by
  design (proprietary host SDKs).
- **Auto-captions (speech-to-text)**, **music library**, **5.1/7.1 surround**,
  **HDR / RAW / color-space management** — not implemented in v0.3.
- **Reverse** previews at ~10 fps (export is full quality; source ≤ 60 s due
  to the FFmpeg reverse filter's memory model).
- **Speed ramping / time-remap curves** — constant speed per clip only.
- **Stabilization** appears **only when the bundled FFmpeg actually ships
  vidstab** (runtime capability probe — the button is real or absent).

## Keyboard

`Space` play · `S` split · `A/C/U/J/P/G/H/Z/T` tools · `I/O` in/out ·
`M` marker · `F` freeze frame · `R` reverse · `Ctrl+C/X/V` copy/cut/paste ·
`Ctrl+G` / `Ctrl+Shift+G` group/ungroup · `Ctrl+Z/Y` undo/redo ·
`Ctrl+S` save · `Ctrl+E` export · `Ctrl+=/-` zoom · `←/→` (+Shift = 1 s) ·
`Home/End` · `L` loop · `Esc` deselect.

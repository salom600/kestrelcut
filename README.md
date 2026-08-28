# KestrelCut 🪶

**Feather-light, crash-proof, cross-platform video editing.**
مونتير فيديو احترافي فائق الخفة ومستقر — لـ Windows و Linux.

![platforms](https://img.shields.io/badge/platform-Windows%20%7C%20Linux-blue) ![lang](https://img.shields.io/badge/UI-Arabic%20%7C%20English-green)

KestrelCut is a professional non-linear video editor built around one idea:
**the media engine can never take the editor down.** Decoding and encoding run
in isolated FFmpeg processes; the GPU-accelerated UI stays responsive on
low-spec machines and scales up to 8K proxy workflows on workstations.

## Highlights

- **Stability by architecture** — process-isolated media engine, bounded
  memory pipelines (a frame ring of 4), no decoder crash can kill the editor.
- **Familiar professional layout** — media pool, dual monitors, magnetic
  multi-track timeline (V1–V3 / A1–A3), primary color panel + live scopes
  (waveform / vectorscope / RGB parade), inspired by Resolve & Premiere.
- **Real tools** — select, razor, slip, pen (audio keyframes), magnetic snap,
  trim/slide, transforms (position / scale / rotate / opacity), LUTs (.cube),
  blur, fades, volume envelopes.
- **Hardware-accelerated export** — auto-detects NVENC / Intel QSV / AMD AMF;
  H.264, H.265/HEVC and AV1 (SVT-AV1) with live progress; the UI never blocks.
- **Proxy workflow for low-spec PCs** — one click generates 540p proxies;
  decoding transparently switches to them.
- **Bilingual RTL UI** — Arabic interface with real Unicode contextual
  shaping (presentation forms) + simplified bidi, and one-click English.

## Build

```bash
cargo build --release
# run (Arabic UI)
./target/release/kestrelcut
# English UI with a populated demo timeline
./target/release/kestrelcut --lang en --demo
# scripted end-to-end functional test (imports, splits, grades, exports, verifies)
KC_SELFTEST_REPORT=report.json ./target/release/kestrelcut --selftest
```

Runtime dependency: **FFmpeg** (on PATH). On Windows/portable builds,
[ffmpeg-sidecar](https://crates.io/crates/ffmpeg-sidecar) downloads it
automatically on first launch.

## CI artifacts

Every push builds via GitHub Actions:

| OS | Artifacts |
|----|-----------|
| Linux | `kestrelcut_<ver>_amd64.AppImage`, `kestrelcut_<ver>_amd64.deb` |
| Windows | `KestrelCut_<ver>_x64.msi`, `KestrelCut_<ver>_win64_portable.zip` |

Tagged pushes (`v*`) additionally attach everything to a GitHub Release.

## Keyboard

| Key | Action | | Key | Action |
|-----|--------|-|-----|--------|
| `Space` | Play / Pause | | `I` / `O` | Mark In / Out |
| `S` | Split at playhead | | `←` / `→` | Frame step |
| `A` / `C` / `P` | Select / Razor / Slip | | `Ctrl+Z` / `Ctrl+Y` | Undo / Redo |
| `G` / `H` / `Z` / `T` | Pen / Hand / Zoom / Text | | `Ctrl+Wheel` | Timeline zoom |

## Language / toolkit rationale (2026)

- **Rust** — compile-time memory safety and race freedom (the editor core is
  panic-isolated per job), fearless message-passing concurrency, first-class
  GPU via wgpu/glow, single static binaries for painless cross-distribution.
- **egui/eframe (GPU, OpenGL)** — immediate-mode rendering keeps complex
  custom timeline painting simple and fast; zero system UI dependencies keeps
  the AppImage/deb/msi tiny.
- **FFmpeg 7.x as an isolated engine** — industry-grade codec coverage
  (incl. hwaccel NVDEC/QCV/VCN decode + NVENC/QSV/AMF encode) with strict
  crash isolation.

## License

MIT — see [LICENSE](LICENSE).

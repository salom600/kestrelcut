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
- **Real tools** — select, razor, slip, pen (audio keyframes), magnetic snap
  toggle, trim/slide, transforms (position / scale / rotate / opacity),
  speed, blur, fades, volume envelopes, titles.
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

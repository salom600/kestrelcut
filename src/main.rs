//! Entry point: arg parsing, window options, app icon.

use kestrelcut::app::App;

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    let mut demo = false;
    let mut selftest = None;
    let mut ppi: Option<f32> = None;
    let mut seek: Option<f64> = None;
    let mut ws: Option<String> = None;
    let mut autoplay = false;

    let mut it = args.iter().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--demo" => demo = true,
            "--selftest" => selftest = Some(kestrelcut::selftest::SelfTest::new()),
            "--where" => {
                // Diagnostics: print resolved FFmpeg/ffprobe and exit.
                let rep = kestrelcut::media::where_report();
                println!("{rep}");
                let ok = kestrelcut::media::ffmpeg_ok();
                if !ok { eprintln!("ERROR: no usable ffmpeg"); }
                std::process::exit(if ok { 0 } else { 1 });
            }
            "--ppi" => ppi = it.next().and_then(|v| v.parse().ok()),
            "--ws" => ws = it.next().map(|s| s.to_ascii_lowercase()),
            "--seek" => seek = it.next().and_then(|v| v.parse::<f64>().ok()),
            "--play" => autoplay = true,
            "--gen-icon" => {
                let path = it.next().cloned().unwrap_or_else(|| "icon.png".into());
                let size: u32 = it.next().and_then(|v| v.parse().ok()).unwrap_or(512);
                let p = std::path::PathBuf::from(&path);
                if let Some(parent) = p.parent() { let _ = std::fs::create_dir_all(parent); }
                if let Err(e) = kestrelcut::exporter::gen_icon(&p, size) {
                    eprintln!("gen-icon failed: {e}");
                    std::process::exit(1);
                }
                println!("icon written: {path}");
                return Ok(());
            }
            "--help" | "-h" => {
                println!("KestrelCut — feather-light video editor (FFmpeg bundled, works offline)");
                println!("  --demo         preload demo media + populated timeline");
                println!("  --selftest     run scripted end-to-end functional test");
                println!("  --where        print resolved FFmpeg/ffprobe paths and exit");
                println!("  --ws NAME      start workspace: edit|color|audio|fx|export");
                println!("  --seek T       set the playhead position (seconds)");
                println!("  --play         start playback automatically (with --demo)");
                println!("  --ppi N        override pixels-per-point");
                println!("  --gen-icon P [SIZE]  write app icon (png/ico) and exit");
                return Ok(());
            }
            _ => {}
        }
    }

    // window icon generated at runtime (also used by packaging via --gen-icon)
    let icon = {
        let tmp = std::env::temp_dir().join("kestrelcut_icon.png");
        kestrelcut::exporter::gen_icon(&tmp, 64).ok().and_then(|_| std::fs::read(&tmp).ok())
            .and_then(|bytes| {
                let img = image::load_from_memory(&bytes).ok()?.to_rgba8();
                Some(egui::IconData { width: img.width(), height: img.height(), rgba: img.into_raw() })
            })
    };

    let mut vp = egui::ViewportBuilder::default()
        .with_inner_size([1536.0, 1024.0])
        .with_min_inner_size([1200.0, 760.0])
        .with_decorations(false)
        .with_title("KestrelCut");
    if let Some(icon) = icon {
        vp = vp.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport: vp,
        ..Default::default()
    };

    eframe::run_native(
        "KestrelCut",
        options,
        Box::new(move |cc| {
            if let Some(p) = ppi {
                cc.egui_ctx.set_pixels_per_point(p);
            }
            let mut app = App::new(cc, demo, selftest);
            app.pending_seek = seek;
            app.autoplay = autoplay;
            if let Some(w) = ws {
                app.workspace = match w.as_str() {
                    "color" => kestrelcut::app::Workspace::Color,
                    "audio" => kestrelcut::app::Workspace::Audio,
                    "fx" | "effects" => kestrelcut::app::Workspace::Fx,
                    "export" | "deliver" => kestrelcut::app::Workspace::Export,
                    _ => kestrelcut::app::Workspace::Edit,
                };
            }
            Ok(Box::new(app))
        }),
    )
}

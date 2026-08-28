//! Entry point: arg parsing, window options, app icon.

use kestrelcut::app::App;
use kestrelcut::i18n::Lang;

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    let mut lang = Lang::Ar;
    let mut demo = false;
    let mut selftest = None;
    let mut ppi: Option<f32> = None;
    let mut seek: Option<f64> = None;

    let mut it = args.iter().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--lang" => {
                if let Some(l) = it.next() {
                    lang = if l == "en" { Lang::En } else { Lang::Ar };
                }
            }
            "--demo" => demo = true,
            "--selftest" => selftest = Some(kestrelcut::selftest::SelfTest::new()),
            "--ppi" => ppi = it.next().and_then(|v| v.parse().ok()),
            "--seek" => seek = it.next().and_then(|v| v.parse::<f64>().ok()),
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
                println!("KestrelCut — feather-light video editor");
                println!("  --lang ar|en   UI language (default: ar)");
                println!("  --demo         preload demo media + populated timeline");
                println!("  --selftest     run scripted end-to-end functional test");
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
            let mut app = App::new(cc, lang, demo, selftest);
            app.pending_seek = seek;
            Ok(Box::new(app))
        }),
    )
}

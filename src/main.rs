#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
use app_uninstaller::{ui::Desktop, updater};

fn main() -> eframe::Result {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "--apply-update") {
        if args.len() == 2 {
            let path = std::path::Path::new(&args[1]);
            if let Err(error) = updater::apply_helper(path) {
                // Do not display UI from the updater helper. Preserve diagnostics beside its plan.
                if let Some(parent) = path.parent() {
                    let _ = std::fs::write(
                        parent.join("result.txt"),
                        format!(
                            "Update failed: {error:#}. Existing backup/payload files were preserved. Inspect this staging directory before retrying."
                        ),
                    );
                }
                std::process::exit(1);
            }
        } else {
            std::process::exit(2);
        }
        return Ok(());
    }
    if !args.is_empty() {
        std::process::exit(2);
    }
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1120., 760.])
            .with_min_inner_size([900., 620.]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "AppUninstaller",
        options,
        Box::new(|cc| Ok(Box::new(Desktop::new(cc)))),
    )
}

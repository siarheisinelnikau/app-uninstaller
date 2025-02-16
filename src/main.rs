#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
use app_uninstaller::updater;

fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    #[cfg(windows)]
    if args.first().is_some_and(|arg| arg == "--verify-rendering") {
        anyhow::ensure!(args.len() == 2, "Expected diagnostic output path");
        return app_uninstaller::software::verify(std::path::PathBuf::from(&args[1]));
    }
    #[cfg(windows)]
    if args.first().is_some_and(|arg| arg == "--screenshot") {
        anyhow::ensure!(
            args.len() == 4 || args.len() == 6,
            "Expected --screenshot PATH --screen NAME [--size WIDTHxHEIGHT]"
        );
        anyhow::ensure!(args[2] == "--screen", "Expected --screen");
        let size = if args.len() == 6 {
            anyhow::ensure!(args[4] == "--size", "Expected --size");
            let raw = args[5].to_string_lossy();
            let (width, height) = raw
                .split_once('x')
                .ok_or_else(|| anyhow::anyhow!("Invalid size"))?;
            [width.parse()?, height.parse()?]
        } else {
            [1120, 760]
        };
        return app_uninstaller::capture::run(
            std::path::Path::new(&args[1]),
            &args[3].to_string_lossy(),
            size,
            1.,
        );
    }
    if args.first().is_some_and(|arg| arg == "--apply-update") {
        anyhow::ensure!(args.len() == 2, "Expected update plan path");
        if let Err(error) = updater::apply_helper(std::path::Path::new(&args[1])) {
            updater::report_helper_failure(&error);
            return Err(error);
        }
        return Ok(());
    }
    let diagnostics = if args.len() >= 2 && args[0] == "--diagnostics" {
        Some(std::path::PathBuf::from(&args[1]))
    } else {
        None
    };
    let duration = if args.len() == 4 && args[2] == "--diagnostics-duration" {
        Some(args[3].to_string_lossy().parse::<u64>()?)
    } else {
        None
    };
    anyhow::ensure!(
        args.is_empty() || (diagnostics.is_some() && (args.len() == 2 || duration.is_some())),
        "Unsupported command arguments"
    );
    #[cfg(windows)]
    {
        app_uninstaller::software::run(diagnostics, duration)
    }
    #[cfg(not(windows))]
    {
        let _ = (diagnostics, duration);
        anyhow::bail!("The desktop application supports Windows only")
    }
}

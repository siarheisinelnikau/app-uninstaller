//! Product configuration and host-worker adapters for the shared updater.
use anyhow::{Context, Result};
pub use rust_auto_updator::Release as Update;
use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
};

pub fn config() -> Result<rust_auto_updator::Config> {
    let mut config = rust_auto_updator::Config::new(
        "siarheisinelnikau",
        "app-uninstaller",
        crate::VERSION,
        "AppUninstaller",
    )
    .map_err(anyhow::Error::msg)?;
    config.allow_asset_digest = true;
    config.expected_subsystem = Some(2);
    config.max_executable_size = 150 * 1024 * 1024;
    config.install.stage_prefix = ".AppUninstaller-update-".into();
    config.install.helper_name = "AppUninstaller-update-helper.exe".into();
    Ok(config)
}

/// Clones share one verified payload. Only a successful handoff consumes it.
#[derive(Clone)]
pub struct StagedUpdate(Arc<Mutex<Option<rust_auto_updator::Prepared>>>);

pub fn check() -> Result<Option<Update>> {
    rust_auto_updator::check_latest(&config()?).map_err(anyhow::Error::msg)
}

pub fn download(update: &Update, progress: impl FnMut(u64, u64)) -> Result<StagedUpdate> {
    let prepared = rust_auto_updator::prepare_download(update, &config()?, progress)
        .map_err(anyhow::Error::msg)?;
    Ok(StagedUpdate(Arc::new(Mutex::new(Some(prepared)))))
}

pub fn launch_helper(staged: &StagedUpdate) -> Result<()> {
    let mut guard = staged
        .0
        .lock()
        .map_err(|_| anyhow::anyhow!("Update state is unavailable"))?;
    let prepared = guard.as_mut().context("Update was already handed off")?;
    rust_auto_updator::handoff(prepared).map_err(anyhow::Error::msg)?;
    *guard = None;
    Ok(())
}

pub fn apply_helper() -> Result<()> {
    rust_auto_updator::apply_update(&config()?).map_err(anyhow::Error::msg)
}

/// Called only after the real host has initialized successfully.
pub fn cleanup_pending() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 2
        && args[0] == "--cleanup-update"
        && let Ok(config) = config()
    {
        rust_auto_updator::cleanup_after_update(PathBuf::from(&args[1]), config.install);
    }
}

// Read reports left by older AppUninstaller helpers during the migration.
pub fn previous_result() -> Result<Option<String>> {
    let exe = std::env::current_exe()?;
    let parent = exe.parent().context("Missing executable parent")?;
    let mut results = Vec::new();
    for entry in fs::read_dir(parent)?.take(1000) {
        let entry = entry?;
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with(".AppUninstaller-update-")
        {
            continue;
        }
        let path = entry.path().join("result.txt");
        if path.is_file()
            && crate::leftovers::reject_links(&path).is_ok()
            && fs::metadata(&path)?.len() <= 16384
        {
            results.push((fs::metadata(&path)?.modified()?, path));
        }
    }
    results.sort_by_key(|(modified, _)| *modified);
    results
        .last()
        .map(|(_, path)| {
            Ok(format!(
                "Previous update: {}\nReport: {}",
                fs::read_to_string(path)?,
                path.display()
            ))
        })
        .transpose()
}

/// A Windows-subsystem helper has no console; retain failures for the next launch.
pub fn report_helper_failure(error: &anyhow::Error) {
    let Ok(config) = config() else {
        return;
    };
    let Ok(helper) = std::env::current_exe() else {
        return;
    };
    let Some(stage) = helper.parent() else {
        return;
    };
    if helper
        .file_name()
        .is_none_or(|name| name != std::ffi::OsStr::new(&config.install.helper_name))
        || stage.file_name().is_none_or(|name| {
            !name
                .to_string_lossy()
                .starts_with(&config.install.stage_prefix)
        })
        || crate::leftovers::reject_links(stage).is_err()
    {
        return;
    }
    let report = stage.join("result.txt");
    if report.exists() && crate::leftovers::reject_links(&report).is_err() {
        return;
    }
    let message: String = format!("Update failed: {error:#}. The previous.exe backup is retained when replacement reached that stage. Settings and quarantine data were not removed.").chars().take(8192).collect();
    let _ = fs::write(report, message);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn product_adapter_accepts_canonical_digest_assets_and_rejects_other_products() {
        let config = config().unwrap();
        let name = format!("{}-v99.0.0.exe", config.product_name);
        let url = config.repository_url();
        let mut release = serde_json::json!({
            "tag_name":"v99.0.0", "html_url":format!("{url}/releases/tag/v99.0.0"),
            "draft":false, "prerelease":false, "body":"Fixture release",
            "assets":[{"name":name,"size":2048,
                "browser_download_url":format!("{url}/releases/download/v99.0.0/{name}"),
                "digest":format!("sha256:{}", "ab".repeat(32))}]
        });
        assert!(
            rust_auto_updator::parse_release(&release.to_string(), &config)
                .unwrap()
                .is_some()
        );
        release["assets"][0]["name"] = "OtherProduct-v99.0.0.exe".into();
        assert!(rust_auto_updator::parse_release(&release.to_string(), &config).is_err());
    }
}

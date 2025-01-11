//! Public stable-channel updates. Network and replacement run only on workers/helper.
use crate::{
    VERSION,
    leftovers::{hash_file, reject_links},
};
use anyhow::{Context, Result, bail, ensure};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

const API: &str = "https://api.github.com/repos/siarheisinelnikau/app-uninstaller/releases/latest";
const DOWNLOAD: &str = "https://github.com/siarheisinelnikau/app-uninstaller/releases/download/";
const MAX_DOWNLOAD: u64 = 150 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize)]
pub struct Release {
    pub tag_name: String,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub body: String,
    pub assets: Vec<Asset>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Asset {
    pub name: String,
    pub size: u64,
    pub browser_download_url: String,
    pub digest: Option<String>,
}
#[derive(Clone, Debug)]
pub struct Update {
    pub version: String,
    pub notes: String,
    pub asset: Asset,
    pub checksum: Option<Asset>,
}

fn agent(seconds: u64) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(seconds)))
        .build()
        .into()
}

pub fn select(release: Release, installed: &str) -> Result<Option<Update>> {
    if release.draft || release.prerelease {
        return Ok(None);
    }
    let version = Version::parse(release.tag_name.trim_start_matches('v'))?;
    ensure!(
        version.pre.is_empty(),
        "Stable releases cannot have a prerelease version"
    );
    if version <= Version::parse(installed)? {
        return Ok(None);
    }
    let expected = format!("AppUninstaller-v{version}.exe");
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == expected)
        .cloned()
        .context("Newer release has no compatible Windows x64 portable executable")?;
    ensure!(
        (2..=MAX_DOWNLOAD).contains(&asset.size),
        "Release asset size is invalid"
    );
    let prefix = format!("{DOWNLOAD}{}/", release.tag_name);
    ensure!(
        asset.browser_download_url == format!("{prefix}{expected}"),
        "Unexpected release asset location"
    );
    let checksum = release
        .assets
        .iter()
        .find(|a| a.name == "SHA256SUMS.txt")
        .cloned();
    if let Some(checksum) = &checksum {
        ensure!(
            checksum.size <= 65536
                && checksum.browser_download_url == format!("{prefix}SHA256SUMS.txt"),
            "Invalid checksum asset"
        );
    }
    ensure!(
        asset
            .digest
            .as_deref()
            .and_then(|d| d.strip_prefix("sha256:"))
            .is_some_and(valid_hash)
            || checksum.is_some(),
        "The release supplies no SHA-256 checksum; automatic installation is unavailable"
    );
    Ok(Some(Update {
        version: version.to_string(),
        notes: release.body,
        asset,
        checksum,
    }))
}
pub fn check() -> Result<Option<Update>> {
    let response = agent(20)
        .get(API)
        .header(
            "User-Agent",
            concat!("AppUninstaller/", env!("CARGO_PKG_VERSION")),
        )
        .header("Accept", "application/vnd.github+json")
        .call();
    let mut response = match response {
        Ok(response) => response,
        Err(ureq::Error::StatusCode(404)) => bail!(
            "The repository or a published release is unavailable. This project has not been published yet"
        ),
        Err(ureq::Error::StatusCode(403 | 429)) => bail!(
            "GitHub refused the request or the public API rate limit was reached. Try again later"
        ),
        Err(error) => return Err(error.into()),
    };
    let body = response
        .body_mut()
        .with_config()
        .limit(1024 * 1024)
        .read_to_string()?;
    select(serde_json::from_str(&body)?, VERSION)
}
fn valid_hash(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}
pub fn checksum_line(text: &str, name: &str) -> Result<String> {
    let mut matches = text.lines().filter_map(|line| {
        let mut words = line.split_whitespace();
        let hash = words.next()?;
        let file = words.next()?.trim_start_matches('*');
        (file == name && words.next().is_none() && valid_hash(hash)).then(|| hash.to_lowercase())
    });
    let result = matches
        .next()
        .context("No matching checksum for the executable")?;
    ensure!(matches.next().is_none(), "Duplicate executable checksums");
    Ok(result)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstallPlan {
    pub target: PathBuf,
    pub payload: PathBuf,
    pub helper: PathBuf,
    pub expected_hash: String,
    pub original_hash: String,
    pub parent_pid: u32,
}

pub fn download(update: &Update, progress: impl Fn(u64, u64)) -> Result<PathBuf> {
    ensure!(
        cfg!(all(windows, target_arch = "x86_64")),
        "Only Windows x64 portable updates are supported"
    );
    let current = std::env::current_exe()?;
    prepare_download(update, &current, std::process::id(), progress)
}
fn prepare_download(
    update: &Update,
    target: &Path,
    parent_pid: u32,
    progress: impl Fn(u64, u64),
) -> Result<PathBuf> {
    reject_links(target)?;
    let parent = target.parent().context("Executable has no parent folder")?;
    let stage = parent.join(format!(".AppUninstaller-update-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&stage).context("Cannot write beside the executable. Move the portable app into a writable folder; updates never request elevation")?;
    let result = (|| {
        let expected_hash = if let Some(digest) = update
            .asset
            .digest
            .as_deref()
            .and_then(|d| d.strip_prefix("sha256:"))
            .filter(|d| valid_hash(d))
        {
            digest.to_lowercase()
        } else {
            let checksum = update.checksum.as_ref().context("No checksum supplied")?;
            let text = agent(20)
                .get(&checksum.browser_download_url)
                .header("User-Agent", "AppUninstaller")
                .call()?
                .body_mut()
                .with_config()
                .limit(65536)
                .read_to_string()?;
            checksum_line(&text, &update.asset.name)?
        };
        let payload = stage.join("payload.exe");
        let mut output = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&payload)?;
        let mut response = agent(300)
            .get(&update.asset.browser_download_url)
            .header("User-Agent", "AppUninstaller")
            .call()?;
        let mut input = response.body_mut().as_reader().take(update.asset.size + 1);
        let mut bytes = 0;
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let n = input.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            bytes += n as u64;
            ensure!(
                bytes <= update.asset.size && bytes <= MAX_DOWNLOAD,
                "Download exceeds the advertised size"
            );
            output.write_all(&buffer[..n])?;
            progress(bytes, update.asset.size);
        }
        output.sync_all()?;
        drop(output);
        ensure!(bytes == update.asset.size, "Incomplete update download");
        ensure!(
            hash_file(&payload, MAX_DOWNLOAD)?.sha256 == expected_hash,
            "Update checksum mismatch; current installation was not changed"
        );
        let helper = stage.join("helper.exe");
        fs::copy(target, &helper)?;
        let plan = InstallPlan {
            target: target.into(),
            payload,
            helper,
            expected_hash,
            original_hash: hash_file(target, MAX_DOWNLOAD)?.sha256,
            parent_pid,
        };
        let path = stage.join("plan.json");
        fs::write(&path, serde_json::to_vec_pretty(&plan)?)?;
        Ok(path)
    })();
    // Keep failed downloads for diagnosis; they are never installed or executed.
    result.with_context(|| format!("Update staging folder: {}", stage.display()))
}

pub fn launch_helper(path: &Path) -> Result<()> {
    let plan = read_plan(path)?;
    validate_plan(path, &plan)?;
    std::process::Command::new(&plan.helper)
        .arg("--apply-update")
        .arg(path)
        .spawn()
        .context("Could not start the update helper; the current app remains open")?;
    Ok(())
}
fn read_plan(path: &Path) -> Result<InstallPlan> {
    ensure!(
        fs::metadata(path)?.len() <= 16384,
        "Update plan is too large"
    );
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
fn validate_plan(path: &Path, plan: &InstallPlan) -> Result<()> {
    let stage = path.parent().context("Invalid update plan path")?;
    ensure!(
        stage
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with(".AppUninstaller-update-")),
        "Invalid update staging directory"
    );
    ensure!(
        plan.target.is_absolute() && plan.target.parent() == stage.parent(),
        "Update target must be beside staging"
    );
    ensure!(
        plan.target
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("exe")),
        "Invalid update target"
    );
    ensure!(
        path == stage.join("plan.json")
            && plan.payload == stage.join("payload.exe")
            && plan.helper == stage.join("helper.exe"),
        "Unexpected update paths"
    );
    ensure!(
        valid_hash(&plan.expected_hash) && valid_hash(&plan.original_hash),
        "Invalid update checksums"
    );
    reject_links(stage)?;
    reject_links(&plan.payload)?;
    reject_links(&plan.target)?;
    reject_links(&plan.helper)?;
    Ok(())
}
pub fn apply_helper(path: &Path) -> Result<()> {
    let plan = read_plan(path)?;
    validate_plan(path, &plan)?;
    ensure!(
        std::env::current_exe()? == plan.helper,
        "Update replacement must run from the staged helper"
    );
    #[cfg(windows)]
    {
        use windows_sys::Win32::{
            Foundation::{CloseHandle, WAIT_OBJECT_0},
            System::Threading::{OpenProcess, SYNCHRONIZATION_SYNCHRONIZE, WaitForSingleObject},
        };
        let process = unsafe { OpenProcess(SYNCHRONIZATION_SYNCHRONIZE, 0, plan.parent_pid) };
        if !process.is_null() {
            let status = unsafe { WaitForSingleObject(process, 120000) };
            unsafe {
                CloseHandle(process);
            }
            ensure!(
                status == WAIT_OBJECT_0,
                "The application did not exit within two minutes. Current installation was preserved"
            );
        } else {
            // ERROR_INVALID_PARAMETER means that the parent has already exited.
            ensure!(
                std::io::Error::last_os_error().raw_os_error() == Some(87),
                "Cannot wait for the running application; replacement cancelled"
            );
        }
    }
    #[cfg(not(windows))]
    bail!("Update installation requires Windows");
    #[cfg(windows)]
    {
        validate_plan(path, &plan)?;
        ensure!(
            hash_file(&plan.payload, MAX_DOWNLOAD)?.sha256 == plan.expected_hash,
            "Staged update changed"
        );
        ensure!(
            hash_file(&plan.target, MAX_DOWNLOAD)?.sha256 == plan.original_hash,
            "Installed executable changed; replacement cancelled"
        );
        let backup = path.parent().unwrap().join("previous.exe");
        ensure!(!backup.exists(), "An update backup already exists");
        fs::rename(&plan.target, &backup)?;
        if let Err(error) = fs::rename(&plan.payload, &plan.target) {
            let rollback = fs::rename(&backup, &plan.target);
            bail!("Replacement failed: {error}; rollback: {rollback:?}");
        }
        if let Err(error) = std::process::Command::new(&plan.target).spawn() {
            let move_new = fs::rename(&plan.target, &plan.payload);
            let rollback = if move_new.is_ok() {
                fs::rename(&backup, &plan.target)
            } else {
                move_new
            };
            bail!("Restart failed: {error}; rollback: {rollback:?}");
        }
        fs::write(
            path.parent().unwrap().join("result.txt"),
            "Update installed and restart requested. Previous executable retained as previous.exe. Settings and quarantine journals were preserved.",
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn release(version: &str) -> Release {
        Release {
            tag_name: format!("v{version}"),
            draft: false,
            prerelease: false,
            body: "Changes".into(),
            assets: vec![Asset {
                name: format!("AppUninstaller-v{version}.exe"),
                size: 100,
                browser_download_url: format!("{DOWNLOAD}v{version}/AppUninstaller-v{version}.exe"),
                digest: Some(format!("sha256:{}", "a".repeat(64))),
            }],
        }
    }
    #[test]
    fn versions_are_numeric_and_release_channel_is_stable() {
        assert!(select(release("0.10.0"), "0.9.0").unwrap().is_some());
        assert!(select(release("0.9.0"), "0.10.0").unwrap().is_none());
        let mut r = release("1.0.0");
        r.draft = true;
        assert!(select(r, "0.1.0").unwrap().is_none());
        let mut r = release("1.0.0-beta.1");
        r.prerelease = true;
        assert!(select(r, "0.1.0").unwrap().is_none());
    }
    #[test]
    fn incompatible_or_unverifiable_assets_are_rejected() {
        let mut r = release("1.0.0");
        r.assets[0].browser_download_url = "https://example.org/evil.exe".into();
        assert!(select(r, "0.1.0").is_err());
        let mut r = release("1.0.0");
        r.assets[0].digest = None;
        assert!(select(r, "0.1.0").is_err());
        let mut r = release("1.0.0");
        r.assets[0].name = "AppUninstaller-v1.0.0-arm64.exe".into();
        assert!(select(r, "0.1.0").is_err());
    }
    #[test]
    fn checksum_matching_is_exact() {
        let hash = "a".repeat(64);
        assert_eq!(
            checksum_line(
                &format!("{hash} *AppUninstaller-v1.0.0.exe"),
                "AppUninstaller-v1.0.0.exe"
            )
            .unwrap(),
            hash
        );
        assert!(checksum_line(&format!("{hash} other.exe"), "wanted.exe").is_err());
        assert!(
            checksum_line(
                &format!("{hash} wanted.exe\n{hash} wanted.exe"),
                "wanted.exe"
            )
            .is_err()
        );
    }
}

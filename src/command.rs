//! Pure uninstall planning. No shell interpretation, PATH search, or quiet mode.
use crate::inventory::InstalledApp;
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct UninstallPlan {
    pub executable: PathBuf,
    pub arguments: String,
    pub explanation: String,
}

pub fn valid_product_code(s: &str) -> bool {
    s.len() == 38
        && s.starts_with('{')
        && s.ends_with('}')
        && s[1..37].bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

pub fn plan(app: &InstalledApp, windows_dir: &str) -> Result<UninstallPlan> {
    if app.windows_installer && valid_product_code(&app.key_name) {
        ensure!(
            windows_absolute(windows_dir),
            "Windows directory is not absolute"
        );
        return Ok(UninstallPlan {
            executable: PathBuf::from(format!("{}\\System32\\msiexec.exe", windows_dir.trim_end_matches('\\'))),
            arguments: format!("/x {} /norestart", app.key_name),
            explanation: "Windows Installer removal for the registered product code; interactive UI, no automatic restart.".into(),
        });
    }
    parse_registered(&app.uninstall_string)
}

pub fn windows_absolute(s: &str) -> bool {
    let s = s.strip_prefix("\\\\?\\").unwrap_or(s);
    let b = s.as_bytes();
    b.len() > 3
        && b[0].is_ascii_alphabetic()
        && b[1] == b':'
        && b[2] == b'\\'
        && !s.contains('/')
        && !s.contains('\0')
        && !s[3..].split('\\').any(|part| part == ".." || part == ".")
        && !s[2..].contains(':')
}

pub fn parse_registered(raw: &str) -> Result<UninstallPlan> {
    let raw = raw.trim();
    ensure!(!raw.is_empty(), "No registered uninstall command");
    ensure!(
        !raw.chars().any(|c| c == '\0' || c == '\r' || c == '\n'),
        "Invalid command characters"
    );
    let (exe, args) = if let Some(rest) = raw.strip_prefix('"') {
        let end = rest
            .find('"')
            .ok_or_else(|| anyhow::anyhow!("Unclosed executable quote"))?;
        let tail = &rest[end + 1..];
        ensure!(
            tail.is_empty() || tail.starts_with(char::is_whitespace),
            "Missing separator after executable"
        );
        (&rest[..end], tail.trim_start())
    } else {
        let end = raw.find(char::is_whitespace).unwrap_or(raw.len());
        (&raw[..end], raw[end..].trim_start())
    };
    ensure!(
        windows_absolute(exe),
        "Only absolute local executable paths are accepted; ambiguous unquoted paths must be fixed by the publisher"
    );
    ensure!(
        exe.to_ascii_lowercase().ends_with(".exe"),
        "Only direct .exe uninstallers are supported"
    );
    let name = exe.rsplit('\\').next().unwrap_or("").to_ascii_lowercase();
    if [
        "cmd.exe",
        "powershell.exe",
        "pwsh.exe",
        "wscript.exe",
        "cscript.exe",
        "mshta.exe",
        "rundll32.exe",
        "regsvr32.exe",
        "reg.exe",
        "msiexec.exe",
        "explorer.exe",
        "bash.exe",
        "wsl.exe",
    ]
    .contains(&name.as_str())
    {
        bail!(
            "Shells, script hosts and generic launchers are blocked; MSI requires a registered product code"
        );
    }
    Ok(UninstallPlan { executable: exe.into(), arguments: args.into(), explanation: "Runs the exact registered executable with its registered arguments, without a command shell. Review the publisher and path before continuing.".into() })
}

pub fn execute(plan: &UninstallPlan) -> Result<u32> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        ensure!(
            plan.executable.is_file(),
            "Registered uninstaller no longer exists"
        );
        let status = std::process::Command::new(&plan.executable)
            .raw_arg(&plan.arguments)
            .status()?;
        Ok(status.code().unwrap_or(-1) as u32)
    }
    #[cfg(not(windows))]
    {
        let _ = plan;
        bail!("Uninstall execution is available only on Windows")
    }
}

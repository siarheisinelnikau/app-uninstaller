//! Pure uninstall planning. No shell interpretation, PATH search, or quiet mode.
use crate::inventory::InstalledApp;
#[cfg(windows)]
use anyhow::Context;
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct UninstallPlan {
    pub executable: PathBuf,
    pub arguments: String,
    pub explanation: String,
}

pub fn windows_directory() -> Result<String> {
    #[cfg(windows)]
    {
        let mut buffer = [0u16; 32768];
        let length = unsafe {
            windows_sys::Win32::System::SystemInformation::GetWindowsDirectoryW(
                buffer.as_mut_ptr(),
                buffer.len() as u32,
            )
        };
        ensure!(
            length > 0 && (length as usize) < buffer.len(),
            "Cannot determine the trusted Windows directory"
        );
        Ok(String::from_utf16_lossy(&buffer[..length as usize]))
    }
    #[cfg(not(windows))]
    bail!("Windows directory is unavailable on this platform")
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

/// Recheck the approved identity and command before either kind of launch.
pub fn validate_approved(
    current: &InstalledApp,
    approved_app: &InstalledApp,
    approved: &UninstallPlan,
    windows_dir: &str,
) -> Result<UninstallPlan> {
    let fresh = plan(current, windows_dir)?;
    ensure!(
        &fresh == approved
            && current.id == approved_app.id
            && current.install_location == approved_app.install_location
            && current.name == approved_app.name
            && current.publisher == approved_app.publisher
            && !current.system_component,
        "Registration changed. Prepare and review it again"
    );
    Ok(fresh)
}

#[derive(Debug, PartialEq, Eq)]
pub enum LaunchErrorKind {
    ElevationRequired,
    Cancelled,
    Other,
}
pub fn launch_error_kind(error: &anyhow::Error) -> LaunchErrorKind {
    match error
        .downcast_ref::<std::io::Error>()
        .and_then(|e| e.raw_os_error())
    {
        Some(740) => LaunchErrorKind::ElevationRequired,
        Some(1223) => LaunchErrorKind::Cancelled,
        _ => LaunchErrorKind::Other,
    }
}

/// Closing this handle never terminates the publisher's process.
pub struct RunningUninstaller {
    #[cfg(windows)]
    handle: std::os::windows::io::OwnedHandle,
}
impl RunningUninstaller {
    pub fn try_wait(&mut self) -> Result<Option<u32>> {
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::{Foundation::*, System::Threading::*};
            let handle = self.handle.as_raw_handle();
            // SAFETY: this object owns a live process handle; timeout zero never blocks.
            match unsafe { WaitForSingleObject(handle, 0) } {
                WAIT_TIMEOUT => Ok(None),
                WAIT_OBJECT_0 => {
                    let mut code = 0;
                    // SAFETY: valid process handle and initialized output storage.
                    if unsafe { GetExitCodeProcess(handle, &mut code) } == 0 {
                        return Err(std::io::Error::last_os_error().into());
                    }
                    Ok(Some(code))
                }
                _ => Err(std::io::Error::last_os_error().into()),
            }
        }
        #[cfg(not(windows))]
        bail!("Process observation is available only on Windows")
    }
}

pub fn spawn(plan: &UninstallPlan) -> Result<RunningUninstaller> {
    #[cfg(windows)]
    {
        use std::os::windows::io::{FromRawHandle, IntoRawHandle, OwnedHandle};
        use std::os::windows::process::CommandExt;
        ensure!(
            plan.executable.is_file(),
            "Registered uninstaller no longer exists"
        );
        let child = std::process::Command::new(&plan.executable)
            .raw_arg(&plan.arguments)
            .spawn()
            .context("Could not run registered uninstaller")?;
        // SAFETY: into_raw_handle transfers exclusive ownership from Child.
        Ok(RunningUninstaller {
            handle: unsafe { OwnedHandle::from_raw_handle(child.into_raw_handle()) },
        })
    }
    #[cfg(not(windows))]
    {
        let _ = plan;
        bail!("Uninstall execution is available only on Windows")
    }
}

/// Explicit user-initiated UAC retry of an already revalidated direct EXE.
/// A successful Shell launch may provide no process handle; registration observation
/// remains available, but such a session cannot expose a leftover snapshot.
pub fn spawn_elevated(plan: &UninstallPlan) -> Result<Option<RunningUninstaller>> {
    #[cfg(windows)]
    {
        use std::{
            os::windows::{
                ffi::OsStrExt,
                io::{FromRawHandle, OwnedHandle},
            },
            ptr,
        };
        use windows_sys::Win32::{
            System::Com::*,
            UI::{Shell::*, WindowsAndMessaging::SW_SHOWNORMAL},
        };
        ensure!(
            plan.executable.is_file(),
            "Registered uninstaller no longer exists"
        );
        ensure!(!plan.arguments.contains('\0'), "Invalid command arguments");
        let file: Vec<u16> = plan
            .executable
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        ensure!(
            !file[..file.len() - 1].contains(&0),
            "Invalid executable path"
        );
        let arguments: Vec<u16> = plan.arguments.encode_utf16().chain(Some(0)).collect();
        let verb: Vec<u16> = "runas".encode_utf16().chain(Some(0)).collect();
        // SAFETY: initialize COM on this worker thread, paired with uninitialization.
        let result = unsafe {
            CoInitializeEx(
                ptr::null(),
                (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32,
            )
        };
        ensure!(
            result >= 0,
            "Could not initialize Windows launch support (HRESULT {result:#x})"
        );
        struct Apartment;
        impl Drop for Apartment {
            fn drop(&mut self) {
                unsafe { CoUninitialize() };
            }
        }
        let _apartment = Apartment;
        let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI;
        info.lpVerb = verb.as_ptr();
        info.lpFile = file.as_ptr();
        info.lpParameters = arguments.as_ptr();
        info.nShow = SW_SHOWNORMAL;
        // SAFETY: structure size is set; UTF-16 buffers live through the synchronous
        // launch. File and arguments are separate, with no command interpreter.
        if unsafe { ShellExecuteExW(&mut info) } == 0 {
            return Err(std::io::Error::last_os_error())
                .context("Could not launch the uninstaller with administrator permission");
        }
        if info.hProcess.is_null() {
            return Ok(None);
        }
        // SAFETY: SEE_MASK_NOCLOSEPROCESS transfers the returned handle to us.
        Ok(Some(RunningUninstaller {
            handle: unsafe { OwnedHandle::from_raw_handle(info.hProcess) },
        }))
    }
    #[cfg(not(windows))]
    {
        let _ = plan;
        bail!("Administrator launch is available only on Windows")
    }
}

#[cfg(test)]
mod launch_tests {
    use super::*;
    #[cfg(windows)]
    #[test]
    fn native_process_observation_does_not_treat_an_active_process_as_exited() {
        use std::os::windows::io::{FromRawHandle, OwnedHandle};
        use windows_sys::Win32::System::Threading::*;
        // Read-only handle to this test harness. No child or UAC request is launched.
        let handle = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                0,
                GetCurrentProcessId(),
            )
        };
        assert!(!handle.is_null());
        let mut process = RunningUninstaller {
            handle: unsafe { OwnedHandle::from_raw_handle(handle) },
        };
        assert_eq!(process.try_wait().unwrap(), None);
        drop(process);
    }
    #[test]
    fn elevation_and_uac_cancellation_use_os_codes_not_translated_messages() {
        for (code, expected) in [
            (740, LaunchErrorKind::ElevationRequired),
            (1223, LaunchErrorKind::Cancelled),
            (5, LaunchErrorKind::Other),
            (2, LaunchErrorKind::Other),
        ] {
            let error = anyhow::Error::from(std::io::Error::from_raw_os_error(code))
                .context("Localized launch failure");
            assert_eq!(launch_error_kind(&error), expected);
        }
        assert_eq!(
            launch_error_kind(&anyhow::anyhow!("error 740")),
            LaunchErrorKind::Other
        );
    }
    #[test]
    fn administrator_retry_rejects_changed_approval_or_identity() {
        let original = InstalledApp {
            id: "fixture".into(),
            name: "Fixture".into(),
            publisher: "Publisher".into(),
            uninstall_string: r#""C:\Fixture\remove.exe" /uninstall"#.into(),
            ..Default::default()
        };
        let approved = plan(&original, r"C:\Windows").unwrap();
        assert!(validate_approved(&original, &original, &approved, r"C:\Windows").is_ok());
        let mut changed = original.clone();
        changed.uninstall_string.push_str(" /extra");
        assert!(validate_approved(&changed, &original, &approved, r"C:\Windows").is_err());
        let mut changed = original.clone();
        changed.publisher = "Different publisher".into();
        assert!(validate_approved(&changed, &original, &approved, r"C:\Windows").is_err());
        let mut changed = original.clone();
        changed.system_component = true;
        assert!(validate_approved(&changed, &original, &approved, r"C:\Windows").is_err());
    }
}

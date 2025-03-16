//! Read-only access to the four uninstall registry scopes/views.
use super::*;
use anyhow::{Context, Result, bail};
use std::{mem, ptr};
use windows_sys::Win32::{
    Foundation::*,
    System::{Environment::*, Registry::*},
};

const UNINSTALL: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall";
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
struct Key(HKEY);
impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            RegCloseKey(self.0);
        }
    }
}

fn open(parent: HKEY, name: &str, view: u32) -> Result<Key> {
    let mut handle = ptr::null_mut();
    // SAFETY: terminated UTF-16 input, valid parent, initialized output pointer.
    let code =
        unsafe { RegOpenKeyExW(parent, wide(name).as_ptr(), 0, KEY_READ | view, &mut handle) };
    if code != ERROR_SUCCESS {
        return Err(std::io::Error::from_raw_os_error(code as i32))
            .with_context(|| format!("Registry read failed for {name}"));
    }
    Ok(Key(handle))
}

fn value(key: &Key, name: &str) -> Option<(u32, Vec<u8>)> {
    let mut kind = 0;
    let mut size = 0;
    let name = wide(name);
    unsafe {
        if RegQueryValueExW(
            key.0,
            name.as_ptr(),
            ptr::null(),
            &mut kind,
            ptr::null_mut(),
            &mut size,
        ) != 0
            || size > 65536
        {
            return None;
        }
        let mut bytes = vec![0; size as usize];
        if RegQueryValueExW(
            key.0,
            name.as_ptr(),
            ptr::null(),
            &mut kind,
            bytes.as_mut_ptr(),
            &mut size,
        ) != 0
        {
            return None;
        }
        bytes.truncate(size as usize);
        Some((kind, bytes))
    }
}
fn string(key: &Key, name: &str) -> String {
    let Some((kind, bytes)) = value(key, name) else {
        return String::new();
    };
    if kind != REG_SZ && kind != REG_EXPAND_SZ {
        return String::new();
    }
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|v| u16::from_le_bytes([v[0], v[1]]))
        .take_while(|v| *v != 0)
        .collect();
    let text = String::from_utf16_lossy(&units);
    if kind != REG_EXPAND_SZ {
        return text;
    }
    let input = wide(&text);
    unsafe {
        let len = ExpandEnvironmentStringsW(input.as_ptr(), ptr::null_mut(), 0);
        if len == 0 || len > 32768 {
            return String::new();
        }
        let mut expanded = vec![0u16; len as usize];
        let result = ExpandEnvironmentStringsW(input.as_ptr(), expanded.as_mut_ptr(), len);
        if result == 0 || result > len {
            return String::new();
        }
        String::from_utf16_lossy(&expanded[..result.saturating_sub(1) as usize])
    }
}
fn dword(key: &Key, name: &str) -> Option<u32> {
    let (kind, bytes) = value(key, name)?;
    if kind != REG_DWORD {
        return None;
    }
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}
pub fn registration_exists(app: &InstalledApp) -> Result<bool> {
    anyhow::ensure!(
        !app.key_name.is_empty() && !app.key_name.contains('\\'),
        "Invalid registration key"
    );
    let root = match app.scope.as_str() {
        "Current user" => HKEY_CURRENT_USER,
        "All users" => HKEY_LOCAL_MACHINE,
        _ => bail!("Unknown registration scope"),
    };
    let path = format!("{UNINSTALL}\\{}", app.key_name);
    let mut error = None;
    for flag in [KEY_WOW64_64KEY, KEY_WOW64_32KEY] {
        match open(root, &path, flag) {
            Ok(_) => return Ok(true),
            Err(e)
                if e.downcast_ref::<std::io::Error>()
                    .is_some_and(|e| matches!(e.raw_os_error(), Some(2 | 3))) => {}
            Err(e) => error = Some(e),
        }
    }
    if let Some(error) = error {
        return Err(error);
    }
    Ok(false)
}

pub fn enumerate() -> Result<Inventory> {
    let mut inventory = Inventory::default();
    let mut opened = 0;
    for (root, scope) in [
        (HKEY_CURRENT_USER, "Current user"),
        (HKEY_LOCAL_MACHINE, "All users"),
    ] {
        for (flag, view) in [(KEY_WOW64_64KEY, "64-bit"), (KEY_WOW64_32KEY, "32-bit")] {
            let parent = match open(root, UNINSTALL, flag) {
                Ok(k) => {
                    opened += 1;
                    k
                }
                Err(error)
                    if error
                        .downcast_ref::<std::io::Error>()
                        .is_some_and(|e| e.raw_os_error() == Some(2)) =>
                {
                    opened += 1;
                    continue;
                }
                Err(error) => {
                    inventory.warnings.push(format!("{scope}, {view}: {error}"));
                    continue;
                }
            };
            let mut exhausted = false;
            for index in 0..20000 {
                let mut name = [0u16; 256];
                let mut len = name.len() as u32;
                let code = unsafe {
                    RegEnumKeyExW(
                        parent.0,
                        index,
                        name.as_mut_ptr(),
                        &mut len,
                        ptr::null(),
                        ptr::null_mut(),
                        ptr::null_mut(),
                        ptr::null_mut(),
                    )
                };
                if code == ERROR_NO_MORE_ITEMS {
                    exhausted = true;
                    break;
                }
                if code != ERROR_SUCCESS {
                    inventory
                        .warnings
                        .push(format!("{scope}/{view}: subkey read failed ({code})"));
                    continue;
                }
                let name = String::from_utf16_lossy(&name[..len as usize]);
                let key = match open(parent.0, &name, flag) {
                    Ok(k) => k,
                    Err(e) => {
                        inventory.warnings.push(e.to_string());
                        continue;
                    }
                };
                let display_name = string(&key, "DisplayName");
                let location = string(&key, "InstallLocation");
                let nameless = display_name.trim().is_empty();
                if nameless && location.trim().is_empty() {
                    continue;
                }
                let icon = string(&key, "DisplayIcon");
                let app = InstalledApp {
                    id: format!("{scope}/{view}/{name}"),
                    key_name: name.clone(),
                    scope: scope.into(),
                    view: view.into(),
                    name: if nameless {
                        format!("Unnamed registration: {name}")
                    } else {
                        display_name
                    },
                    publisher: string(&key, "Publisher"),
                    version: string(&key, "DisplayVersion"),
                    estimated_kib: dword(&key, "EstimatedSize").map(u64::from),
                    install_location: (!location.trim().is_empty())
                        .then(|| PathBuf::from(location.trim_matches('"'))),
                    uninstall_string: string(&key, "UninstallString"),
                    windows_installer: dword(&key, "WindowsInstaller") == Some(1),
                    system_component: nameless || dword(&key, "SystemComponent") == Some(1),
                    icon: extract_icon(&icon),
                    display_icon: icon,
                };
                // WOW64 can expose the same per-user registration in both views.
                if let Some(existing) = inventory.apps.iter_mut().find(|a| {
                    a.scope == app.scope
                        && a.key_name == app.key_name
                        && a.name == app.name
                        && a.uninstall_string == app.uninstall_string
                        && a.install_location == app.install_location
                }) {
                    existing.view = "32/64-bit".into();
                } else {
                    inventory.apps.push(app);
                }
            }
            if !exhausted {
                inventory.warnings.push(format!(
                    "{scope}/{view}: inventory exceeded the 20,000 registration limit"
                ));
            }
        }
    }
    if opened == 0 {
        bail!("No uninstall registry view could be read");
    }
    inventory.apps.sort_by_cached_key(|a| a.name.to_lowercase());
    Ok(inventory)
}

fn extract_icon(spec: &str) -> Option<IconPixels> {
    use windows_sys::Win32::{
        Graphics::Gdi::*,
        UI::{Shell::ExtractIconExW, WindowsAndMessaging::*},
    };
    let (path, index) = spec
        .rsplit_once(',')
        .and_then(|(p, i)| i.trim().parse::<i32>().ok().map(|n| (p, n)))
        .unwrap_or((spec, 0));
    let path = path.trim().trim_matches('"');
    // No network paths or arbitrary shell namespace handlers.
    if !crate::command::windows_absolute(path) {
        return None;
    }
    unsafe {
        let mut icon = ptr::null_mut();
        if ExtractIconExW(wide(path).as_ptr(), index, ptr::null_mut(), &mut icon, 1) == 0
            || icon.is_null()
        {
            return None;
        }
        let dc = CreateCompatibleDC(ptr::null_mut());
        if dc.is_null() {
            DestroyIcon(icon);
            return None;
        }
        let mut info: BITMAPINFO = mem::zeroed();
        info.bmiHeader.biSize = mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = 32;
        info.bmiHeader.biHeight = -32;
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        let mut pixels = ptr::null_mut();
        let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut pixels, ptr::null_mut(), 0);
        if bitmap.is_null() || pixels.is_null() {
            DeleteDC(dc);
            DestroyIcon(icon);
            return None;
        }
        let old = SelectObject(dc, bitmap);
        // A solid neutral background also renders legacy icons without alpha correctly.
        let buffer = std::slice::from_raw_parts_mut(pixels as *mut u8, 32 * 32 * 4);
        for p in buffer.chunks_exact_mut(4) {
            p.copy_from_slice(&[238, 238, 238, 255]);
        }
        let ok = DrawIconEx(dc, 0, 0, icon, 32, 32, 0, ptr::null_mut(), DI_NORMAL) != 0;
        let mut rgba = buffer.to_vec();
        for p in rgba.chunks_exact_mut(4) {
            p.swap(0, 2);
            p[3] = 255;
        }
        SelectObject(dc, old);
        DeleteObject(bitmap);
        DeleteDC(dc);
        DestroyIcon(icon);
        ok.then_some(IconPixels {
            rgba,
            width: 32,
            height: 32,
        })
    }
}

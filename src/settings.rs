use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub check_on_startup: bool,
    pub language: crate::i18n::Language,
    pub theme: Theme,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    Dark,
    #[default]
    #[serde(other)]
    Light,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            check_on_startup: true,
            language: crate::i18n::Language::default(),
            theme: Theme::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn older_settings_keep_update_preference_and_gain_display_defaults() {
        let settings: Settings = serde_json::from_str(r#"{"check_on_startup":false}"#).unwrap();
        assert!(!settings.check_on_startup);
        assert_eq!(settings.language, crate::i18n::Language::English);
        assert_eq!(settings.theme, Theme::Light);
    }

    #[test]
    fn unknown_future_display_choices_fall_back_without_enabling_startup_checks() {
        let settings: Settings = serde_json::from_str(
            r#"{"check_on_startup":false,"language":"future_language","theme":"future_theme"}"#,
        )
        .unwrap();
        assert!(!settings.check_on_startup);
        assert_eq!(settings.language, crate::i18n::Language::English);
        assert_eq!(settings.theme, Theme::Light);
    }

    #[test]
    fn display_choices_survive_atomic_settings_roundtrip() {
        let directory = tempfile::tempdir().unwrap();
        let settings = Settings {
            language: crate::i18n::Language::Russian,
            theme: Theme::Dark,
            check_on_startup: false,
        };
        save(directory.path(), &settings).unwrap();
        let restored = load(directory.path()).unwrap();
        assert_eq!(restored.language, settings.language);
        assert_eq!(restored.theme, settings.theme);
        assert!(!restored.check_on_startup);
    }
}
pub fn data_dir() -> Result<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")
        .context("LOCALAPPDATA is unavailable; settings cannot be stored")?;
    Ok(PathBuf::from(local).join("AppUninstaller"))
}
pub fn load(dir: &Path) -> Result<Settings> {
    let path = dir.join("settings.json");
    if !path.exists() {
        return Ok(Settings::default());
    }
    ensure!(
        fs::metadata(&path)?.len() <= 64 * 1024,
        "Settings file is too large"
    );
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
pub fn save(dir: &Path, settings: &Settings) -> Result<()> {
    fs::create_dir_all(dir)?;
    let temp = dir.join(format!("settings-{}.tmp", uuid::Uuid::new_v4()));
    fs::write(&temp, serde_json::to_vec_pretty(settings)?)?;
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        // MoveFileEx replaces atomically; a failed write leaves previous settings intact.
        unsafe extern "system" {
            fn MoveFileExW(from: *const u16, to: *const u16, flags: u32) -> i32;
        }
        let from: Vec<u16> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<u16> = dir
            .join("settings.json")
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), 0x1 | 0x8) } == 0 {
            let error = std::io::Error::last_os_error();
            let _ = fs::remove_file(temp);
            return Err(error.into());
        }
    }
    #[cfg(not(windows))]
    fs::rename(temp, dir.join("settings.json"))?;
    Ok(())
}

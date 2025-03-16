use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct InstalledApp {
    pub id: String,
    pub key_name: String,
    pub scope: String,
    pub view: String,
    pub name: String,
    pub publisher: String,
    pub version: String,
    pub estimated_kib: Option<u64>,
    pub install_location: Option<PathBuf>,
    pub display_icon: String,
    pub uninstall_string: String,
    pub windows_installer: bool,
    pub system_component: bool,
    pub icon: Option<IconPixels>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IconPixels {
    pub rgba: Vec<u8>,
    pub width: usize,
    pub height: usize,
}

#[derive(Default)]
pub struct Inventory {
    pub apps: Vec<InstalledApp>,
    pub warnings: Vec<String>,
}

impl InstalledApp {
    pub fn matches(&self, query: &str) -> bool {
        let q = query.to_lowercase();
        self.name.to_lowercase().contains(&q) || self.publisher.to_lowercase().contains(&q)
    }
}

pub fn enumerate() -> anyhow::Result<Inventory> {
    #[cfg(windows)]
    {
        windows::enumerate()
    }
    #[cfg(not(windows))]
    {
        anyhow::bail!("Installed application inventory requires Windows")
    }
}
/// Read only the selected registration in both architecture views. No icon scan.
pub fn registration_exists(app: &InstalledApp) -> anyhow::Result<bool> {
    #[cfg(windows)]
    {
        windows::registration_exists(app)
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        anyhow::bail!("Registration observation requires Windows")
    }
}

#[cfg(windows)]
#[path = "windows_inventory.rs"]
mod windows;

pub mod branding;
#[cfg(windows)]
pub mod capture;
pub mod command;
pub mod design;
pub mod inventory;
pub mod leftovers;
pub mod removal;
pub mod repaint;
pub mod settings;
#[cfg(windows)]
pub mod software;
pub mod ui;
pub mod updater;
pub mod worker;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const REPOSITORY: &str = "https://github.com/siarheisinelnikau/app-uninstaller";

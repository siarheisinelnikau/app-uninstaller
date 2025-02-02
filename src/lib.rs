pub mod branding;
pub mod command;
pub mod inventory;
pub mod leftovers;
pub mod settings;
pub mod ui;
pub mod updater;
pub mod worker;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const REPOSITORY: &str = "https://github.com/siarheisinelnikau/app-uninstaller";

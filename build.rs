#[path = "src/icon_pixels.rs"]
mod icon_pixels;
fn main() {
    println!("cargo:rerun-if-changed=src/icon_pixels.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let path =
        std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("AppUninstaller.ico");
    std::fs::write(&path, icon_pixels::ico()).expect("Cannot write application icon");
    let version = std::env::var("CARGO_PKG_VERSION").unwrap();
    winresource::WindowsResource::new()
        .set_icon(path.to_str().unwrap())
        .set("ProductName", "AppUninstaller")
        .set("FileDescription", "AppUninstaller")
        .set("FileVersion", &version)
        .set("ProductVersion", &version)
        .set(
            "OriginalFilename",
            &format!("AppUninstaller-v{version}.exe"),
        )
        .compile()
        .expect("Cannot embed application icon and version resources");
}

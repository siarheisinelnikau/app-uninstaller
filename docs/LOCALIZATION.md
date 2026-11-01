# Localization and appearance in 0.2.0

The complete English UI is the catalog key/fallback language. `locales/messages.json` stores five translations per message in Russian, French, German, Spanish and Italian order. `src/i18n.rs` embeds the catalog once and provides explicit lookup plus positional parameter substitution. User names, commands, paths and other substituted values are never looked up or interpreted again.

The 192-message catalog covers application navigation and controls, Settings/updates, uninstall/restore confirmations, removal-state/countdown text, leftover explanations and common recoverable safety errors. Remote release notes, third-party licenses, publisher/user data and technical error chains stay in their original language. The Windows title retains the product name. This app has no tray menu.

`Settings.language` and `Settings.theme` use additive defaults and unknown-value fallbacks. Older JSON settings preserve `check_on_startup`. Display choices save with the existing atomic file replacement; a queue/send error or asynchronous save error restores the previous settings. `Message` preserves its template and values so language changes also update existing status text.

`design::palette` supplies separate light/dark background, surface, text, secondary text, accent, accent-text and selection roles. Dark-mode accent text uses a lighter mint than filled action buttons. Egui’s theme covers controls and confirmation windows; diagnostic text uses theme-specific warning/error roles. The saved choice pins egui’s theme preference, so an opposite Windows theme event cannot switch to another style. Typography and spacing are configured for both styles. `Desktop::ensure_theme` applies visuals only after a theme change. Neither preference introduces a timer, polling worker or recurring invalidation.

## Verification

- The source audit found no untranslated literal labels in primary/settings/confirmation screens. Catalog completeness and placeholder equality are tested for every translation, with explicit literal-lookup coverage.
- Tests exercise old and future settings, an atomic file roundtrip on a temporary directory, status reformatting while preserving the app name, failed-save theme recovery, and Light → Dark → Light switching against opposite OS-theme events.
- The existing CPU rasterizer, retained scene cache, GDI presentation and `ControlFlow::Wait` scheduling were reviewed. Hidden redraws return before UI construction. Actual new-binary idle/hidden/restore measurements were not taken.
- The repository instructions prohibit launching the application or taking screenshots in this task. The README preserves real 0.1.2 screenshots and labels their version. New 0.2.0 main/settings screenshots and visual checks for long translations and dark mode are pending; source checks do not establish visual fit or interactive performance.

Completed on 2026-11-01 with the existing Windows x64 MSVC toolchain and the shared bounded Cargo helper (repository-local target; two jobs per invocation):

- `cargo fmt --all -- --check` passed.
- `cargo clippy --locked --offline --all-targets -- -D warnings` passed.
- `cargo test --locked --offline --all-targets` passed: **40 tests** (30 library and 10 integration); the binary test harness ran zero tests and opened no window.
- `cargo build --release --locked --offline --target x86_64-pc-windows-msvc` passed.
- The binary was packaged as `dist/AppUninstaller-v0.2.0.exe` with notices, license and `SHA256SUMS.txt` without re-executing it. Read-only PE/resource inspection confirmed Windows x64 GUI subsystem, product version 0.2.0, no imported GPU API or dynamic MSVC runtime, and seven intact 32-bit icon images (16/20/24/32/48/64/256px), each with visible artwork and transparent corners.

Package size: **8,623,616 bytes**. SHA-256: `6d4b958abb8da9b69437ef7efed0365e57e206980a087ace5c7c6ef83fadd803`. See [package metadata](evidence/package-0.2.0.json). Resource-byte checks do not establish native window/taskbar appearance. No new application/settings screenshots were generated.

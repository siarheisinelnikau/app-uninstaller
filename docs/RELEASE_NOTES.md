# AppUninstaller — local release drafts

These versions are local and unpublished. There is no preceding published release or verified remote comparison baseline.

## 0.1.1 — removal status and list lifecycle

Changes since the local 0.1.0 implementation (`c46a30a`). Windows x64 portable package: `AppUninstaller-v0.1.1.exe`.

### Added

- Observe the selected Windows uninstall registration automatically, including after a detached launcher exits. Two consecutive absent checks confirm removal; read errors and a returning registration keep the item visible.
- Mark confirmed removals in the list immediately, show a ten-second countdown, then fade and collapse the row over 260 ms. Neighboring rows move up and rendering returns to idle.
- Stop observation without terminating the publisher's process. Observation has finite deadlines and checks only the selected registration once per second, without inventory/icon rescans or repaints for unchanged results.

### Fixed

- Missing uninstaller files now show a launch failure without leaving a stale “waiting for the publisher” message. Missing files and process exit codes alone never claim successful removal.
- Show the active removal state in the details instead of inviting another launch. Keep leftover review separate and unavailable until registration absence and a suitable process exit are observed.
- Preserve the grace row during Refresh, cancel it when the registration returns, and prevent a final observer event from resurrecting an expired row.

### Verification and limits

Formatting, Clippy and 29 fixture tests passed. Real application-generated screenshots cover all removal states. The exact packaged EXE passed the isolated CPU/GDI visible/hidden/restore gate after a live fixture collapse, with zero settled UI/raster/presentation work. No real app was uninstalled and no update was installed; publisher behavior, native registry observation and hands-on interaction still require disposable-program validation. See [verification](VERIFICATION.md) and [manual tests](MANUAL_TESTS.md).

## 0.1.0 — initial implementation

### Added

- Review classic Windows applications from current-user and machine-wide uninstall registrations in both registry views, with search, sorting and available publisher/version/size/icon metadata.
- Inspect the exact registered uninstaller before explicitly running it. MSI product registrations use interactive Windows Installer removal; ambiguous and shell-based commands are refused.
- Capture bounded ownership evidence before removal and review unchanged installation-folder leftovers with path-level explanations and SHA-256 fingerprints.
- Move an explicitly selected, revalidated folder into same-volume quarantine and restore it without overwriting existing files. Recovery journals persist between launches.
- Configure background startup update checks, perform manual checks, view release notes and initiate a checksummed download/restart installation. Retain the previous EXE and roll back replacement/restart failures.
- Package a Windows x64 standalone `AppUninstaller-v0.1.0.exe` with checksum and open-source notices. Source and application use English; the project is MIT licensed.

### Improved during local review

- Replaced the OpenGL host with CPU rasterization and GDI presentation. Settled visible/hidden/restored fixture windows stop layout, rasterization and presentation; unchanged scenes reuse cached pixels.
- Improved typography, contrast, navigation and spacing. The uninstall review action stays visible at the minimum window size; technical explanations are disclosed when needed.
- Added bounded offscreen screenshot capture and an isolated runtime rendering check, without desktop automation or effects on installed applications.

## Requirements and current limits

Windows x64; the application uses CPU rendering with Win32 GDI, with no OpenGL/DirectX context. Administrator-only uninstallers may require Windows Installed apps; AppUninstaller does not request elevation. Leftover suggestions deliberately exclude uncertain ownership, shared/system/document locations, custom/vendor-nested installation folders and registry values. Quarantine preserves disk usage and does not undo the publisher's uninstall.

Complete the deferred manual checks before public distribution. No release has been created or uploaded.

# AppUninstaller 0.1.0 — initial release draft

This is a local, unpublished release candidate. There is no preceding published release or verified remote comparison baseline.

## Added

- Review classic Windows applications from current-user and machine-wide uninstall registrations in both registry views, with search, sorting and available publisher/version/size/icon metadata.
- Inspect the exact registered uninstaller before explicitly running it. MSI product registrations use interactive Windows Installer removal; ambiguous and shell-based commands are refused.
- Capture bounded ownership evidence before removal and review unchanged installation-folder leftovers with path-level explanations and SHA-256 fingerprints.
- Move an explicitly selected, revalidated folder into same-volume quarantine and restore it without overwriting existing files. Recovery journals persist between launches.
- Configure background startup update checks, perform manual checks, view release notes and initiate a checksummed download/restart installation. Retain the previous EXE and roll back replacement/restart failures.
- Package a Windows x64 standalone `AppUninstaller-v0.1.0.exe` with checksum and open-source notices. Source and application use English; the project is MIT licensed.

## Requirements and current limits

Windows x64 with an OpenGL-compatible driver. Administrator-only uninstallers may require Windows Installed apps; AppUninstaller does not request elevation. Leftover suggestions deliberately exclude uncertain ownership, shared/system/document locations, custom/vendor-nested installation folders and registry values. Quarantine preserves disk usage and does not undo the publisher's uninstall.

Automated fixture verification and compilation are recorded separately in `VERIFICATION.md`. GUI operation, live inventory/uninstall, real application/settings screenshots and a full published-release update/restart have **not** been verified in the unattended session. Complete `MANUAL_TESTS.md` before public distribution. No release has been created or uploaded.

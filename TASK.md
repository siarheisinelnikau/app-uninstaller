# AppUninstaller

Repository: `app-uninstaller`
Priority: 6 of 15 (user-requested launch order)
Product: AppUninstaller
Purpose: Application removal with explainable leftover review
Inspiration: https://www.revouninstaller.com/buy-now-revo-uninstaller-pro/

Independent open-source implementation; no affiliation, competitor code or copied assets.

## Requested implementation

Build a friendly installed-app inventory and uninstall workflow. Implement Windows registry enumeration for per-user/per-machine and 32/64-bit registrations, display names/icons/publishers/sizes when available, search/sort and the appropriate registered uninstall action. Treat command lines and paths safely rather than passing interpolated text to a shell.

After uninstall, propose leftover files/settings only with concrete ownership evidence. Explain why each item is suggested, exclude shared/system/user-document locations, require a preview and explicit selection, and support quarantine/backup where practical. Never implement indiscriminate registry cleaning. Real discovery and command-building logic must be separated from effectful operations. Tests use fixture registrations and temporary folders. Do not uninstall anything, terminate processes, edit the host registry or scan/delete real application leftovers in this unattended run.

## Packaging and publication target

Windows standalone asset: `AppUninstaller-vVERSION.exe` (for the initial release, `AppUninstaller-v0.1.0.exe`). Append architecture only when distributing multiple executable architectures in the same release.
Planned canonical repository: https://github.com/siarheisinelnikau/app-uninstaller (not yet created or published; keep availability/error reporting truthful).

## Working agreement

Follow AGENTS.md. Complete as much real implementation and safe noninteractive verification as possible in this unattended code-only session. Manual UI, device and hardware tests will happen later. Record required external components and their verification status. Keep changes committed locally in meaningful increments.


# AppUninstaller

A Windows desktop application for reviewing installed programs, running their registered uninstallers, and deciding what to do with proven leftovers. Original implementation and interface; no affiliation with Revo or use of competitor code/assets.

**Status:** first local implementation, version 0.1.0. Windows x64 is the packaging target. The repository and releases have not been published. CPU/GDI rendering, isolated idle/hidden/restore behavior and static UI screenshots have been checked. Destructive live workflows and final hands-on responsiveness remain pending.

## What works in the code

- Read-only inventory of current-user/all-users registrations in 32-bit and 64-bit registry views. Shows publisher, version, estimated size and registered icon when supplied. Mirrored registrations are merged; search and sorting are available.
- A two-step uninstall workflow: inspect the executable and arguments, explicitly accept, then run the publisher's registered action. Commands never pass through `cmd`, PowerShell or interpolated shell text. MSI registrations use their validated product code with interactive `/x` and `/norestart`.
- Bounded ownership evidence captured before removal. After the registration disappears, review only unchanged items in the exact registered installation folder. Every suggested path has an explanation and file SHA-256; nothing is selected by default.
- Reversible quarantine by same-volume folder rename, with a flushed JSON recovery journal written first. Restore refuses to overwrite an existing original folder or restore changed quarantine contents. Quarantine retains the same disk usage; permanent deletion remains manual.
- Settings with installed version, **Check for updates**, persistent **Check for updates on startup** (enabled by default), release notes, progress and user-initiated **Download and install**. A separate confirmation restarts into the verified update. A helper copy of the existing EXE replaces the file after exit, retains `previous.exe`, and rolls back failed replacement/restart attempts.
- One bounded background worker for registry, filesystem, process and network work; a virtualized inventory and reactive egui desktop UI. No continuous application repaint loop or background scanning. Unchanged scenes skip rasterization; native exposure can reuse the retained bitmap. Ordinary settled hidden windows do not build or draw the UI.

## Build and review

Requires a Windows x64 Rust MSVC toolchain (Rust 1.92 or later; verified with 1.96.1), the MSVC linker and Windows SDK. The UI uses egui 0.33.3 with CPU rasterization and Win32 GDI presentation. The application creates no OpenGL, DirectX or Vulkan context. No WebView runtime, installer, service, driver or external AI runtime is needed. The x64 release statically links the MSVC runtime where supported.

```powershell
Set-Location E:\work\github\app-uninstaller
./scripts/check.ps1 -Offline
./scripts/package.ps1 -Offline
```

Omit `-Offline` only when dependency downloads are available and desired. Cargo uses two jobs and this repository's `target` folder. Packaging writes `dist/AppUninstaller-v0.1.0.exe`, `SHA256SUMS.txt` and license notices. It never executes the application or publishes anything. Run the EXE manually only when interactive testing is authorized.

The executable contains third-party license/font notices under Settings; `THIRD_PARTY_NOTICES.txt` also accompanies packaged artifacts. Refresh it after dependency changes with `python scripts/notices.py` using the local Cargo cache.

## Removal and recovery

1. Choose an application in **Applications**, then **Review uninstall…**.
2. Read the publisher, executable, arguments and ownership limitation. Check the consent box and select **Run registered uninstaller**.
3. Complete all publisher windows. In **Leftover review**, confirm that it finished and request a review. A remaining registration blocks quarantine. Restart-required exit codes disable leftover quarantine for that session.
4. Review the folder, ownership evidence and individual file/folder preview. Explicitly select the folder, then quarantine it.
5. In **Quarantine**, review the original destination and explicitly restore when needed. Restore preserves data but does not reinstall the application.

Settings and recovery journals live in `%LOCALAPPDATA%\AppUninstaller`. Quarantine folders are siblings of the original folder, named `.AppUninstaller-quarantine-<UUID>`, so the move stays on the same volume and preserves file data, ACLs and streams. If access is denied, the original stays in place and an error explains the limitation. The app never requests administrator elevation. A publisher uninstaller requiring elevation may need to be run through Windows Installed apps instead.

Keep the JSON journal with its quarantine folder. If the journal is lost, the folder can be recovered manually by renaming it to its original name while that name is absent. Do not permanently delete quarantine data until you have verified that you no longer need it.

## Deliberate limits

- Inventory covers classic registry-based desktop applications, not a full MSIX/Store, Windows feature, extension or portable-program inventory. Missing metadata remains unknown. Estimated size is publisher metadata, not measured disk usage.
- Shell/script-host commands, network executables, ambiguous unquoted paths and MSI commands without validated product registrations are refused. No forced or batch uninstall, process termination, automatic elevation, registry editing or registry cleaning is implemented.
- Ownership review requires an exact application-name folder directly under Program Files or AppData, a registered uninstaller inside it, no overlapping registration, no protected/shared root and no symbolic links/reparse points. Versioned/vendor-nested/custom folders, most MSI installations and separately stored application settings often lack this evidence and receive **no cleanup suggestion**. The app does not guess AppData or registry ownership from names.
- Capture is limited to 5,000 tree entries, 512 MiB of file contents and 32 folder levels. New/changed files or folders invalidate a preview. Hidden entries are included. Encrypted/offline files and reparse points are excluded.
- Quarantine is selected at the proven-folder level; individual files are previewed but cannot be separately removed. Ownership evidence is session-only. Closing the app loses an unfinished removal review; existing quarantine journals remain recoverable.
- File hashes and repeated checks protect ordinary review mistakes, not hostile concurrent filesystem/registry mutation. Do not run simultaneous cleaners/installers against the same folder. A directory rename is not an OS restore point and cannot undo the publisher's actions.
- Only Windows x64 portable releases are selected for updates. No release currently exists: an unavailable repository/release is reported as unavailable, never “up to date.” Installation requires a writable EXE folder and a supplied SHA-256 digest or matching `SHA256SUMS.txt`. Checksums use the same GitHub trust channel; Authenticode signing is not configured.
- A failed helper records `result.txt` under `.AppUninstaller-update-<UUID>` beside the EXE. The most recent report is visible in Settings on the next manual launch. `previous.exe` and staged files remain for manual recovery; helper/staging cleanup is intentionally deferred. A successful launch request does not prove that the new GUI finished starting.
- In-place updates preserve the original launch path. If that filename contains an older version, Settings shows the actual installed binary version; newly published download assets still use their correct versioned filenames.

## Verification and screenshots

See [verification record](docs/VERIFICATION.md), [architecture](docs/ARCHITECTURE.md), [manual validation](docs/MANUAL_TESTS.md) and [initial release notes draft](docs/RELEASE_NOTES.md).

These are real client-area PNGs saved by the packaged application's production UI components and CPU rasterizer. They use deterministic sample registrations, contain no personal inventory, and do not show native Windows chrome. All actions are disabled in capture mode.

![Installed applications and registered uninstall action](docs/screenshots/applications.png)

Installed applications: a focused list, readable installation details and a review action visible without scrolling, including at the minimum supported width.

![Settings with startup update preference and CPU rendering details](docs/screenshots/settings.png)

Settings: installed version, persistent startup-check preference, manual update checking and local recovery storage.

![Explainable leftover review with explicit quarantine selection](docs/screenshots/leftovers.png)

Leftover review: ownership evidence and an unchecked selection before reversible quarantine. This is sample leftover data rendered by the real screen; no real application was uninstalled.

To reproduce static captures without desktop automation:

```powershell
./scripts/capture.ps1
```

Each capture runs offscreen, waits for stable layout, saves a PNG plus metadata, and exits with a finite frame/time budget. `--screenshot ABSOLUTE_PATH --screen applications|settings|leftovers|quarantine [--size WIDTHxHEIGHT]` is a developer-only mode, not part of consumer settings. Captures verify visual layout, not keyboard/mouse behavior or normal hidden operation.

For the separate bounded runtime gate, run `./scripts/verify-rendering.ps1`. It uses 840 isolated sample registrations, shows/hides/restores only its own nonactivating, pointer-transparent test window, records CPU and actual UI/raster/presentation counters, and exits after three settled five-second intervals (36-second internal limit). It never accesses the registry, network, normal settings or real leftovers. A nonzero input count invalidates a quiet sample. See the evidence record for results and remaining limits.

## License

[MIT](LICENSE). Dependency and font terms appear in [third-party notices](THIRD_PARTY_NOTICES.txt).

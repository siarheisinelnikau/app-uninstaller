# Architecture

The application is a Windows-first Rust binary. `main.rs` dispatches the desktop UI, explicit `--apply-update` helper, bounded `--screenshot` capture, or isolated `--verify-rendering` diagnostics. No inventory, uninstaller, network request or self-replacement runs during unit tests or compilation.

## Boundaries

| Module | Responsibility | Effects |
| --- | --- | --- |
| `inventory.rs` / `windows_inventory.rs` | Installed application models; typed, bounded HKCU/HKLM and WOW64 reads; local icon extraction | Read-only registry/files and GDI icon extraction |
| `command.rs` | Validated pure uninstall planning, MSI product-code parsing; isolated process execution | Process creation only in `spawn`; returns a child handle without waiting |
| `removal.rs` | Registration observation state, confirmation thresholds, bounded deadlines and finite row progress | Pure model; injected time and observations |
| `leftovers.rs` | Ownership capture, bounded manifest/SHA-256, post-uninstall review, quarantine/recovery | Read-only discovery separated from explicit rename/journal operations |
| `settings.rs` | Startup update preference, per-user data location | Small JSON read; atomic replacement on save |
| `updater.rs` | Stable-release selection, checksums, bounded HTTPS downloads, helper/rollback | Network/staging only after check/download commands; replacement only in helper |
| `worker.rs` | Serial operation sequencing and revalidation | One worker, two queued commands, sixteen result messages |
| `ui.rs` / `design.rs` | Inventory, review, consent, recovery, settings and shared visual roles | Sends explicit commands; uploads prepared icon textures |
| `software.rs` / `repaint.rs` | Reactive winit host, CPU rasterizer, retained bitmap, Win32 GDI and scheduling | Native window/presentation; optional bounded diagnostic counters |
| `capture.rs` | Actual production UI rendered offscreen with isolated fixtures | Writes a selected client-area PNG and metadata, then exits |

The worker sleeps on channel receive when idle. During one active removal it uses a one-second receive timeout, `Child::try_wait` and a read-only probe of the exact registration in both registry views. No inventory/icon rescan runs on that timer. Only changed status or errors publish an event and wake egui. Long update operations and additional removals are disabled during observation.

The UI uses filtered/sorted index snapshots and virtual rows for the installed-app list. A viewport layout reserves each row's current height but only constructs visible row widgets, allowing one removed row and its gap to collapse without rebuilding hundreds of widgets. It does not scan the registry/filesystem, hash files, wait for uninstallers, or perform network work in a frame callback. Snapshots are cloned only when submitting work or publishing a changed result. Busy spinners repaint during pending work; the idle app does not request continuous redraws. A bounded isolated runtime gate measured zero settled UI/raster/presentation work in visible, hidden and restored states after a live fixture transition. Hands-on responsiveness, native scaling and broader platform compatibility remain separate checks.

## Removal observation

The observer requires two consecutive absent registration probes before reporting removal. Either registry view still containing the key means present. Permission/read errors mean unknown, never absent. Reappearance or uncertainty cancels confirmation and the UI countdown. Neither a missing uninstaller file nor a zero process exit code proves removal. A failed launch starts observation with an explicit error; no publisher process is claimed to be running.

The row is marked removed immediately after confirmation, held for ten seconds with one-second label updates, then faded/clipped and collapsed over 260 ms. Only that finite transition requests animation frames. Expired rows are removed from the inventory; their session timestamps prevent a later completion event from resurrecting them. Full Refresh preserves a pending grace row but cancels it if the registration is present again. Hidden windows consume changed worker state without rendering; elapsed row transitions finish on the next visible update.

Observation ends after a confirmed registration has been absent for at least 12 seconds and the launched process has stopped, after two minutes following an unconfirmed exit/failed launch, or at the 30-minute absolute deadline. `Stop watching` ends observation without killing a process. Dropping a child handle never terminates the uninstaller. This tracks one selected removal, including a detached launcher's registration changes, rather than every application on the system.

## Ownership and quarantine

Capture must establish all of: a registered installation folder; exact normalized folder/application name; a direct registered EXE uninstaller inside that folder; an allowed direct parent; no shared/protected root, reparse ancestry or other installation overlap. The entire bounded tree is read and ordinary file contents are hashed before the uninstaller starts. Registry reads are repeated before each effectful step. Partial inventories block destructive workflow steps. Nameless registrations with install locations remain in the model as inspection-only entries so that they still protect overlapping folders.

The worker exposes the captured snapshot only after confirmed registration absence and a known process exit, excluding restart-required codes. The user separately confirms that all publisher windows have finished and requests review; a parent process exit alone cannot establish that detached children are done. A fresh complete inventory must still find the registration gone; a matching entry in another view or another installation using the directory blocks the action. Remaining paths must be an unchanged subset of the captured tree, including empty directories. A new/changed entry rejects the whole suggestion. A further complete inventory and manifest comparison run immediately before quarantine. Observation never starts cleanup automatically.

Quarantine uses a unique sibling destination and `rename`, not copy/delete. The recovery record is created exclusively, written and flushed before renaming. A crash before rename leaves a harmless pending journal; a crash after rename leaves enough paths/hashes for recovery. The JSON record stores original/destination paths, application name, UUID and remaining tree fingerprints. Restore validates the destination naming/parent relationship, checks the complete quarantine manifest, and refuses to overwrite the original location. No recursive deletion or registry removal exists in this code.

The registry is a publisher-provided evidence source, not an authenticity guarantee. A hash proves unchanged bytes, not that a file contains no user data. Folder review and explicit consent remain necessary. Reparse checks and hashes do not provide a handle-based, adversarially atomic tree transaction. Same-user malicious mutation and privileged interference are outside the first version's guarantees; no elevated helper expands its privileges.

## Update lifecycle

Startup loads preferences before deciding whether to check. Corrupt/unavailable settings disable the startup request for that session and expose the error. A stable-release check uses the canonical public GitHub API, no login/token, numeric SemVer and an exact x64 portable asset name. Drafts/prereleases are excluded; errors, 404s, rate limits and missing compatible/checksummed assets are not treated as up-to-date results.

Download is initiated by a button and runs with a size cap, timeouts and throttled progress. It stages beside the installed EXE, verifies size and SHA-256 from GitHub's release digest or `SHA256SUMS.txt`, copies the running EXE as a helper, and writes a constrained plan. The user then chooses restart/install. Before spawning the helper, hashes are checked again. The helper waits for the parent to exit, revalidates paths/hashes, renames the current file to `previous.exe`, installs the payload and requests a restart. Failed replacement or launch rolls back with errors preserved. Neither settings nor journals are replaced. No updater code is executed by the test suite; the replacement engine is tested with inert fixture bytes and an injected no-op/failing launch callback.

Staged reports/backups are retained and surfaced on next launch. Signed binaries, automatic helper cleanup, multi-instance coordination and an end-to-end published-release update test remain future validation/work. The updater does not create scheduled tasks, services, registry entries or elevated processes.

## Implementation references

- [Microsoft uninstall registry properties](https://learn.microsoft.com/en-us/windows/win32/msi/uninstall-registry-key)
- [Rust Windows command argument handling](https://doc.rust-lang.org/std/os/windows/process/trait.CommandExt.html#tymethod.raw_arg)

The application title/window mark and UI arrangement are original code. Publisher icons are read from each installed application's registration, when available; no competitor assets are bundled.

## CPU rendering and redraw boundaries

The Windows dependency graph contains no eframe/glow/glutin/wgpu renderer. egui builds clipped shapes; `egui_software_backend` rasterizes on the CPU; softbuffer's Win32 implementation presents via GDI. Windows composition remains the operating system's responsibility. AccessKit is initialized while the native window is still invisible, before its first show. The product uses the installed Segoe UI font when available, with bundled egui fonts as fallback; no Windows font file is redistributed.

The host sleeps in `Wait`/`WaitUntil`, preserves finite egui deadlines, coalesces queued redraw requests and discards stale repaint generations. High-frequency input is coalesced to the monitor cadence; this does not schedule periodic idle frames. Mouse buttons and keys are handled promptly. Shape, scale, size and texture changes invalidate the cached CPU scene. Equal scenes skip rasterization and input-driven presentation. OS exposure can blit the retained bitmap. Hidden/minimized windows retain dirty state and stop UI layout, rasterization and presentation until restoration.

`--verify-rendering` initializes the normal host with 840 in-memory registrations and no worker. One removed fixture row uses the live clock to finish its countdown and collapse before the visible phase can settle. Its own finite diagnostic events hide/restore that disposable window. Reports count UI passes, actual rasterizations, presentations, exposure blits, wakeups and input events separately; CPU is sampled off the UI thread. These fixture results do not certify real uninstaller I/O, live icon enumeration, input timing or every graphics/Windows configuration.

`--screenshot` uses the same `Desktop::update`, visual system, fonts and CPU rasterizer without creating a native window. It uses no worker, registry, network or normal settings. Stable layout must be reached within 12 frames and five seconds. The parent capture script applies a 20-second process timeout. The resulting PNG verifies only the rendered client area, not native chrome or interactive behavior.

Worker results are drained from native user events independently of layout, including while hidden. Dirty state and texture updates are kept for the next visible frame. A user-initiated updater exit can complete without forcing a hidden frame. Restore/resize sets an exposure flag so a coalesced native paint request still gets the retained bitmap.

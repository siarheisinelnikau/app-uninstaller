# Architecture

The application is a Windows-first Rust binary. `main.rs` dispatches either the desktop UI or the explicit `--apply-update` helper mode. No inventory, uninstaller, network request or self-replacement runs during unit tests or compilation.

## Boundaries

| Module | Responsibility | Effects |
| --- | --- | --- |
| `inventory.rs` / `windows_inventory.rs` | Installed application models; typed, bounded HKCU/HKLM and WOW64 reads; local icon extraction | Read-only registry/files and GDI icon extraction |
| `command.rs` | Validated pure uninstall planning, MSI product-code parsing; isolated process execution | Process creation only in `execute` |
| `leftovers.rs` | Ownership capture, bounded manifest/SHA-256, post-uninstall review, quarantine/recovery | Read-only discovery separated from explicit rename/journal operations |
| `settings.rs` | Startup update preference, per-user data location | Small JSON read; atomic replacement on save |
| `updater.rs` | Stable-release selection, checksums, bounded HTTPS downloads, helper/rollback | Network/staging only after check/download commands; replacement only in helper |
| `worker.rs` | Serial operation sequencing and revalidation | One worker, two queued commands, sixteen result messages |
| `ui.rs` | Inventory, review, consent, recovery, settings/update UI | Sends explicit commands; uploads prepared icon textures |

The worker sleeps on channel receive when idle. It wakes egui only when publishing a result/progress update. The UI uses filtered/sorted index snapshots and virtual rows for the installed-app list. It does not scan the registry/filesystem, hash files, wait for uninstallers, or perform network work in a frame callback. Snapshots are cloned only when submitting review/quarantine work, rather than on every frame. Busy spinners repaint during pending work; the idle app does not request continuous redraws. Actual idle CPU, frame latency, scaling and graphics compatibility still require measurement on a real desktop.

## Ownership and quarantine

Capture must establish all of: a registered installation folder; exact normalized folder/application name; a direct registered EXE uninstaller inside that folder; an allowed direct parent; no shared/protected root, reparse ancestry or other installation overlap. The entire bounded tree is read and ordinary file contents are hashed before the uninstaller starts. Registry reads are repeated before each effectful step. Partial inventories block destructive workflow steps. Nameless registrations with install locations remain in the model as inspection-only entries so that they still protect overlapping folders.

After the publisher's process exits, the user separately requests review. The registration must be gone; a matching entry in another view or another installation using the directory blocks the action. Remaining paths must be an unchanged subset of the captured tree, including empty directories. A new/changed entry rejects the whole suggestion. A further complete inventory and manifest comparison run immediately before quarantine.

Quarantine uses a unique sibling destination and `rename`, not copy/delete. The recovery record is created exclusively, written and flushed before renaming. A crash before rename leaves a harmless pending journal; a crash after rename leaves enough paths/hashes for recovery. The JSON record stores original/destination paths, application name, UUID and remaining tree fingerprints. Restore validates the destination naming/parent relationship, checks the complete quarantine manifest, and refuses to overwrite the original location. No recursive deletion or registry removal exists in this code.

The registry is a publisher-provided evidence source, not an authenticity guarantee. A hash proves unchanged bytes, not that a file contains no user data. Folder review and explicit consent remain necessary. Reparse checks and hashes do not provide a handle-based, adversarially atomic tree transaction. Same-user malicious mutation and privileged interference are outside the first version's guarantees; no elevated helper expands its privileges.

## Update lifecycle

Startup loads preferences before deciding whether to check. Corrupt/unavailable settings disable the startup request for that session and expose the error. A stable-release check uses the canonical public GitHub API, no login/token, numeric SemVer and an exact x64 portable asset name. Drafts/prereleases are excluded; errors, 404s, rate limits and missing compatible/checksummed assets are not treated as up-to-date results.

Download is initiated by a button and runs with a size cap, timeouts and throttled progress. It stages beside the installed EXE, verifies size and SHA-256 from GitHub's release digest or `SHA256SUMS.txt`, copies the running EXE as a helper, and writes a constrained plan. The user then chooses restart/install. Before spawning the helper, hashes are checked again. The helper waits for the parent to exit, revalidates paths/hashes, renames the current file to `previous.exe`, installs the payload and requests a restart. Failed replacement or launch rolls back with errors preserved. Neither settings nor journals are replaced. No updater code is executed by the test suite; the replacement engine is tested with inert fixture bytes and an injected no-op/failing launch callback.

Staged reports/backups are retained and surfaced on next launch. Signed binaries, automatic helper cleanup, multi-instance coordination and an end-to-end published-release update test remain future validation/work. The updater does not create scheduled tasks, services, registry entries or elevated processes.

## Implementation references

- [Microsoft uninstall registry properties](https://learn.microsoft.com/en-us/windows/win32/msi/uninstall-registry-key)
- [Rust Windows command argument handling](https://doc.rust-lang.org/std/os/windows/process/trait.CommandExt.html#tymethod.raw_arg)
- [eframe 0.33.3 documentation](https://docs.rs/eframe/0.33.3/eframe/)

The application title/window mark and UI arrangement are original code. Publisher icons are read from each installed application's registration, when available; no competitor assets are bundled.

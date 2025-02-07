# Verification record

Date: 2026-10-31 (Europe/Minsk). Work stayed inside `E:\work\github\app-uninstaller`. No remote Git operation, publication, credentials flow, elevation, GUI launch, desktop automation or screenshot capture was performed.

## Completed noninteractive checks

- Dependencies resolved from the existing local Cargo cache with `cargo generate-lockfile --offline`; `Cargo.lock` is committed. No dependency download or toolchain installation was required.
- Toolchain reported `rustc 1.96.1 (31fca3adb 2026-06-26)` and `cargo 1.96.1 (356927216 2026-06-26)` on Windows.
- `cargo fmt --all -- --check` passed.
- `cargo clippy --offline --all-targets --locked -j 2 -- -D warnings` passed.
- `cargo test --offline --all-targets --locked -j 2` passed: **16 tests**, comprising 6 updater/planning tests and 10 safety/inventory/settings tests. The main binary's test harness ran zero tests; it did not call the GUI entry point.
- Test fixtures are created exclusively under `target/test-fixtures` and automatically removed. Tests do not enumerate the host registry, run an uninstaller, create a process from fixture executables, fetch releases, invoke the updater helper or touch real application leftovers.
- A complete fixture cycle captures ownership, removes an inert pretend uninstaller file, reviews remaining contents, writes a journal, renames to quarantine and restores exact bytes. Other cases verify occupied destinations, modified backups, forged journal paths, stale previews, new empty directories, new/changed files, overlapping registrations, protected/custom locations and absent leftovers.
- Pure command tests reject shell hosts, remote/relative paths, ambiguous unquoted paths, scripts, traversal, alternate streams and malformed commands. MSI tests verify exact GUID-based interactive removal planning. Search covers case-insensitive names/publishers and missing metadata.
- Update fixtures verify numeric SemVer, stable-channel selection, exact canonical asset URLs/names, required checksums, exact checksum-file matching, constrained target paths, backup preservation and rollback after simulated replacement/restart errors. Replacement tests inject an inert callback and never execute the fixture bytes.
- Settings tests verify enabled-by-default behavior, persistent opt-out and a visible parse error for corrupted JSON. They do not make network requests.
- Dependency attribution was generated from locked local metadata with `python scripts/notices.py`; packaged fonts' OFL/UFL/MIT notices are included. Settings exposes embedded notices, so the portable EXE carries the attribution text.

## Packaging

- `./scripts/check.ps1 -Offline` passed on the final application sources: formatting, Clippy with warnings denied, and all 16 tests.
- `./scripts/package.ps1 -Offline` successfully compiled the optimized `x86_64-pc-windows-msvc` target using two jobs and static MSVC runtime flags. The initial release compilation took 3 minutes 18 seconds. No executable was launched.
- Artifact: `dist/AppUninstaller-v0.1.0.exe`, **10,131,968 bytes**.
- SHA-256: `8a2fc680b67d8f861d74439413ff287406a1ba1bc9790131e3f97568a74872e0`. It matches `dist/SHA256SUMS.txt`.
- `python scripts/inspect_pe.py dist/AppUninstaller-v0.1.0.exe` passed by reading bytes only: x86_64 machine type, PE32+ format, Windows GUI subsystem. Imports are Windows system DLLs, including `OPENGL32.dll`; no dynamic `VCRUNTIME`/`MSVCP` imports were found. This is not a clean-machine runtime test.
- `dist/LICENSE` and `dist/THIRD_PARTY_NOTICES.txt` accompany the EXE and checksum. `dist`/`target` are ignored build outputs, not committed binaries.
- `git diff --check` passed. All source, lockfile, scripts and documentation changes were committed locally. Nothing was published.

## Not verified

The connected live Windows inventory, publisher uninstall behavior, UAC/admin-only cases, graphics/idle performance, DPI/layout/accessibility, real ACL/stream preservation and end-to-end in-place update/restart have not been exercised. Real application/settings screenshots are deferred. CI configuration is prepared but has not run on GitHub. No public repository/release availability was probed or invented; the configured canonical location comes from TASK.md.

The first test attempt used the operating system temporary folder and encountered sandbox access-denied rename operations. Fixtures were moved inside the repository's target directory; these tests now pass. A fixture path with mixed separators was corrected, while valid extended local Windows paths are accepted by command planning. There were no approval/elevation requests and no skipped or ignored tests in the final passing suite.

Follow [the deferred manual checklist](MANUAL_TESTS.md) before calling this release ready for public distribution.

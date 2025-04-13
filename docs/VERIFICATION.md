# Verification record

Date: 2026-11-01 (Europe/Minsk). Current verification uses no Computer Use. Real applications were not uninstalled, real leftovers were not quarantined, and no update was downloaded or installed. No remote Git operation or publication occurred. No native UAC request was executed during verification.

## Rust workspace audit (2026-11-01)

The reusable update implementation now lives in the sibling `rust-auto-updator` core. This repository retains product configuration, its existing sleeping worker, explicit user download/restart actions and bounded durable helper-error reporting. The adapter accepts GitHub SHA-256 digests when no checksum asset is present, and asks the core to validate the GUI PE subsystem and target architecture. New startup handles the core cleanup protocol only after native host initialization. Tests do not download or install updates.

The application previously set only a window icon and lacked an embedded EXE icon. `build.rs` now embeds the same original mark and version metadata at 16/20/24/32/48/64/256px. The host sets both ICON_SMALL and ICON_BIG/taskbar through winit. This application has no tray, so there is no tray resource or interaction to certify. `python scripts/verify-icons.py EXE OUTPUT.png` inspects the final EXE as data and renders Win32 resource icons offscreen without launching it.

The optimized static-CRT Windows x64 build passed (`cargo build --release --offline --locked --target x86_64-pc-windows-msvc`). Native LoadImageW/DrawIconEx proof from that exact EXE passed at all seven embedded sizes on light/dark backgrounds; the saved PNG was inspected. Artwork remained visible, with transparent corners and no clipping. SHA-256: `356b1dd67aac2841578338fb1750c672d26db57c3d4df48f771d7e9ac4045a40`. Resource proof does not establish OS window/taskbar integration.

Checks: all-target Clippy with warnings denied passed; 31 fixture tests passed (21 library, 10 integration). Redraw regressions verify consumed deadlines stay idle, hidden dirty work waits for visibility, identical scenes skip rasterization, and worker progress/exit is consumed without constructing a hidden frame. The Windows dependency tree contains `egui_software_backend` and `softbuffer` without eframe/glow/glutin/wgpu; the pinned softbuffer Win32 presenter uses CreateDIBSection and BitBlt. Existing CPU/GDI scheduling remains event driven; no rendering-backend rewrite was necessary.

This audit does not launch AppUninstaller, following this repository's code-only instruction. Fresh visible/hidden CPU/frame measurements, taskbar/Alt-Tab integration and perceived input responsiveness remain unverified; the older bounded evidence below is historical. No Computer Use or real uninstallation was performed. Source checkout/CI now requires the sibling updater workspace; it has not been published or fetched in this task. Existing real main/settings screenshots remain in README.

## Prior automated checks

- `./scripts/check.ps1 -Offline` passed: formatting, Clippy with warnings denied, and **35 tests** (25 library and 10 integration tests). The main binary test harness ran zero tests and did not open the GUI.
- Tests cover safe command/MSI planning, search, ownership boundaries, changed/new files and folders, quarantine/restore preservation and overwrite refusal, settings persistence/corruption, release/checksum selection, simulated updater rollback, redraw deadlines/cache invalidation, and draining background progress/exit events without constructing a UI frame.
- Removal regressions cover two-read confirmation, process exit without removal, registry uncertainty/reappearance, detached-launcher timing, observation expiry, the ten-second hold and finite collapse, stale error/wait text, preserving the grace row across Refresh and preventing resurrection after expiry.
- Administrator retry regressions cover typed OS errors 740/1223 versus access denied/missing files, changed approvals, stale/unoffered retry commands, canceled requests, offer withdrawal on removal/stop and UI state reset after launch. A Windows test opens a read-only handle to its own test process, checks that nonblocking observation reports it still running, then closes the handle. No child process is launched by that test.
- File-effect tests use inert data under `target/test-fixtures`. No test enumerates real uninstall registrations, launches an uninstaller or executes an update helper. UI state tests may read the installed Windows UI font; they perform no external application operation.
- `./scripts/package.ps1 -Offline` passed with the repository-local target and two Cargo jobs. Toolchain: rustc/cargo 1.96.1, Windows x64 MSVC.
- `git diff --check` passed. CI is prepared but has not run on GitHub.

## Actual backend and package

Artifact: `dist/AppUninstaller-v0.1.2.exe`, **9,681,408 bytes**.

SHA-256: `84c3b22907ce608c8dd5c94231fade9e371cbe531e25cf5b03186426d3b3025a`. Matches `dist/SHA256SUMS.txt`, the read-only [PE inspection](evidence/package-inspection.json), and the running-process diagnostic report. Administrator retry source changes are recorded in local commit `d51f630`.

The active code path is egui → `egui_software_backend` CPU rasterization → retained bitmap → softbuffer's Win32 GDI presenter. The Windows dependency graph has no eframe/glow/glutin/wgpu renderer. The packaged PE has no OpenGL/D3D/DXGI/Vulkan imports and no dynamic MSVC runtime import. The sampled running instance had no loaded graphics API modules matching OpenGL/D3D/DXGI/Vulkan. These module/import checks support the source-level backend evidence; they do not measure Windows compositor GPU activity.

## Bounded runtime gate

`./scripts/verify-rendering.ps1` passed for PID **29988**, an optimized instance of the exact packaged EXE. Workload: **840 deterministic in-memory registrations**, virtualized list, selected details, installed Windows UI font and one live removal countdown/collapse. No worker, registry reads, normal settings or network access were started. The disposable test window is nonactivating and pointer-transparent so this is specifically a no-input test, not an interaction test.

The diagnostic waits for five consecutive quiet samples before moving from visible to hidden to restored, with a 36-second internal and approximately 40-second external limit. It exits normally. The report recorded **0 input events**.

| State | Clean interval | CPU, one logical core | UI / raster / present delta |
| --- | --- | --- | --- |
| Visible, settled | 5 consecutive one-second samples | 0.00–0.00% | 0 / 0 / 0 |
| Hidden | 5 consecutive one-second samples | 0.00–0.00% | 0 / 0 / 0 |
| Restored, settled | 5 consecutive one-second samples | 0.00–1.56% | 0 / 0 / 0 |

Machine-normalized CPU is included separately in [the raw report](evidence/rendering.json). Zero CPU samples are limited by process CPU-timer resolution; zero frame counters are exact recorded deltas. The maximum observed draw callback was **7.790 ms**, including startup and the finite transition; this is not a pointer-latency percentile.

The run recorded 84 UI passes, 78 rasterizations and 79 presentations in total, including warm-up, the remaining countdown, collapse and restore. All transition work finished before the five-second visible quiet interval. Restore explicitly presented the retained bitmap (1 cache presentation) and then returned to zero recurring work. Background worker results are drained independently of layout while hidden, preventing bounded progress queues or a requested updater exit from blocking on window painting. A focused unit regression covers that dispatch without running the updater.

An earlier noisy interval was rejected because unrelated input arrived; another fixed-time attempt contained late layout work. Those runs were not treated as settled evidence. The final verifier uses readiness (five consecutive quiet samples), retains all warm-up/transition samples, and fails if a phase never settles before its deadline.

## Real visual evidence without desktop automation

`./scripts/capture.ps1` ran the packaged EXE for Applications, Settings, Leftover review and six removal states: missing uninstaller, running, confirmed removal, half-collapsed row, administrator permission needed and UAC canceled. Each invocation rendered the actual `Desktop::update`, shared visual tokens, fonts and production CPU rasterizer offscreen, saved a PNG plus JSON metadata, and exited successfully. Layout stabilized within the finite 12-frame/five-second budget; the script applied a 20-second external timeout. Images were inspected as actual PNGs and embedded in README. Static temporal states use a frozen fixture clock; the separate native runtime test exercises the live transition. These captures do not execute or depict the Windows UAC dialog itself.

Captures show client areas at 1120×760, 100% scale, with deterministic non-sensitive sample data and all effectful actions disabled. An additional 900×620 administrator-state capture was inspected; its retry action stays visible and details remain scrollable. Screenshots do not certify native title bars, keyboard traversal or mouse response.

Visual changes: a coherent teal/neutral palette, installed Windows UI typography with bundled-font fallback, aligned spacing, a simple list/selected-details composition, visible primary action, readable path wrapping and settings grouped by user decisions. Technical removal explanations are disclosed on demand. This is a sustained review workspace; a tray-only design would hide the inventory and ownership information needed for the task.

Removal-state review confirmed that a failed launch no longer leaves a misleading publisher-wait message, an active row shows observation status instead of another launch action, and confirmed removal shows strikethrough, a readable countdown and a finite neighbor-moving collapse. The removed detail pane does not show a misleading zero-second countdown during collapse.

Administrator-state review confirmed a concise permission-needed or canceled status, an explicit retry beside the reviewed command, and no raw 740 error paragraph. A footer shortcut returns to the pending request only when its details are not already selected. AppUninstaller is not restarted or elevated by this feature.

## Remaining limits

Hands-on responsiveness after this renderer change still needs user confirmation. No Computer Use was used for the final verification. Keyboard navigation, native menus/chrome, OS DPI transitions, actual minimization, live icon-enumeration startup cost, actual UAC approval/cancellation/policy errors, elevated publisher process handles, real publisher removal/registration probing, ACL/stream preservation and a published-release update/restart remain unverified. The state model and real UI transition were tested with fixtures; no destructive end-to-end removal was performed. GPU utilization/wait telemetry was not sampled; Windows composition is outside the application's CPU-rasterization claim.

The original unattended implementation used 16 fixture tests and compile-only checks. A later user-authorized GUI opening exposed lag; the final CPU/GDI and isolated visual/runtime evidence above supersede those initial rendering limitations. The app remains an unpublished local first version, not a claim of production readiness. See [manual validation](MANUAL_TESTS.md).

# Verification record

Date: 2026-10-31 (Europe/Minsk). Current verification uses no Computer Use. Real applications were not uninstalled, real leftovers were not quarantined, and no update was downloaded or installed. No remote Git operation or publication occurred.

## Automated checks

- `./scripts/check.ps1 -Offline` passed: formatting, Clippy with warnings denied, and **20 tests** (10 library and 10 integration tests). The main binary test harness ran zero tests and did not open the GUI.
- Tests cover safe command/MSI planning, search, ownership boundaries, changed/new files and folders, quarantine/restore preservation and overwrite refusal, settings persistence/corruption, release/checksum selection, simulated updater rollback, redraw deadlines/cache invalidation, and draining background progress/exit events without constructing a UI frame.
- File-effect tests use inert data under `target/test-fixtures`. No test enumerates real uninstall registrations, launches an uninstaller or executes an update helper. The new UI state test may read the installed Windows UI font; it performs no external application operation.
- `./scripts/package.ps1 -Offline` passed with the repository-local target and two Cargo jobs. Toolchain: rustc/cargo 1.96.1, Windows x64 MSVC.
- `git diff --check` passed. CI is prepared but has not run on GitHub.

## Actual backend and package

Artifact: `dist/AppUninstaller-v0.1.0.exe`, **9,609,728 bytes**.

SHA-256: `70c14bc61c989806137ef1bd9367edc27031a528393f4046bf5be73167a50197`. Matches `dist/SHA256SUMS.txt`, the read-only [PE inspection](evidence/package-inspection.json), and the running-process diagnostic report.

The active code path is egui → `egui_software_backend` CPU rasterization → retained bitmap → softbuffer's Win32 GDI presenter. The Windows dependency graph has no eframe/glow/glutin/wgpu renderer. The packaged PE has no OpenGL/D3D/DXGI/Vulkan imports and no dynamic MSVC runtime import. The sampled running instance had no loaded graphics API modules matching OpenGL/D3D/DXGI/Vulkan. These module/import checks support the source-level backend evidence; they do not measure Windows compositor GPU activity.

## Bounded runtime gate

`./scripts/verify-rendering.ps1` passed for PID **47596**, an optimized instance of the exact packaged EXE. Workload: **840 deterministic in-memory registrations**, virtualized list, selected details, installed Windows UI font. No worker, registry reads, normal settings or network access were started. The disposable test window is nonactivating and pointer-transparent so this is specifically a no-input test, not an interaction test.

The diagnostic waits for five consecutive quiet samples before moving from visible to hidden to restored, with a 36-second internal and approximately 40-second external limit. It exits normally. The report recorded **0 input events**.

| State | Clean interval | CPU, one logical core | UI / raster / present delta |
| --- | --- | --- | --- |
| Visible, settled | 5 consecutive one-second samples | 0.00–1.56% | 0 / 0 / 0 |
| Hidden | 5 consecutive one-second samples | 0.00–0.00% | 0 / 0 / 0 |
| Restored, settled | 5 consecutive one-second samples | 0.00–0.00% | 0 / 0 / 0 |

Machine-normalized CPU is included separately in [the raw report](evidence/rendering.json). Zero CPU samples are limited by process CPU-timer resolution; zero frame counters are exact recorded deltas. The maximum observed draw callback was **10.386 ms**, including startup; this is not a pointer-latency percentile.

Startup required 18 rasterizations. Restore explicitly presented the retained bitmap (1 cache presentation) and then returned to zero recurring work. Background worker results are drained independently of layout while hidden, preventing bounded progress queues or a requested updater exit from blocking on window painting. A focused unit regression covers that dispatch without running the updater.

An earlier noisy interval was rejected because unrelated input arrived; another fixed-time attempt contained late layout work. Those runs were not treated as settled evidence. The final verifier uses readiness (five consecutive quiet samples), retains all warm-up/transition samples, and fails if a phase never settles before its deadline.

## Real visual evidence without desktop automation

`./scripts/capture.ps1` ran the packaged EXE for Applications, Settings and Leftover review. Each invocation rendered the actual `Desktop::update`, shared visual tokens, fonts and production CPU rasterizer offscreen, saved a PNG plus JSON metadata, and exited successfully. Layout stabilized within the finite 12-frame/five-second budget; the script applied a 20-second external timeout. Images were inspected as actual PNGs and embedded in README.

Captures show client areas at 1120×760, 100% scale, with deterministic non-sensitive sample data and all effectful actions disabled. An additional 900×620 capture was inspected during iteration; the primary review action stays visible and details remain scrollable. Screenshots do not certify native title bars, keyboard traversal or mouse response.

Visual changes: a coherent teal/neutral palette, installed Windows UI typography with bundled-font fallback, aligned spacing, a simple list/selected-details composition, visible primary action, readable path wrapping and settings grouped by user decisions. Technical removal explanations are disclosed on demand. This is a sustained review workspace; a tray-only design would hide the inventory and ownership information needed for the task.

## Remaining limits

Hands-on responsiveness after this renderer change still needs user confirmation. No Computer Use was used for the final verification. Keyboard navigation, native menus/chrome, OS DPI transitions, actual minimization, live icon-enumeration startup cost, administrator-only uninstallers, real publisher removal, ACL/stream preservation and a published-release update/restart remain unverified. GPU utilization/wait telemetry was not sampled; Windows composition is outside the application's CPU-rasterization claim.

The original unattended implementation used 16 fixture tests and compile-only checks. A later user-authorized GUI opening exposed lag; the final CPU/GDI and isolated visual/runtime evidence above supersede those initial rendering limitations. The app remains an unpublished local first version, not a claim of production readiness. See [manual validation](MANUAL_TESTS.md).

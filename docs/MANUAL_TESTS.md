# Deferred Windows validation

The CPU/GDI runtime gate and application-generated client-area captures are now recorded in VERIFICATION.md. The remaining hands-on checks below require a later authorized interactive session. Use a disposable Windows VM and deliberately installed disposable programs for destructive tests.

## Desktop and inventory

- Start the exact packaged EXE; record its hash, Windows build and graphics driver. Check launch errors, resizing, keyboard navigation, 100/150/200% DPI, long/Unicode metadata, empty inventory, missing icons and large virtualized lists.
- Compare current-user and all-users registrations across 32/64-bit views against Windows Installed apps; verify mirrored entries, inspection-only components and truthful read-error warnings. Confirm that no registry writes occur.
- Search publishers/names and sort size/name/publisher. Check that metadata unknowns are clear and rows/detail panes do not clip.
- Measure idle and interaction CPU/frame latency separately in a release build. Verify worker completion wakes the window, minimizing/restoring does not spin or freeze, and closing exits without a tray process. Do not infer these from compilation alone.

## Uninstall and leftover review

- Test a disposable per-user EXE uninstaller and a disposable MSI product. Review executable/arguments, cancel the first confirmation, then explicitly run it. Confirm no shell or silent mode is introduced.
- Exercise denied access, missing executables, malformed/ambiguous commands, cancellation/nonzero exit, a detached child uninstaller and Windows restart-required codes. No action should automatically elevate or restart Windows.
- Install a disposable exact-name folder directly under a permitted root; verify ownership evidence. Finish its uninstaller and review unchanged fixture leftovers. Keep the registration present and confirm review is blocked.
- Check that vendor-nested/custom/documents/shared/reparse locations, another registration's folder and uncertain ownership produce no suggestion. Test a junction only within disposable data; no user documents should be traversed.
- Change a leftover file or add an empty folder after capture/preview. Quarantine must refuse. Verify selection defaults to unchecked and the preview explains every proposed path.
- Quarantine a disposable folder; verify bytes, ACLs, streams and journal preservation. Restart the app and restore. An occupied original path or modified quarantine content must block restoration.
- Deny the sibling rename or journal write and confirm original data stays intact. In a disposable VM, interrupt after journal creation/rename and manually verify recovery from the journal. Do not test against real applications/data.

## Updates

- Before any publication, confirm that missing repository/releases show unavailable and that failures never claim “up to date.” Test network timeout/rate limit and incompatible assets.
- Opt out, close/relaunch, and use a network trace to verify zero startup update request. Manual checking must remain available. Corrupt settings must disable startup checking for that session and show an error.
- Using authorized test releases only, test numeric versions (0.9 versus 0.10), stable-channel filtering, wrong/missing checksum, truncated/oversized download, writable/read-only EXE folders and clear progress/errors.
- Explicitly download then restart/install. Confirm parent exit, old EXE backup, rollback on locked destination/launch failure, application relaunch, persistent preferences/journals and visible diagnostic reports. The current tests simulate only the file replacement engine and never run this helper against the installed app.
- Confirm downloaded target architecture and inspect dependency DLL imports for unintended runtime requirements. Verify both asset naming and `SHA256SUMS.txt` after final packaging. Authenticode signing is currently absent.

## Documentation before publishing

- README already includes real app-generated **Applications**, **Leftover review** and **Settings** client-area PNGs with clearly labeled fixtures. Refresh them after visual changes; native window chrome and actual keyboard/mouse interactions remain separate checks.
- Update the verification record and draft release notes with actual outcomes. Review all limitations and third-party notices. Publish only with separate user authorization through the connected GitHub plugin.

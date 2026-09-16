# Native focus edit audit — 2026-09-15

Build: `/Users/lordcasser/workspace/projects/goal/.artifacts/desktop/Planner.app` (`dev.lordcasser.planner`), SHA256 `b572daea61c458cdd9854eff9a3d79b94d960d9795c8a10841a5865e65b5601f`. Tested through native macOS CUA. No source changes and no LLM calls were made.

## Completed focus-block deletion

`Drag audit focus` was an earlier self-owned 25m fixture. Its Workspace options menu exposed `Delete`. The product confirmation dialog explicitly showed `Delete “Drag audit focus”` and stated that the focus block and recorded focus time would be removed. After clicking `Delete focus block`, the card disappeared and the Workspace total changed from `2h 25m planned` to `2h planned`. Other focus blocks and the user entries `hello` and `Test` were not deleted.

## Schedule editor

The current day was 2026-09-15. The live value read from the existing `Test` block before editing was `03:45–04:45`, 60 minutes.

- Opened the new `Edit schedule for Test` popover. AX exposed `Start time for Test`, `Duration in minutes for Test`, `Ends at`, `Cancel`, `Save schedule for Test`, and `Move Test to Unscheduled`.
- Temporarily changed the draft to `09:45` and `45` minutes, then clicked Cancel. Schedule immediately remained `Test · 03:45–04:45`, so the draft did not persist. This native Cancel path was directly observed.
- Reopened the editor, set start time to `09:15` and duration to `45`, then clicked Save. Schedule showed `Test · 09:15–10:00`.
- Reopened the popover. AX showed `Start time for Test, Value: 09:15`, `Duration in minutes for Test, Value: 45`, and `Ends at 10:00`.
- Switched to Workspace while the saved value was active. The Focus blocks section showed `Test 45m planned`, confirming the duration crossed views.
- Restored the original user block values to `03:45` and `60` and saved. Schedule returned to `Test · 03:45–04:45`; the final open popover showed start `03:45`, duration `60`, and end `04:45`.

The saved `09:15`/`45` popover screenshot and the final restored popover screenshot were emitted in the CUA session. CUA exposes screenshots for display but has no local PNG-save operation, so no new PNG path was produced.

During the Workspace read, the unrelated `Drag audit goal` appeared completed and struck through; it was not changed by this audit. The final `Test` value was read back as its original `03:45`/60m after restoration.

The app remains open on Calendar → Schedule with the restored Test editor popover visible.

# macOS release GUI smoke — 2026-09-15

## Bundle and method

- Bundle exercised: `/Users/lordcasser/workspace/projects/goal/.artifacts/desktop/release-review/Planner.app`
- Visible app identity: `Planner` (`dev.lordcasser.planner`)
- Surface: native macOS window through CUA; no shell or restricted SecurityAgent UI was used.
- No API key field was opened or copied. No LLM request was made.

## Preflight of the old window

The old Planner window was initially on Calendar → Schedule for 2026-09-15 with the existing `Test` block shown as `03:45–04:45`. Its time editor was open, but the visible values were the persisted `03:45` start and `60` minute duration, with no AX dirty indicator. `Later` was off; Coach and Issues were collapsed; no running focus timer or AI activity was visible. The editor was closed with Cancel without changing the record.

The first Cmd+Q did not remove the running app. The native Planner menu was then opened and `Quit Planner` was selected; the app disappeared from the running-app inventory without a confirmation or unsaved-changes dialog.

## Fresh bundle smoke

- Launch from the exact bundle path succeeded.
- No visible Keychain, SecurityAgent, signing, or permission prompt appeared. Restricted security UI was not inspected.
- Initial Calendar view loaded the existing September 2026 data and remained responsive.
- Workspace tab opened successfully. The visible workspace contained the existing cycles, long-term goals, weekly plan, daily plan, and focus blocks. It showed `0m focused`; no timer was started.
- Calendar tab returned successfully and retained the existing 2026-09-15 plan.
- Settings opened and closed successfully. The default General page showed the existing gray theme and Monday week start. The AI settings page was not opened, so no credential material was exposed.
- Final state: the new bundle remains open on Calendar with no modal or editor open.

## Evidence

CUA `getScreenshot()` emitted visible screenshots for the fresh Calendar launch, Workspace, and Settings General page during the run. The available CUA API provided in-session image emission but no local screenshot-file export path; no screenshot bytes were written outside the conversation. The AX snapshots above are the durable textual evidence for this smoke.

No plans, tasks, goals, schedules, settings, or credentials were created, deleted, or changed.


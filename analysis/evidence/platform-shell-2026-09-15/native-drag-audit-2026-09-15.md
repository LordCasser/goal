# Native drag audit — 2026-09-15

Build: `/Users/lordcasser/workspace/projects/goal/.artifacts/desktop/Planner.app` (`dev.lordcasser.planner`), tested through native macOS CUA. No source changes and no LLM calls were made.

## Test fixtures

Existing user entries `hello` and `Test` were left untouched. Temporary audit data created for this run:

- cycle `Long-term` (2026-09-15–2026-10-13)
- long-term goal `Drag audit goal`
- weekly task `Drag audit weekly`
- daily tasks `Drag audit A` and `Drag audit B` on 2026-09-15

The cycle, goals, and audit fixtures were retained at the end of this pass because the user was also inspecting this dataset. Original entries `hello` and `Test` were not modified.

## Results

### Calendar daily-task reorder handle

The initial A/B attempts used the coordinate near x=898, which later inspection identified as the color/goal slot, not the six-dot reorder grip. Those attempts are invalid as reorder evidence and are excluded from the result. The actual grip is left of the completion checkbox, around x=846 in the 1228×775 Calendar screenshot.

The 2026-09-15 calendar day-card handle (small dotted handle at the card's upper right) was also dragged a few pixels within the same card to avoid changing the existing day assignment. The selected day and task list remained unchanged; no drag preview, insertion marker, or other state change appeared.

### Workspace cross-column affiliation

Dragging the temporary weekly task `Drag audit weekly` from its reorder handle toward the temporary long-term goal row left it in the weekly column. AX still exposed `Link Drag audit weekly to long-term goal`; work mix remained `0 goal-linked, 1 standalone weekly`. No insertion marker, drag ghost, or affiliation change appeared.

### Initial focus-block observation

At the initial baseline the Focus blocks section was empty: `0m focused · 0m planned`, with only `Add focus block` / `Add your first block`. A timeline handle could not be tested until a self-owned block was created.

## Evidence / environment

CUA emitted native screenshots for the Calendar card before and after the handle attempt and for the Workspace empty Focus blocks state in the session transcript. The available CUA API exposes screenshots for display but no local screenshot-save operation, so there are no additional PNG paths. The app remained open after testing.

## Supplemental drag checks

- Calendar day-plan relocation: dragged the visible 2026-09-15 day-card handle toward the known-empty 2026-09-16 card. The selected date stayed 2026-09-15 and all four tasks stayed on that day; no relocation occurred, so there was no state to restore.
- Focus-block timeline start: the initial attempt used the left circular Start affordance, which is a timer control rather than a scheduling handle; it is not treated as valid timeline-drag evidence. The final package's supported scheduling surface was verified separately through Calendar → Schedule below.

These supplemental tests were performed before the final package regression; the temporary audit objects remain in place for inspection.

## Repaired-build regression notes

The repaired build was opened from `/Users/lordcasser/workspace/projects/goal/.artifacts/desktop/Planner.app`.

- Task A → B reorder: the attempted CUA `drag([898,260],[898,300])` hit the color/goal slot, not the six-dot reorder grip; no reorder conclusion is drawn. The true grip is around x=846, left of the completion checkbox.
- Weekly → long-term goal: the attempted CUA `drag([821,225],[300,225])` also used the row's color/goal area; no affiliation conclusion is drawn from that coordinate.
- 2026-09-15 day-card → empty 2026-09-16: CUA `drag([220,376],[300,400])` left the day plan on 2026-09-15.
- Earlier repaired-build Schedule observation: Calendar → Schedule exposed `Day timeline 2026-09-15`, an `Unscheduled` 25m `Drag audit focus`, and visible 00:00–11:00 rows. The CUA call `drag([950,188],[1000,640])` later appeared as `01:15–01:40`, despite targeting the visible 09:00 row, indicating a coordinate-to-time mapping defect or scaling mismatch in that run. Final-package verification below used the popover editor and records the saved 09:00 result.

The first post-rebuild launch also reloaded all test data, confirming persistence across restart.

## Coordinate correction

The repaired-build A/B and weekly affiliation coordinates are explicitly marked invalid above: they hit color/goal slots rather than the six-dot grips. They must not be reported as drag failures.

## Final package verification

Final package tested: `/Users/lordcasser/workspace/projects/goal/.artifacts/desktop/Planner.app`, SHA256 `e0a41d55420168200ac115e7b5ec5f2e10674336792e0d6b14b58716c6b1bf36`.

- Valid A/B handle attempt used the six-dot grip at approximately `(846,260)` and dragged to `(846,300)` with `currentPlanner.drag([846,260],[846,300])`. After the UI settled, the daily order remained A before B; there was no insertion line or drag ghost. This is the first valid mouse-grip failure observation. Keyboard fallback worked: with A focused, `pressKey("Down")` persisted B before A. The two results are recorded separately.
- Workspace exposes the handle as a dedicated `Reorder …` button, separate from the completion checkbox, task text entry, and color/goal control. The color/goal slot is not a reorder handle.
- Calendar → Schedule exposes the scheduled `Drag audit focus` block and an ellipsis popover. The popover's hour and minute steppers were edited to `09` and `00`, then Save persisted `09:00–09:25`. Reopening the popover still showed Value `09:00`; the popover fit within the native window and was not clipped. One CUA move attempt from the visible 09:00 block toward 10:00 (`drag([1000,618],[1000,665])`) did not change the final text and was not repeated.
- The Workspace focus section showed `Drag audit focus 25m planned`. The schedule editor also exposed `Move to Unscheduled`; no additional move was performed after the saved 09:00 state.
- Creating `Drag audit color goal` produced a distinct goal with AX `Goal color: red`; the existing `Drag audit goal` remained `Goal color: gold`. This confirms automatic color assignment and separation from the reorder control.
- `Drag audit weekly` currently exposes `Change parent goal for Drag audit weekly` with Help `Linked to Drag audit goal`. This is the observed current dataset state; this pass does not attribute that link to an earlier invalid coordinate attempt.
- CUA emitted the final Schedule/popover screenshot in the session transcript. The CUA API exposed screenshots for display but no local PNG-save operation, so no new PNG path was generated. The app remained open.

## Final root verification

Root inspected the final popover screenshot and AX state: `09:00`, duration `25m`, controls fully visible. Clicking the popover’s `Move to Unscheduled` button removed the time slot and displayed `Drag audit focus` with `25m` in the staging area. The final app state is therefore **Unscheduled**, intentionally verifying slot removal without deleting the block. Native physical-mouse drag acceptance remains pending; synthetic DOM tests and keyboard sorting are separate evidence.

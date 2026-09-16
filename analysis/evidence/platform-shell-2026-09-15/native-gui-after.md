# Planner production native macOS GUI verification — 2026-09-15

Target: `/Users/lordcasser/workspace/projects/goal/.artifacts/desktop/Planner.app`, production identifier `dev.lordcasser.planner`. Tests used `cua_repl` only. No task, goal, schedule, provider, API key, or LLM request was created or changed.

## Launch and empty-state baseline

- `cua.getApp` resolved and launched the production app separately from the older `Planner UX` (`dev.lordcasser.planner.uxaudit`). AX reported `Window: "Planner", App: Planner`, HTML surface `tauri://localhost`.
- Workspace initially showed `The workspace could not be loaded.` with `Retry`. One retry reproduced the error.
- Calendar remained reachable and loaded an empty first-use state for `2026-09-15`: no day plan, no weekly goals, and no long-term goals. The `Create day plan` action was left untouched.
- Coach and Issues were exposed as disabled (`Create a plan to use Coach` / `Create a plan to review issues`). Clicking either produced no AX change and no panel opened.

## Native shell and panels

- Normal window screenshot showed actual macOS red/yellow/green traffic lights. AX exposed `close button`, `full screen button`, and `minimize button`.
- Two drags over blank top-bar coordinates produced no observable window movement. Attempts to drag the right/bottom edge for a smaller viewport were also ineffective; one far-corner attempt returned Cua `windowNotFoundAtPosition`, so an exact `960×600` resize could not be established through this environment.
- Settings opened as a centered modal with a dimmed background and a close button. Clicking the dimmed top navigation area closed the modal; no underlying state change was observed. While Settings was open, `⌘⇧L` produced no AX change and did not open Later.
- On the main surface, `⌘⇧L` successfully opened Later (`Value: on`) with a `New parked goal` field and `Nothing parked yet`; invoking the same shortcut restored `Later` to `off` without entering data.
- The native green full-screen control worked: entering full screen removed title-bar/traffic-light AX elements; `⌃⌘F` exited and restored them. Moving/clicking at the top edge while full screen did not expose temporary native controls in the captured state. `Later` stayed at the top-left with no overlap or obstruction.

## Settings and IPC errors

- Settings > AI initially displayed `get_ai_settings not allowed. Command not found`; no provider was connected, and model selection/context controls were disabled. Later provider loading settled to `还没有连接供应商`.
- Settings > General initially exposed white/gray theme controls. Clicking gray produced `set_theme not allowed. Command not found`; the visual selection stayed `白底` on / `灰底` off. Clicking white again left the original white theme unchanged. Thus both-theme switching and restoration could not be completed because the command was unavailable.
- The same General page displayed `get_settings not allowed. Command not found`.
- After the `set_theme` attempt, the Cua close action failed with `Planner.app … procNotFound: no eligible process with specified descriptor`; `cua.getState()` confirmed the production Planner process was no longer running. The app therefore could not be left open after this verification.

## Screenshot evidence

`getAXStateAndScreenshot` emitted concrete screenshots for the initial empty Calendar, Settings modal/AI error, normal native shell, full screen, and General/theme error states in the Cua transcript. The Cua API exposes screenshot bytes but no local file-save method, so this record intentionally does not claim nonexistent screenshot paths.


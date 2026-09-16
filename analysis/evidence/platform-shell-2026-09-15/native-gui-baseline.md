# Planner native macOS GUI baseline — 2026-09-15

Source: `cua_repl` native macOS Computer Use. No repository code or user plans were changed.

## Environment

- Running app resolved by bundle ID `dev.lordcasser.planner.uxaudit`, display name `Planner UX`.
- Window title observed by AX: `Planner`; HTML surface URL: `tauri://localhost`.
- Cua capabilities available: native app resolution, accessibility tree (`getAXState`), accessibility screenshots (`getScreenshot` / `getAXStateAndScreenshot`), click, keyboard, scroll, drag, text selection/value entry, and secondary accessibility actions.
- Browser inventory: Codex In-app Browser present with no tabs.
- Cua screenshot calls return displayable image bytes but expose no file-save API. The baseline, Coach, zoom, and fullscreen screenshots were emitted in the Cua tool transcript; no screenshot file path is claimed here.

## Initial state

- Main view: `Calendar` selected; density `Week` selected; date range `2026-09-14 – 2026-09-20`.
- `Later` toggle: `off`; AX help: `Do Later (⌘⇧L)`.
- `Coach`: collapsed, with `Expand` secondary action and `Open Coach panel` help.
- `Issues`: collapsed.
- `Settings`: button visible and reachable.
- Day detail: `2026-09-17`, `Plan` selected; existing weekly/long-term goal rows were visible. No task, goal, or schedule mutation was made.

## Settings reachability

Clicking `Settings` opened a modal container titled `设置` with a `Close dialog` button. Categories were reachable: `通用 外观与规划偏好`, `AI 模型 供应商与连接`, `提醒 时间与免打扰`, and `诊断 日志与应用信息`.

The default diagnostics page showed log level `info`, an `打开日志目录` button, and schema version `10`. The General page showed gray background enabled, white background disabled, week start `周一`, and an auto-save note. The AI page showed an already configured `DeepSeek BYOK / deepseek-flash` provider/model, marked connected/tested, with `API Key 已配置`; no connection test, key edit/removal, model change, or AI request was performed.

## Coach reachability

Clicking `Coach` changed AX state from `collapsed` to `expanded` and opened a `COACH` panel. It exposed context retention `15 m`, `Close Coach panel`, `Start planning`, a `Message Coach` entry area, and a disabled `Send message` button. No planning action or message was triggered. The panel was closed and the initial collapsed state restored.

## Window round trips

1. Native full-screen button AX element `full screen button` exposes secondary action `zoom the window`.
2. Invoking `zoom the window` changed the window to the screen-filling zoomed geometry; invoking it again restored the prior window geometry.
3. Clicking the native full-screen button removed the title bar/traffic-light controls while retaining the Planner surface and menu bar. `⌃⌘F` exited native full screen and restored the title bar, traffic-light controls, and normal AX tree.
4. After both round trips, Calendar/Week, Later off, Coach collapsed, and Settings remained available; no plan data changed.


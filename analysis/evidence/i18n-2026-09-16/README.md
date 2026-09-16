# Goal i18n native macOS GUI verification — 2026-09-16

Target: `/Users/lordcasser/workspace/projects/goal/.artifacts/desktop/i18n-review/Goal.app`.

This run used native CUA only. No source code, tasks, goals, schedules, providers, API keys, or Coach messages were changed. The CUA screenshot API emits images in the tool transcript but has no local screenshot-file export; the representative Chinese and English screenshots are therefore the inline CUA images immediately preceding this record, and no nonexistent PNG paths are claimed here.

## Steps and observations

1. Launched the exact `Goal.app` path. Native AX reported `Window: "Goal", App: Goal`, `tauri://localhost`, and the macOS menu bar showed `Goal`. The app inventory reported `displayName: Goal`, bundle identifier `dev.lordcasser.planner`, running.
2. Initial Workspace was Simplified Chinese. Existing task names such as `Drag audit goal`, `Drag audit weekly`, `hello`, `Test`, and focus blocks were visible before and after all checks.
3. Opened Settings → General. The Chinese settings screenshot showed the language selector as `简体中文`, long labels, and a complete modal layout.
4. Selected `English`. The Settings view changed immediately to English, then the main Workspace and Calendar changed immediately to English. Calendar retained the existing linked goals and task entries. The English Calendar screenshot showed the month grid, `Drag audit weekly`, and `Drag audit goal`.
5. Opened Coach on the English Calendar. AX and screenshot showed the right-side panel with `Talk about the current plan...`, `Start planning`, `Message Coach`, `Share what you have in mind…`, and the 15-minute context label without clipping or horizontal overflow.
6. Opened Settings → AI model. AX and screenshot showed English long labels (`Plan with AI entry`, `Coach context retention`, `Manage providers and models`) fitting the modal. No provider was connected, so no BYOK request was sent and no key was read.
7. Closed the window with the native close control and relaunched the same `Goal.app`. Calendar reopened in English, proving the language preference persisted across restart. Existing goals/tasks remained visible.
8. Reopened Settings → General, selected `简体中文`, closed the language menu and Settings dialog. Final UI was Simplified Chinese; the existing Calendar data remained intact.

## Result

Pass for immediate `zh-CN`/English switching, locale persistence across restart, Window/app identity `Goal`, Workspace/Calendar/Coach/AI settings layout, and preservation of existing visible task data. BYOK was not exercised because the installed app had no connected provider. No blocking system security prompt appeared.

## Follow-up scope

This initial pass did not catch wrapping in narrow Chinese month cells or the untranslated Later label. The user reported both after this pass. The follow-up build replaces duplicate block counts with one compact progress indicator and standardizes Chinese feature names. Final narrow-card and process-restart verification is tracked separately; the initial pass is not evidence of exhaustive layout coverage.

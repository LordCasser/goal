# Calendar GUI regression — 2026-09-16

## Bundle and language persistence

- Old review bundle was exited before the final bundle check. The old process PID `45949` disappeared.
- Final bundle: `/Users/lordcasser/workspace/projects/goal/.artifacts/desktop/i18n-final/Goal.app`.
- Final process: PID `55784`, verified from its executable path; no other `Goal.app/Contents/MacOS/goal` process remained.
- After reopening the final bundle, the UI remained in 简体中文. Settings was switched to English for a visual pass and then restored to 简体中文.
- Month cards were checked in both languages. English showed compact `0/2 tasks` and `0/1`; Chinese showed compact `0/2 个任务` and `0/1` progress. The card header did not repeat a count and the narrow cards did not wrap.

## Isolated test data

The two temporary sessions were created through the Goal UI. They have no `repeat_id`, are unfinished, and retain their original 25-minute duration:

| title | session id | day id | date |
| --- | --- | --- | --- |
| `GUI drag roundtrip temp` | `568c5a07-f77f-4c12-8636-3f11177828ec` | `ce4ed797-e725-4077-85f7-ee8f932fbf0b` | 2026-10-22 |
| `GUI day drag roundtrip` | `1cd3dc8c-1e56-41f9-a012-2a3e4d9bec1f` | `7f22496a-f618-4177-85f7-ee8f932fbf0b` | 2026-09-24 |

The Sep 24 day was selected because it had no generated repeat instance; no existing repeated block was moved.

## Drag attempts

- In the Oct 22 Schedule view, the temporary unscheduled card was dragged from the staging list toward the 09:00 timeline position. The UI stayed in the unscheduled list and the read-only SQLite row remained unscheduled.
- In the September Month view, the Sep 24 day grip was dragged toward Sep 25 twice (once to the target grip and once into the target cell). The UI did not move the day and no extra session/day row appeared.
- These direct CUA HTML5 drag injections did not expose a successful drop result, so this run cannot claim a successful GUI day roundtrip. The existing frontend drag tests and the Rust repeat regression remain the deterministic coverage for the lifecycle and duplicate prevention.

The two temporary sessions are pending deletion through the Goal UI confirmation flow. No source files were changed and no existing repeated session was edited.

## Cleanup and repeat-template check

- Both temporary sessions were deleted through the Goal GUI confirmation flow. A read-only SQLite check now returns no rows for either temporary title.
- Both temporary sessions had `repeat_id` NULL; neither was a repeating block.
- The only active repeating template was `Drag audit focus` (`a8aef654-3bb4-4b87-aaa1-4faa0fa7907a`). Per the follow-up validation request, its existing instance menu was used to choose “停止重复”; the template is now archived (`archived = 1`) and existing instances were retained.
- A new day plan was created through the GUI for 2026-09-26 after stopping the template. Workspace showed `已专注 0 分钟 · 计划 0 分钟` and the empty-state action `添加第一个专注块`; no focus block was generated.
- Sep 21–24 plans were not edited. Sep 24 was used only for the temporary block, which was deleted; no existing task, session, or repeat instance on those dates was moved or removed.

## v0.1.1-review final GUI language pass

- The previous Goal process was exited from the Goal application menu (`Quit Goal`); the process list was empty before launching the new bundle.
- Final bundle launched: `/Users/lordcasser/workspace/projects/goal/.artifacts/desktop/v0.1.1-review/Goal.app`.
- New process: PID `69810`, verified from its executable path; no older Goal process remained.
- Chinese workspace navigation was visible after launch: `稍后`, `助理`, `计划问题`, `设置`. Existing focus block entries showed one title layer (`Drag audit focus`) followed by a separate duration line (`计划 25 分钟`); no duplicated title string appeared inside an entry.
- Chinese AI settings were checked in Settings → AI 模型: `当前使用`, `计划区 AI 规划入口`, `助理上下文保留时间`, `管理供应商与模型` and related labels were present.
- Language was switched to English and checked: `Later`, `Coach`, `Planning Issues`, `Settings`, `AI models`, `Current model`, `Plan with AI entry`, and `Coach context retention` rendered correctly. The app was then switched back to 简体中文 and left there.
- No plans, sessions, tasks, or settings other than the requested language toggle were created or edited in this pass. Existing repeat instances remained unchanged.

## v0.1.1-final Select GUI pass

- The review bundle was targeted explicitly at `/Users/lordcasser/workspace/projects/goal/.artifacts/desktop/v0.1.1-review/Goal.app` and exited through the native Goal menu (`Quit Goal`). The final bundle was then targeted explicitly at `/Users/lordcasser/workspace/projects/goal/.artifacts/desktop/v0.1.1-final/Goal.app`; the returned UI was the final Chinese build (`稍后`, `助理`, `计划问题`).
- The window was resized through the native lower-right edge to approximately 1000×625 (the screenshot surface was 1003×625). Settings remained readable at that size. Screenshots were inspected inline through CUA; this run did not produce a saved PNG path.
- Settings language switched from `简体中文` to `English` and back to `简体中文`; the workspace navigation changed with the selection and remained Chinese after restoration.
- The bottom `周起始日` trigger opened an upward-facing menu. All seven day labels were readable and the menu stayed inside the window. The first Escape closed the list and returned focus to the trigger; the second Escape closed Settings.
- The AI model selector showed the empty state `选择供应商 / 模型` because no saved provider/model exists, so no activation request was sent. The unsaved provider form API selector exposed `Anthropic Messages`, `OpenAI Chat Completions`, and `OpenAI Responses`; Anthropic was selected briefly and then the draft was restored to OpenAI Chat Completions. The form was never saved.
- Settings → 提醒 showed `通知权限：由系统管理`. No task, session, provider, model, reminder, or persisted preference was created or changed by this pass; the final app was left in 简体中文.

## Published v0.1.1 smoke

The public macOS ARM64 DMG was downloaded and verified against SHA256SUMS, the pinned signing requirement and metadata. The extracted `.artifacts/desktop/v0.1.1-release/Goal.app` was launched (PID 12201), replacing debug PID 8164. The Chinese workspace and language menu opened normally; no unexpected system dialog appeared. Settings was closed and the published app left running. No plan/provider data was changed and no LLM call was sent.

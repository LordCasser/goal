# Coach input and approval repair — 2026-09-16

## Scope and implementation

The working tree already contained the global Coach migration and other ongoing work. This change preserves that implementation and addresses the Coach presentation, completion/read race, approval coordination, and task-import verification.

- `src/ui/ai.tsx` contains controlled, typed adaptations of Halaska's PromptInputPattern, MessageThreadPattern, ApprovalCardPattern and ScrollArea. The upstream source is https://github.com/Halaska-Studio/ui/blob/main/halaska-kit/halaska-kit-v1.0.jsx. The existing MIT notice remains in `public/licenses/halaska-ui.txt`. No npm dependency, demo timer, global stylesheet injection or remote font was added.
- The composer grows to 160 px, uses a thin themed scrollbar, and keeps its send button below the text. Enter sends; Shift+Enter and IME composition remain input operations. Users can prepare another draft while a request runs; completing the previous request does not erase a changed draft.
- The transcript and pending approvals share one scroll treatment. Approval cards no longer nest their own scrolling containers inside the footer scroll area. Cards show their real busy state instead of relying on a remount.
- `completeAgentTurn` publishes the completed IPC result and cancels older conversation reads before they can reintroduce `active_turn_id`. Background query refresh is independent of the completed turn's mutation status.
- Proposal events invalidate discovery of affected cycles as well as the per-cycle details. Task/action decisions share a mutation key, which also blocks new planning sends during confirmation.
- Persisted task-decision receipts now include the actual destination cycle. The frontend renders the structured destination even when the user has browsed to another plan.

## Verification

- 92 frontend tests passed across the Agent, Proposals, App, query-event, and localization suites. Added coverage verifies first-display approve/reject without reopening, late reads after turn completion, pending-cycle discovery, destination receipts, and preservation of a new draft.
- 18 Rust turn tests and 12 action tests passed. The batch-import regression uses the real ToolRegistry, preview service, decision service, SQLite database and editor projection, with a scripted FakeProvider supplying three `create_goal` calls across two target cycles. Approval commits all three; rejection leaves none; the unrelated focused cycle remains unchanged.
- The date fixture was subsequently corrected to use the exclusive end `2026-09-17` for the user's inclusive `09/14–09/16` range. The coordinating global-Coach task separately reran both corrected fixtures: 2 passed. Final shared-workspace integration verification reported 483 frontend tests passed across 62 files and 572 Rust tests passed, 0 failed, 11 ignored. The macOS debug app was built, signed with the existing identity, launched, and checked in the native GUI at `.artifacts/desktop/global-coach/Goal.app`. The check covered drafts surviving day/week, workspace/calendar, and close/reopen transitions; it did not call an external model or add user plan data.
- `npm run build` and `git diff --check` passed. The existing large-bundle advisory remains.
- Chromium checked the actual AgentPanel at a 424 px panel width and 600/800 px viewport heights, both existing themes, multiline input, and first-load approval/rejection. A 14-line draft measured 160 px viewport / 322 px scroll content with `scrollbar-width: thin`; the send button stayed visible and no horizontal page overflow occurred. Confirm/reject removed both pending rows and produced transcript receipts.

`fixture.html` is an isolated UI harness with fake IPC and no access to user data. It is served by Vite at `/analysis/evidence/ai-panel-2026-09-16/fixture.html` and does not ship in the app entry bundle. `long-input.png` shows the composer; `receipt.png` shows post-decision feedback. Browser fixtures verify UI behavior; only the Rust tests verify real database writes. These checks do not claim to verify a live provider's interpretation of the text.

## Separate follow-up boundaries

The current tool contract needs an existing cycle ID to stage a task. Creating a missing date/range container and then importing its tasks remains a dependent workflow; this change does not add a new import entity or an automatic model-resumption protocol. Testing that workflow against a configured model, and deciding whether to support one atomic container-plus-tasks proposal, should be handled separately from this UI repair.

The turn executor also retains produced tool messages in memory until a turn succeeds. Persisting useful partial history on provider failure is separate backend work; no second transcript store or recovery policy was introduced here.

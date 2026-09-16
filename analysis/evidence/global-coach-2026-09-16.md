# Global Coach — 2026-09-16

OpenSpec change: `make-coach-global`.

## Implementation

- Migration 14 replaces cycle-owned conversations with the existing table's single `coach` row. It preserves message IDs, payloads and turn membership, orders turns deterministically and renumbers messages. The cycle foreign key is removed, so deleting a plan cannot delete the conversation. Existing idle TTL behavior is retained.
- Coach stays mounted in the application shell, including while hidden. Workspace/calendar selection updates only the next send's transient page context; history, drafts, scroll position and in-flight replies do not belong to a selected cycle.
- Each send captures the active plan, view, selected long-term/week/day IDs and dates. Rust resolves these against storage, represents absent plans as null, and adds hidden `<page_state>` context. Browsing does not generate user messages or model calls. Tools keep their original turn's scope.
- Pending approvals are visible globally and retain their original targets. Synchronous receipts claim and finish in their caller's transaction. Asynchronous decisions acquire an owner-token guard before any action effect, preserving the global turn exclusion through the receipt; failure cleanup cannot clear another owner's turn.

## Verification

- Full frontend suite: 62 files / 483 tests passed. TypeScript check passed.
- Browser GUI used the real AgentPanel and PanelMotion with isolated in-memory IPC fixtures, no external LLM or user database writes. Verified draft/history across day A/B, original action target after changing selection, a delayed reply surviving navigation and panel close/reopen, the next message receiving the new page context, and old-message scroll position surviving navigation and panel close/reopen. Hidden panel was inert with zero width.
- Backend tests cover real persistence/migration, independent-connection singleton initialization, global in-flight exclusion, turn context/history across days, missing plans, original approval targets, receipt owner guards and existing TTL/English skill/language behavior. Full suite: 572 passed, 0 failed, 11 ignored. After the parallel AI-panel task corrected two import-test date fixtures, both affected cases passed again. Independent connection initialization exposed a real WAL read-snapshot upgrade conflict; short standalone writer transactions now use BEGIN IMMEDIATE, including task-preview approval.

- OpenSpec strict validation: 33 passed, 0 failed; this change has 4/4 tasks checked.
- Native macOS debug build and stable-identity signing passed. Running app: `.artifacts/desktop/global-coach/Goal.app`. Native GUI verified a draft across day/week selection, workspace/calendar and panel close/reopen. The temporary draft was cleared; no test plans or external model calls were added to the user database.
- The separate AI-panel task changed shared receipt/UI code during verification. Its final changes were retained, then the full frontend suite and native build were rerun against the integrated tree.
- Temporary fixture/server and project Rust build cache were removed after verification; the signed app remains available.

## Deferred findings

- Existing interrupted-process recovery: a future process crash during a claimed turn needs a dedicated startup recovery policy for abandoned `active_turn_id` and interrupted actions. Migration 14 clears old process claims once; normal startup recovery is a separate lifecycle change.
- Calendar week presentation still has Monday-based layout assumptions. This change makes its hidden Coach page context respect `week_start_day`; broader calendar layout consistency remains separate work.
- These findings do not introduce new entities or expand this change into unrelated lifecycle/calendar work.

Later source-type behavior and its separate verification are recorded in [later-plan-type-2026-09-16.md](later-plan-type-2026-09-16.md).

# Later plan type — 2026-09-16

Implemented `preserve-later-plan-type` (5/5 tasks).

- Migration 13 stores nullable `tasks.later_plan_type`, constrained to Later rows and month/week/day. Existing NULL rows remain intact and use long-term semantics; no source type is inferred from titles or links. New quick notes explicitly use month.
- Moving into Later records the source cycle type for the same-type subtree. Independently parked levels remain separate roots. Promoting clears metadata and validates the root's external parent against the destination.
- Week/day default promotion resolves the current local week (respecting week_start_day) or today, atomically creating missing cycles with the move. Long-term items retain the existing cycle picker.
- Rows display bilingual type captions and destination-specific actions. Pending requests lock the row; errors stay inline; success refreshes the affected views without a toast.

## Verification

- Full Rust suite: 560 passed, 0 failed, 11 ignored (external-provider/helper/doc tests); includes 12 Later integration cases and the migration-12-to-13 upgrade test. Rollback was exercised by aborting the move after destination-cycle creation.
- Full frontend suite: 62 files / 475 tests passed. After updating the Later explanatory copy, the 4 affected suites were rerun: 32 passed.
- TypeScript and native debug app build passed. OpenSpec strict validation: 32 passed, 0 failed. Independent backend review reported no remaining verified findings.
- Browser GUI using the production LaterPanel and an isolated in-memory IPC fixture: verified all three type labels in Chinese/English; week/day/long-term promotion actions; failed promotion retains the row with a full-width inline error; success removes the row without a toast. This GUI fixture verifies frontend interactions; real persistence/default-cycle/transaction behavior is covered by Rust integration tests.
- Native Goal opened successfully with the existing database and Later panel. The local app uses the existing stable Goal signing identity. No test tasks were added to the user's database.

The previous source type cannot be recovered for pre-migration Later rows because it was not stored. Such rows continue to behave as long-term goals.

# Planning controls review — 2026-09-16

## Scope

OpenSpec changes: `add-provider-request-headers` and `refine-planning-cycle-controls`.
This is a local review build; it does not replace the published v0.1.1 release.

## Review findings addressed

- Deletion previews previously missed task descendants linked from other cycles. The same recursive impact now supplies previews, confirmation tokens, reminder cleanup and query invalidation.
- A changed deletion scope fails closed and reloads the preview. It is never retried automatically. Coach retains its own single approval flow.
- Coach cycle approvals also compare the impact token, so content changes with unchanged counts require another review.
- Focus blocks have cycle ownership, not a task foreign key. Deleting a task does not infer associations from dates or names; independent blocks remain intact.
- New progress-check data is read in repository projections and the review carry query. A full Rust regression exposed the latter projection omission; it is fixed.
- Wheel routing no longer treats an unselected overflowing pane or programmatically focused task textarea as a deliberate request to scroll vertically. Tab intent is consumed once. Headers beside a scrolling region do not swallow wheel events.
- Request headers are names-only in ordinary configuration and runtime-only credentials in sampling. All three protocols share merging, validation and redaction; redirects do not forward credentials.
- Empty provider navigation no longer stretches to the form height. Add is in the management heading; populated navigation follows its content and moves above the form at narrow widths.

## Automated verification

- Frontend final full suite: 56 files, 400 tests passed, including the UI UX Pro Max keyboard and Header-focus regressions.
- Rust full suite: 521 passed, 8 ignored. The ignored tests are existing opt-in/live or documentation checks.
- Additional database constraint regression: `custom_cycles` 6/6 passed, including malformed JSON, invalid dates, out-of-range checks and conflicting schedule fields.
- AI action regression: 11/11 passed, including same-count deletion impact changes. Locale regression: 4/4 passed.
- UI UX Pro Max review is recorded in `ui-ux-pro-max-review-2026-09-16.md`; final TypeScript, Vite and macOS native build passed after the compact layout and keyboard refinements.
- TypeScript, Vite and the native macOS debug bundle build passed.
- Both OpenSpec changes validate with `--strict`.
- `git diff --check` passed.

Native GUI evidence is recorded separately in `planning-controls-2026-09-16.md` and `provider-request-headers-2026-09-16.md`. No Windows or Linux desktop was available for this GUI pass; wheel semantics are covered by DOM event tests and macOS native review, not claimed as Windows hardware verification.

## Follow-up boundaries

Progress checks are persisted planning metadata and a bounded date preview. They do not create tasks or notification jobs. Frontend date helpers currently contain overlapping ISO parsing functions; consolidating them is a separate cleanup, not part of this change. The existing historical-cycle deletion guards remain in effect.

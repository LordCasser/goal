# Provider request headers GUI review — 2026-09-16

- Bundle: `/Users/lordcasser/workspace/projects/goal/.artifacts/desktop/headers-review/Goal.app`
- Final process: PID 77816, executable path under `headers-review/Goal.app/Contents/MacOS/goal`
- Locale restored: 简体中文 (zh-CN)
- Final provider state: fixture provider deleted; no active provider/model selected
- Plan data: no plan data created or changed by this review

## Checks

The fixture provider `Goal header audit` used the local mock at `http://127.0.0.1:59246`, model `header-audit`, and header `x-goal-audit`.

- Chinese UI: add row focus, show/hide masking, case-insensitive duplicate rejection, reserved `Host` rejection, add model, test/save, reopen masked value, blank keep, replacement, invalid-value failure retaining the draft, remove header, and provider deletion all passed.
- English UI at a narrow window (~1003×633 after resize): translated header form remained visible without clipping; saved state showed the masked value and `Saved · leave blank to keep`; fixture was deleted afterward.
- Runtime Coach request returned `ok` from the mock with the header present.
- No real API key or paid request was used.

## Mock evidence

Raw log: `.artifacts/audit/provider-headers-request-log.jsonl`.

Observed variants include successful `first`/keep, successful `replacement`, rejected `invalid`, successful `absent` after removal, and successful `chat` with one header. The invalid probe returned `ok: false`; all other listed probes returned `ok: true`.

Screenshots were inspected inline through CUA. No PNG screenshot was persisted, so there is no screenshot file path to attach.

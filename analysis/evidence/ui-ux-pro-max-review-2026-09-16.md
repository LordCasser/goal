# UI UX Pro Max review — 2026-09-16

## Source and scope

Requested skill: `/Users/lordcasser/.codex/skills/ui-ux-pro-max/SKILL.md`.

Reviewed the current custom-cycle form, date-adjacent progress-check entry, provider navigation and request headers, and workspace wheel routing against Goal's existing `docs/design.md` and semantic tokens. This is a local review of the existing OpenSpec changes, not a new visual system or release.

Ran the skill's required design-system query (`desktop productivity planner minimal`, project `Goal`), UX query (`progressive disclosure form controls keyboard focus`), React query (`keyboard focus popover forms`), and Tailwind query (`responsive forms labels focus reduced motion`). The generated landing-page structure and replacement palette/font are not appropriate to Goal's established desktop workspace. Relevant guidance is applied below.

## Design decisions

- **Progressive disclosure:** show the actual check cadence next to the cycle date; open a small anchored popover for the rule, count and bounded date preview. This removes the permanent full-width disclosure bar and keeps the task area dominant.
- **Form hierarchy (revised after visual feedback):** keep dates above an inline Repeat/Once radio choice with no full-width track. Render cadence as “Every + short number input + content-width unit menu”, giving the number and unit their own focus indication. The once date uses a compact labeled row. Show only the active mode's fields and preserve each draft. Summarize total days and check count together; wrap a bounded number of dates beneath them.
- **Provider navigation:** use a content-height list, with Add beside the management heading. The empty state has no empty navigation column. Header rows keep explicit labels, masked values, local validation, and wrapping controls.
- **Interaction:** clickable local controls receive pointer and stable color feedback. Radio groups require a single Tab stop with arrow-key selection. Popovers retain Escape/outside dismissal and return focus to the trigger. Existing modal focus handling and reduced-motion tokens remain authoritative.
- **Scrolling:** preserve the intentionally horizontal workspace. Mouse-wheel mapping follows explicit pane-entry intent; it does not replace native horizontal gestures, input/menu interactions, or zoom.

## Contrast and layout review

Measured token pairs using WCAG relative luminance: white-theme hint on white **4.86:1**; gray-theme hint on subtle surface **4.53:1**; secondary text on selected surface **5.06:1**; focus blue on white **6.18:1**. Neither theme requires a palette change for these controls.

The desktop shell's supported minimum is 960×600. Dialogs use bounded, scrollable content and fixed action areas; the custom editor can stack and Header rows wrap. Goal intentionally has two light themes rather than a dark theme. Mobile landing-page layout checks from the generic skill do not define this desktop product's acceptance criteria.

## Repairs and follow-up

- Both cycle radio groups now use one Tab stop and arrow-key selection with focus following the selected option.
- Removing a Header row focuses the next row, previous row, or Add control. Empty Header values disable the visibility toggle; saved credentials are never presented as readable values.
- Local clickable controls use pointer cursors and 150 ms color transitions without changing geometry.

Existing architecture debt, kept separate from this visual refinement: selecting another provider unmounts its form and silently discards unsaved drafts. A follow-up should define form ownership and a consistent leave-with-unsaved-changes policy across provider selection and settings closure, then test that policy. It is not resolved by this pass.

## Verification

- Final frontend suite: **56 files, 400 tests passed**.
- TypeScript, Vite and the macOS native debug bundle passed. The copied review app uses the existing stable Goal signing identity.
- Both affected OpenSpec changes pass strict validation; `git diff --check` passes.
- Removed approximately 6.5 GiB of Rust build cache and the superseded local review bundles; retained `.artifacts/desktop/uiux-review/Goal.app`.
- Root-agent screenshot review confirms the progress entry sits beside the date and does not add a full-width bar. The GUI pass is recorded separately in `ui-ux-pro-max-gui-2026-09-16.md`.
- Real Windows/Linux mouse-wheel behavior remains a separate hardware acceptance item; DOM routing tests are not described as that acceptance.

## Follow-up visual refinement

The user rejected the stretched segmented track and the full-width interval group. Replaced both with the inline radio/cadence arrangement above. Checked the actual DurationDialog in an isolated Vite preview in Chinese/gray and English/white, including Repeat and Once. Both fit naturally without stretching the interval controls; direction-key logic, errors and drafts remain unchanged. Existing DurationDialog regressions: 10/10 passed. The preview used no planner data or IPC writes and was removed after review.

## Explicit wheel boundaries

Further user clarification makes mouse click the sole activation signal. Removed Tab activation and the fallback that changed an empty pane back to horizontal. Scrollport markers now sit on the actual content regions (including the focus-block list), not the wrapper/header. Primary clicks activate a subtle inset boundary; outside clicks, app blur and hiding the workspace clear it. Vertical-dominant gestures discard minor horizontal drift, and the active vertical region contains overscroll at its ends.

Verified the production `useWorkspaceWheelRouting` hook in a temporary browser fixture using real CUA clicks and scroll actions, then read rendered DOM scroll positions:

- Inactive pane: workspace `scrollLeft` changed from 0 to 150; both pane `scrollTop` values stayed 0.
- Clicked short/empty pane: the next downward scroll left workspace at 150 and pane at 0, with its activation marker present.
- Clicked long pane: pane `scrollTop` changed to 150 while workspace stayed at 150.
- Clicked the long pane's header: activation cleared; another downward scroll changed workspace from 150 to 225 while pane stayed at 150.
- At the long pane's bottom (`scrollTop = maxTop = 1193`), another downward scroll left both pane and workspace (225) unchanged.

This is an actual browser-input verification with the production hook, not Windows hardware verification. The temporary fixture and server were removed afterward. No user plan data was modified.

Final follow-up validation: PlannerWorkspace, DurationDialog and CycleColumn regressions passed (**3 files, 30 tests**). TypeScript, Vite and the macOS native debug bundle passed again. The latest stable-identity-signed local bundle is `.artifacts/desktop/uiux-refined/Goal.app`, which replaced the running review app. Native WKWebView inspection confirmed the compact custom-cadence controls and the active content-region inset boundary, with the title outside that boundary. The dialog was canceled without creating a cycle. Removed another **1.3 GiB** of regenerated Rust build cache after retaining the verified bundle; the earlier full-suite results above belong to the preceding review pass.

## Follow-up visual refinement — preceding icon-and-text version — 2026-09-16

The user requested a narrower scroll affordance after the review: remove the large inset blue body outline and reserve an inline header slot for a non-interactive up/down SVG plus localized secondary text (`上下滚动` / `Vertical scroll`). The slot remains reserved to prevent layout shift; the hint is visible only while that exact body has `data-wheel-active="true"`. It uses existing gray secondary text and semantic tokens, with no background, pill, or new row. Narrow time navigation becomes icon-only with an accessible label and tooltip.

Implemented with one shared hint component and CSS scoped to each direct child scrollport. Primary-click activation, title/outside reset, empty/boundary retention, keyboard-focus behavior, and wheel routing remain unchanged. PlannerWorkspace, CycleColumn and i18n regressions passed (3 files, 25 tests); TypeScript, Vite, native macOS bundling and stable-signature verification passed. Native WKWebView screenshots confirm the long-term body no longer has an inset outline, clicking blank content reveals the header hint without moving the title/menu/body, and the subsequent title interaction hides it. The running local bundle is `.artifacts/desktop/scroll-hint/Goal.app`. Removed 1.3 GiB of regenerated Rust cache and the superseded bundle after verification. Windows physical-mouse acceptance remains pending.

This section records the preceding icon-and-text implementation, including the native screenshot that showed `↕ 上下滚动`. That version was superseded after a native visibility defect allowed an earlier arrow to persist when activation moved or the workspace lost focus. It is retained as historical evidence and does not describe the final icon-only state.

## Final icon-only validation — 2026-09-16

The final affordance renders only the existing 14px neutral, non-interactive up/down icon while the exact body is active. The localized `上下滚动` / `Vertical scroll` label remains sr-only for assistive technology, and the native title tooltip explains the wheel behavior. The wrapper reserves 14px; the hook toggles the matching hint's `hidden` state in the same activation transition, so exactly one current hint can be shown. Previous, outside, header, window-blur and disabled-workspace resets hide all hints. No opacity animation or visible hint text is used. Narrow time navigation remains arrow-only with its full accessible name and tooltip.

Latest validation from the root agent: native screenshots showed zero arrows initially; clicking long-term showed only its arrow; clicking weekly hid the old arrow and showed only weekly's arrow; clicking the weekly heading hid all arrows. PlannerWorkspace visibility regressions passed (**19 tests**); TypeScript, Vite, native build and stable-signing verification passed. The latest running bundle is `.artifacts/desktop/scroll-state/Goal.app`. Windows physical-mouse acceptance remains pending.


## Chronological current-week navigation and bounded cards — 2026-09-16

Applied UI UX Pro Max guidance to preserve existing Goal typography, neutral surfaces, compact controls and stable geometry. The current week now appears once in chronological order as `W38 · 本周` / `W38 · This week`. The same button is sticky at either edge (`top: 0; bottom: 0`), with its normal flow space retained. A zero-height anchor preserves the true chronological location for centering after clicking the current week. Selection opens the existing cycle; it does not create a new one. Lists are capped at 320px and shrink with fewer weeks. A start-date subline disambiguates week numbers when years differ. Query refreshes do not reset manual scrolling.

Plan cards have a 360px minimum (clamped by available space), grow naturally and stop at the workspace height. The task list owns its existing query and renders its work-mix summary outside the middle scrollport in panel mode; the calendar keeps its flow layout. The header and summary stay visible. The day focus-block panel uses the same bounded height and an independent scrollport. Scrollports remain direct siblings of their corresponding headers, preserving click-only wheel activation and the existing hidden-arrow behavior.

Verification used a temporary browser fixture containing the actual production components and wheel hook, with in-memory query data and no planner IPC writes:

- 70 chronological weeks: current row centered, then scrolled to top and bottom sticky states via real clicks/wheel input. The scroll height stayed **3781px**, button count stayed **70**, and the chronological anchor stayed **2430px** throughout. Clicking the bottom-stuck current row returned its center to **255.75px**, against the list center of **256px**.
- Cards with 2, 8 and 32 tasks measured **360px, 566px and 612px** respectively; 612px was the available workspace height. At 32 tasks the middle scrollport was **410px** with **1228px** of content. Scrolling to 818px did not move the summary (top **586px**, bottom **675px**).
- Reducing available height capped the card at **412px**, with the summary still visible and a **210px** middle scrollport. English daily summary wrapping remained within the card.
- With 20 focus blocks, the day card remained **412px** and the independent focus scrollport was **323px** with **1508px** of content. Window-level vertical overflow did not appear.
- WeekNavigation and PlannerWorkspace: **27 tests passed**. TaskList, CycleColumn and CalendarContent: **35 tests passed**. TypeScript/Vite build, native macOS bundle and stable-identity signature verification passed. Strict OpenSpec validation and `git diff --check` passed.
- Native WKWebView accessibility and screenshot checks confirmed the single combined current-week entry and compact cards with the fixed summary. Updated running review bundle: `.artifacts/desktop/week-cards/Goal.app`.
- Closed and removed the temporary fixture/server, removed the superseded scroll-state bundle, and cleaned **4.0 GiB** of Rust cache. No user plan data was changed for validation. Windows/Linux hardware acceptance remains separate and pending.


## Linked day/week navigation — 2026-09-16

Continued `refine-planning-cycle-controls` with day navigation and a two-way browsing contract before implementation. Existing saved week boundaries remain authoritative; new weeks already take the configured first weekday. Seven date rows are generated per existing week without creating planner records. Each row remains 44px, and the viewport caps at 308px. Today uses the same in-flow dual-edge sticky row as the weekly navigator. An explicit week selection scrolls to that week's start; daily content prefers today or the earliest existing day in the week. The user can inspect missing dates before explicitly creating a plan.

Explicit navigation requests are separate from scroll-driven selection. A programmatic scroll cannot feed back into week selection; a manual gesture can interrupt it. Manual browsing switches only after a 140ms pause and after the viewport midpoint crosses the current week's boundary plus a 14px dead band. The existing 180ms card fade remains, with native smooth scrolling for explicit positioning and instant positioning under reduced motion. Empty daily content reserves the same width as a populated day and focus area, avoiding horizontal scroll clamping when crossing an empty date. Diagnostic request IDs are handled once, so query refreshes do not replay navigation.

Validation:

- Production PlannerWorkspace in a temporary in-memory fixture with 18 Sunday-based weeks: initial current-week start at scrollTop **2772px**; user scrolling to **3096px** selected the following week and September 23 without snapping back. Clicking W42 reached its Sunday October 18 anchor at **4312px**, with no reverse selection of intermediate weeks.
- Clicking Today returned the date and current week with Today centered in the 308px list. Scrolling back to **2340px** selected the preceding week and kept Today stuck at the bottom. The same row also stuck at the top when browsing later dates.
- Selecting an empty date preserved horizontal scrollLeft **1152px** and scrollWidth **2432px**, showed one Create daily plan action and no error. No date cycle was created by browsing.
- Complete frontend suite: **58 files / 426 tests passed**. The final default-day selection adjustment was rechecked with **3 files / 41 navigation tests passed**. TypeScript/Vite, native macOS bundling, stable-signature verification and strict OpenSpec validation passed. Independent code review found no actionable defects in the linked navigation.
- Native WKWebView: selecting W39 changed the weekly plan; selecting Today returned to W38 and September 16. Real native list scrolling moved to W37, showed the September 12 empty-date view and left Today stuck at the bottom; clicking the stuck Today restored W38 and September 16. Screenshot review confirmed stable widths and the compact date rows. These macOS observations do not replace pending Windows/Linux hardware acceptance.
- Final local review app: `.artifacts/desktop/linked-navigation/Goal.app`. Temporary preview files/server and the previous week-cards bundle were removed after verification; regenerated Rust cache was cleaned again.

Pending follow-up (not implemented in this change): the user also requested associating a focus block with a daily plan and aggregating focus time without adding visual clutter. Current sessions already belong to a day cycle and accrue elapsed time into its parent chain. A clarification is pending whether the intended new association is to an individual daily task. Architecture inventory identified the add-session service, cycle persistence mapping, editor task query, task movement and recursive deletion impact as affected paths. The user's earlier recursive-deletion requirement must be retained if task-linked sessions are introduced; do not silently default to preserving linked blocks with `ON DELETE SET NULL`.


## Damped date drums and current-row pinning — 2026-09-16

This supersedes the preceding native-scroll navigation and pending task-association notes. Applied UI UX Pro Max motion/reduced-motion guidance while retaining Goal’s existing typography, neutral tokens and compact controls. A shared DateDrum renders a bounded chronological index window with critical damping, whole-row snapping and a 500ms input-quiet plus motion-settled commit. Hovered drums own wheel input; task content keeps explicit click activation. Programmatic week/day alignment does not create user commits, and a later navigation intent cancels a pending earlier gesture. Browsing empty dates remains read-only.

The current week/today is a single keyed row: it follows its natural logical position, then clamps to the top or bottom row when that position leaves the visible range. No duplicate footer or inserted spacer changes the geometry. Clicking it immediately opens the corresponding plan and smoothly brings that same row to the center. Saved week starts that share a canonical week slot remain independently reachable after week-start setting changes.

Validation:

- Full frontend suite: **60 files / 437 tests passed**. After TypeScript fixture fixes, targeted DateDrum, WeekNavigation, DayNavigation and i18n coverage: **4 files / 23 tests passed**. TypeScript and Vite production compilation passed as part of the macOS bundle build.
- Isolated browser fixture used actual PlannerWorkspace components and cached in-memory data. Top/bottom current-row pinning and return-to-center were verified with real wheel, keyboard and click input. Navigation height remained **267.986px**; workspace scrollLeft remained **537.222px** through drum scrolling. No real planner writes were made.
- Native macOS WKWebView: browsing to June 8 pinned W38 at the bottom; clicking opened W38 and aligned the day to September 14. Browsing to March 1, 2027 pinned W38 at the top; clicking returned to W38. Selecting Today opened September 16. Screenshots confirmed the edge row and stable compact geometry; empty dates offered explicit create actions.
- Optional focus-task association is implemented separately in `add-focus-task-association`. Browser and native forms defaulted to no association and listed only real tasks for the selected day. The native form was canceled without writing a focus block. Task time is derived without a permanent new counter in the UI.
- Rust full suite: **532 passed / 7 ignored**; subsequent migration-upgrade regression plus focus association suite: **10 passed**. Review covered deletion impact/cascades, ancestor time accounting, task/day movement, repeat behavior and Coach confirmation. Historical pre-existing aggregate repair is documented separately.
- macOS bundle and stable signing verification passed. Latest running local review bundle: `.artifacts/desktop/drum-navigation/Goal.app`. Windows/Linux physical input acceptance remains pending; browser and macOS checks do not claim that coverage.


## Smooth drum frames and plan transitions — 2026-09-16

Applied UI UX Pro Max transform/opacity, reduced-motion and React refs guidance to the reported stutter. DateDrum now keeps continuous positions in its frame loop and writes transforms/opacity directly to bounded row refs. React updates only when the center crosses a row; the window reuses formatted labels and stable keyed nodes. Removed continuous text scaling to avoid repeated text resampling. The accessible current-date marker derives from the current index rather than cached label data. The 500ms quiet + settled contract and top/bottom current-row pinning remain unchanged.

Week/day panels reuse the existing per-cycle task and per-day session query caches to load the target before replacing the old content. A scoped PlanTransition preserves the old card during a 90ms exit and pending query, makes it inert, then introduces the new content with a 160ms ease-out from opacity .72 and 4px offset. It suppresses the nested full-opacity-zero card entry effect. Unchanged panels stay interactive; same-identity query refreshes do not replay animation. New navigation cancels stale replacements. Reduced motion swaps immediately when data is ready. Card height remains content-driven within the existing viewport limit; no duplicate task trees are mounted. The relation batch query remains unchanged.

Validation:

- React Profiler regression: 60 simulated animation frames produce at most 2 React commits and only 1 newly formatted date label for a one-row gesture, while more than 10 distinct transforms are painted. The same current row node is retained.
- Actual browser input using production PlannerWorkspace with isolated in-memory data: synchronized leaving/leaving → entering/entering → idle/idle; minimum opacity .72; no absent panel content; both drum heights fixed at 268px, workspace scrollLeft unchanged at 0. Three ~1.8-second local samples recorded 214/215/213 frame intervals, with maximum gaps 25/26/33ms and 0/1/1 intervals exceeding 25ms. These samples are local observations, not a universal FPS guarantee.
- Native macOS WKWebView: selecting W39 opens its week and September 21 empty-day view; subsequent wheel input selects August 31 without moving the workspace horizontally. Current-week pinning and compact natural card height remain visible. Final signed app was rebuilt after the accessible-current-marker fix and launched successfully.
- Full frontend regression: **61 files / 449 tests passed**, including data-readiness synchronization, stale-query rejection, StrictMode, reduced-motion, timer cleanup, current marker and prior wheel routing tests. TypeScript, Vite, macOS native bundle and pinned-identity verification passed. No Rust business logic changed in this pass.
- Latest local review app: `.artifacts/desktop/smooth-navigation/Goal.app`. Temporary fixture/server and superseded drum-navigation bundle were removed. Generated Rust cache was cleaned after the bundle was retained. Windows/Linux physical input acceptance remains pending.

## Follow-up: calm plan switching

The previous 90ms exit / 160ms entry moved the whole card from -3px to +4px at replacement while it was still 72% opaque; different natural card heights also snapped. The revised transition keeps the card surface opaque and its top edge stationary. Only its contents fade out over 160ms and fade in over 260ms with gentle acceleration/deceleration; differing natural heights interpolate during entry within the existing viewport cap. No scale or positional movement is applied to the card.

Loading is a separate waiting phase: existing content stays readable and inert until both target task/focus queries are ready. Interruptions read the currently displayed opacity and height before cancelling animations. Returning to a still-displayed plan also restores its natural height smoothly. Only one live task tree is present. Both columns use the document animation timeline. Reduced-motion changes during either exit or entry cancel motion and timers immediately.

Real production PlanTransition + CycleColumn components were exercised with an in-memory QueryClient fixture, using the browser UI (no user data writes). Recorded frames showed stationary x/y, surface opacity always 1, monotonic height interpolation from 360px to the available height and back, and waiting-phase content opacity 1. The fixture covered short/long lists, delayed data, interruption, and returning to the displayed plan. Native macOS build verification follows; Windows physical mouse acceptance remains pending.

Final verification: 61 frontend suites / 452 tests passed; TypeScript and the native macOS bundle passed. The new stable-signed Goal.app was launched and week/day selection plus wheel-settled cross-week navigation were exercised in WKWebView. The reduced-motion entering-phase timer finding from review was fixed and covered by a regression. OpenSpec 4.5/4.6 are complete; Windows physical mouse acceptance is still pending.

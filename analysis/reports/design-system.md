# hyperfocus 0.15.0 — Design System & Interaction-Visual-Language Report

> 2026-09-14 官网/UI 复核：本表是应用静态产物的观测值，不是重做的强制设计 token。官网与应用的字体声明、间距尺度和画布区域有差异；官网深色段落不证明应用有深色主题，窄屏截图不证明移动交互。连线及 SVG 方形端点同色，深色描边属于行前色槽。详见 [33-website-ux-ui-review.md](33-website-ux-ui-review.md)。用户确认的点击持续查看关联记录为 UXR-033；低对比度、禁用、完成和关系淡化需要分别验收。

Evidence base (all local, read-only):

| File | Size | Role |
|---|---|---|
| `evidence/web/_assets_index-Bk3EgsqR.css` | 64,576 B | Main stylesheet: Tailwind preflight + utilities (~74 %) + custom component layer (~26 %, starts at byte 48,000 with `:root{--app-header-height:40px}`) |
| `evidence/web/_assets_DismissibleHint-B0TgJuQ-.css` | 12,629 B | Task-list rows, drag handles, task-link slots/colors, preview rows |
| `evidence/web/_assets_AgentConversation-ca0ZS18p.css` | 1,586 B | Agent option buttons + voice-bar keyframes |
| `evidence/web/_assets_LaterSidebar-JGFe0scG.css` | 89 B | One keyframe (`laterSidebarSlideIn`) |
| `evidence/web/*.js` | ~904 KB | Markup class strings cross-referenced for usage frequency and component signatures |

**Build fingerprint (explains token shapes).** Tailwind v3.x (presence of `--tw-bg-opacity` / `--tw-text-opacity` triplets and `theme(spacing.1)` inside an arbitrary value) with `@tailwindcss/typography` (`prose` / `prose-sm` and 36 `--tw-prose-*` variables), plus tippy.js v6 (`.tippy-box`, `data-theme~=defaultTooltip`) for tooltips. Output is minified by Lightning CSS: `@media (min-width:640px)` became `@media (width>=640px)`, `100%` became `to`, `#00000000` became `#0000`, and some authoring-space colors were emitted as hex + `@media (color-gamut:p3)` / `@supports (color:color(display-p3 0 0 0%))` pairs. Corroborating evidence for wide-gamut authoring: the hand-written `--task-link` rules use `color-mix(in oklch, var(--task-link-slot-color) 8%, white)`.

---

## 1. Color palette

### 1.1 Strategy: light-only, hardcoded, no theme layer

- **No dark mode.** Zero occurrences of `prefers-color-scheme`, `dark:`, `color-scheme`, `.dark` (the single `.dark` string is the component class `.button-icon-sm.dark:hover`), or any `@media` color-scheme query.
- **No app design-token CSS variables.** The only non-Tailwind custom properties defined anywhere are `--app-header-height: 40px` (in `:root`) and the eight `--task-link-slot-*` triplets. Everything else is a Tailwind theme color compiled to literal hex + `rgba(..., var(--tw-*-opacity,1))`.
- The app background is painted per-surface with utility classes (`bg-gray-25` for the canvas, `bg-white` for panels), not globally. `html, body, #root` are explicitly `background: 0 0` (transparent) — the window chrome supplies nothing.
- **91 distinct hex values** and **54 distinct `rgba()` values** appear; **655 distinct utility class tokens** are emitted in the stylesheet (i.e. Tailwind's content scan purged everything unused — the utility set ≈ the exact set used in the app).

### 1.2 App-scale neutral ("gray") — the monochrome backbone

This is a **12-step custom gray scale that replaces Tailwind's default gray** (step names are the Tailwind keys, so `.bg-gray-25`, `.text-gray-950`, … are app tokens, not Tailwind defaults; compare Tailwind's own `#f9fafb / #f3f4f6 / #e5e7eb / …`).

| Token | Hex | Roles observed in CSS/JS |
|---|---|---|
| `gray-25` | `#fbfbfb` | App canvas / editor area (`flex-1 bg-gray-25`) |
| `gray-50` | `#f6f6f6` | Panel headers, hover surface on white, disabled input background, list-row hover |
| `gray-100` | `#e7e7e7` | Top bar (`bg-gray-100/50`), icon-button hover, active/selected row, drag-handle hover |
| `gray-200` | `#d1d1d1` | Borders/dividers, disabled button surface, secondary-button hover, scroll thumb colour |
| `gray-300` | `#b0b0b0` | Placeholder text, dashed empty-state borders, disabled text, spinner |
| `gray-400` | `#888888` | Muted icons/text, meta |
| `gray-500` | `#6d6d6d` | Secondary text, progress-bar fill, focus border |
| `gray-600` | `#5d5d5d` | Secondary text (slightly stronger) |
| `gray-700` | `#4f4f4f` | Secondary-button text, `focus-visible` outline colour |
| `gray-800` | `#454545` | Tooltip surface, `.button-sm` surface, primary text |
| `gray-900` | `#3d3d3d` | Default body/menu text, `.button` surface |
| `gray-950` | `#1e1e1e` | Highest-emphasis text, hover state of every button, dialog backdrops (`/40`, `/30`, `/70`) |
| alpha variants used | — | `gray-100/50#e7e7e780`, `gray-100/75#e7e7e7bf`, `gray-200/75`, `gray-300/40#b0b0b066`, `gray-300/50#b0b0b080`, `gray-950/30#1e1e1e4d`, `gray-950/40#1e1e1e66`, `gray-950/70#1e1e1eb3` |

Note the scale is **compressed toward the dark end**: the visual step from `gray-600`→`gray-800` spans only `#5d5d5d`→`#454545`.

**Off-scale one-offs** (hardcoded, not part of the palette):

| Value | Where | Note |
|---|---|---|
| `#fdfdfd` | task `input[type=checkbox]+span` background | near-white but not `#fff` |
| `#ebeff0` | task checkbox hover | first blue-tinted value in the sheet |
| `#364045` | task checkbox 1 px border | dark slate, well outside the neutral ramp |
| `#a4b2b7` | completed-task text + `line-through` colour | the app's only "muted blue-gray" |
| `#F0F0F0` (via `bg-[#F0F0F0]`) | task-link visualization canvas | arbitrary-value utility |
| `#fbf4e6` / `#f2c86c` | agent-edit preview row bg / left rule | warm "pending" tint |
| `#222`, `#0b0b0b`, `#484848`, `#bebebe`, `#eee` | `.task-preview-keep` / `.task-preview-undo` buttons | small inline toolbar, outside the ramp |
| `#333` | tippy default (unthemed) box | library default, overridden by `defaultTooltip` |
| `#fff2e2` | clarity "stars" dot hover / selected-text highlight | warm amber tint |
| `#f3f4f6` | `.task-evaluating .clarity-indicator-dot:hover` | Tailwind default `gray-100` leaking in |

### 1.3 Accent scales

The `amber`, `red`, `indigo`, `gold` scales are **custom** (they do not match Tailwind defaults); `green-600` does match Tailwind.

| Family | Step | Hex | Observed use |
|---|---|---|---|
| amber | 50 | `#fef1ef` | subtle amber surface |
| amber | 100 | `#fde2df` | selected radio-card background, amber button hover |
| amber | 200 | `#fac1ba` | disabled amber border/text |
| amber | 400 | `#f77d63` | amber button border |
| amber | 500 | `#ed5c2d` | **the single brand accent**: `accent-color` on radios, dot indicators, focus border + `focus:ring-1`, links |
| amber | 600 | `#bc4721` | link text (higher contrast than 500) |
| red | 50 | `#ffeded` | destructive-preview row background |
| red | 200 | `#feb7b7` | destructive-preview border |
| red | 500 | `#f80b09` | `fill-red-500` (icon) |
| red | 600 | `#cc0605` | `.button-sm.danger` background, **editor caret colour** |
| red | 700 | `#9b0403` | danger hover, danger text |
| indigo | 100 | `#e4e1eb` | planning/agent context surface |
| indigo | 600 | `#6b5885` | indigo surface |
| indigo | 700 | `#514266` | indigo text |
| gold | 300 | `#f4b700` | single use: `bg-gold-300` (authored in P3: `color(display-p3 .91998 .72663 .24591)`) |
| green | 600 | `#16a34a` | `fill-green-600` (success icon) only; no green text or border tokens |

### 1.4 Task-link palette (only true semantic color system in the app)

Eight named colors, each a pair `slot colour` (light, used for the 6×13 px marker and connector lines) + `border colour` (dark, used as the 1 px marker outline). All defined as component classes, not Tailwind tokens.

| Class | `--task-link-slot-color` | `--task-link-slot-border-color` |
|---|---|---|
| `.task-link-color-red` | `#fe987c` | `#e44a08` |
| `.task-link-color-gold` | `#fbb961` | `#c08600` (P3 `color(display-p3 .73851 .52417 0)`) |
| `.task-link-color-green` | `#b0c265` | `#6e9200` (P3 `.45585 .57215 .08544`) |
| `.task-link-color-cyan` | `#63ccc0` | `#009791` (P3 `0 .60282 .58441`) |
| `.task-link-color-blue` | `#79b4ff` | `#007dd6` (P3 `.51792 .69915 .9875` / `.13377 .47512 .86631`) |
| `.task-link-color-plum` | `#c49bf3` | `#8c5ad3` |
| `.task-link-color-pink` | `#f893bc` | `#d14693` |
| `.task-link-color-neutral-gray` | `#d1d1d1` | `#888888` |

Default (uncoloured) slot: `--task-link-slot-color:#64748b` (Tailwind `slate-500`), `--task-link-slot-placeholder-color:#b8c0c7`, `--task-link-slot-border-color:transparent`. Empty-slot hover border `#8b98a5`.

### 1.5 Tailwind-default values that are *not* app tokens

`#111827`, `#374151`, `#4b5563`, `#6b7280`, `#9ca3af`, `#d1d5db`, `#e5e7eb`, `#f3f4f6`, `#1f2937`, `#3b82f6` and the 36 `--tw-prose-*` / `--tw-prose-invert-*` variables come from **`@tailwindcss/typography` defaults** (`.prose{color:var(--tw-prose-body);max-width:65ch}`), not from the app's palette. In `AgentConversation` the app overrides them at the usage site: `class="prose prose-sm text-body text-gray-950"`. `#3b82f6...80` and `#ffffff1a` are also library artifacts (selection ring / invert kbd shadow).

### 1.6 Text-on-surface pairs (the working foreground hierarchy)

- Default text `gray-900 #3d3d3d` (33 uses in markup) and `gray-950 #1e1e1e` (31 uses) on `white` / `gray-25`.
- Secondary `gray-600 #5d5d5d` (20) and `gray-500 #6d6d6d` (22); tiers `gray-700 #4f4f4f` (8), `gray-400 #888888` (10), `gray-300 #b0b0b0`.
- Inverted text is always pure `#fff` on `gray-800 #454545` (tooltip, `.button-sm`) or `gray-900/gray-950` (buttons).

---

## 2. Typography

### 2.1 Font faces

Three `@font-face` rules, all self-hosted, `font-display: swap`:

| Family (as declared) | Weight axis | File |
|---|---|---|
| `IBM Plex Sans var experimental` | `100 900` (variable) | `fonts/IBM_Plex_Sans_Var_Roman.woff2` (225 KB) |
| `IBM Plex Mono` | `400` | `fonts/IBMPlexMono-Regular.woff2` |
| `IBM Plex Mono` | `700` | `fonts/IBMPlexMono-Bold.woff2` |

No italic faces, no subsetting per-script, no `unicode-range`. `html,:host` and `body` both set `font-family: IBM Plex Sans var experimental, sans-serif`, with `-webkit-font-smoothing: antialiased; -moz-osx-font-smoothing: grayscale`. `.font-mono` → `IBM Plex Mono, monospace`. **Note the family name itself is `IBM Plex Sans var experimental`** (the upstream variable-font name), not `IBM Plex Sans` — a direct hand-off detail for reimplementation.

### 2.2 Type scale (exact declarations)

`text-transform: uppercase` is **baked into every heading class**, so headings are always set in caps. Weights are 400 / 450 / 700 — 450 is an intermediate variable-axis weight used only for "medium".

| Class | Size | Weight | Line-height | Uppercase | Markup uses |
|---|---|---|---|---|---|
| `.text-heading-1` | 18 px | 700 | 21 px | yes | 1 |
| `.text-heading-2` | 16 px | 700 | 18 px | yes | 9 |
| `.text-heading-3` | 14 px | 700 | 16 px | yes | 8 |
| `.text-heading-4` | 12 px | 700 | 16 px | yes | 2 |
| `.text-body` | 14 px | 400 | 20 px | no | 33 |
| `.text-body-medium` | 14 px | 450 | 20 px | no | 2 |
| `.text-body-bold` | 14 px | 700 | 20 px | no | 5 |
| `.text-body-sm` | 13 px | 400 | 19.5 px | no | 11 |
| `.text-body-sm-bold` | 13 px | 700 | 19.5 px | no | 4 |
| `.text-label-sm` | 13 px | 400 | 15 px | no | 32 |
| `.text-label-sm-medium` | 13 px | 450 | 15 px | no | 6 |
| `.text-label-sm-bold` | 13 px | 700 | 15 px | no | 3 |
| `.text-label-xs` | 12 px | 400 | 14 px | no | 1 |
| `.text-label-xs-bold` | 12 px | 700 | 14 px | no | 1 |

Three families of sizes only: **18/16/14/12 px headings, 14/13 px body, 13/12 px labels** — a 12–18 px total range. Half-pixel line height (19.5 px) on `body-sm`; `heading-2` has an 18 px line height at 16 px font size (ratio 1.125).

Ad-hoc sizes via arbitrary values: `text-[12px]`, `text-[13px]`, `text-[21px]`, `text-[24px]`, plus Tailwind default `text-sm` (`.875rem`/`1.25rem) used in menus, and default `text-xs`. Arbitrary `leading-[13px]`, `leading-[27px]`, `leading-[32px]`. Task-row body text uses `line-height: 1.6` (`ul[data-type=taskList] p`).

### 2.3 Letter-spacing / weight extras

- `letter-spacing` appears exactly once: `.tracking-wide` = `.025em`, used in markup as `text-heading-3 tracking-wide text-gray-800` (eyebrow-style headings).
- `font-weight` values in the sheet: `400` (14×), `700` (14×), `600` (6×), `500` (5×), `450` (3×), `800` (2×), `900` (1×), `bolder` (1×) — the 600/800/900 come from library/CSS defaults, not the type scale; `450` is used only by `.text-label-sm-medium` / `.text-body-medium` and the utility `font-[450]`.
- Mono (`IBM Plex Mono`) is used for `.font-mono` and via `.tippy-content`-adjacent code paths only; no mono in headings or labels.

---

## 3. Layout & spacing

### 3.1 Spacing scale

Base 4 px with a **2 px half-step**, extracted from the emitted spacing utilities:

`0, 0.5 (2px), 1 (4px), 1.5 (6px), 2 (8px), 3 (12px), 4 (16px), 5 (20px), 6 (24px), 8 (32px), 10 (40px)`.

Observed distribution in markup: `gap-1` (50 uses), `gap-2` (34), `gap-0.5` (32), `px-2` (27), `py-1` (18), `px-3` (14) — i.e. an 4/8 px rhythm dominates, 12 px is the section gutter, 16 px is the page gutter. Gaps of 1 px (`gap-px`) and 2 px (`gap-[2px]`, `gap-[3px]`, `gap-[6px]`) are used for icon+label clusters.

### 3.2 App shell

| Element | Value | Evidence |
|---|---|---|
| Header height | **40 px** | `:root{--app-header-height:40px}`; used as `h-[var(--app-header-height)]`, `top-[var(--app-header-height)]`, `pt-[var(--app-header-height)]`, `h-[calc(100vh-var(--app-header-height))]` |
| Header surface | `bg-gray-100/50` + `border-b border-gray-200`, `fixed`, `z-40`, `w-screen`, `overflow-clip`, `data-tauri-drag-region` | index JS |
| Left/right header zones | `basis-[min(32vw,320px)]`, collapsing to `w-[240px] basis-[240px]` under `max-[900px]` | index JS |
| "Later" sidebar | `w-[440px]`, `fixed left-0 top-[var(--app-header-height)]`, `bg-white`, `border-r border-gray-200`, `z-30`, `overflow-y-auto no-scrollbar` | LaterSidebar JS |
| Right-hand side panel | `w-[424px]`, `fixed right-0`, `border-l border-gray-200`, `bg-white`, `z-30`; header 40 px with `bg-gray-50` + `border-b border-gray-200` | index JS (`fixed right-0 top-[var(--app-header-height)] … w-[424px]`) |
| Sidebar toggle | `w-10 h-10 border-x border-gray-300/50 group` | index JS |
| Content column | `max-w-2xl w-full` (= 42 rem / **672 px**) inside `px-4`/`px-6 py-4` | `max-w-2xl w-full flex flex-col`, `max-w-2xl px-6 py-4 max-h-[calc(100vh-48px)]` |
| Widest container token | `max-w-7xl` (80 rem) present in CSS but not found in markup | CSS only |
| Dialog widths | `w-[420px]`, `max-w-[420px]`, `max-w-[366px]`, `w-[512px]`, `max-w-[576px]`, `w-[220px]`, `w-[360px]`, `w-[450px]`, `w-[512px]`, `w-[560px]`, `w-[600px]`, `w-[624px]`, `w-[700px]` | CSS arbitrary values |
| Dialog padding | `p-[42px]` (cycle-length dialog), `px-4 py-4` (destructive dialog) | markup |
| Breakpoints emitted | `640 / 768 / 1024 / 1280 / 1536 px` (Tailwind `container`) + `max-[900px]` for the compact header | CSS |

### 3.3 Grid patterns

Only three grid templates exist:

- `grid-cols-[theme(spacing.1)_auto]` → `8px auto` (onboarding numbered-step list).
- `grid-cols-[minmax(0,0fr)_minmax(0,1fr)]` → collapsing two-pane; flips to `minmax(0,1fr) minmax(0,1fr)` when `data-split=true`, animated with `transition-[grid-template-columns] duration-300 ease-out`.
- Otherwise everything is flexbox: `flex` appears 220× in markup strings, `flex-col` 89×, `items-center` 111×.

### 3.4 Radii

Extremely restricted — **the app is effectively square**:

| Value | Source | Use |
|---|---|---|
| `0` | default (no radius class anywhere) | buttons, inputs, dialogs, menus, cards, panels, tooltips — all four corners square |
| `.125rem` (2 px) | `.rounded-sm` (3 markup uses) | small chips |
| `.25rem` (4 px) | `.rounded` | agent bar / popover |
| `2px` | `.task-preview-keep` / `.task-preview-undo`, `rounded-[2px]` | inline preview buttons, small chips |
| `3px` | `.task-link-inline-marker` | task-link marker (6×13 px) |
| `9999px` / `50%` | `.rounded-full`, clarity dots, drag-handle dot, `planning-issues-loader` | dots and rings only |
| `.3125rem` / `.375rem` / `4px` | `@tailwindcss/typography` `kbd` & `pre`, tippy default `.tippy-box` | **library defaults, not app choices** (the app sets `border-radius:0` on its `defaultTooltip` theme) |

### 3.5 Borders

- **1 px everywhere.** `border-width: 1px` (4 declarations), `border-{top,right,bottom,left}-width: 1px` (12 declarations). The only thicker rule is `border-left: 3px solid #f2c86c` on `li[data-is-preview=true]`.
- Divider colors: `gray-100 #e7e7e7` (18 uses) and `gray-200 #d1d1d1` (17) dominate; `gray-300 #b0b0b0` (8) for stronger/empty-state borders; `gray-300/50` for the header toggle.
- **Dashed** (`border-style:dashed`) is a deliberate empty-state/chrome device: 7 uses in markup, e.g. the dashed empty-state frame and `h-12 border-l border-dashed border-gray-400` used as a vertical connector in the onboarding step list. One `border-dotted` exists.
- The task checkbox uses a **1 px `#364045`** border (visible alternative checkbox design, not the neutral ramp).

### 3.6 Shadows

**The app is essentially shadow-free.** The entire stylesheet contains:

- `.shadow` (Tailwind default) emitted once: `0 1px 3px #0000001a, 0 1px 2px -1px #0000001a`.
- One inset hairline: `.task-link-visual-highlight:before { box-shadow: inset 0 0 0 1px color-mix(in oklch, var(--task-link-slot-color) 25%, white) }`.
- Ring shadows (`--tw-ring-*`) from `ring` / `focus:ring-1`.
- `.drag-handle:before { box-shadow: 6px 0, 0 6px, 6px 6px, 0 12px, 6px 12px }` — a hand-built 2×3 dot grid, not a drop shadow.

Dialogs (`z-[90]`), popovers and panels rely on a **`bg-gray-950/40` full-screen scrim** or a 1 px border instead of elevation. This is the single biggest structural decision in the visual language.

---

## 4. Component inventory

Signatures below are class strings observed in markup (JS) or component rules in CSS, with state variants.

### 4.1 Buttons

| Component | Signature | States |
|---|---|---|
| `.button` (default, solid dark) | `bg #3d3d3d`, `#fff`, `padding:8px 12px`, `14px/400/20px`, `display:inline-flex`, `align-items:center`, **no radius** | hover `#1e1e1e`; disabled `pointer-events:none; opacity:.4` |
| `.button.secondary` | `bg #e7e7e7`, text `#4f4f4f` | hover `bg #d1d1d1`, text `#1e1e1e` |
| `.button-legacy` | `bg #454545`, `#fff`, `text-transform:uppercase`, `padding:8px 16px`, `14px/700/16px` | hover `#1e1e1e`; `.secondary` = `bg #d1d1d1` text `#4f4f4f` → hover `bg #b0b0b0` text `#1e1e1e` |
| `.button-sm` | `bg #454545`, `#fff`, `padding:4px 8px`, `13px/400/15px`, `justify-content:center` | hover `#1e1e1e`; `.secondary` = `border 1px #e7e7e7`, `bg #fff`, text `#6d6d6d` → hover `border #d1d1d1`, `bg #f6f6f6`, text `#454545`; `.danger` = `bg #cc0605` `#fff` → hover `#9b0403`; `:disabled` `opacity:.4; pointer-events:none` |
| `.button-sm-icon-text` (ghost) | transparent, text `#4f4f4f`, `height:22px`, `gap:4px`, `padding:0 8px`, `13px/400/15px` | hover `bg #f6f6f6`, text `#3d3d3d`; disabled `opacity:.4` |
| `.button-icon-sm` (icon-only) | transparent, `width:22px; height:22px`, centered flex | hover `bg #e7e7e7`; `.dark:hover` `background:#ffffff26`; disabled `opacity:.4` |
| `.agent-option-button` | `border 1px #1e1e1e`, `bg #1e1e1e`, `#fff`, `justify-content:space-between`, `gap:16px`, `padding:.375rem 16px`, `13px/400/15px` | `:not(:disabled):hover` `bg #3d3d3d`; `:disabled{cursor:default;opacity:.3}`; `.secondary` = `border 1px #d1d1d1`, `bg #fff`, text `#888`, `justify-content:flex-start` → hover `border #b0b0b0`, `bg #f6f6f6`, text `#4f4f4f` |
| Header actions (`.header-action-*`) | `!important`-driven: secondary = transparent bg + `border-color:#d1d1d1` + text `#4f4f4f`; primary = `bg #1e1e1ecc` + `#fff`; invitation = transparent + text `#bc4721` (amber-600) | hovers: secondary `bg #ffffff4d`/`border #b0b0b0b3`/text `#3d3d3d`; primary `bg #1e1e1ee6`; invitation `bg #f6f6f64d` |
| Header controls (`.header-control-idle` / `-active`) | idle transparent → hover `#ffffff59`; active `#ffffff73` | driven by `aria-expanded` in JS |
| `.task-preview-keep` / `.task-preview-undo` | `border-radius:2px`, `padding:0 6px`, `12px/500`; keep = `#fff` on `#222`, undo = `#484848` on `#fff` with `1px solid #bebebe` | hovers `#0b0b0b` / `bg #eee`, `border-color:gray` |

Common to all: **no `border-radius` by default, no box-shadow, no transition** on the semantic button classes (state changes are instantaneous); only the Tailwind-driven variants (`transition-colors`) animate.

### 4.2 Inputs

| Input | Signature |
|---|---|
| Questionnaire / feedback textarea | `h-[88px] w-full resize-none border border-gray-300 bg-white p-1.5 text-body text-gray-900 outline-none transition-colors focus:border-amber-500 focus:ring-1 focus:ring-amber-500 disabled:bg-gray-50` |
| Another feedback textarea | `min-h-[104px] resize-y border border-gray-200 bg-white p-1 text-body text-gray-950 outline-none placeholder:text-gray-300 focus:border-gray-500 disabled:bg-gray-50 disabled:text-gray-400` |
| Chat composer | wrapper `w-full overflow-hidden border border-gray-100 bg-white`; textarea `min-h-[64px] max-h-[144px] resize-none overflow-y-auto px-2 pb-0.5 pt-1 text-body outline-none` |
| Numeric session-duration input | `w-[36px] bg-transparent text-right text-label-sm text-gray-950 outline-none` + `.session-duration-input{appearance:textfield}` and `::-webkit-{inner,outer}-spin-button{appearance:none;margin:0}` |
| `:placeholder` | `.placeholder\:text-gray-300::placeholder` → `#b0b0b0` |

All square (no radius), 1 px border, `outline-none` on all of them, and focus expressed either as `border-amber-500 + ring-1` (form flow) or `border-gray-500` (quiet form).

### 4.3 Checkbox / radio

| Control | Signature |
|---|---|
| Task checkbox (editor, custom) | `input[type=checkbox]{position:absolute;visibility:hidden;width:15px;height:15px}` + `input+span{width:15px;height:15px;background:#fdfdfd;border:1px solid #364045;display:inline-block}`; hover `background:#ebeff0`; checked = `opacity:.7` **plus a base64 Material "check" SVG** (`viewBox 0 0 16 16`, path fill `#777777`); checked row: `label>span{opacity:.7}` and text `color:#a4b2b7; opacity:.7; line-through .5px` |
| Radio card (AI provider choice) | `flex cursor-pointer items-start gap-1 border border-gray-200 bg-white p-2 has-[:checked]:border-amber-500 has-[:checked]:bg-amber-100`; inner input `shrink-0 accent-amber-500 mt-[1px]` |

`accent-color: #ed5c2d` is set only via the `accent-amber-500` utility. There is no checkbox/switch component in the Tailwind layer.

### 4.4 List rows

| Row | Signature |
|---|---|
| Task row (editor) | `ul[data-type=taskList] li{display:flex;flex-direction:row}` → `>label{height:2.1em;padding:4px 0;margin-right:.5rem}` → `>.task-row-body{flex:auto;min-width:0}` → `>.task-item-content>p{padding-top:3.5px;padding-bottom:4.5px}`, text `line-height:1.6`; `>ul{padding:0 0 0 3px}`; `>li>.task-item-controls{order:2;gap:4px;min-height:20px;margin-left:auto;padding:4px 0;justify-content:flex-end}` |
| Menu row | `flex w-full flex-row items-center gap-1 px-2 py-1 text-left text-gray-900 hover:bg-gray-100 hover:text-gray-950 disabled:cursor-not-allowed disabled:text-gray-400` (one variant uses `text-label-sm text-gray-800` + `px-2 py-1.5`) |
| Icon/inline button row | `flex flex-row items-center gap-1 px-2 py-1 text-left hover:bg-gray-100` |
| Hover-revealed row affordances | `.task-move-action`, `.clarity-indicator-dot`: `opacity:0;pointer-events:none` → `li:hover … {opacity:1;pointer-events:auto}` |
| Selected/active row | `data-[task-menu-active=true]:bg-gray-100` |

### 4.5 Surfaces: panel / card / popover / dialog / tooltip

| Primitive | Signature |
|---|---|
| Card / framed block | `flex flex-col gap-2 border border-gray-200 bg-white p-3` (no radius, no shadow) or `min-h-[120px] w-full flex-1 items-center border border-dashed border-gray-300 px-6 py-4` (empty state) |
| Popover / agent bar | `fixed z-50 flex flex-col gap-2 border border-gray-200 bg-white p-3` + `animate-[fadeInUp…]`; entry animation class `.agent-bar-pop{transform-origin:0;animation:.25s ease-out forwards agent-bar-pop}` |
| Menu | `flex flex-col py-1 bg-white border border-gray-100 text-sm text-gray-900 min-w-40 pointer-events-auto role=menu` |
| Side panel | `fixed right-0 top-[var(--app-header-height)] z-30 … w-[424px] border-l border-gray-200 bg-white`; panel header `h-[40px] border-b border-gray-200 bg-gray-50 px-3` |
| Modal | backdrop `fixed inset-0 z-[90] bg-gray-950/40 p-3 animate-[fadeIn_0.16s_ease-out] motion-reduce:animate-none`; dialog `bg-white px-4 py-4 text-gray-900 animate-[fadeInUp_0.22s_ease-out] motion-reduce:animate-none`, `role=dialog aria-modal=true tabindex=-1`, closed with `sr-only` h2 label; danger dialog adds `max-w-[420px]` and `button-sm danger !h-[32px] !px-[12px]` |
| Tooltip (tippy) | `.tippy-box[data-theme~=defaultTooltip]{background:#454545;color:#fff;border-radius:0;font-size:14px;font-weight:400;line-height:20px}`; `.tippy-content{white-space:normal;padding:8px 16px}`; arrow colour forced to `#454545` per placement |
| Badge / status chip | No badge component: statuses are rendered as `size-[7px] rounded-full bg-amber-500` dots + `text-body-bold`, or `px-2 py-1 text-body bg-gray-100 text-gray-800` |
| Progress bar | `h-[3px] bg-gray-100` track, `h-full bg-gray-500 transition-…` fill, `role=progressbar aria-valuemin=0` (div-based, in `Getting started` guide) |
| Progress ring | SVG circle `r=7`, `fill=none`, `stroke-gray-500`, `stroke-width=2`, `stroke-linecap=butt`, animated via `stroke-dashoffset` (`transition-[stroke-dashoffset] duration-150 motion-reduce:transition-none`) |
| Loader | `.planning-issues-loader`: 12 px ring built from `--size:.3px`, `border:calc(5*var(--size)) solid var(--color-1)` where `--color-1:#b0b0b0`, `border-bottom-color:transparent`, `border-radius:50%`, `1s linear infinite` |
| Table / prose blocks | Only via `@tailwindcss/typography` (`.prose`, `.prose-sm`) with app overrides (`prose prose-sm text-body text-gray-950`) |
| Toolbar strip | `.workspace-review-bar`: `opacity:0; pointer-events:none; max-height:0; transition:max-height .18s, opacity .15s; overflow:hidden` → `--visible{opacity:1;pointer-events:auto;max-height:48px}` |

### 4.6 Drag & drop primitives

| Primitive | Signature |
|---|---|
| Drag handle | `.drag-handle{opacity:0;pointer-events:none;color:rgba(176,176,176);background:transparent;border:0;z-index:5;position:absolute}`; `:before` = 6 dots via `box-shadow` at `border-radius:9999px; width:3.25px; height:3.25px`; `:after` = invisible 20×24 px hit area, hover `background:#e7e7e7`, active `#b0b0b0` |
| Dragging state | `.ProseMirror.dragging li.dragging-task-item{opacity:.45}`; `body.dragging .ProseMirror{user-select:none}` |
| Copy-drag | `body.task-drag-operation-copy, body.task-drag-operation-copy * {cursor:copy!important}` |
| Sortable drag ghost | `.focus-block-sortable-drag{opacity:.96;pointer-events:none}` |
| Sortable drop target | `.focus-block-sortable-placeholder{outline:1px dashed #9ca3af; outline-offset:-1px; opacity:1}` + `>*{visibility:hidden}` |
| Global sort mode | `body.sorting-active, body.sorting-active *{user-select:none!important}` |
| Task-link visualization | `[data-task-link-visualization-scope].task-link-visualization-active .drag-handle{opacity:0!important;display:none!important}`; overlay `position:fixed;width:100vw;height:100vh;pointer-events:none;z-index:20`; connector `fill:none; stroke:var(--task-link-slot-color); stroke-width:1px; stroke-linecap:round; vector-effect:non-scaling-stroke`; terminal square `fill:var(--task-link-slot-color)`; highlighted row shadow via `color-mix(in oklch, var(--task-link-slot-color) 8%, white)` with a `25%` inset 1 px ring |

---

## 5. Interaction affordances

### 5.1 Focus

- **Two focus languages coexist.**
  1. *Utility-level, invisible-to-mouse:* `focus:outline-none` (12 uses) plus an explicit visible ring `focus-visible:outline focus-visible:outline-1 focus-visible:outline-gray-700` (3 uses each) → `.focus-visible\:outline:focus-visible{outline-style:solid}`, `.focus-visible\:outline-1:focus-visible{outline-width:1px}`, `.focus-visible\:outline-gray-700:focus-visible{outline-color:#4f4f4f}`. A **1 px solid `#4f4f4f` outline** is the app's canonical focus ring.
  2. *Form-level:* `focus:border-amber-500` + `focus:ring-1` + `focus:ring-amber-500` → 1 px amber (`#ed5c2d`) ring on textareas/inputs.
- `:focus-visible` background/border swaps are also used as focus indication: `focus-visible:bg-gray-100` (2), `focus-visible:border-gray-800` (1), `focus-visible:underline` (1).
- The task-link slot removes the outline entirely on focus but compensates: `.task-link-slot-interactive:focus{outline:none}` and `:hover:before, :focus-visible:before{background:color-mix(in oklch, var(--task-link-slot-color) 25%, white)}` — a color-mix "highlight plate" replaces the ring.
- Tailwind's `ring` default is `0 0 0 calc(3px + offset)`; only `ring-1` (1 px) is actually used.
- `.outline-none` sets `outline: 2px solid #0000; outline-offset: 2px` (transparent, so it only suppresses the UA ring while keeping layout).

### 5.2 Cursor

Only 6 cursor declarations exist: `pointer` (1), `text` (1), `not-allowed` (1, plus 4 `disabled:cursor-not-allowed` utilities), `copy` (1, drag-with-modifier), `default` (5, incl. `.task-link-slot` and `.agent-option-button:disabled`). No `grab`/`grabbing`/`move`/`col-resize` anywhere — dragging is signalled by the handle dots, `opacity:.45` and the `body.dragging` user-select lock, **not** by cursor shape.

### 5.3 Transitions & animations

**Durations:** `.1s / .15s / .18s / .2s / .24s / .25s / .3s / .4s / .55s / .6s / .7s / .85s / 1s / 1.5s / 1.8s / 2s`. Tailwind utilities used: `duration-100, 150, 200, 300, 1000`.

**Easings:** Tailwind `ease-out = cubic-bezier(0,0,.2,1)` and `ease-in-out = cubic-bezier(.4,0,.2,1)` (the dominant pair, 9 declarations each), `ease-linear`, plus three custom curves used by the onboarding sequence: `cubic-bezier(.22,1,.36,1)` (expo-out; `top`/`transform` at `.7s`), `cubic-bezier(.4,0,.2,1)` at `.24s/.55s/.7s/.85s`, and tippy's `cubic-bezier(.54,1.5,.38,1.11)` (overshoot).

**Transition properties:** `transition-colors` (`color,background-color,border-color,text-decoration-color,fill,stroke`), `transition-opacity`, `transition-all`, and four property-targeted ones: `[grid-template-columns]`, `[max-width]`, `[stroke-dashoffset]`, `[width]`. All Tailwind transitions default to `.15s` + `cubic-bezier(.4,0,.2,1)`. `.task-preview-actions{transition:opacity .15s}`; `.workspace-review-bar{transition:max-height .18s,opacity .15s}`.

**All 18 `@keyframes`:**

| Name | Content | Applied as |
|---|---|---|
| `fadeIn` | `0%:opacity 0 → to:1` | `animate-[fadeIn_0.16s_ease-out]` (modal scrim), `.1s`, `.2s forwards` |
| `fadeInUp` | `opacity 0 / translateY(20px) → 0` | `animate-[fadeInUp_0.16s/0.22s_ease-out]` |
| `toastIn` / `toastOut` | `opacity+translateY(4px)+scale(.98)` | `.15s ease-out forwards` / `.12s ease-in forwards` |
| `spin` | `rotate 45deg → 585deg` | library spinner |
| `spin-linear` | `rotate 0 → 360deg` | `.animate-spin-linear{1s linear infinite}` |
| `planning-issues-loader-rotation` | rotate 360 | `.planning-issues-loader` |
| `pulse-dot` | `opacity 1 / scale 1 → .8/1.8 → 1` | `.animate-pulse-dot{.2s ease-out}` |
| `pulse-scale` | `opacity .5 / scale 1 → 1/1.5` | library |
| `dots` | `content "." → ".." → "..."` | `.animated-dots:after{1.5s step-end infinite}` |
| `clarity-pulse` | `scale 1 → 1.2, opacity 1 → .6` | `.task-evaluating .clarity-indicator-dot svg{1s ease-in-out infinite}` |
| `pop-stars` | `scale 1 → 1.4 → 1` | `.task-just-evaluated … {.4s ease-in-out forwards !important}` |
| `task-item-pop` | `scale .985 → 1.2 → 1` | `.animate-task-agent-update{.25s ease-in-out}`, `li[data-is-preview=true]` |
| `agent-bar-pop` | `scale 1 → 1.3 → 1` | `.agent-bar-pop{.25s ease-out forwards}` |
| `chat-input-voice-bar` | `scaleY(1) → var(--voice-bar-scale)` | `.animate-[chat-input-voice-bar_1800ms_ease-in-out_infinite]` on a 2×12 px bar |
| `laterSidebarSlideIn` | `translateX(-100%) → 0` | `animate-[laterSidebarSlideIn_150ms_ease-out_backwards]` |
| `onboarding-entrance` | `opacity 0 / translateY(4px) → 0/0` | `.24s ease-out both` / `.18s ease-out both` |
| `onboarding-split-panel-in` | `opacity 0 / translateX(-8px) → 0/0` | `.6s ease-out both` |

Height/width/column changes are animated with `max-height`, `max-width`, `width` and `grid-template-columns` transitions rather than layout-free tricks — the onboarding split pane and review bar use `max-height:48px`/`max-width` transitions.

### 5.4 Reduced motion

Four `@media (prefers-reduced-motion: reduce)` blocks exist and they are **inconsistent**:

1. Tailwind variant utilities emitted for markup use: `.motion-reduce\:animate-none{animation:none}`, `.motion-reduce\:opacity-100{opacity:1}`, `.motion-reduce\:transition-none{transition-property:none}` — 5 markup uses of `motion-reduce:animate-none`, 1 of `motion-reduce:transition-none`.
2. `.workspace-review-bar{transition:none}`.
3. `.onboarding-welcome-entrance,.onboarding-flow-entrance,[data-onboarding-goals-slot][data-visible=true] .onboarding-goals-panel{animation:none}` plus a blanket `.onboarding-shell *, :before, :after{scroll-behavior:auto!important; transition-duration:.01ms!important; animation-duration:.01ms!important; animation-iteration-count:1!important}`.
4. `.animate-task-agent-update, li[data-is-preview=true]{animation:none}`.

**Not covered:** the infinite loops `clarity-pulse` (1 s), `planning-issues-loader-rotation` (1 s), `spin-linear` (1 s), `animated-dots` (1.5 s), `chat-input-voice-bar` (1.8 s), and all four `.tippy-*` transitions. So the app's most persistent motion (agent thinking / streaming indicators) does **not** respect reduced-motion.

### 5.5 Skeletons & empty states

- **No skeleton component.** `animate-pulse`/`animate-spin` are never used; loading is expressed by the custom spinners above and by `opacity`/`disabled` states.
- **Empty state is a first-class pattern:** a full-width dashed rectangle — `min-h-[120px] w-full flex-1 items-center border border-dashed border-gray-300 px-6 py-4`, inner column `max-w-[366px] flex-col gap-2 py-4`, containing `<h2 class="text-heading-2 text-gray-950">` (rendered uppercase via the class), a `text-body text-gray-600` sentence, and a primary `button` — e.g. the Focus Blocks empty state. Dashed borders also double as **decorative connectors** in the onboarding step list (`h-12 border-l border-dashed border-gray-400`).
- The "Later" sidebar's empty hint (`DismissibleHint` with `icon` + `children`) reuses the same dashed container with a `fill-gray-500` 20 px icon.

### 5.6 Scrollbars

Minimal and opt-in only:

- `.no-scrollbar::-webkit-scrollbar{display:none}` and `.no-scrollbar{-ms-overflow-style:none; scrollbar-width:none}` — applied to the Later sidebar and the task-link canvas.
- **No global scrollbar styling at all** (no `::-webkit-scrollbar` width/thumb/track rules, no `scrollbar-color`, no `scrollbar-gutter`, no `overscroll-behavior`). The app leaves native macOS overlay scrollbars untouched everywhere else.
- `overflow` usage: `overflow-y-auto` (15 markup uses), `overflow-hidden`, `overflow-x-auto`, `overflow-clip`, `overflow-y-hidden`.
- `scroll-behavior` appears only inside the reduced-motion onboarding block (`scroll-behavior:auto!important`).

---

## 6. Notable visual signature

1. **Monochrome with a single warm accent.** The entire chrome is a 12-step gray ramp from `#fbfbfb` to `#1e1e1e`; `#ed5c2d` (amber-500) is the only saturated color in the app chrome, used for the radio `accent-color`, the input focus border/ring, small status dots, and primary links; `#cc0605` (red-600) is the only other chrome color, reserved for destructive actions and the text caret. All other color in the product lives in the task-link slot system (8 user-chosen colors) and the typography-plugin prose defaults.
2. **Square by default, dotted only where playful.** With `border-radius:0` on every button, input, dialog, menu, panel, tooltip and card, and only `rounded-sm` (2 px, 3 uses) and `rounded-full` (dots/rings) in the entire utility set, the app reads as printed-graphic/architectural rather than iOS-native. Corners are never softened for "friendliness".
3. **Elevation is replaced by hairlines and scrims.** One `.shadow` in the whole sheet; modal and popover layering is done with `bg-gray-950/40` scrims, 1 px `#e7e7e7`/`#d1d1d1` borders, and z-index bands `0/1/2/5/10/20/30/40/50/90/100`.
4. **Uppercase headings are baked into the type scale** (`text-heading-1..4` all set `text-transform:uppercase`), giving an editorial/technical-label rhythm — e.g. `<h1 class="truncate text-heading-2">DO LATER</h1>` and the Focus Blocks panel's `text-heading-3` label.
5. **A near-white, non-pure canvas.** `bg-gray-25 #fbfbfb` (canvas) against `bg-white #fff` (panels) with `bg-gray-50 #f6f6f6` inset chrome creates three barely-separated planes; `#fdfdfd` (task checkbox) is a fourth.
6. **Dashed containers as the "nothing here yet" grammar** (empty states) and dashed rules as *decorative* structure (the onboarding step connector), i.e. dashed ≠ only-error.
7. **Opacity as the disabled/muted channel** rather than color swaps alone: `.4` for disabled buttons, `.3` for disabled agent options, `.7` for the completed checkbox, `.45` for a dragged row, `.96` for the drag ghost, `.85` for deleting text — 13 distinct opacity values.
8. **Hover-to-reveal controls.** Row affordances (`.task-move-action`, `.clarity-indicator-dot`, `.drag-handle`, `.task-preview-actions`) start at `opacity:0; pointer-events:none` and are unlocked on `li:hover` / `.group:hover`, keeping the reading surface (task text at `line-height:1.6`) visually quiet.
9. **Editor-grade detail where it matters:** task rows carry bespoke `3.5px/4.5px` text padding, `text-indent:12px` when a link slot is present, `1px dashed` sort placeholders, `color(display-p3)` task-link colors, and a `color-mix(in oklch, …, white)` highlight plate — the only place the app uses modern color functions.
10. **Icon system = Material Symbols**, 20×20 px, tinted with `fill-current` or `fill-gray-{400,500,600}` (30 occurrences of the `0 -960 960 960` viewBox in the index bundle).

---

## 7. Accessibility observations

### 7.1 Contrast (WCAG 2.1 relative-luminance ratios, computed from the literal values above)

| Foreground | Background | Ratio | Verdict |
|---|---|---|---|
| `gray-950 #1e1e1e` | `gray-25 #fbfbfb` (canvas) | **16.11** | AAA |
| `gray-900 #3d3d3d` | `#fbfbfb` | 10.50 | AAA |
| `gray-800 #454545` | `#fbfbfb` | 9.27 | AAA |
| `gray-700 #4f4f4f` | `#fbfbfb` | 7.92 | AAA |
| `gray-600 #5d5d5d` | `#fbfbfb` | 6.36 | AA (normal text) |
| `gray-500 #6d6d6d` | `#fbfbfb` | **5.00** | AA (normal) |
| `gray-400 #888888` | `#fbfbfb` | 3.43 | AA-large / UI only — **fails for 13–14 px text** |
| `gray-300 #b0b0b0` (placeholder / disabled) | `#fbfbfb` | 2.10 | **fails** (placeholders + `.disabled:text-gray-300`) |
| `gray-500 #6d6d6d` | `#ffffff` | 5.17 | AA |
| `gray-400 #888888` | `#ffffff` | 3.54 | AA-large only |
| `gray-600 #5d5d5d` | `gray-100 #e7e7e7` (header bar) | 5.32 | AA |
| `#ffffff` | `.button` `#3d3d3d` | 10.86 | AAA |
| `#ffffff` | `.button-sm` `#454545` | 9.59 | AAA |
| `gray-700 #4f4f4f` | `.button.secondary` `#e7e7e7` | 6.62 | AA |
| `#ffffff` | `.button-sm.danger` `#cc0605` | 5.84 | AA |
| `#ffffff` | tooltip `#454545` | 9.59 | AAA |
| amber-500 `#ed5c2d` | `#ffffff` | 3.40 | **fails AA for text; OK as UI/non-text (3:1)** — it is used as border/ring/dot, and as *text* in links (see below) |
| amber-600 `#bc4721` | `#ffffff` | 5.17 | AA — this is why links use 600, not 500 |
| amber-500 `#ed5c2d` | amber-100 `#fde2df` (selected radio card) | 2.77 | **fails** — the selected-state label (`text-gray-…` on that card is fine; the amber border is decorative) |
| red-500 `#f80b09` | `#ffffff` | 4.16 | AA-large only |
| red-700 `#9b0403` | red-50 `#ffeded` | 7.71 | AAA |
| indigo-700 `#514266` | indigo-100 `#e4e1eb` | 7.01 | AAA |
| completed task `#a4b2b7` | `#fbfbfb` | **2.11** | **low literal contrast** — observed styling, not proof of intent (struck-through, `opacity:.7`, `text-decoration-thickness:.5px`), the weakest pair in the app |
| preview row `#fbf4e6` | `gray-950` | 15.23 | AAA |
| delete-preview `#ffeded` | `gray-500` on it (opacity .85) | 4.58 | AA |
| checkbox border `#364045` | `#fdfdfd` | 10.45 | AAA |

Summary: the **core reading pairs are comfortably AAA**, the low-contrast literal pairs are (a) placeholder/disabled text at 2.10, (b) muted `gray-400` meta text at 3.43, (c) completed/checked-out task text at 2.11, (d) amber-500 used as text (3.40 — the app already routes link text through amber-600 at 5.17, so this is mostly consistent).

### 7.2 Non-text contrast

- Focus outline `#4f4f4f` on `#fbfbfb`: **7.92** — well above the 3:1 non-text requirement.
- Amber focus ring `#ed5c2d` on white: **3.40** — passes 3:1 for UI components.
- Borders `#e7e7e7` on `#fff` = 1.14 and `#d1d1d1` on `#fff` = 1.57: **both far below 3:1**, so hairline separators carry no standalone meaning (acceptable as dividers; problematic if a border is the only selected indicator — e.g. the `has-[:checked]:border-amber-500` radio card pairs the border with a background tint, which mitigates it).

### 7.3 Focus handling

- `focus:outline-none` is applied 12× in markup, paired with either `focus-visible:outline focus-visible:outline-1 focus-visible:outline-gray-700` (3×) or `focus-visible:bg-gray-100` (2×) — the `focus-visible:`-only pattern correctly keeps the ring out of mouse interactions.
- But `focus:border-amber-500`/`focus:bg-gray-100`/`focus:border-gray-500` (mouse-triggered `:focus`, not `:focus-visible`) also exist, which will show a colored border on click.
- Where the outline is fully suppressed (`.task-link-slot-interactive:focus{outline:none}`), the compensation is a `color-mix` highlight plate on `:focus-visible` — present, but only 1 px inset around a 6×13 px target.
- Interactive hit areas can be small: `.drag-handle:after` is 20×24 px, `.task-link-slot` 6×(1.6 em) with a `-1px -6px` pseudo hit area, `.button-icon-sm` 22×22 px, `.clarity-indicator-dot` 20×20 px, menu rows `py-1` (≈4 px vertical padding, ~22 px rows). `session-duration` number inputs are 36 px wide.

### 7.4 Semantics / ARIA seen in markup

`role=menuitem`, `role=menu`, `role=dialog aria-modal=true tabindex=-1` (with `sr-only` headings for label association, e.g. `id=getting-started-guide-dialog-label`), `role=progressbar aria-valuemin=0`, `aria-expanded` toggled imperatively on header controls, `aria-hidden=true` on decorative icons/dots, `sr-only` labels for every form field (`<label class=sr-only for=openrouter-api-key>`), `aria-label` on icon-only buttons (`aria-label="Close issues panel"`, `aria-label="Toggle Later sidebar"`). No `aria-checked`, `aria-selected`, `role=tab`/`tablist`, or `role=switch` anywhere — there are **no tab or switch widgets** in this build.

---

## 8. Commands run

All commands were executed from `/Users/lordcasser/workspace/projects/goal/analysis/evidence/web/` (read-only). No files were created or modified except the report itself.

```bash
# inventory
ls -la && wc -c *.css *.js

# palette
grep -o -- '--[a-zA-Z0-9_-]*' _assets_index-Bk3EgsqR.css | sort | uniq -c | sort -rn
python3 -c "…re.findall(r'#[0-9a-fA-F]{3,8}\b', css)…"          # hex frequency (91 distinct)
python3 -c "…re.findall(r'rgba?\([^)]*\)', css)…"                 # rgb/rgba frequency (54 distinct)
grep -o -E '#bc4721|#514266|188, 71, 33|81, 66, 102|…' _assets_index-Bk3EgsqR.css
python3 - # brace-aware rule parser -> per-class color reverse map   (palette table, §1.2–1.4)
python3 - # all `.bg-*/.text-*/.border-*/.ring-*/.fill-*` decls incl. `!important` and `disabled:` variants (§1.3, amber-200)
grep -o -E '\.[a-z-]*\\:border-amber-200[^{]*\{[^}]*\}' _assets_index-Bk3EgsqR.css
grep -o -E '.{0,40}gold-300[^{]*\{[^}]*\}' _assets_index-Bk3EgsqR.css
grep -o -E '.{150}--app-header-height.{150}' _assets_index-Bk3EgsqR.css
grep -c 'prefers-color-scheme' ; grep -o -E '.{80}\[data-theme.{160}'   # confirms no dark theme

# typography
python3 - # @font-face extraction (3 faces) §2.1
python3 - # `.text-*`/`.font-*` custom typography class bodies §2.2
grep -o -E 'letter-spacing:[^;}]*' / 'text-transform:[^;}]*' / 'font-weight:[^;}]*'

# layout / components
python3 - # class-token regex over the whole sheet -> 655 distinct utilities, grouped by prefix (§3.1–3.2)
python3 - # custom-layer isolation from `:root{--app-header-height:40px}` to EOF (§4)
python3 - # per-file rule dump with truncation (DismissibleHint / AgentConversation / LaterSidebar)
grep -o -E '.{110}has-\[:checked\].{110}' / 'type=text' / 'focus:ring-amber-500' / 'resize-'
python3 - # keyframes / transition / animation / cursor / outline / z-index / radius / border-width census
grep -o -E '@media[^{]{0,120}' _assets_index-Bk3EgsqR.css | sort | uniq -c | sort -rn

# JS cross-reference
python3 - # extract `class=` / `classList=` string literals from all 13 bundles -> token frequency
python3 - # structural template chunks (`aside`/`header`/`role=`) for shell geometry
grep -o -E ".{140}bg-\[\#F0F0F0\].{160}" *.js
grep -o -E ".{70}role=tab|tablist|aria-checked|role=switch.{80}" _assets_index-BZ_MKiTS.js   # (none found)
grep -o -E 'background-image:url\(data:image/svg\+xml;base64…\)' … | base64 -d    # checkbox check glyph

# contrast
python3 - # WCAG relative-luminance ratio computation over 31 foreground/background pairs (§7.1)
```

## 9. Evidence limits

- **Minified, single-line CSS.** All extraction used brace matching; no declaration can be attributed to a source file/line, and the original authoring order (Tailwind `@layer` boundaries) is flattened away — "custom component layer" (§4) is a heuristic based on byte offset 48,000 in the index sheet.
- **JS class strings are static only.** Token frequencies come from literal `class="…"` / `classList` string arguments; dynamically composed classes (`classList.toggle`, ternaries, template interpolation) are undercounted. Frequency tables are therefore a lower bound on usage, though the *presence* of a class is reliable.
- **Tailwind config not available.** The theme extension can only be *inferred* from emitted values. Palette step names (`gray-25`, `amber-500`, …) are read from the class names, which is safe; the mapping from those names to a config source and any unused steps is not observable.
- **No screenshots/computed styles in the original token audit.** Later captures are indexed in 31 and website comparisons in 33; they do not constitute a full cascade or contrast audit. This remains a static stylesheet analysis; no rendered DOM, no cascade resolution against real elements, no verification of which of the two focus languages wins on a given node, and no measurement of real hit areas or computed contrast with alpha/`color-mix`/opacity applied.
- **`bg-gray-25` as "the app canvas"** rests on the markup `flex-1 bg-gray-25 overflow-hidden flex flex-col`, not on a screenshot; Tauri window background (outside the web layer) is not in evidence.
- **Modal/popover inventory is incomplete by nature** — only components whose classes were purged-in and found are listed. There is no `tab`, `switch`, `skeleton`, `badge`, `avatar`, `kbd`, `tooltip` (other than tippy), `accordion`, `combobox` or `toast` *component* in this stylesheet; toasts exist only as keyframes (`toastIn`/`toastOut`) with no corresponding rules in these four CSS files.
- **`.button-sm-icon` is referenced in `_assets_Agent-BkuKkIG4.js` but does not exist in any of the four CSS files** (the real class is `.button-icon-sm`). Either a dead/inert class or it is styled by a stylesheet not present in this extraction set.
- **No `prefers-contrast`, `forced-colors`, or `print` handling** appears anywhere.
- The woff2 files were not parsed: variable-axis ranges beyond the declared `font-weight: 100 900`, and the availability of an italic or optical-size axis, are unverified.
- `#F0F0F0`, `#364045`, `#a4b2b7`, `#ebeff0`, `#fbf4e6` and the task-preview values are asserted as "off-palette" on the basis of their absence from every Tailwind-scale class; they may correspond to palette steps that only ever appear via arbitrary-value utilities.

# 前端 UX 交互拆解

> 官网对照补充：见 [33-website-ux-ui-review.md](33-website-ux-ui-review.md)。官网有指定任务的默认连线和预设对话，不等于实际编辑/计时/AI 已运行；官网与应用的同名间距类取值不同。用户已确认在自有产品增加点击条目持续查看关联，需求见 UXR-033；本文的原产品静态路径不承担该新增行为。

> 2026-09-14 复核：本报告保留静态拆解过程。连线实际是关注对象的直接关系揭示；任务编辑器拖拽与专注块排序需分别判断。仅凭 vendor 样式或配置不能证明具体菜单采用某个平台 API，也不能坐实作者动机。重做时以 [实际使用与证据](31-product-use-and-evidence.md) 和 [待决策表](32-rebuild-decisions-and-gaps.md) 核对边界。

> 对象：从 hyperfocus 0.15.0 二进制中还原出的前端产物（`analysis/evidence/web/`，19 个文件）
> 目标：把「交互怎么做的」拆到可直接对照重建的粒度——产物怎么切、状态放哪、层怎么叠、动效参数是什么、可访问性怎么处理。
> 相关：设计 token 全表见 `design-system.md`；交互设计意图见 `00-technical-report.md` §0x03。

**方法说明**：本文结论来自前端产物本身（chunk 划分、DOM 模板、data-* 属性、aria 清单、库配置、keyframes），不是从 UI 观感反推。凡属推断的都标了「推断」。

---

## 0x00 产物怎么切

Vite 按入口模块命名 chunk，所以文件名本身透露了模块的**第一个模块**而不是它的职责。真实职责要从它的导入与调用内容判断：

| 文件 | 大小 | 模板片段 | 实际职责 |
| --- | --- | --- | --- |
| `index-BZ_MKiTS.js` | 206 KB | 594 | 主入口。工作台与周期列、任务列表、设置面板、引导清单、退出调查、应用菜单、Hint 系统 |
| `DismissibleHint-DA_pck3w.js` | 122 KB | 36 | **任务条目交互层**：预览态 Keep/Revert、拖拽、跨层链接可视化、行内控件 |
| `AgentConversation-BH0-xy3l.js` | 102 KB | 36 | agent 会话 UI + **API client 主体**（周期、引导、设置、AI 配置的命令封装都从这里导出） |
| `editor-tiptap-BwIvA7sR.js` | 108 KB | — | TipTap 封装 |
| `editor-prosemirror-woe0hEPc.js` | 216 KB | — | ProseMirror 内核 |
| `vendor-ui-C8LHtCC1.js` | 35 KB | — | UI 原语：popover/tooltip 封装、按钮、状态动画 |
| `vendor-solid-l_e3XZcw.js` | 15 KB | — | SolidJS 运行时 |
| `close-B3887pRn.js` | 11 KB | — | 共享模块（首个模块是 close 图标，实际承载任务/编辑器客户端与图标集） |
| `Agent-BkuKkIG4.js` | 5.8 KB | 24 | agent 面板的边角组件（"This planning page has ended." / "Cancel restore previous"） |
| `LaterSidebar-D4Ub6dP3.js` | 1.6 KB | 7 | Do Later 侧栏 |
| `index-Bk3EgsqR.css` | 65 KB | — | 设计系统 |

**拆解要点**：`DismissibleHint` 和 `AgentConversation` 的体积（122 KB / 102 KB）远超它们名字暗示的范围。文件名叫 `DismissibleHint`，但它装的是**整个任务条目的交互状态机**。这说明 Vite 的 chunk 命名不能当模块划分用——重建时应该按职责命名，不要沿用这种命名巧合。

---

## 0x01 交互状态是 DOM 属性，不是 JS 内部状态

这是这份拆解里最有价值的一条。应用把交互状态**暴露成 DOM 的 `data-*` 属性**，而不是藏在闭包里。完整清单：

### 实体身份

| 属性 | 出现 | 用途 |
| --- | --- | --- |
| `data-task-id` | 6 | 任务节点标识 |
| `data-parent-id` | 4 | 父子关系 |
| `data-session-id` | 4 | 专注块标识 |
| `data-type` | 5 | 节点类型 |
| `data-position` | 3 | 排序位置 |

### 预览态（提议机制）

| 属性 | 出现 | 用途 |
| --- | --- | --- |
| `data-is-preview` | 4 | 该行是待确认改动 |
| `data-preview-type` | 5 | 改动类型（新增 / 删除 / 修改） |
| `data-is-fake-first-task` | 5 | 空的首行占位（agent 新建目标时复用它，与后端 `is_fake_first_task` 对应） |
| `data-agent-updated-at` | 3 | 该行被 agent 改过的时间戳，用于徽标与动画 |
| `data-checked` | 4 | 完成状态 |

### 清晰度与着色

| 属性 | 出现 | 用途 |
| --- | --- | --- |
| `data-needs-refinement` | 2 | 目标表述待澄清 |
| `data-needs-breakdown` | 2 | 待拆解 |
| `data-clarity-level` | 1 | context 高低（驱动澄清提示的显隐） |
| `data-root-color-key` | 4 | 长目标色系 |
| `data-link-color-key` | 4 | 下层任务继承的链接色 |

### 链接可视化

| 属性 | 出现 | 用途 |
| --- | --- | --- |
| `data-task-link-connector` | 1 | 跨层链接的连接线元素 |
| `data-task-link-visualization-scope` | 1 | 可视化范围容器（悬停时高亮整条链路） |

### 拖拽

| 属性 | 出现 | 用途 |
| --- | --- | --- |
| `data-session-dnd-ignore` | 5 | 拖拽排除区（专注块内的控件不能触发拖动） |
| `data-drag-copy-parent-link-intent` | 1 | 拖动时携带「同时建立父链接」的意图 |

### 菜单与浮层

| 属性 | 出现 | 用途 |
| --- | --- | --- |
| `data-menu-id` | 2 | 菜单实例标识 |
| `data-task-menu-active` | 1 | 任务菜单展开态 |
| `data-task-parent-link-menu` | 1 | 父链接菜单 |
| `data-task-root-color-menu` | 1 | 颜色菜单 |
| `data-delete-planning-cycle-action` | 1 | 周期删除动作 |
| `data-placement` / `data-popper-*` | — | tippy/popper 定位状态 |

### 生命周期与过渡

| 属性 | 出现 | 用途 |
| --- | --- | --- |
| `data-visible` | 3 | 可见性（驱动显隐过渡） |
| `data-state` | 3 | 组件状态机 |
| `data-exiting` | 1 | 卸载前的退出动画 |
| `data-animation` / `data-inertia` / `data-split` | 2 / 2 / 2 | 参数化动效 |
| `data-animation` | 2 | 动画开关 |

### 遮罩层

每个浮层配一个 backdrop 元素：`data-app-content-backdrop-surface`、`data-feedback-backdrop`、`data-getting-started-guide-backdrop`、`data-header-menu-backdrop`。**backdrop 由状态驱动而非手动挂载**，负责点外关闭。

### 其他

| 属性 | 用途 |
| --- | --- |
| `data-tauri-drag-region` × 8 | 无边框窗口的可拖拽区域（自绘标题栏） |
| `data-planning-task-viewport` × 3 | 任务列表的滚动视口（虚拟化/自动滚动的边界） |
| `data-migrated-from-task-id` | 旧数据迁移血缘（过渡期遗留，重建可省） |
| `data-migrations-count` | 数据库迁移数量（诊断展示） |
| `data-maxlength` | 输入长度约束 |
| `data-testid` × 4 | 测试钩子（很少，说明测试以 data-* 语义属性为主） |

**为什么这样做值得学**：

1. 交互状态可被 CSS 直接消费（`[data-exiting]`、`[data-visible]`、`[data-state]`），不需要 JS 在类名和状态之间同步。
2. 端到端测试可以用稳定语义选择器，而不是脆弱的 CSS 路径。
3. 状态在 DOM 上可见，调试时不用打开 devtools 的组件树。
4. 属性名即契约——`data-is-preview` 的语义和后端 `agent_proposal` 字段一一对应。

**重建时的取舍**：不必照抄属性名，但**同类做法值得保留**。需要注意的代价：DOM 属性是全局命名空间，属性多了以后要有一套命名规范，否则会失控。

---

## 0x02 浮层体系：优先用平台能力

| 机制 | 用途 | 证据 |
| --- | --- | --- |
| **原生 `popover` API** | 下拉菜单、颜色选择、父链接选择 | CSS 里出现 `:popover-open` 选择器 |
| **原生 `:modal`** | 对话框（周期选择、删除确认） | CSS 里出现 `:modal` 选择器 |
| **tippy.js（仅作定位引擎）** | tooltip 与跟随式浮层 | 配置 `{allowHTML:false, animation:"fade", arrow:true, inertia:false, maxWidth:350, role:"tooltip", zIndex:9999}` |
| **自建 backdrop** | 点外关闭、模态遮罩 | 每个浮层配一个 `data-*-backdrop` 元素 |

**关键取舍：tippy 只用来算位置，不用来渲染内容。** `allowHTML: false` 说明他们刻意不让 tooltip 承载富内容——避免 XSS 面，也避免 tooltip 变成第二个 UI 层。真正的菜单用平台 `popover` 做，因为 `popover` 自带 light-dismiss、层级提升（top layer，不受 z-index 与 overflow 影响）和无障碍语义。

**z-index 分层**（来自设计系统）：`0/1/2/5/10/20/30/40/50/90/100`。90 是模态遮罩（`bg-gray-950/40`），100 是最高层提示。

---

## 0x03 拖拽：SortableJS + FLIP 动画

配置与行为证据：

- 库：**SortableJS**（`animationTime`、`prevFromRect`/`fromRect`/`prevToRect` 等内部字段出现在产物里）
- 动画：FLIP 位移补偿。距离公式出现在产物里：`sqrt((top-top)² + (left-left)²) / sqrt((top-top)² + (left-left)²) × animation`——按位移距离缩放动画时长，短距离快速完成，长距离慢一点
- **排除区**：`data-session-dnd-ignore` 标记不可拖拽的元素（专注块内的按钮/控件），避免误触
- **意图携带**：`data-drag-copy-parent-link-intent`——把长目标拖进周计划时，拖动过程本身就携带「建立父子链接」的意图，而不需要事后单独操作
- **落点指示**：`task-drop-indicator` 类名 + 拖拽中给根元素加 `dragging-task-item`
- 自动滚动：SortableJS 的 `ancestorScroll` 配置用于长列表拖到边缘时滚动

**重建要点**：拖拽是这类应用里最容易做坏的一环。值得保留的三条：排除区、意图携带、FLIP 距离补偿。第三条决定「拖起来是否跟手」。

---

## 0x04 可访问性：不是补丁，是基础设施

`aria-*` 属性出现次数：

```
aria-hidden 33 · aria-label 27 · aria-labelledby 15 · aria-live 14
aria-expanded 8 · aria-modal 7 · aria-busy 7 · aria-haspopup 6
aria-describedby 6 · aria-atomic 3 · aria-disabled 2 · aria-current 2
aria-controls 2 · aria-valuenow/min/max 各 1 · aria-invalid 1
```

值得注意的几条：

1. **`aria-live` 用了 14 处**。这是流式 agent 输出的正确做法：模型逐字产出时，屏幕阅读器需要被主动告知内容变化。`aria-atomic` 与 `aria-busy` 配合表示"这块内容正在生成，完成后整体播报"。
2. **`aria-valuenow/min/max`** 用在顶栏的引导进度环上——进度是真实的可访问控件，不是一张图。
3. **`aria-haspopup` + `aria-expanded`** 用在菜单触发上，配合原生 `popover` 的语义。
4. **`aria-modal`** 用在对话框上。
5. 焦点样式统一为 `focus-visible:outline-1 outline-gray-700`（1px 实线环）或表单的 `amber-500` 边框 + ring。

**已知缺口**：`motion-reduce` 覆盖了入场动画与模态，但**没有覆盖无限循环的 agent 指示器**（`clarity-pulse`、加载器、语音条）。对前庭功能敏感的用户，这些是持续的动效源。重建时应当补上。

---

## 0x05 动效

| 项 | 取值 | 用途 |
| --- | --- | --- |
| 通用过渡 | `.15s cubic-bezier(.4,0,.2,1)` | 绝大多数 hover/状态变化 |
| 引导流程 | `cubic-bezier(.22,1,.36,1)` | 更慢、更有"落定感" |
| 模态入场 | `fadeIn .16s` / `fadeInUp .22s` | 遮罩淡入 + 面板上浮 |
| 拖拽 | FLIP，时长随位移距离缩放 | 列表重排 |
| 语音条 | CSS 变量 `--voice-bar-scale` + `animation-delay` | 按音频幅度驱动条高 |
| 光标 | `ProseMirror-cursor-blink 1.1s steps(2,start) infinite` | 富文本光标 |

产物里共出现 **18 个 keyframes**（完整清单在 `design-system.md`）。动效采用**属性驱动**：`data-animation`、`data-inertia`、`data-split`、`data-exiting`、`data-visible`、`data-state` 控制参数与阶段，而不是在 JS 里算好类名。

---

## 0x06 数据流与状态归属

前端的客户端模块按域分组，命令封装集中在一处：

```js
// AgentConversation chunk 里导出的 API client（节选，参数名照抄）
getPlan:               (selection=null) => invoke(`get_plan`, { selection })
updateCycle:           (cycleId, params) => invoke(`update_cycle`, { cycle_id: cycleId, params })
createPlanningCycle:   (input) => invoke(`create_planning_cycle`, { input })
finishSessionCycle:    (id) => invoke(`finish_cycle`, { cycle_id: id })
addSession:            (parentId) => invoke(`add_session`, { parent_id: parentId })
reorderSessions:       (parentId, ids) => invoke(`reorder_sessions`, { parent_id: parentId, ordered_session_ids: ids })
...
setAiProvider:         (provider) => invoke(`set_ai_provider`, { provider })
```

要点：

1. **参数名是 `snake_case`**，和 Rust 侧字段一一对应，中间没有转换层。重建时如果前端用 camelCase，就需要一层显式映射（不算坏事，但要意识到这是一处新增的契约转换点）。
2. **命令名即函数名**（`createPlanningCycle` ↔ `create_planning_cycle`），一一对应，容易核对。
3. **事件以 `listen()` 订阅**，确认存在的三个：`cycle:finished`、`tasks:patched`、`agent:conversation_updated`。事件驱动局部刷新。
4. **服务端（Rust）是唯一真相源**；前端拿事件后重新拉取，不自己算派生状态。这一点和交互上的 `data-*` 属性并不矛盾——属性表达的是**渲染状态**，不是数据副本。

**未验证**：是否对拖拽排序做乐观更新。产物里没有直接证据，从 FLIP 动画的存在推断拖动过程是本地先行渲染的（否则会卡）。

---

## 0x07 空状态与文案

空状态是固定四件套（模板里反复出现）：

```
虚线框（border-dashed border-gray-300, min-h-[120px]）
  └─ 线性图标（24px 描边风格）
  └─ 大写标题（text-heading-2 uppercase）
  └─ 一句用途说明（body 字号，灰色）
  └─ 一个主按钮（button-sm primary）
```

容器内层限宽 `max-w-[366px]`，所以文字不会拉成一行。

文案语气（382 条里的一致特征）：

- 直陈、不客气：`"Past cycles can't be deleted."`、`"Finish other focus block first"`
- 给边界不给要求：`"Aim for 1-3 clear outcomes"`、`"Aim for up to 5 tasks"`
- 允许不完美：`"You can start messy, we'll clarify things as we go."`
- 错误文案带下一步：`"Included AI access has ended. Connect AI provider in Settings to continue."`（说清是什么 + 去哪解决）

---

## 0x08 对照前端特性的拆解结论

把上面的事实归纳成判断，方便重建时取舍：

**由平台特性驱动的（重建时必须遵守）**

1. WKWebView 里没有 Chromium 的私有 API，所以浮层优先用原生 `popover` / `<dialog>` —— 这两个 API 在 Safari 16+ 可用，且自带 top-layer 与 light-dismiss，比自己实现 z-index 管理可靠。
2. `data-tauri-drag-region` 是 Tauri 无边框窗口的必需契约，自绘标题栏绕不开。
3. `aria-live` 对流式输出不是可选项——没有它，屏幕阅读器用户听不到 agent 的回复。

**由设计选择驱动的（重建时可以改）**

1. 交互状态放 DOM 属性 —— 好维护，但属性命名需要规范。
2. SolidJS 细粒度响应式 —— 在 React 里对应的是"状态尽量局部化 + 避免整树重渲染"，实现手段不同，目标一致。
3. SortableJS + FLIP —— 用 dnd-kit 之类也能达到同样手感，关键是保留距离补偿与排除区。
4. tippy 只做定位 —— 换成 Floating UI 或纯 CSS anchor positioning 都行。

**容易被忽略但影响体验的**

1. **hover 才出现的行内控件**（`task-item-controls` 默认 `opacity:0; pointer-events:none`，`li:hover` 时显现）——静态时画面干净，但键盘用户需要 `:focus-within` 也能显现，否则键盘用户看不到控件。
2. **`data-session-dnd-ignore` 这类排除区**——不做的话专注块上的按钮会把拖动搞得很难用。
3. **`data-exiting`**——先从 DOM 上标记退出，动画结束后再卸载。少了这一步，关闭动作会跳变。
4. **进度环用真实 ARIA 进度语义**，而不是画一张 SVG。

---

## 0x09 重建落地清单

如果把这份拆解交给一个前端重建任务，可直接用的清单：

**组件（按出现频次与耦合度排序）**

| 组件 | 关键交互契约 |
| --- | --- |
| 周期列 | 列头（标题/剩余时间/选项菜单）、横向滚动容器、空状态 |
| 任务条目 | `data-task-id` / `data-is-preview` / `data-checked` / `data-needs-*`；hover 显露控件；预览态高亮；Keep/Revert 行内动作 |
| 待确认底栏 | `You have N pending edits by agent` + Keep / Revert / Keep all / Undo all |
| Agent 侧栏 | 消息区（用户右对齐带底 / 模型靠左 / 状态行带图标）、流式区带 `aria-live`、候选回答带 `⌘1..n`、输入区 |
| 时长选择弹窗 | 三选项 + 右侧实时时间线 |
| Do Later 抽屉 | 单输入框 + 一次性说明卡 |
| 引导清单 | 五步 + 进度环（ARIA 进度语义）+ Skip |
| Hint 卡 | 标题 + 一句说明 + 关闭，关闭状态持久化 |
| 菜单 / Popover | 原生 `popover` + backdrop，`aria-haspopup` / `aria-expanded` |
| 对话框 | 原生 `:modal`，`aria-modal` |

**必须实现的交互状态（否则体验会明显退化）**

1. 预览态三态：无预览 / 有预览待确认 / 预览已应用
2. 拖拽六态：idle / dragging / over-target / 排除区 / 落点指示 / FLIP 收尾
3. 流式三态：生成中（`aria-busy`）/ 增量播报 / 完成
4. 浮层四态：closed / opening / open / exiting

**重建时应当补的缺口**

1. `prefers-reduced-motion` 覆盖所有无限循环动效（原实现没做）。
2. hover 显露的控件同时支持 `:focus-within` 显露（原实现只做了 hover）。
3. 弱化文本的低对比度取舍（2.10 / 2.11）要么接受并写进测试快照，要么提到 AA——原实现是刻意选的，重建时要显式决定而不是默认继承。

---

## 附：证据位置

| 内容 | 文件 |
| --- | --- |
| 153 条 DOM 模板骨架（class 完整保留） | `analysis/evidence/ui/templates.html` |
| 382 条用户可见文案（按 chunk 分组） | `analysis/evidence/ui/copy.txt` |
| 设计 token 全表（颜色/字阶/间距/动效/组件签名） | `analysis/reports/design-system.md` |
| 还原出的 JS/CSS 产物 | `analysis/evidence/web/` |
| 提取脚本 | `analysis/scripts/extract_ui.py` |

# 官网分析与「目标拆解链接」UX

> 2026-09-14 再复核：本文保留官网演示机制的推导，已修正缩进、锚点颜色、商业与服务端动机的过度推断。完整 UX/UI 对照见 [33-website-ux-ui-review.md](33-website-ux-ui-review.md)；用户新增的点击持续查看关联见 [需求基线](30-core-ux-requirements.md) UXR-033。

> 来源：`https://hyperfocus.in/`（2026-09-14 抓取）
> 证据：`analysis/evidence/website/`（DOM 快照、55 KB 样式表、mock 截屏、全页截屏）
> 目标：把官网当作产品的**自我陈述**来读——它比二进制更能说明"这是给谁做的、解决什么、怎么用"。

---

## 0x00 官网是什么形态

官网不是静态宣传页，而是**把产品界面用 HTML/CSS 重实现了一遍**当作 hero 演示。这一点很关键：

| 事实 | 证据 |
| --- | --- |
| hero 里是一个可交互的规划板复刻 | DOM 里有 `.hero-plan-mock-shell`、`.current-plan-board`、`.current-planning-column`，宽 2580px（超出视口，可横向滚） |
| 样式是完整的一套，不是截图 | `main-CufOTVcg.css` 里 674 行 mock 专属规则（`.current-*` / `.mock-*` / `.hero-plan-*`） |
| 字体家族相近 | 正文及演示使用 IBM Plex Sans，Mono 在局部使用；应用声明名与间距取值不同，见 33 报告 |
| 连线是**真实绘制的 SVG** | `.current-task-link-connector-overlay` 里有 2 个 `<g>`，各含 1 条 `<path>` + 2 个 `<rect>` |

也就是说，官网既是营销材料，**也是一份可读的实现参考**。下面第 0x03 节就是从这里反推出的连线算法。

---

## 0x01 定位：一份明确的反向宣言

官网的措辞比应用内文案更锋利，因为它要说服而不是服务。

**一句话定位**：`Free planner that turns goals into daily progress.`
meta description：`a goals-first planner that helps you choose what matters most, shape weeks and days around it, and move it forward with focused work.`

**核心论点**（写在作者自述里，标题是 "Task managers got it backward."）：

> 它们都承诺同一件事——只要你收集更多、整理更多、跟上更多，你就能把一切控制住。
> **我认为这是反的。**
> 当一切都从收件箱开始时，你是在对事情做反应，而不是把时间花在真正重要的目标上。你整理东西、处理东西、在东西之间跳来跳去。你感觉很忙，但不总是有产出。
> 还有另一种工作方式。用 Hyperfocus，你从"未来几个月我想达成什么"开始。回答这个问题需要时间。但当你真的有了答案，其余的事会各就各位。

**对照拆解结论**：这段话精确解释了为什么数据模型是「周期 → 目标 → 计划」而不是「inbox → 项目 → 任务」。**官网没有把收件箱作为规划起点**，Do Later 是唯一的收纳口，而且它挂在长周期体系上（`type='month'` 的 Later 容器），不是独立的 inbox 表。这个 schema 与定位相容，但不能据此确认原作者的设计因果，也不要求自有实现沿用该容器编码。

**目标受众的自我描述**（这段来自应用内，与官网一致）：作者本人用自己产品管理"发布 Hyperfocus"和"跑完 Cappadocia Ultra 超马"，这两件事都出现在官网的演示数据里。

---

## 0x02 方法论：四步，写在官网上

官网标题 `A method that puts goals first.`，下面是一条四步流程（`aria-label="Hyperfocus planning flow"`）：

```
flag          view_week      today         timer
Goals    →    Weeks     →    Days     →    Focus blocks
```

四个图标名（`flag` / `view_week` / `today` / `timer`）与产品里的四个层级一一对应，**且和应用内是同一套 Material Symbols 图标**。

这条流程是产品对外承诺的全部：不问"你有什么任务"，而是从 Goal 开始逐层收窄到 Focus block。

**四个功能卖点**（官网四张卡片，标题即价值主张）：

| 标题 | 原文 | 对应的规范 |
| --- | --- | --- |
| Your plans, connected to what matters. | "Stay focused on what matters most by connecting long-term goals to your weekly and daily plans in one workspace." | `task-graph` 跨层目标链接 + 本文 0x03 节的可视化 |
| Second opinion on your goals and plans. | "Hyperfocus AI reviews your goals and plans **as you write**, flagging vague outcomes, unrealistic scope, and missing next steps. Apply the feedback yourself – or work through it with an assistant built for goal-setting, prioritization, and planning." | `planning-issues`（实时审查）+ `agent-conversation` / `goal-clarification` / `prioritization` |
| Your next step, always clear. | "A quick weekly and daily planning ritual narrows everything down to one clear next step – so you can spend less time deciding, and more time doing." | `planner-workspace` 周/日规划 |
| Turn plans into progress, one block at a time. | "Make progress with time-boxed sessions where you focus on one task at a time." | `planning-cycles` 的 session 层 |

**值得注意的定位差异**：官网把「AI 实时审查计划」放在第一位，把「AI 助手对话」放在第二位。而拆解二进制时，我看到的是大量 agent 会话与技能代码。

这支持把随写审查视为重要入口；助手是进一步处理问题的另一条路径。功能排序不能证明差异化强弱或用户研究结论。

这条对扩展方向的启示：`planning-issues`（计划健康检查）不是附属功能，它是核心卖点，值得在重建时优先做扎实。

---

## 0x03 目标拆解链接的 UX（重点）

这是你特别点出的部分。官网 hero 的演示数据完整展示了这条链路。

### 演示数据揭示的层级链

```
LONG-TERM GOALS          Jun 22 – Sep 13 · 11 weeks left
├─ 🔵 Launch Hyperfocus on Product Hunt in top3
│    ├─ Validate onboarding with five beta users
│    ├─ Ship the launch-ready macOS app
│    ├─ Publish the new landing page and demo
│    ├─ Prepare the Product Hunt launch (hunter, supporters, assets)
│    ├─ Launch & engage with users
│    └─ Track 200 downloads and 20 paying customers
├─ 🟠 Finish Cappadocia Ultra under 7 hours
│    ├─ Finalize the training plan with a coach
│    ├─ Complete 40k test race
│    ├─ Validate pacing, fueling, and equipment
│    └─ Arrive healthy, recovered and ready
└─ 🟣 Publish 12 essays by Dec 31
     ├─ Focus is not about managing more
     ├─ What games teaches us about experience
     └─ Why focus still matters in AI world
                                    ● 1 issue

WEEKS                    W29 · 4 days left
├─ 🔵 Create a product hunt launch checklist
│    ├─ Research best practices
│    ├─ Chat w/ Anton, Rahul, James on launches
│    └─ Create a launch checklist
├─ 🔵 Update landing page with product video     ←── 与上面的长目标连了线
│    ├─ Finalize hero & feature copy
│    ├─ Record product video
│    ├─ Run 3 quick tests
│    └─ Publish
├─ 🟣 Publish "Focus is not about managing more"
└─ ⚪ Order present for mom's birthday            ←── 未链接；官网仍有中性占位色槽

DAYS                     Wed · 10 hours left
├─ Write 1000 words for essay
├─ Finalize hero & feature copy
│    ├─ Hero screenshot
│    ├─ Method
│    ├─ Feature 1: long-term goals
│    ├─ Feature 2: weekly & daily plans
│    ├─ Feature 3: Focus blocks
│    └─ Manifest
└─ Pay the rent to Mariam

FOCUS BLOCKS             90 min · 已跑 37:38
├─ Finalize copy for landing page
├─ Update hero screenshot
├─ Add old way vs hyperfocus way
├─ Align features copy with new value props
└─ Afternoon focus   90/90 min
```

**一条可以逐级追踪的真实链路**（这就是产品的全部论点）：

```
Launch Hyperfocus on Product Hunt in top3   （长周期目标）
   └─→ Update landing page with product video   （周计划·W29）
          └─→ Finalize hero & feature copy      （周三计划）
                 └─→ Finalize copy for landing page   （专注块内的任务）
```

这是四阶段的语义链。官网实际绘制的目标边覆盖长期→周、周→日；日行动怎样进入专注块不能仅凭标题相似确定，仍见 Q04。

### 三个演示数据的细节设计

1. **三个长目标是不同领域**：工作（发布产品）、身体（跑超马）、表达（写文章）。这是在说"这个工具不只用于工作"。
2. **周计划里有未链接的普通事务**。演示使用中性色槽，既没有目标归属色，也没有目标连线。这支持允许普通事务存在，不强迫所有内容都挂在长期目标下。
3. **长周期列底部有 `● 1 issue`**。演示数据里也有一个计划问题，说明"计划会被审查出问题"是常态而非异常。

### 连线可视化的实现（反推）

本节仅解释官网演示。当前 JS 用 `zeroStateTaskId: w-1` 作为无悬停、无键盘焦点时的默认对象；显示与该对象直接相连的边。官网的默认连线不等于桌面产品永久显示所有关系。

这是本节最有复用价值的部分。

**容器结构**

```html
<div class="current-plan-board-content">        <!-- width: max-content，横向可滚 -->
  <div class="current-plan-year-bar">2026</div>
  <div class="current-plan-columns">
    <div class="current-planning-column">        <!-- Cycles gutter + 长周期列 -->
    <div class="current-planning-column">        <!-- Weeks gutter + 周列 -->
    <div class="current-planning-column">        <!-- Days gutter + 日列 + 专注块列 -->
  </div>
  <svg class="current-task-link-connector-overlay">  <!-- 覆盖整个 board -->
</div>
```

```css
.current-plan-link-visualization-scope { position: relative; overflow: hidden; display: flex; }
.current-plan-board-content { width: max-content; min-width: 100%; position: relative; }

/* 连线层覆盖整个内容区，所以线可以跨列 */
.current-task-link-connector-overlay {
  position: absolute; inset: 0; width: 100%; height: 100%;
  z-index: 20; pointer-events: none; overflow: visible;
}

/* 线本身：1px 发丝，非缩放描边，颜色由 CSS 变量注入 */
.current-task-link-connector-line {
  fill: none;
  stroke: var(--task-link-visualization-color);
  stroke-linecap: round;
  stroke-width: 1px;
  vector-effect: non-scaling-stroke;
}

/* 端点：6x6 实心方块 */
.current-task-link-connector-square { fill: var(--task-link-visualization-color); }
```

**几何算法**（从真实 DOM 里读出的两个 `d` 属性）

```svg
<!-- 长周期行 → 周行（向下） -->
<path d="M 672.796875 151.5
         C 769.65625 151.5, 769.65625 273.09375,
           866.515625 273.09375"/>

<!-- 周行 → 日行（向上） -->
<path d="M 1327.515625 273.09375
         C 1419.19921875 273.09375, 1419.19921875 181.8984375,
           1510.8828125 181.8984375"/>
```

抽象出来就是：

```
起点  = (父行右边缘 x1, 父行垂直中心 y1)
终点  = (子行左边缘 x2, 子行垂直中心 y2)
xm    = (x1 + x2) / 2
路径  = M x1,y1  C xm,y1  xm,y2  x2,y2
```

两个控制点共用同一个 x（两端点的水平中点），水平方向出发、水平方向进入——标准的**对称 S 形三次贝塞尔**。上一条向下弯、下一条向上弯，用的是同一个公式，只是 y2 大于或小于 y1。

验证：第一条 `xm = (672.80 + 866.52) / 2 = 769.66` ✓；第二条 `xm = (1327.52 + 1510.88) / 2 = 1419.20` ✓。

端点方块用 `x = 起点x − 3, y = 起点y − 3`（6×6 居中）。

**颜色继承**

```css
.task-link-color-blue { --task-link-slot-color: #79b4ff; --task-link-slot-border-color: #007dd6; }
.task-link-color-red  { --task-link-slot-color: #fe987c; --task-link-slot-border-color: #e44a08; }
/* 连线和方形端点取浅色，行前窄色槽用深色描边 */
.task-link-color-blue, .task-link-color-red, ... {
  --task-link-visualization-color: var(--task-link-slot-color);
}
```

以下为部分关系调色值（浅色 = 连线及其端点，深色 = 行前色槽描边；完整应用八色见 design-system）：

| 色名 | 浅色（连线） | 深色（描边） |
| --- | --- | --- |
| red | `#fe987c` | `#e44a08` |
| gold | `#fbb961` | `#c08600` |
| green | `#b0c265` | `#6e9200` |
| cyan | `#63ccc0` | `#009791` |
| blue | `#79b4ff` | `#007dd6` |
| plum | `#c49bf3` | `#8c5ad3` |
| （另有 pink 等） | — | — |

样式表里深色还给了一份 `color(display-p3 ...)` 覆盖，说明**作者关心广色域屏上的色彩准确度**——这是很少见的细致处理。

**这套设计里三个值得抄的点**

1. **覆盖层跨列**：连线层不放在列内，而是覆盖整个内容区。这解决了"链接横跨两列，但两列各自 `overflow: hidden`"的经典冲突。代价是需要自己处理滚动同步。
2. **`vector-effect: non-scaling-stroke`**：不管容器怎么缩放，线宽恒为 1px。这与整个设计语言"细线分隔、无阴影"一致。
3. **连线与行前标记分工**：SVG 连线与方形端点同色；行前窄色槽使用浅色填充与深色描边。旧报告将色槽描边误认为连线锚点的深色，既有规范需要据此修正。

### 与应用内的对应关系

| 官网 mock | 应用内（二进制/数据库证据） |
| --- | --- |
| 任务前的窄色槽 | `tasks_table.root_color_key`（触发器强制只能长周期） |
| 连线颜色 | `--task-link-slot-color`，由 `link_color_key` 解析 |
| 连线层容器 | `data-task-link-visualization-scope`、`data-task-link-connector` |
| 行缩进层级 | 同周期步骤来自 `subtasks`；`parent_id` 表达跨周期服务关系，二者独立 |
| 列头 `11 weeks left` / `4 days left` / `10 hours left` | `cycles_table.starts_on` / `ends_on` / `duration` |
| `● 1 issue` | `planning_issue_report` + `planning_issue_dismissals` |
| 专注块 `90 min` / `37:38` / `Stop` | `cycles_table.duration` / `focused_time` / `started` |

**结论：官网 mock 与应用使用相近的视觉和领域词汇，但运行行为不能视为完全相同。** 它用于理解关系表达，实际操作契约仍需产品实测与应用静态路径支持。

---

## 0x04 商业模型：纠正一个此前的推断错误

FAQ 里有一条明确否定了我之前的推断：

> **Is Hyperfocus really free? What's included?**
> Yes. Hyperfocus is free forever. There is no Pro subscription, paid plan, or pack of AI credits to buy.

> **How does AI work? Is it also free?**
> All AI features use third-party AI providers. Hyperfocus includes AI for your **first 14 days**, so you can try every AI feature without setting anything up. After that, you can **connect your own OpenRouter account**, add a few dollars, and pay as you go. You pay the provider directly. **I don't take a cut or make money from your AI usage.**

**纠正**：我在技术报告和需求报告里写过"内置 AI 14 天试用 → 自带 OpenRouter key **或订阅**"。**"订阅"这一项是错的**。产品没有 Pro 档，规划器永久免费，AI 只有"内置 14 天"和"自带 OpenRouter"两种状态。

那二进制里 `entitlement` 令牌的 `status ∈ {trial, paid, blocked}` 怎么解释？

这些枚举只证明协议中存在这些状态。`paid`、`blocked` 的服务端触发规则尚未验证，不能推定它们只是预留位，也不能从字段名创造订阅功能。

**为什么这个纠正重要**：

1. 我据此写的 `ai-access` 规范里"授权与试用"的框架是对的，但如果按"订阅"去设计，会多造一个不存在的能力（计费、套餐、额度包）。**规范里不能出现未被确认的付费实体**。
2. 扩展方向里"本地模型支持"的动机要重新表述：它的价值**不是"绕过付费"**（本来就不收费），而是**离线可用 + 隐私完全自持 + 不依赖第三方账号**。
3. 对重建的启示：应明确首次使用、可用性、费用及数据去向。官网商业模式不决定我们怎样分配实现投入，也不自动确定本地模型优先级。

其余 FAQ 事实：

| 问题 | 答案要点 |
| --- | --- |
| Which OS? | 目前仅 macOS |
| 与任务管理器有何不同？ | "Most task managers **start with an inbox**… Hyperfocus **starts with your goals**." |
| 数据存哪？ | "**local-first**. Your goals, plans, and tasks are stored locally on your Mac, and the planner **works offline**. If you use AI, the parts of your plan relevant to your request may be sent to the third-party AI provider… For the included AI usage, requests are sent under a **no-logging policy**. If you connect your AI provider, that provider's logging and data-handling policy applies." |
| 联系方式 | `dmitri@hyperfocus.in`（FAQ）/ `dmts@hyperfocus.in`（页脚） |

官网对附带 AI 作出了不记录日志的声明。本轮未审计服务端；`workers.hyperfocus.in` 端点的存在不能证明其全部设计目的或声明已经落实。自有产品不能未经验证继承该承诺。

---

## 0x05 此前对照需求的差量记录

本节记录此前已执行的规范补充；这些规范仍有待修正的行为和视觉断言。当前差量与用户确认需求以 30/32/33 报告为准，本轮未自动同步 OpenSpec。

把官网读出来的东西对照现有规范，得到三处调整：

### 1. 需要补一条规范：跨层链接的可视化

现有 `task-graph` 规范写了链接的**数据**语义（`parent_id`、相邻层级限制、着色范围），但**没写可视化**。官网 mock 证明可视化是产品核心卖点（第一条功能卡片就是它），必须补。

已补入 `openspec/specs/planner-workspace/spec.md` 的「跨层链接可视化」需求。

### 2. `planning-issues` 的定位要上调

官网把它排为 AI 能力第一项，措辞是"as you write"（边写边审）。这意味着它是**实时/近实时**的，而不是我在规范里写的"打开问题面板时计算"。

现有规范场景：

> **WHEN** 用户重新打开问题面板 **THEN** 系统重新计算当前周期的问题集合

这与官网承诺的"像 Grammarly 一样边写边审"不一致。需要补一条实时审查的要求。

### 3. `ai-access` 的表述要去掉付费暗示

规范里"试用结束后的降级行为"框架正确，但需明确**不存在可购买的订阅**，避免重建时造出计划/额度包这类实体。

---

## 附：证据位置

| 内容 | 文件 |
| --- | --- |
| 渲染后的完整 DOM（含 mock 全结构与文案） | `analysis/evidence/website/site-index.html` |
| mock 的完整样式表（55 KB，674 行 mock 规则） | `analysis/evidence/website/site-mock-css.txt` |
| hero 规划板截屏（清晰看到连线与色标） | `analysis/evidence/website/hf-mock.png` |
| 全页截屏 | `analysis/evidence/website/hf-site-full.png` |
| 页面可访问性快照 | `analysis/evidence/website/page-*.yml` |

旧证据是渲染后 DOM/CSSOM。此次还保存了原始 HTML，包含外链 JS/CSS 和响应式图片；不能再说静态 HTML 没有样式入口。浏览器实际渲染、资源清单与哈希见 33 报告。

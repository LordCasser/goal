# Hyperfocus 原版交互对照

日期：2026-09-15  
对象：`/Applications/hyperfocus.app`，bundle id `com.hyperfocus.desktop`。本记录只描述原版 Hyperfocus，不包含 Planner UX 的实现结论。

## 方法与作用域

本轮先用 `cua.getApp('/Applications/hyperfocus.app')` 绑定原版窗口，再通过原生 GUI 读取 AX 树和截图。实际打开了颜色、父目标、日任务父目标、Getting started、Later、Issues、应用菜单、Settings 和 macOS View 菜单；没有选择颜色、链接/解除链接、勾选任务、Dismiss/Clarify issue、发送反馈或点击 `Improve with Agent`，因此没有主动改写原版业务数据，也没有把目标文本发送给 AI。

在空的日任务输入框按过 Enter 以读取编辑提示，界面出现了一个空的辅助行；没有输入任何文字。重新绑定原版窗口后该辅助行消失，原始目标、周目标和日期内容仍保持不变。Enter/Tab 观察因此只记录可见行为，不宣称产生了真实任务。

`analysis/evidence/gui-review-2026-09-15/original-*.png` 是原版 Hyperfocus GUI 证据的统一 PNG 副本；其中连线、Later、Issues、Settings 等画面来自仓库已有的原版 GUI 记录，本轮通过 CUA 重新核对了对应控件和页面状态。AX 原始文字仍可在 `analysis/evidence/ux-audit-2026-09-14/` 中按同名编号查阅。

## 1. 长期目标色槽与颜色含义

长期目标行的视觉顺序是：空的 checkbox → 很窄的竖向色槽 → 正文标题。当前目标 `Write a book draft` 的颜色菜单入口在行内，AX 名称为 `Change link color`。打开后浮层标题是 `Choose linked goal color`，候选为：

`Blue`（当前选中）、`Ochre`、`Violet`、`Olive`、`Teal`、`Rose`、`Amber`。

这个菜单把颜色命名为 linked goal color。已链接的周目标会显示与长期目标相同的色槽，因此色槽表达的是跨层关系的颜色身份；它与 checkbox 的完成状态分开。颜色选择是浮层内的可逆选择入口，当前核验没有点击其他颜色。

## 2. 周目标 → 长期目标

周目标行的父目标入口是行内 pop-up button。已有父目标时 AX 会显示 `Change parent goal`；打开后浮层标题为 `Link long-term goal`，当前候选 `Write a book draft` 带 selected/check 状态，底部另有 `Unlink long-term goal`。

因此关联和解除关联都在周目标行内完成，不需要进入独立页面。`original-week-link-menu.png` 保留了候选和解除入口的空间布局；`original-week-linked.png` 还显示了链接后的色槽和连线状态。

## 3. 日任务 → 周目标

日任务输入行的父目标入口同样是行内 pop-up button，未展开时 AX 名称为 `Link to parent goal`。打开后浮层标题变为 `Link short-term goal`，当前周目标 `Hello` 带 selected 状态。这里的 short-term goal 就是当前周目标，层级与周目标选择长期目标不同。

顶部 Getting started 清单也把该动作单独写成 `Connect daily task to weekly goal`。清单观察到 5 步中“Connect weekly goal to long-term goal”已完成，而“Connect daily task to weekly goal”未完成，说明这是原版明确区分的第二次跨层关联。

## 4. 色槽、正文和连线

已有原版 GUI 证据 `original-week-linked.png` 显示：当关联被悬停查看时，长期目标行和周目标行同时出现浅蓝高亮；两行之间出现蓝色水平连线，连线两端各有同色小方块，线色随关联色槽而来。

本轮通过 CUA 点击长期目标正文只进入该正文的文本编辑态，没有观察到点击后的持久关系选中或连线。原生 CUA 接口没有独立 hover/mouse-move 操作，所以本轮没有把“点击正文能锁定关系”写成原版行为；连线结论只引用已有的原版悬停截图和 AX 记录。

## 5. checkbox、正文、色槽/小点与状态

目标和任务行都把 checkbox 放在正文左侧，正文紧随色槽或中性占位标记。当前长期目标、周目标、日任务均观察到未勾选的空方框；Getting started 清单中已完成的步骤显示勾选方框，说明完成状态由 checkbox 表达。

卡片底部另有一个红橙色小点和 `1 issue` 文本。这个点不是任务完成状态，而是该周期的计划问题计数入口；点击后打开右侧 `Plan feedback` 面板。空的、未关联的输入行在正文前可见很淡的中性小标记/`0`，AX 没有为它提供状态名称，不能把它解释成“进行中”或“部分完成”；在当前证据里它与“没有 linked goal 色槽”的空态一起出现。

任务正文获得焦点时，行右侧可出现 `Improve with Agent`、`Do Later` 和关系 pop-up 等上下文动作；正文仍是可编辑区域。没有实际点击 AI 动作，也没有把它称为全局 Coach。

## 6. Enter / Tab 编辑提示

在当前已结束的 Monday 日计划空任务输入框聚焦后按 Enter，AX 出现辅助文本 `Make subtask with (⇥ Tab)`，视觉上出现下一行空 checkbox/输入行。没有输入文字，因此没有创建可验证的真实任务。

在该状态按 Tab，焦点移到 `Day cycle options`；提示仍可见，但没有形成子任务。重新绑定原版窗口后辅助空行消失。仓库中此前保存的原版 Later 实测还显示：Later 输入框按 Enter 后保留同列表下一行，下一行提示同样是 `Make subtask with (⇥ Tab)`；已有文字行再使用 Tab 才会形成缩进子任务。这两点属于原版编辑器提示，不应改写为层级关联选择。

## 7. Later、Coach、Issues、Workspace / Calendar

| 入口 | 原版实际形态 | 与页面切换的区别 |
| --- | --- | --- |
| Later | 顶部 `Toggle Later sidebar` 打开的左侧 `DO LATER` 抽屉，包含一个输入行；主工作台仍在右侧 | 辅助抽屉，不是路由页面；通过同一按钮关闭 |
| Issues | 各周期卡片底部 `1 issue` 打开的右侧 `Plan feedback` 抽屉，标题下显示 issue 数、说明、`Clarify`、`Dismiss`，顶部 `Close issues panel` | 当前周期的上下文面板，不是独立工作区页面；关闭后回到同一张计划板 |
| Coach | 当前原版没有全局 `Coach` 入口或 `Coach` 页面文字；目标/任务行有 `Improve with Agent` | 只能确认存在行级 AI 动作，未点击，不能据此宣称全局 Coach 的面板行为 |
| Workspace | 没有名为 Workspace 的独立入口；长期/周/日周期和 Focus Blocks 直接铺在同一横向计划工作台 | 计划板本身就是工作区 |
| Calendar | 应用菜单、原生 View 菜单和当前 AX 树都没有 Calendar 入口；View 菜单只显示 `Toggle Full Screen` | 日期通过周期标题、Weeks/Days 和当前选中日呈现；Focus Blocks 随日计划显示，没有独立日历页 |

应用菜单（右上角）实际只有 `Settings`、`Send feedback`、`Copy support ID`。因此 Later 和 Issues 应保持“可从工作台打开的辅助面板”语义，不能按 Settings 那样处理成全页路由。

## 8. Settings 与关闭路径

右上角应用菜单 → `Settings` 打开独立 Settings 页面。页面有 `Back to Plan` 返回按钮、`SETTINGS` 标题和 `AI ACCESS` 区块，说明文字为：`Hyperfocus includes AI for your first 14 days. After that – you can connect your own AI provider.`

AI ACCESS 下有两张单选卡：

- `Hyperfocus AI`：`Included for 14 days. No setup required.`，当前选中；
- `OpenRouter`：`Use OpenRouter account to pay for usage directly.`，当前未选中。

本轮只读取设置并点击 `Back to Plan` 关闭，未切换供应商。返回后计划工作台恢复，设置页没有额外保存/确认流程。

## 证据索引

- [原版工作台](../evidence/gui-review-2026-09-15/original-baseline.png)
- [Getting started 五步清单](../evidence/gui-review-2026-09-15/original-getting-started.png)
- [Later 左侧抽屉](../evidence/gui-review-2026-09-15/original-later-open.png)
- [Issues 右侧 Plan feedback 面板](../evidence/gui-review-2026-09-15/original-issues-panel.png)
- [周目标父目标菜单](../evidence/gui-review-2026-09-15/original-week-link-menu.png)
- [周目标关联后的色槽与悬停连线](../evidence/gui-review-2026-09-15/original-week-linked.png)
- [日计划与 Focus Blocks 布局](../evidence/gui-review-2026-09-15/original-day-focus.png)
- [Settings / AI ACCESS](../evidence/gui-review-2026-09-15/original-settings.png)

## 补充：专注块、周期结束/复盘、Later 归入计划

### Focus Blocks 与日任务

日计划卡在前，`FOCUS BLOCKS` 卡在同一天的右侧。已保存的原版画面中，Monday 计划有自己的日任务行（例如 `UX audit daily action`），Focus Blocks 则有独立的块标题、计划分钟数和块内行动输入行（例如 `Afternoon focus`、`90 min`、`Type your goal here...`）。块内行动行也有 checkbox 和正文，但没有显示日任务的 `Link to parent goal` 入口。

这说明专注块是执行时间容器，块内行动清单和日任务列表是两组相邻的编辑对象；原版画面没有把日任务自动复制成块内行动，也没有在本轮观察到两者的自动关系提示。`original-day-focus.png` 是该布局证据。

当前原版已有的 Monday 已结束，所以 AX 显示 `FOCUS BLOCKS 0 min`、`Afternoon focus`、`90 min`，`Start` 和 `Focus block options` 都是 disabled。没有点击 Start。

此前原版 GUI 记录中的未启动块菜单只打开查看过，选项为 `Repeat daily` 与 `Delete`；两项都没有点击。此前真实运行记录观察到块运行态显示 `Stop`、减少中的剩余时间和块内行动，停止后显示 `0 / 90 min`；这些是已有原版证据，不是本轮重新启动计时所得，因而本轮没有重复启动。

### 周期结束与复盘入口

当前日计划顶部有一条结束提示：`Day ended. Review to continue.`，日期显示 `Ended Sep 15`，旁边唯一的动作是 `+ New day`。点击结束提示文字不会打开页面或浮层；AX 中该提示也是普通 text，不是 button。`+ New day` 能继续创建新日，但属于写入操作，本轮没有点击。

日周期菜单打开后只显示：`Past days can’t be deleted.` 和 disabled `Delete`。周周期菜单只显示：`This week contains a past day, so it can’t be deleted.` 和 disabled `Delete`；长期周期菜单也有同样的 past day 删除保护。没有在这些菜单中看到 Review、Summary、Retrospective 或结果预览入口。

因此本轮可以确认结束态的保护文案、结束日布局和新日入口；没有安全、只读的复盘预览可打开。不要把 `Review to continue` 这条提示本身解释成已经存在的复盘页面；“结束周期后复盘”的完整流程仍标记为未验证。已有需求分析也明确指出当前 0.15.0 没有可直接证明的完整复盘模块。

### Later 归入计划

本轮打开 Later 抽屉时，AX 为 `DO LATER` 和一个空输入行，抽屉内没有已有条目，所以没有可安全观察的“从 Later 重新归入周/日计划”行内动作。长期目标、周目标和任务行上可以看到 `Do Later` 按钮；点击它会移动用户数据，本轮没有点击。

此前原版操作记录只验证过把计划项移入 Later 后的移除与短暂 `Undo later` 提示，没有在当前空 Later 状态下伪造条目，也没有把“Later → 计划”的目标周期选择、确认或撤销行为写成已验证。该方向保留为未验证项。

| 行为 | 本轮可观察到 | 本轮未做的写入动作 |
| --- | --- | --- |
| 查看 Focus Blocks | 块标题、分钟数、Start/Stop 形态、块选项和独立行动行；当前结束日为 disabled | 启动/停止计时、重复块、删除块 |
| 查看周期结束态 | `Day ended. Review to continue.`、`Ended Sep 15`、`+ New day`、删除保护菜单 | 新建日、手动结束周/月、任何复盘提交 |
| 查看 Later | 左侧 `DO LATER` 抽屉和空输入行；计划行有 `Do Later` 入口 | 移入 Later、从 Later 归回周/日计划、确认 Undo |

## 补充二：Day Plan 与 Focus Blocks 并排组合

### 只读基线

本次重新打开原版时，当前日切换到 `Tuesday`，状态为 `Today`，Focus Blocks 为空。1200×800 窗口中，白色工作台内板约从 x=104 到 x=1127（约 1023px）；`TUESDAY PLAN` 约占 510px，`FOCUS BLOCKS` 约占 513px。两张卡共用顶部和底部边界，中间只有一条竖向分隔线，形成近似 1:1 的左右组合。

空 Focus Blocks 卡的标题行右侧有 `+`，卡内是虚线边界、说明文字 `Create time-boxed sessions to focus deeply on specific tasks.` 和 `Add focus block` 按钮。日计划卡仍单独显示 checkbox、任务正文和 `Plan with AI`。

对日计划卡、空 Focus Blocks 卡和整个工作区分别执行只读滚轮下滚，当前内容量下都没有发生可见位移，也没有出现内部滚动条。这个数据量不足以证明有内容溢出时两张卡各自滚动；本次仅能确认当前空态/单行态没有独立滚动反馈。

### 添加一条测试专注块后的卡片

按照本轮授权，实际点击了唯一一次 `Add focus block`，没有填写标题或行动，也没有启动计时。原版立即创建一条默认测试块，标题为 `Afternoon focus`，时长为 `90 min`；该测试块保留在原版数据中，未删除。

添加后，右侧 Focus Blocks 卡仍保持原来的约 513px 宽度，内部出现约 462px 宽的块卡（左右各留约 24px 空间）。块卡顶行依次显示 `Start`、`Afternoon focus`、`90 min`，右侧有展开/更多操作图标；下一行是独立 checkbox 与 `Type your goal here...` 行。AX 当前将 Start、Focus block options 和块内输入标为 disabled，本轮没有尝试启动或修改它们。

已有原版并排截图可见同一结构：[original-focus-block-card.png](../evidence/gui-review-2026-09-15/original-focus-block-card.png)。该截图中的块有日任务文本，当前 CUA 创建的测试块则保持空行动行；两者用于确认结构，不把测试标题写入原版报告数据。

### 日任务到专注块的入口

点击/聚焦当前日任务行后，AX 仍只暴露 `Link to parent goal` 和 `Plan with AI`，没有 `Start focus block`、`Add to focus block` 或块选择器。添加块后，块内行动输入也只显示 checkbox 与正文输入，没有日任务父链接入口。

因此原版从日任务进入专注块没有可见的显式跳转或绑定入口；用户需要从同一日右侧 Focus Blocks 的 `+`/`Add focus block` 单独创建块。当前测试未输入行动文字，未启动计时，也未修改日任务。

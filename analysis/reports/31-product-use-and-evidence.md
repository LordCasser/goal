# hyperfocus 0.15.0 实际使用与证据复核

本轮把仓库已有结论与用户提供的安装包、运行中的应用交叉核对。结果用于 [核心 UX 需求](30-core-ux-requirements.md)，不是对现有实现的开发或重构。

## 0x00 样本身份与方法

| 项目 | 核对结果 |
| --- | --- |
| 安装包 | `/Users/lordcasser/Downloads/hyperfocus_0.15.0_mac_universal.dmg` |
| DMG SHA-256 | `7cc217dfc6748ec99816e2575dfc3c76a1d7d9e07d3454526df8617d5da4ec3c` |
| 包内应用 | `/Volumes/hyperfocus/hyperfocus.app`；版本 `0.15.0`；标识 `com.hyperfocus.desktop` |
| 实际操作的应用 | `/Applications/hyperfocus.app` |
| 两处二进制 SHA-256 | 均为 `4e4c483343fe2682bcf3678a44a418fb7c1baf0c0005bdb99a96e07e38fcf74e` |
| 前端原始证据 | 仓库 19 个资源均与包内 Brotli 解压内容逐字节相同 |
| 数据结构 | 运行时 SQLite 已安装 57 条迁移；本轮核对目标、周期、快照等结构 |
| 本轮记录 | 24 组操作/诊断记录，20 张实际界面截图；4 组仅保存 AX 文本 |

完整指纹与压缩块偏移见 [source-manifest.json](../evidence/ux-audit-2026-09-14/source-manifest.json)。资源校验方法：定位资源路径字节，读取紧邻压缩块，用 Node 标准库 `brotliDecompressSync` 解压，再与仓库资源逐字节比较。校验不是按 chunk 文件名猜测职责。

实际操作通过原生应用 UI 进行；数据库用于只读交叉核对、测试前备份及结束恢复。未通过直接写数据库伪造 AI 提议或成功会话。开始时数据库已有一个目标与若干周期，**本轮不是全新安装首启测试**。

测试使用 `UX audit` 合成任务，检查编辑、链接、执行、复制与收纳。结束后恢复测试前业务数据，并恢复本轮新增的引导完成记录。恢复核对见 [cleanup-result.json](../evidence/ux-audit-2026-09-14/cleanup-result.json) 和 [最终 AX](../evidence/ux-audit-2026-09-14/24-cleanup-verified.ax.txt)。完整真实数据备份留在系统临时目录，不进入仓库。

## 0x01 实际使用记录

下面编号用于核心需求的 `Oxx` 来源标记。每个文件链接都是本轮保存的材料；`.ax.txt` 提供文字、控件状态和焦点，`.jpg` 提供实际空间布局。

| 编号 | 操作与观察 | 证据 |
| --- | --- | --- |
| O01 | 初始工作台：长期、周、日内容在同一工作台；已有目标；并非全空首启 | [图](../evidence/ux-audit-2026-09-14/01-baseline.jpg)、[AX](../evidence/ux-audit-2026-09-14/01-baseline.ax.txt) |
| O02 | 展开五步引导，包含收纳、两次跨层链接、30 分钟专注；Escape 后入口重新聚焦 | [图](../evidence/ux-audit-2026-09-14/02-guide.jpg)、[AX](../evidence/ux-audit-2026-09-14/02-guide.ax.txt) |
| O03 | `⌘⇧L` 展开 Do Later，主工作区为抽屉让位；本轮没有首次提示卡 | [图](../evidence/ux-audit-2026-09-14/03-later-open.jpg)、[AX](../evidence/ux-audit-2026-09-14/03-later-open.ax.txt) |
| O04 | 写收纳项并 Enter；下方出现可继续输入的任务行，提示 Tab 建子任务；引导进度变 1/5 | [图](../evidence/ux-audit-2026-09-14/04-later-created.jpg)、[AX](../evidence/ux-audit-2026-09-14/04-later-created.ax.txt) |
| O05 | 在新行按 Tab 后输入，形成缩进子任务；数据库保存在该项的嵌套 `subtasks` 中 | [图](../evidence/ux-audit-2026-09-14/05-later-subtask.jpg)、[AX](../evidence/ux-audit-2026-09-14/05-later-subtask.ax.txt) |
| O06 | 周反馈面板：建议 1～4 个周目标、发现 0 个长期链接；提供 Plan 与 Dismiss | [图](../evidence/ux-audit-2026-09-14/06-week-feedback.jpg)、[AX](../evidence/ux-audit-2026-09-14/06-week-feedback.ax.txt) |
| O07 | 新建周任务，打开父目标菜单，候选为当前长期目标；候选可用 Enter 确认 | [图](../evidence/ux-audit-2026-09-14/07-link-parent-menu.jpg)、[AX](../evidence/ux-audit-2026-09-14/07-link-parent-menu.ax.txt) |
| O08 | 链接成功后周项继承色标，悬停时出现连线与两端高亮；对应问题消失，引导变 2/5 | [图](../evidence/ux-audit-2026-09-14/08-week-linked.jpg)、[AX](../evidence/ux-audit-2026-09-14/08-week-linked.ax.txt) |
| O09 | 创建日任务、链接周任务后引导变 3/5；Focus Blocks 与所选日一起呈现，没有独立日期导航 | [图](../evidence/ux-audit-2026-09-14/09-day-and-focus-layout.jpg)、[AX](../evidence/ux-audit-2026-09-14/09-day-and-focus-layout.ax.txt) |
| O10 | 未启动块选项含 Repeat daily 与 Delete；标题可以行内修改 | [AX](../evidence/ux-audit-2026-09-14/10-focus-options.ax.txt) |
| O11 | 启动时观察到 Start 3，随后 Stop 与减少的倒计时；其他块启动禁用，实际运行内容保留 | [图](../evidence/ux-audit-2026-09-14/11-focus-running.jpg)、[AX](../evidence/ux-audit-2026-09-14/11-focus-running.ax.txt) |
| O12 | 提前 Stop 后进入结束态并折叠，显示实际/计划分钟；不到一分钟读为 0/90 | [图](../evidence/ux-audit-2026-09-14/12-focus-stopped.jpg)、[AX](../evidence/ux-audit-2026-09-14/12-focus-stopped.ax.txt) |
| O13 | 日选项 Delete 禁用，说明该日含已经启动的块，不能删除 | [AX](../evidence/ux-audit-2026-09-14/13-day-delete-guard.ax.txt) |
| O14 | 再增长期周期的默认预览从既有结束日 Dec 7 开始，3 个月后 Mar 1，未确认创建 | [图](../evidence/ux-audit-2026-09-14/14-cycle-choice-three-months.jpg)、[AX](../evidence/ux-audit-2026-09-14/14-cycle-choice-three-months.ax.txt) |
| O15 | 切换 1 个月，开始日仍 Dec 7，结束即时变为 Jan 4，相差 28 天 | [图](../evidence/ux-audit-2026-09-14/15-cycle-choice-one-month.jpg)、[AX](../evidence/ux-audit-2026-09-14/15-cycle-choice-one-month.ax.txt) |
| O16 | 切换 6 个月，结束变为 May 31，相差 168 天；Escape 取消 | [图](../evidence/ux-audit-2026-09-14/16-cycle-choice-six-months.jpg)、[AX](../evidence/ux-audit-2026-09-14/16-cycle-choice-six-months.ax.txt) |
| O17 | 新建 W39，Plan with AI 进入侧栏并激活规划模式；随后收到地区不支持的 400 错误与 Retry | [图](../evidence/ux-audit-2026-09-14/17-ai-planning-error.jpg)、[AX](../evidence/ux-audit-2026-09-14/17-ai-planning-error.ax.txt) |
| O18 | 关闭 AI 错误后 Copy from previous week 成功；副本保留长期父目标与来源，原任务仍存在 | [图](../evidence/ux-audit-2026-09-14/18-copy-previous-week.jpg)、[AX](../evidence/ux-audit-2026-09-14/18-copy-previous-week.ax.txt) |
| O19 | 未来周的周期选项只显示 Delete | [AX](../evidence/ux-audit-2026-09-14/19-week-options.ax.txt) |
| O20 | 请求删除打开影响预览：W39 与 1 task；默认焦点 Cancel；最终选择取消 | [图](../evidence/ux-audit-2026-09-14/20-delete-impact-preview.jpg)、[AX](../evidence/ux-audit-2026-09-14/20-delete-impact-preview.ax.txt) |
| O21 | 应用菜单含 Settings、Send feedback、Copy support ID；设置页显示托管/OpenRouter 两选项 | [图](../evidence/ux-audit-2026-09-14/21-ai-access-settings.jpg)、[AX](../evidence/ux-audit-2026-09-14/21-ai-access-settings.ax.txt) |
| O22 | 将本周目标 Do Later 后，源项移除，下游日项变为 Link to parent goal，出现短暂 Undo later | [图](../evidence/ux-audit-2026-09-14/22-move-later-undo.jpg)、[AX](../evidence/ux-audit-2026-09-14/22-move-later-undo.ax.txt) |
| O23 | 诊断性恢复测试前数据库并重启后，引导仍 3/5；配置保留三个已完成步骤 | [图](../evidence/ux-audit-2026-09-14/23-restored-data-guide-retained.jpg)、[作用域核对](../evidence/ux-audit-2026-09-14/guide-retention-cross-check.json) |
| O24 | 恢复本轮新增的引导记录后，工作台回到原业务数据，进度 0/5，测试任务不存在 | [AX](../evidence/ux-audit-2026-09-14/24-cleanup-verified.ax.txt)、[恢复结果](../evidence/ux-audit-2026-09-14/cleanup-result.json) |

O11 的启动倒计时和 O02 的关闭焦点来自操作后的即时 AX 观察；随后保存的截图可能已经进入下一状态。不能用单张截图替代完整时序。

![周目标关联时的连线和高亮](/Users/lordcasser/workspace/projects/goal/analysis/evidence/ux-audit-2026-09-14/08-week-linked.jpg)

![专注运行状态](/Users/lordcasser/workspace/projects/goal/analysis/evidence/ux-audit-2026-09-14/11-focus-running.jpg)

## 0x02 静态证据索引

`S` 证明产物里存在相应结构和处理路径，不能替代整个用户流程实测。定位片段包含资源哈希、定位词和 Unicode 字符范围，见 [static-excerpts.txt](../evidence/ux-audit-2026-09-14/static-excerpts.txt)。原始文件仍保留在 `analysis/evidence/web/`，仅作为分析证据。

| 编号 | 核对点 | 原始位置或定位片段 |
| --- | --- | --- |
| S01 | 连线只支持 month→week、week→day；需周期上下文兼容；边必须直接接触当前 hover 对象；缺少链接色时跳过 | `DismissibleHint` 的 `link-hover`、`link-layout` |
| S02 | Enter split、Shift-Tab lift、Tab 嵌套；复制序列化去掉空行，粘贴清除实体标识和链接元数据 | `subtask-shortcuts`、`clipboard-identity` |
| S03 | 移动成功后 Undo 通知；撤销调用反向移动；记录的是任务和源/目标周期 | `move-undo`；需结合 O22 的关系表现 |
| S04 | planning/doing/reflecting 状态；三秒启动；兄弟块运行互斥；重复菜单与已启动删除限制 | `focus-running-exclusion`、`focus-states`、`focus-countdown`、`focus-start-reasons`、`focus-repeat-menu` |
| S05 | 删除预览分类统计、Cancel 初始焦点、键盘焦点约束、删除中防重复操作 | `cycle-delete-focus` |
| S06 | 三档时长用途文案、周期选择界面；实际日期算法用 O14～16 和运行时日期/时长交叉核对 | `cycle-choice-copy`、[运行核对](../evidence/ux-audit-2026-09-14/runtime-cross-check.json) |
| S07 | 引导监听任务变化、专注结束等事件并 reconcile，终态停止重复核对；关闭和跳过路径 | `guide-reconcile`；配置记录另见 O23 |
| S08 | 轻量边界、小色标、hover 控件、焦点和减弱动效样式 | 已核对的三个 CSS；设计细表见 `design-system.md`，其作者动机属于解释 |
| S09 | 候选标记剥离、技能状态、工具事件展示、错误和 Retry | `AgentConversation` 的 `candidate-replies`、`conversation-error` |
| S10 | 目标结构提取、标题澄清阶段、优先级分类；反馈界面和忽略作用域 | [binary-excerpts.json](../evidence/ux-audit-2026-09-14/binary-excerpts.json)、原始 schema、O06；后端解释见 `backend-internals.md` |
| S11 | 单条/批量 Keep 与 Undo 命令，预览汇总及快照结构；批量命令未带周期参数 | `proposal-ipc`、`proposal-review-state`、[运行核对](../evidence/ux-audit-2026-09-14/runtime-cross-check.json) |
| S12 | 本地持久化、语音采集、AI 接入与启动更新能力 | 运行时 schema、已核对的 worklet、前端主产物、O21；详细记录见既有报告 |

## 0x03 既有成功会话如何使用

`H01` 是仓库原有消息样本，记录 `start_planning`、`create_goal` 与返回 `status: applied`，可以证明此前成功创建过目标。样本本身不能证明用户后来点击过 Keep 或 Revert。

原文件虽名为 `.jsonl`，实际每行是 `序号|消息类型|JSON`。本轮按这个格式解析 12 条消息，去掉模型不透明签名等无关字段，输出真正的 [historical-agent-sample.jsonl](../evidence/ux-audit-2026-09-14/historical-agent-sample.jsonl)。该文件是**历史材料归一化**，不是本轮生成的会话。

## 0x04 两个不能只看截图的边界

**关系隐藏不等于关系删除。** O22 移动后日任务的选择器显示未链接，但只读数据库核对发现两个原 `parent_id` 都还在：周任务移到 Later 后仍引用长期目标，日任务仍引用该周任务。静态连线代码另外要求周期类型、所选父周期匹配，所以关系可能存着，但在当前上下文不可用。不能把 UI 无色标直接解释成已清空父引用。

**引导更像已体验记录。** O23 恢复数据库后旧动作已不在当前计划中，重启仍显示 3/5，配置持久化了三个完成步骤。这是诊断性数据恢复观察，未执行产品内删除，不能宣称所有删除路径都保持完成。但它足以说明“每次直接按当前数据重新打勾、删除必退回”不是已有证据支持的唯一规则。

部分 AX 编辑区摘要与其子文本、截图、数据库值存在短暂差异，例如摘要少一尾字符。没有把这种自动化观察差异写成产品输入规则或已确认的产品缺陷。涉及文字与结构结论时使用渲染结果和持久化内容交叉核对。

## 0x05 本轮未完成的实测

- 当前托管模型返回 `User location is not supported for the API use.`。未重试同一个不可恢复条件、未更改供应商凭据；成功澄清、优先级对话、真实新提议的 Keep/Revert 仍待复测。
- 未首次全新安装；未验证引导清空、跳过后的全部行为，也未花 30 分钟完成专注引导步骤。
- Undo later 入口在点击前过期；未实测它是否恢复完整关系，不能写成通过。
- 看过重复菜单，未启用每日重复、跨日生成、更新未来实例或停止重复。
- 未实际跨列表拖拽；编辑器任务拖拽和专注块排序要分别核对，不能用 SortableJS 存在代替任务拖拽流程验证。
- 未测试真实断网、输入法组合、睡眠/跨午夜、崩溃、退出调查、录音、通知权限、更新安装和反馈发送。

这些缺口已经进入独立决策与复测表。它们不阻止形成有来源、可修改的需求候选，但不能在实现验收时默认为原产品行为已确认。

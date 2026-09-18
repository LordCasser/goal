## Purpose

定义任务与目标的统一表示。目标、周计划项、日任务、子任务在数据层是同一种对象（`tasks_table`），靠所属周期的类型与 `parent_id` 区分角色；跨层链接用 `parent_id` 表达，视觉分组用 `root_color_key` 表达。

## Requirements

### Requirement: 任务内容的编辑态

系统 SHALL 按周期向界面提供任务的编辑所需内容（后端存在 `get_editor_workspace(cycle_id)` 与 `get_editor_workspaces_by_cycle_ids(cycle_ids)` 两个入口，后者按周期批量返回，键为周期标识）。编辑内容的权威存储 MUST 在后端，界面 MUST NOT 成为长期真相源。

#### Scenario: 打开一个周期的编辑内容
- **WHEN** 界面需要渲染某个周期的任务内容
- **THEN** 通过单个周期入口取得该周期的编辑内容

#### Scenario: 一次取得多个周期
- **WHEN** 工作台同时显示多个周期
- **THEN** 通过批量入口一次取得，不按周期逐个请求

#### Scenario: 未加载的周期
- **WHEN** 请求一个不存在或不可见的周期
- **THEN** 返回空结果而不是错误，界面按空状态渲染

#### Scenario: 编辑内容不被界面缓存为真相
- **WHEN** 数据被其它入口修改
- **THEN** 界面按事件刷新，不依赖本地编辑态做出最终判断

### Requirement: 子任务的结构化存储与文本渲染

子任务 SHALL 以结构化形式存储，并 SHALL 能渲染为 Markdown 供 AI 上下文与会话使用。结构化形式与渲染结果 MUST 语义一致，渲染 MUST NOT 引入原数据中不存在的层级。

#### Scenario: 子任务渲染供模型阅读
- **WHEN** 组装 agent 上下文
- **THEN** 子任务以 Markdown 形式注入，层级与顺序与结构化数据一致

#### Scenario: 空子任务列表
- **WHEN** 任务没有子任务
- **THEN** 渲染结果为空，不产生空标题或占位符

#### Scenario: 子任务标题含 Markdown 元字符
- **WHEN** 子任务标题包含会破坏 Markdown 结构的字符
- **THEN** 渲染结果仍能正确表达层级，不出现结构错乱

> 字段级结构（`EditorWorkspace` 的完整字段）在静态分析中未完全还原，实现时以本条的场景为准，按需补充精确 schema。

### Requirement: 任务的自引用树

任务 SHALL 支持 `parent_id` 自引用形成树，同级顺序由 `position` 决定；子任务可以以 Markdown/JSON 形式存于 `subtasks` 字段。

#### Scenario: 添加子任务
- **WHEN** 用户在某个任务下添加子任务
- **THEN** 新任务 `parent_id` 指向该任务，并出现在其下方

#### Scenario: 删除父任务
- **WHEN** 删除一个带有子任务的任务
- **THEN** 其子任务一并删除（级联），且相关预览快照同步清理

### Requirement: 跨层目标链接

系统 SHALL 允许每周目标链接到 Long-term 目标，每日任务链接到每周目标或直接链接到 Long-term 目标。跨周期链接 MUST 指向更高的合法计划层级，MUST NOT 反向关联。

关联 SHALL 为可选。周事务可以指向不同长期周期中的目标，也可以保持独立；日事务可以指向有长期目标或没有长期目标的周事务，也可以直接指向不同长期周期中的目标或保持独立。周期容器的父级 MUST NOT 代替事务的实际目标链接。

#### Scenario: 一周内混合多类事务
- **WHEN** 用户安排本周的长期目标相关工作与临时事务
- **THEN** 两类条目共存于同一自然周，每条分别选择目标归属或保持独立

#### Scenario: 事务结构统计
- **WHEN** 读取周或日的编辑内容或 AI 上下文
- **THEN** 从同一任务关系图派生长期目标关联、独立周事务、独立日事务与未解析关系的数量
- **AND** 只统计本周期的顶层真实事务，空行、提议和同周期子步骤不重复计数
- **AND** 明确它是当前关系快照的条数及占比，不是耗时、打断次数或生产力分数

#### Scenario: 建立合法链接
- **WHEN** 用户把一个周目标链接到某个 Long-term 目标
- **THEN** 周目标 `parent_id` 指向该 Long-term 目标，两边界面都显示链接关系

#### Scenario: 日任务直接关联长期目标
- **WHEN** 用户在日任务的关联菜单选择 Long-term 目标，或将日任务拖到该目标
- **THEN** 日任务 `parent_id` 直接指向该目标，无需新增周事务，日任务仍留在原日期的顶层
- **AND** 关联连线、颜色继承与事务结构统计均使用此直接关系；用户可更换目标或解除关联

#### Scenario: 建立非法链接
- **WHEN** 用户尝试把 Long-term 目标链接到日任务
- **THEN** 服务拒绝该反向关联

### Requirement: 长周期目标着色

`root_color_key` SHALL 只能存在于 `type='month'` 周期的任务上，用于在同一 Long-term 周期内对目标做视觉分组。系统 MUST 阻止该字段出现在其他层级，也 MUST 阻止已含该字段的周期被改成非 Long-term 类型。

新建 Long-term 根目标 SHALL 默认从现有高饱和度色板按本周期最少使用优先分配颜色，平票按稳定的色板顺序选择。Week、Day、Later 和内部子步骤 MUST NOT 自动分配独立颜色。

#### Scenario: 新目标自动配色
- **WHEN** 用户填写长期目标空输入行，或 Coach 提议新建长期根目标
- **THEN** 系统为该目标分配默认颜色，保留显式指定的颜色，AI 提议撤回时恢复原状态

#### Scenario: 保留无色选择与继承
- **WHEN** 用户选择 No color 后修改目标标题，或把一个着色根目标变为同周期子步骤
- **THEN** 标题修改不重新着色；变为子步骤时清除其独立颜色，改为继承父目标颜色

#### Scenario: 给长目标上色
- **WHEN** 用户为一个 Long-term 目标选择颜色
- **THEN** 该目标获得 `root_color_key`，其链接的下层任务在界面上继承同一色系

#### Scenario: 在周周期上设置颜色
- **WHEN** 对 week 周期中的任务设置 `root_color_key`
- **THEN** 数据库触发器 `reject_task_root_color_key_non_long_term_insert` 以 "root_color_key requires a Long-term cycle" 中止写入

#### Scenario: 修改已着色周期的类型
- **WHEN** 尝试把含 root color 的月周期改成其他类型
- **THEN** 触发器 `reject_cycle_type_change_with_root_colors` 中止写入

### Requirement: 清晰度标记

任务 SHALL 带两个独立布尔标记 `needs_refinement` 与 `needs_breakdown`，分别表示「目标表述还不清楚」与「还没有拆解成可执行步骤」。两者 MUST 可独立存在。

#### Scenario: 新目标默认为待澄清
- **WHEN** 用户手动输入一个新目标
- **THEN** 该目标 `needs_refinement` 为真，界面显示澄清入口

#### Scenario: 只有分解未完成
- **WHEN** 目标表述已澄清但尚未拆解
- **THEN** `needs_refinement=false` 且 `needs_breakdown=true`，界面只提示拆解

### Requirement: 复制血缘

从其他周期复制而来的任务 SHALL 记录来源任务 id。

#### Scenario: 追溯来源
- **WHEN** 查看一个从上一周期复制来的任务
- **THEN** 可以通过 `copied_from_task_id` 找到原任务

### Requirement: 任务移动与排序

系统 SHALL 支持在周期之间移动任务、在同级内重排，并在移动前校验目标容器的可变性。

排序的兄弟组 SHALL 与编辑器展示树一致：`parent_id = null` 表示本周期的可见根组，包括关联其他周期目标的根任务；非空值仅表示本周期内的子步骤组。排序 SHALL 保留已有归属。

#### Scenario: 不同归属的周任务排序
- **WHEN** 两个周任务分别关联不同长期目标，用户通过把手调整它们在同一周列表中的顺序
- **THEN** `reorder_tasks` 使用可见根组更新位置，任务的 `cycle_id` 和 `parent_id` 不变，空输入行仍留在末尾

#### Scenario: 跨栏拖动建立归属
- **WHEN** 用户把周根任务拖到长期目标，或把日根任务拖到所属自然周的周任务
- **THEN** 落点显示“Link to”及目标名称，松开后调用 `set_task_parent_link`，保留任务所在周期及其子步骤
- **AND** 不同层级的跳级链接、跨周日任务链接、锁定项及无效落点不产生写入；解除关联仍通过归属菜单完成

#### Scenario: 拖放取消与保存失败
- **WHEN** 用户按 Escape 取消、拖出有效落点，或排序保存失败
- **THEN** 取消不修改数据；保存失败恢复排序快照并显示错误，拖动提示及关系线隐藏状态被清除

#### Scenario: 移动到另一个周期
- **WHEN** 用户把任务拖到另一个周期
- **THEN** `move_task` 校验源与目标可用性后更新 `cycle_id` 与 `position`

#### Scenario: 目标是已结束周期
- **WHEN** 用户尝试把任务移动到已结束的周期
- **THEN** 操作被拒绝并返回 `cycle_ended` 类错误

#### Scenario: 重排专注块
- **WHEN** 用户在一天内拖动调整专注块顺序
- **THEN** `reorder_sessions` 重写相关 `position`，界面按新顺序显示

### Requirement: Later 项的计划类型元数据

任务在进入或离开 Later 时 SHALL 维护明确的计划类型元数据，并保持同周期树与跨层级关联的语义。Later 是共享收纳容器；不同计划类型的任务不会仅因同时位于 Later 就被视为同一棵同周期子树。

#### Scenario: 只有 Later 项带计划类型
- **WHEN** 任务属于 Later 容器
- **THEN** `later_plan_type` 可以为 `month`、`week`、`day` 或兼容旧数据的 `null`
- **AND** 非 Later 任务的 `later_plan_type` MUST 为 `null`

#### Scenario: 旧 Later 数据按长期处理
- **WHEN** Later 中已有任务的 `later_plan_type` 为 `null`
- **THEN** Later 行和安排入口按 `month` 展示和处理
- **AND** 系统不根据任务标题、创建时间或当前父关系推断其旧来源

#### Scenario: 移入 Later 记录来源类型
- **WHEN** 用户把属于长期、周或日周期的任务移入 Later
- **THEN** 任务的 `later_plan_type` 分别记录为 `month`、`week` 或 `day`
- **AND** 移动不会新建独立的 Later 实体

#### Scenario: 同周期子树一起暂存
- **WHEN** 用户把一个周期内的根任务与其同周期子步骤一起移入 Later
- **THEN** 这些任务保留原有同周期内部嵌套关系，并共享该根任务记录的 `later_plan_type`
- **AND** Later 中的树形展示仍按原有父子顺序呈现

#### Scenario: 不同来源层级分别暂存
- **WHEN** 一个周任务和它所关联的长期目标分别被移入同一个 Later 容器
- **THEN** 两者作为各自计划类型的独立暂存项展示
- **AND** 系统不会因为共享 Later 容器而把周任务误当作长期目标的同周期子步骤
- **AND** 原本合法的跨层级 `parent_id` 关联可以继续保留

#### Scenario: 移出后清空类型
- **WHEN** Later 任务被安排到长期、周或日周期
- **THEN** 任务的 `later_plan_type` 被清空
- **AND** 再次移入 Later 时按它当时所属周期的类型重新记录，而不是沿用旧值

#### Scenario: 移出时处理根项外部父关联
- **WHEN** Later 中作为本次移动根项的任务被移出，且其不在移动子树内的 `parent_id` 指向仍留在 Later 的任务，或该父任务对目标周期不满足同周期/合法跨层级规则
- **THEN** 系统解除该无效父关联并保留任务本身
- **AND** 移动子树内部的父关系以及合法跨层级关联继续保留

### Requirement: 任务进度度量

系统 SHALL 能按层级计算归一化进度（子任务完成比例），用于展示周/日计划的完成度。

#### Scenario: 计算周进度
- **WHEN** 某周目标下 4 个子任务完成 2 个
- **THEN** 该目标归一化进度为 0.5，并计入所属周期的汇总进度

#### Scenario: 只剩空壳的任务不计入
- **WHEN** 一个任务的子任务全部被删除且自身未完成
- **THEN** 该任务按原语义参与统计，不因剔除空任务而使分母为零（由 `remains_non_empty_after_completed_subtasks_removed` 保证）

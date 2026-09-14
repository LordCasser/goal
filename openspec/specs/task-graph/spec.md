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

系统 SHALL 允许把下层目标链接到上一层目标：每周目标可链到 Long-term 目标，每日任务可链到每周目标。链接 MUST 只允许相邻层级之间建立。

#### Scenario: 建立合法链接
- **WHEN** 用户把一个周目标链接到某个 Long-term 目标
- **THEN** 周目标 `parent_id` 指向该 Long-term 目标，两边界面都显示链接关系

#### Scenario: 建立非法链接
- **WHEN** 用户尝试把日任务直接链接到 Long-term 目标
- **THEN** `validate_linkable_task` 拒绝该操作

### Requirement: 长周期目标着色

`root_color_key` SHALL 只能存在于 `type='month'` 周期的任务上，用于在同一 Long-term 周期内对目标做视觉分组。系统 MUST 阻止该字段出现在其他层级，也 MUST 阻止已含该字段的周期被改成非 Long-term 类型。

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

#### Scenario: 移动到另一个周期
- **WHEN** 用户把任务拖到另一个周期
- **THEN** `move_task` 校验源与目标可用性后更新 `cycle_id` 与 `position`

#### Scenario: 目标是已结束周期
- **WHEN** 用户尝试把任务移动到已结束的周期
- **THEN** 操作被拒绝并返回 `cycle_ended` 类错误

#### Scenario: 重排专注块
- **WHEN** 用户在一天内拖动调整专注块顺序
- **THEN** `reorder_sessions` 重写相关 `position`，界面按新顺序显示

### Requirement: 任务进度度量

系统 SHALL 能按层级计算归一化进度（子任务完成比例），用于展示周/日计划的完成度。

#### Scenario: 计算周进度
- **WHEN** 某周目标下 4 个子任务完成 2 个
- **THEN** 该目标归一化进度为 0.5，并计入所属周期的汇总进度

#### Scenario: 只剩空壳的任务不计入
- **WHEN** 一个任务的子任务全部被删除且自身未完成
- **THEN** 该任务按原语义参与统计，不因剔除空任务而使分母为零（由 `remains_non_empty_after_completed_subtasks_removed` 保证）

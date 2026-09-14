## Purpose

定义 AI 写入人类计划的唯一合法路径。Agent 从不直接落库修改用户的计划，而是产生「待确认的改动」（proposal）并保留原始值快照；用户显式 Keep 之后改动才成为正式数据，Revert 则完全还原。

## Requirements

### Requirement: 提议态与已提交态严格分离

系统 MUST NOT 用同一个字段同时表达「已提交的删除」与「尚未处理的提议」。已确认的删除 SHALL 物理移除该行；待确认的改动 SHALL 只由提议字段表达。

理由（来自该功能的历史）：曾用墓碑字段（`deleted_at`）同时表达两件事——已经确认的删除和未处理的删除提议。结果是查询与语义都混乱，后来专门做了一次迁移把两者拆开。这条约束防止回退到那个形态。

#### Scenario: 未确认的删除
- **WHEN** agent 提议删除一个任务
- **THEN** 该行保留，提议字段标记为删除，用户在界面上仍能看到它并选择撤销

#### Scenario: 已确认的删除
- **WHEN** 用户确认删除
- **THEN** 该行被物理移除，不留下墓碑

#### Scenario: 查询可见任务
- **WHEN** 列出某周期的任务
- **THEN** 只需按提议字段区分「已提交」与「待确认」，不需要额外判断墓碑或软删除标记

### Requirement: 不引入修订号做并发控制

系统 MUST NOT 为本地单用户场景引入修订号、版本向量或乐观并发控制。

理由（来自该功能的历史）：曾为周期与任务引入内容修订号与最后修改修订号，两条迁移后全部删除——单用户本地应用不存在并发写入，这些字段只增加复杂度而不产生收益。

#### Scenario: 本地编辑
- **WHEN** 用户修改任务
- **THEN** 直接写入，不做修订号比对

#### Scenario: 界面刷新
- **WHEN** 数据被修改并推送事件
- **THEN** 界面按事件重新读取，不依赖修订号判断新旧

### Requirement: Agent 写入一律经过预览层

所有由 agent 发起的任务变更（新建、更新、删除）SHALL 先落到预览态，MUST NOT 直接成为正式数据。预览态中的任务 SHALL 在界面上可见但以视觉区分（高亮底色）。

#### Scenario: agent 创建一个目标
- **WHEN** agent 调用 `create_goal`
- **THEN** 目标以 `agent_proposal='upsert'` 插入，界面显示为待确认态，底栏出现 "You have 1 pending edit by agent"

#### Scenario: agent 删除一个目标
- **WHEN** agent 调用 `delete_goal`
- **THEN** 目标标记为 `agent_proposal='delete'`，在用户确认前仍可还原

#### Scenario: 拒绝直接写入
- **WHEN** 任何绕过预览层的写入路径被触发
- **THEN** 系统拒绝（无此类公开命令）

### Requirement: 原始值快照

预览态中的每个任务 SHALL 在 `task_preview_originals` 中保留其进入预览前的原始状态（含不存在的情形），以便精确还原。

#### Scenario: 记录「原本不存在」
- **WHEN** agent 新建一个原本不存在的目标
- **THEN** 快照记录 `original_exists=false`；Revert 时该任务被删除

#### Scenario: 记录被修改前的值
- **WHEN** agent 修改一个已存在目标的标题
- **THEN** 快照保留原标题、完成态、子任务、位置、`goal_breakdown`、`parent_id`、`root_color_key` 与清晰度标记

#### Scenario: 周期被删除
- **WHEN** 预览所属周期被删除
- **THEN** 相关快照级联清理

### Requirement: 单条与批量确认

系统 SHALL 支持对单条预览做 Keep / Revert，也 SHALL 支持一次性 Keep all / Undo all。

#### Scenario: 保留单条
- **WHEN** 用户点击某条待确认编辑的 Keep
- **THEN** 该任务转为正式数据，其预览快照被清除，底栏计数减一

#### Scenario: 还原单条
- **WHEN** 用户点击 Revert
- **THEN** 任务被还原为快照状态（原本不存在的则删除），快照清除

#### Scenario: 批量确认
- **WHEN** 用户点击 Keep all
- **THEN** 当前周期所有预览任务同时转为正式数据

#### Scenario: 全部还原
- **WHEN** 用户点击 Undo all
- **THEN** 当前周期所有预览改动被还原，计划回到 agent 介入之前的状态

### Requirement: 新建目标复用空行

当 agent 在当前周期的目标列表中创建新目标时，若列表尾部存在一个空的、用户可见的任务行，系统 SHALL 用新目标替换该空行；否则追加到末尾。

#### Scenario: 输入框恰好为空
- **WHEN** 用户留着空的输入行，然后让 agent 添加目标
- **THEN** 新目标占据该空行的位置，不产生额外的空行

#### Scenario: 没有空行
- **WHEN** 列表中不存在空任务行
- **THEN** 新目标追加到列表末尾

### Requirement: 待确认计数可见

只要当前周期存在未处理的预览改动，界面 SHALL 持续显示待确认计数与 Keep / Revert 操作入口。

#### Scenario: 存在待确认改动
- **WHEN** 预览计数大于 0
- **THEN** 底栏显示 "You have N pending edit(s) by agent" 及操作按钮

#### Scenario: 全部处理完毕
- **WHEN** 计数回到 0
- **THEN** 底栏消失

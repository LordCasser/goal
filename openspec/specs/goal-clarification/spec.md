## Purpose

定义「把模糊想法变成可执行目标」这一核心能力。清晰度不是让模型随口判断，而是一个可计算的结果：系统维护结构化的 `goal_breakdown`，从中推导缺失字段与两个清晰度标记，并驱动 agent 按 why → what → how 的顺序把目标问清楚、改好标题、再拆成步骤。

## Requirements

### Requirement: 提取优先于评价

系统 SHALL 让模型**提取结构化信息**（背景、产出物、结果、范围），清晰度数值 MUST 由确定性代码从该结构推导得出。系统 MUST NOT 依赖模型直接输出「这个目标清晰/不清晰」这类主观判断。

理由（来自该功能的历史）：这一机制曾六次重设计，从 0-100 分 + SMART 维度、到八维评估、再到让模型判断「执行计划清晰度」（三态：模糊/待发现/可执行）。最终收敛到「模型只做提取」的原因是**模型对抽象标签的判断不可靠**，而对事实的提取可靠得多。

#### Scenario: 模型不做判断
- **WHEN** 模型分析一个目标
- **THEN** 它输出的是提取到的字段值，不输出清晰度评分或清晰/模糊的结论

#### Scenario: 清晰度由代码计算
- **WHEN** 需要判断一个目标是否还不够清楚
- **THEN** 由 `goal_breakdown` 的缺失字段推导，结果可重复、可测试

#### Scenario: 同一输入得到同一结论
- **WHEN** 同一份 `goal_breakdown` 被评估两次
- **THEN** 得到完全相同的缺失字段与清晰度标记，不受模型随机性影响

#### Scenario: 不引入评分
- **WHEN** 界面呈现一个目标的清晰度状态
- **THEN** 以「缺什么」表达，MUST NOT 显示 0–100 的分值或等级

### Requirement: GoalBreakdown 结构

系统 SHALL 用四个部分表达一个目标：`context`（背景）、`output`（产出物）、`outcome`（结果与验证方式）、`scope`（工作量与分解状态）。各部分字段可独立更新，`null` 表示清除、缺省表示保留。

#### Scenario: 部分更新不清空其他字段
- **WHEN** agent 只提交 `output.value`
- **THEN** 已有的 `context` / `outcome` / `scope` 保持不变

#### Scenario: 显式清除一个字段
- **WHEN** agent 对某字段显式传 `null`
- **THEN** 该字段被清空，其余字段保留

#### Scenario: 空更新被拒绝
- **WHEN** 一次 `update_goal_breakdown` 不包含任何有效变更
- **THEN** 返回 `empty_update` 错误

### Requirement: 缺失字段计算

系统 SHALL 从 `goal_breakdown` 推导缺失字段集合，至少覆盖 `context.clarification`、`output.value`、`outcome.value`、`outcome.verification_method`。缺失字段 SHALL 作为 agent 提问的引导，但不构成硬性门槛。

#### Scenario: 刚创建的目标
- **WHEN** 目标只有标题
- **THEN** 缺失字段包含 context / output / outcome 三类主要字段

#### Scenario: 模型可以跳过无关字段
- **WHEN** 某缺失字段对当前目标确实不适用
- **THEN** agent 可以跳过该字段继续推进，不被系统强制回退

### Requirement: 清晰度标记推导

系统 SHALL 由 `goal_breakdown` 计算出 `needs_refinement` 与 `needs_breakdown`。`needs_refinement` MUST 只在 context / output / outcome 三部分足以支撑一个明确标题时才被清除；`needs_breakdown` MUST 只在分解已被用户确认且步骤已写回任务时才被清除。

#### Scenario: 三部分齐备
- **WHEN** context 等级为高、`output.value` 与 `outcome.value` 均有值
- **THEN** 系统允许清除 `needs_refinement`

#### Scenario: 未确认分解
- **WHEN** 模型提出了步骤但用户尚未确认
- **THEN** `needs_breakdown` 保持为真

#### Scenario: 分解已完成
- **WHEN** 用户确认步骤且 `scope.fully_decomposed=true`
- **THEN** `needs_breakdown` 被清除，子任务写回任务

### Requirement: why → what → how 对话流程

`goal_setting` 技能 SHALL 按 UNDERSTAND → COLLECT → REFINE_TITLE → BREAK_DOWN → FINISH 推进，且 MUST 一次只问一个焦点问题。收集阶段 MUST 优先澄清 context，因为它决定后续判断。

#### Scenario: 首轮提问
- **WHEN** 用户请求澄清一个目标
- **THEN** agent 说明目标为何不清楚，并只提出一个针对性问题

#### Scenario: 批量保存学习结果
- **WHEN** 用户在一轮里给出了多项信息
- **THEN** agent 可以一次 `update_goal_breakdown` 批量写入

#### Scenario: 用户不知道下一步
- **WHEN** 用户表示不确定
- **THEN** agent 用一个澄清问题帮其收敛，而不是替其决定

#### Scenario: 结果不由用户控制
- **WHEN** `outcome.controlled_by_user` 为假
- **THEN** agent 提议把目标改写为用户可控的表述

### Requirement: 标题精炼

系统 SHALL 按四种情形之一生成标题：既无产出也无结果、只有产出、只有结果、两者都有。

#### Scenario: 只有产出
- **WHEN** `output.value` 存在而 `outcome.value` 缺失
- **THEN** 标题取「动作动词 + 产出物」

#### Scenario: 产出与结果都有
- **WHEN** 两者都存在
- **THEN** 标题取「对产出的可控动作 + to/for + 结果」，并把被动产出改成动作（"signed agreement" → "Sign agreement"）

#### Scenario: 都没有
- **WHEN** 两者都缺失
- **THEN** 保留原标题，只用 context 补充说明

### Requirement: 不编造信息

系统 MUST NOT 替用户发明指标、日期、干系人、产品或范围。所有写入 `goal_breakdown` 的值 SHALL 来源自用户回答或工具结果。

#### Scenario: 用户没给成功标准
- **WHEN** 用户始终没有给出可验证的成功标准
- **THEN** agent 继续追问或跳过该字段，不得填入一个看起来合理的数值

#### Scenario: 保持用户语言
- **WHEN** 用户用中文描述目标
- **THEN** 生成的目标标题与子任务步骤保持中文，不翻译成英文

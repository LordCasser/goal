## Purpose

定义「在一堆都想做的事里决定先做哪个」这一能力。优先级不是一个分数，而是把候选任务分到几个语义桶里，每个落桶的决定都必须带用户说过的理由。

## Requirements

### Requirement: 优先级分类结构

系统 SHALL 用五个桶表达一个周期的优先级结论：`big_wins`（值得投入的大胜）、`bottlenecks`（瓶颈，可为空）、`non_negotiables`（必须做但不算大胜）、`deprioritized`（被放弃/推迟/委派）、`pending_review`（尚未处理的任务 id）。该结构 SHALL 持久化在周期上。

#### Scenario: 初始化排序会话
- **WHEN** agent 调用 `start_prioritization`
- **THEN** 返回该周期已持久化的 breakdown 与 hydrated 表示，`pending_review` 为尚未分类的任务

#### Scenario: 周期没有历史排序
- **WHEN** 首次对某周期排序
- **THEN** 返回空 breakdown，`pending_review` 为当前周期全部候选任务

### Requirement: 分类必须带理由

每个被移入某桶的任务 SHALL 记录 `task_id` 与 `reason`，且理由 MUST 来自用户表述。

#### Scenario: 用户说明理由
- **WHEN** 用户说「这个先放放，等设计稿定了再做」
- **THEN** 该任务进入 `deprioritized`，`reason` 记录为用户给出的理由

#### Scenario: 模型想不出理由
- **WHEN** 模型无法从对话中提取理由
- **THEN** 不应把任务移入该桶，而是继续询问

### Requirement: 增量更新不丢数据

对 breakdown 的更新 SHALL 是合并式的：未在本次提交中出现的数组项 MUST 保留，除非调用方显式移除。越过 agent 归一化与校验的写入 MUST 被拒绝。

#### Scenario: 只更新瓶颈
- **WHEN** 一次更新只提供 `bottlenecks`
- **THEN** 已有的 `big_wins` / `non_negotiables` / `deprioritized` 保持不变

#### Scenario: 非法结构
- **WHEN** 提交的 breakdown 结构不满足校验
- **THEN** `validate_breakdown` 拒绝写入

### Requirement: 排序结果对模型可读

系统 SHALL 能把持久化的 breakdown 渲染成模型可读的表示（含 XML 转义），并在其中标出大胜与不可协商项。

#### Scenario: 渲染给模型
- **WHEN** 排序技能被激活
- **THEN** 提示中的 breakdown 用大胜与不可协商标记区分，且文本已转义

#### Scenario: 任务标题含特殊字符
- **WHEN** 任务标题包含 `<` 或 `&`
- **THEN** 渲染结果中被转义，不破坏模型侧的解析

### Requirement: 排序与规划的分工

排序技能 SHALL 只对已存在的候选任务做取舍，MUST NOT 借此创建或拆解目标（那是规划与澄清技能的职责）。

#### Scenario: 缺少候选任务
- **WHEN** 当前周期没有可排序的任务
- **THEN** agent 引导用户先做规划，而不是凭空生成任务

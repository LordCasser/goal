## ADDED Requirements

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
- **WHEN** Later 中作为本次移动根项的任务被移出，且其不在移动子树内的 `parent_id` 指向仍留在 Later 的任务，或该父任务对目标周期不满足同周期/相邻层级规则
- **THEN** 系统解除该无效父关联并保留任务本身
- **AND** 移动子树内部的父关系以及合法相邻层级关联继续保留

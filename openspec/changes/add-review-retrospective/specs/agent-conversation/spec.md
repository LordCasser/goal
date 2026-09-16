## MODIFIED Requirements

### Requirement: 技能激活

系统 SHALL 用五个技能之一驱动 agent 行为：`goal_setting`、`long_term_planning`、`short_term_planning`、`prioritization`、`review`；无技能时为 `none` 回退块。技能 MUST 由工具调用结果推导并持久化，而不是由前端直接指定。

#### Scenario: 启动长周期规划
- **WHEN** agent 调用 `start_planning` 且当前页面是 month 周期
- **THEN** `active_skill` 置为 `long_term_planning`，下一回使用对应系统提示

#### Scenario: 启动周/日规划
- **WHEN** agent 调用 `start_planning` 且当前页面是 week 或 day 周期
- **THEN** `active_skill` 置为 `short_term_planning`

#### Scenario: 对专注块启动规划
- **WHEN** agent 在 session 周期上调用 `start_planning`
- **THEN** 返回 `unsupported_cycle_type`，且系统提示明确 "Agent mutations are not supported for session cycles."

#### Scenario: 未激活技能时的引导
- **WHEN** 会话处于 `none` 技能状态
- **THEN** 系统提示要求模型先选择正确的 `start_*` 工具再进入流程

#### Scenario: 启动复盘
- **WHEN** agent 调用 `start_review` 且当前周期存在可复盘的数据
- **THEN** `active_skill` 置为 `review`，下一回使用复盘技能的系统提示

#### Scenario: 对专注块启动复盘
- **WHEN** agent 在 session 周期上调用 `start_review`
- **THEN** 返回 `unsupported_cycle_type`，不激活技能

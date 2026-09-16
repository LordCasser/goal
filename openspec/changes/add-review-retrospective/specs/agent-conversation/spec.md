## MODIFIED Requirements

### Requirement: 技能激活

系统 SHALL 向模型提供技能目录和 `load_skill` 工具，由 LLM 根据当前用户问题自行选择、加载和切换工作流。技能覆盖目标澄清、长期规划、周规划、日规划、优先级、单周期复盘、时段分析、计划问题诊断与复盘；无技能时为 Coach 引导。技能状态持久化在全局会话，前端入口可提供初始规划范围但不得固化整段对话的技能。新建的默认 `coach` skill 使用英语提示，既有用户 skill 文件不被覆盖。

#### Scenario: 按发送时快照启动规划
- **WHEN** 用户在发送明确的规划请求时选择了 month、week 或 day 页面，且 agent 调用 `start_planning`
- **THEN** 分别激活 `long_term_planning`、`weekly_planning` 或 `daily_planning`，下一轮使用对应系统提示和工具集合
- **AND** 激活结果写入全局会话，不创建周期会话

#### Scenario: 页面切换不改变在途技能
- **WHEN** 某回合已按发送时页面快照启动技能，用户在工具调用期间切换到另一个页面
- **THEN** 在途回合继续使用原页面快照与原工具 scope，切换不会改写 `active_skill` 或重新触发规划

#### Scenario: 对专注块启动规划
- **WHEN** agent 在 session 周期上调用 `start_planning`
- **THEN** 返回 `unsupported_cycle_type`，且系统提示明确 "Agent mutations are not supported for session cycles."

#### Scenario: 未激活技能时的引导
- **WHEN** 全局会话处于 `none` 技能状态
- **THEN** 系统提示提供技能目录，模型先调用 `load_skill` 或明确的初始化工具，再进入相应流程

#### Scenario: 默认 Coach 技能语言
- **WHEN** 应用首次创建缺失的 `coach/SKILL.md`
- **THEN** 文件使用英语默认提示，既有其他 skill 文件不被覆盖

#### Scenario: 启动复盘
- **WHEN** agent 调用 `start_review` 且当前周期存在可复盘的数据
- **THEN** `active_skill` 置为 `review`，下一回使用复盘技能的系统提示

#### Scenario: 对专注块启动复盘
- **WHEN** agent 在 session 周期上调用 `start_review`
- **THEN** 返回 `unsupported_cycle_type`，不激活技能

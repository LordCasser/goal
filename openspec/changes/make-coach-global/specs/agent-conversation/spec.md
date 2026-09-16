## RENAMED Requirements

FROM: `每周期一条会话`
TO: `全局单例会话`

## MODIFIED Requirements

### Requirement: 全局单例会话

系统 SHALL 为 Coach 持有全局唯一的一条 conversation，会话记录 `active_turn_id`、`revision`、`last_error` 与 `active_skill`。用户在不同计划、Workspace 或 Calendar 中使用 Coach MUST 复用同一条会话，不得按页面或周期创建新的会话。

#### Scenario: 从任一页面打开 Coach
- **WHEN** 用户在长期、周、日、Workspace 或 Calendar 页面打开 Coach
- **THEN** 系统返回同一条全局会话；首次使用时只创建一条全局会话
- **AND** 当前页面选择作为本次交互的页面快照，不改变会话身份

#### Scenario: 切换计划与页面
- **WHEN** 用户在回合之间切换长期、周、日计划或 Workspace/Calendar
- **THEN** 会话历史保持连续，当前草稿、消息滚动位置和未完成回合仍可继续
- **AND** 切换不会把浏览动作写成用户消息或触发新的模型请求

#### Scenario: 全局会话的单例守卫
- **WHEN** 多个入口同时请求初始化 Coach，或数据库中存在旧的多周期会话
- **THEN** 系统最终只保留一个全局会话身份，初始化不会产生第二条可用会话
- **AND** 既有消息按迁移规则合并到该会话且不丢失

### Requirement: 消息模型

会话中的每条消息 SHALL 带 `sequence_number`（全局会话内唯一且递增）与五选一的 `message_type`：`user`、`model_text`、`model_function_call`、`function_result`、`app_tool_result`。消息内容以 JSON 形式存于 `payload_json`。消息不得依赖某个规划周期的外键才能保留。

#### Scenario: 一次工具调用的落库
- **WHEN** 模型决定调用 `update_goal_breakdown`
- **THEN** 全局会话中依次写入 `model_function_call` 与（执行后的）`function_result`，序号连续

#### Scenario: 删除所属计划后保留消息
- **WHEN** 用户删除一个消息曾引用或曾操作的长期、周或日计划
- **THEN** 全局会话与全部消息保留，历史中的目标引用按已有回执/历史语义显示
- **AND** 删除计划不级联删除 Coach 历史

#### Scenario: 回合跨页面完成
- **WHEN** 回合发起后用户切换页面，模型随后返回文本或工具结果
- **THEN** 返回消息继续写入同一全局会话并按原回合顺序显示
- **AND** 不因当前页面已变化而丢弃或改写该回合消息

### Requirement: 技能激活

系统 SHALL 向模型提供技能目录和 `load_skill` 工具，由 LLM 根据当前用户问题自行选择、加载和切换工作流；技能状态持久化在全局会话。技能覆盖目标澄清、长期规划、周规划、日规划、优先级、单周期复盘、时段分析、计划问题诊断；无技能时为 Coach 引导。前端入口可提供初始规划范围，但 MUST NOT 将某次页面范围固化为整段全局对话的技能。新建的默认 `coach` skill 使用英语提示，既有用户 skill 文件不被覆盖。

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

### Requirement: 回合执行与错误可见

每次回合 SHALL 被全局会话的执行状态所约束：同一全局会话同一时刻最多一个进行中的回合；回合失败时错误写入 `last_error` 并可被前端读取。工具结果 SHALL 通过 `app_tool_result` 类型回灌给前端用于即时渲染。回合开始时固定页面选择快照及工具默认 scope，直到该回合完成或失败。

#### Scenario: 回合进行中重复触发
- **WHEN** 前端在全局回合未结束时再次发送消息，或从另一个页面触发发送
- **THEN** 系统拒绝或排队，不产生并发写入，也不创建第二条会话

#### Scenario: 供应商报错
- **WHEN** LLM 调用失败
- **THEN** 错误归一化为 `AgentError` 写入 `last_error`，前端显示可读错误且全局会话保留

#### Scenario: 在途工具 scope 固定
- **WHEN** 回合已发送并在工具调用过程中，用户改变当前选中的长期、周或日计划
- **THEN** 工具仍使用发送瞬间的默认 scope，页面变化只影响下一次显式发送
- **AND** 不因页面浏览追加用户消息或请求

### Requirement: 上下文注入

每次回合 SHALL 注入机器可读的上下文，格式为带标签的文本块，包含发送瞬间的页面选择快照与当前时间。上下文 MUST 只含当前权限范围内可见的数据，并明确允许没有对应计划。页面浏览或选择变化本身 MUST NOT 生成用户消息或触发回合。

上下文至少在 `<page_state>` 中表达 `active_cycle_id`、`view`（`workspace` 或 `calendar`）、`week_starts_on`、`selected_date`，以及所选长期、周、日计划的 id、标题和起止日期；没有计划或没有选择的字段使用字面量 `null`。发送时的 `pageContext` 使用 `long_term_cycle_id`、`week_cycle_id`、`day_cycle_id` 传入选择，页面快照随发送固定，工具默认 scope 也使用同一份快照。

#### Scenario: 注入页面快照
- **WHEN** 用户在 Workspace 选中长期、周和日计划后发送消息
- **THEN** 提示中包含 `view`、三个选中计划 id、`week_starts_on`、`selected_date` 以及当前日期时间
- **AND** 上下文继续包含当前可用的周期元数据与任务事实

#### Scenario: 无计划或无选择
- **WHEN** 用户在尚未创建计划的 Workspace 或 Calendar 页面发送消息
- **THEN** 对应选择字段为 `null`，模型知道该页面没有可读取的计划
- **AND** 系统不为补齐上下文而创建周期或发起额外请求

#### Scenario: 日历页面快照
- **WHEN** 用户在 Calendar 选中一个日期但未选中日计划后发送消息
- **THEN** `view` 为 `calendar`，`selected_date` 保留该本地日期，`selected_day_id` 为 `null`
- **AND** 模型收到该快照而不会把页面浏览写成用户意图

#### Scenario: 页面选择变化不触发回合
- **WHEN** 用户只滚动、切换周/日/长期选择，或在 Workspace 与 Calendar 之间切换而没有发送文本或点击明确 AI 操作
- **THEN** 不写入 `user` 消息、不调用 LLM、不调用 agent 工具

#### Scenario: 在途回合使用发送快照
- **WHEN** 回合发出后用户改变任何页面选择
- **THEN** 该回合的 prompt、工具默认 scope 与结果仍基于发送时快照
- **AND** 下一次显式发送才读取新的页面选择

#### Scenario: 规划技能额外注入
- **WHEN** agent 处于规划类技能
- **THEN** 上下文额外包含快照所指周期的父周期键与子周期键（区分已结束/当前/未来）与该周期的任务列表；若该周期为空则明确返回空事实

#### Scenario: 单任务澄清时的上下文
- **WHEN** agent 处于 `goal_setting` 且已通过 `start_goal_setting` 选中某任务
- **THEN** 提示包含该任务的 `goal_breakdown`、`missing_fields`、`needs_refinement`、`needs_breakdown`

### Requirement: 应用侧工具调用

除了模型发起的工具调用，系统 SHALL 支持**应用侧**发起的工具调用，类型标记为 `app_tool_result`。这类调用只由用户明确触发的 AI 入口或确认操作发起，并使用该次动作的页面/目标快照；进入页面、浏览计划或恢复滚动位置不得单独触发。

#### Scenario: 明确入口启动规划
- **WHEN** 用户在一个计划页面点击 Plan with AI
- **THEN** 应用侧调用 `start_planning`，结果包含发送时页面快照所对应的激活技能与周期上下文
- **AND** 应用命令在激活后立即发起模型回合，使用全局会话；用户不需要再次点击发送

#### Scenario: 浏览不启动应用工具
- **WHEN** 用户只打开 Coach、切换 Workspace/Calendar 或浏览周/日/长期选择
- **THEN** 不调用 `start_planning`、`start_goal_setting` 或其他应用侧工具，也不写入用户消息

#### Scenario: 应用侧结果对模型可见
- **WHEN** 应用侧工具调用完成
- **THEN** 其结果以 `app_tool_result` 消息写入全局会话，模型在后续回合能看到它
- **AND** 若页面在结果返回前切换，结果仍归属于原发起回合

#### Scenario: 技能由结果决定
- **WHEN** 应用侧 `start_planning` 返回
- **THEN** 返回体中的 `activated_skill` 决定全局会话的 `active_skill`，前端无权指定

### Requirement: 用户可编辑技能

内置工作流 SHALL 首次补齐到 `~/.goal/skills/<name>/SKILL.md`。已有文件 MUST NOT 被启动或升级覆盖。每次加载技能及后续模型轮次 SHALL 读取当前文件；无效、缺失或过大的文件 MUST 给出可定位的错误，MUST NOT 静默忽略用户修改。技能仅定义工作方法，工具白名单、参数校验、事实读取和提案确认由代码控制。默认新建的 `coach` skill 使用英语内容。

#### Scenario: 对话内切换到季度分析
- **WHEN** 用户在日计划 Coach 中要求分析一个季度
- **THEN** LLM 可调用 `load_skill` 加载 `period-analysis`，再用 `get_period_context` 获取明确起止日期内的数据
- **AND** 页面选择态不限制分析范围；分析技能不提供计划写工具，未开放的工具调用由后端拒绝

#### Scenario: 默认 Coach 技能的语言
- **WHEN** 应用首次创建缺失的 `coach/SKILL.md`
- **THEN** 文件使用英语默认提示，后续加载遵循用户对该文件的修改
- **AND** 创建默认文件不改写已有其他 skill 文件

#### Scenario: 修改个人工作流
- **WHEN** 用户修改 `daily-planning/SKILL.md`
- **THEN** 下一次加载该技能即使用新内容，无需重启应用

### Requirement: 可配置的会话空闲过期

Coach 全局上下文保留时间 SHALL 在设置中配置为 1–1440 整数分钟，默认 15。计时从最近完成的对话回合起算，读取会话、切换页面或滚动消息不续期。截止时间由后端依据同一配置计算；前端按截止时间自动重取并清空显示。后端在读取、恢复及发起新回合前清除已过期的全局消息、技能与错误；正在进行的回合不被中途清除。任务、提案和复盘数据独立于会话，不随清空删除。

#### Scenario: 调整过期时长
- **WHEN** 用户保存新的分钟数
- **THEN** 后续读取立即使用新时长；已打开的全局 Coach 更新截止时间，若新截止时间已过去则清空

#### Scenario: 过期后发送
- **WHEN** 用户在截止时间之后发送新消息或从休眠恢复
- **THEN** 旧全局上下文不再发送给模型，新问题从空会话和技能选择开始

#### Scenario: 在途回合不被清理
- **WHEN** 全局回合正在执行且达到闲置 TTL
- **THEN** 消息、工具调用和回合状态保持到回合结束；完成后重新开始 TTL 计时
- **AND** 页面浏览不会延长或中断该回合

### Requirement: 决策回执属于聊天记录

成功应用或放弃改动后，系统 SHALL 在全局会话中保存应用侧回执，并在消息滚动区按回合顺序显示；MUST NOT 把成功回执持久固定在输入框上方。待确认项 SHALL 通过现有待确认入口跨页面可见，并始终引用原提案目标。

#### Scenario: 跨页面确认计划
- **WHEN** 模型在周页面创建任务提案，用户随后切到 Calendar 或另一个周期
- **THEN** 待确认入口仍可见；用户确认或放弃时作用于原提案目标，不按当前页面重定向
- **AND** 操作回执写入全局聊天历史

#### Scenario: 确认计划后继续聊天
- **WHEN** 用户确认任务提案后发送下一条消息
- **THEN** 回执保留在全局历史原消息位置，后续模型上下文知道该提案已经处理
- **AND** 输入框上方只保留尚未处理的确认项

#### Scenario: 页面变化不重写待确认目标
- **WHEN** 待确认项存在时用户切换长期、周、日选择或 Workspace/Calendar
- **THEN** 待确认项的目标 id、前后快照和确认状态不变，不生成新的用户消息

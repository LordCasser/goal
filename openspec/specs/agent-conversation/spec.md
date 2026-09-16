## Purpose

定义 AI 陪练的会话模型。Coach 全局只有一条 conversation，会话由「回合（turn）」驱动，每次回合把上下文喂给 LLM、执行工具调用、把结果落库，并通过一个可选的 `active_skill` 决定系统提示与行为流程。

## Requirements

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
- **THEN** 依次写入 `model_function_call` 与（执行后的）`function_result`，序号连续

#### Scenario: 删除所属计划后保留消息
- **WHEN** 用户删除一个消息曾引用或曾操作的长期、周或日计划
- **THEN** 全局会话与全部消息保留，历史中的目标引用按已有回执/历史语义显示
- **AND** 删除计划不级联删除 Coach 历史

#### Scenario: 回合跨页面完成
- **WHEN** 回合发起后用户切换页面，模型随后返回文本或工具结果
- **THEN** 返回消息继续写入同一全局会话并按原回合顺序显示
- **AND** 不因当前页面已变化而丢弃或改写该回合消息

### Requirement: 技能激活

系统 SHALL 向模型提供技能目录和 `load_skill` 工具，由 LLM 根据当前用户问题自行选择、加载和切换工作流。技能覆盖目标澄清、长期规划、周规划、日规划、优先级、单周期复盘、时段分析、计划问题诊断；无技能时为 Coach 引导。前端入口可提供初始规划范围，但 MUST NOT 将某次页面范围固化为整段全局对话的技能；技能状态持久化在全局会话。新建的默认 `coach` skill 使用英语提示，既有用户 skill 文件不被覆盖。

#### Scenario: 按发送时快照启动规划
- **WHEN** 用户在发送明确的规划请求时选择了 month、week 或 day 页面，且 agent 调用 `start_planning`
- **THEN** `active_skill` 置为 `long_term_planning`，同一回合的下一轮模型调用立即使用对应系统提示和工具集合
- **AND** week 与 day 分别激活 `weekly_planning` 或 `daily_planning`，不创建周期会话

#### Scenario: 页面切换不改变在途技能
- **WHEN** 某回合已按发送时页面快照启动技能，用户在工具调用期间切换到另一个页面
- **THEN** 在途回合继续使用原页面快照与原工具 scope，切换不会改写 `active_skill` 或重新触发规划

#### Scenario: 对专注块启动规划
- **WHEN** agent 在 session 周期上调用 `start_planning`
- **THEN** 返回 `unsupported_cycle_type`，且系统提示明确 "Agent mutations are not supported for session cycles."

#### Scenario: 未激活技能时的引导
- **WHEN** 会话处于 `none` 技能状态
- **THEN** 系统提示提供技能目录，模型先调用 `load_skill` 或明确的初始化工具，再进入相应流程

#### Scenario: 默认 Coach 技能语言
- **WHEN** 应用首次创建缺失的 `coach/SKILL.md`
- **THEN** 文件使用英语默认提示，既有其他 skill 文件不被覆盖

### Requirement: 回合执行与错误可见

每次回合 SHALL 被全局会话的执行状态所约束：同一全局会话同一时刻最多一个进行中的回合；回合失败时错误写入 `last_error` 并可被前端读取。工具结果 SHALL 通过 `app_tool_result` 类型回灌给前端用于即时渲染。回合开始时固定页面选择快照及工具默认 scope，直到该回合完成或失败。

#### Scenario: 回合进行中重复触发
- **WHEN** 前端在回合未结束时再次发送消息
- **THEN** 系统拒绝或排队，不产生并发写入

#### Scenario: 供应商报错
- **WHEN** LLM 调用失败
- **THEN** 错误归一化为 `AgentError` 写入 `last_error`，前端显示可读错误且会话保留

#### Scenario: 在途工具 scope 固定
- **WHEN** 回合已发送并在工具调用过程中，用户改变当前选中的长期、周或日计划
- **THEN** 工具仍使用发送瞬间的默认 scope，页面变化只影响下一次显式发送
- **AND** 不因页面浏览追加用户消息或请求

### Requirement: 上下文注入

每次回合 SHALL 注入机器可读的上下文，格式为带标签的文本块，包含发送瞬间的页面选择快照与当前时间。上下文 MUST 只含当前权限范围内可见的数据，并明确允许没有对应计划。页面浏览或选择变化本身 MUST NOT 生成用户消息或触发回合。

上下文的 `<page_state>` 块至少表达 `active_cycle_id`、`view`（`workspace` 或 `calendar`）、`week_starts_on`、`selected_date`，以及带 `id`、标题、起止日期的 `selected_long_term`、`selected_week`、`selected_day`；没有计划或没有选择的字段使用字面量 `null`。发送时的 `pageContext` 使用 `long_term_cycle_id`、`week_cycle_id`、`day_cycle_id` 传入选择，页面快照随发送固定，工具默认 scope 也使用同一份快照。

#### Scenario: 注入页面快照
- **WHEN** 用户在 Workspace 选中长期、周和日计划后发送消息
- **THEN** 提示中的 `<page_state>` 包含页面、活动周期、三个选中计划及其日期元数据
- **AND** 上下文继续包含当前可用的周期元数据与任务事实

#### Scenario: 无计划或无选择
- **WHEN** 用户在尚未创建计划的 Workspace 或 Calendar 页面发送消息
- **THEN** 对应选择字段为 `null`，模型知道该页面没有可读取的计划
- **AND** 系统不为补齐上下文而创建周期或发起额外请求

#### Scenario: 日历页面快照
- **WHEN** 用户在 Calendar 选中一个日期但未选中日计划后发送消息
- **THEN** `view` 为 `calendar`，`selected_date` 保留该本地日期，`selected_day` 为 `null`
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
- **THEN** 上下文额外包含父周期键与子周期键（区分已结束/当前/未来）与该周期的任务列表

#### Scenario: 单任务澄清时的上下文
- **WHEN** agent 处于 `goal_setting` 且已通过 `start_goal_setting` 选中某任务
- **THEN** 提示包含该任务的 `goal_breakdown`、`missing_fields`、`needs_refinement`、`needs_breakdown`

### Requirement: 应用侧工具调用

除了模型发起的工具调用，系统 SHALL 支持**应用侧**发起的工具调用，类型标记为 `app_tool_result`。这类调用只由用户明确触发的 AI 入口或确认操作发起，并使用该次动作的页面/目标快照；进入页面、浏览计划或恢复滚动位置不得单独触发。

#### Scenario: 明确入口启动规划
- **WHEN** 用户在一个计划页面点击 Plan with AI
- **THEN** 应用侧调用 `start_planning`，结果包含发送时目标快照所对应的激活技能与周期上下文
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
- **THEN** 返回体中的 `activated_skill` 决定会话的 `active_skill`，前端无权指定

### Requirement: 工具结果对模型报告成功

当一次写入进入预览态（尚未被用户确认）时，工具返回给模型的 SHALL 是成功状态。模型 MUST NOT 因为"改动还没被确认"而被要求重试或等待。

原因：预览确认是**人与界面之间**的契约，不是模型需要处理的失败。若向模型报告"待确认"，模型会重复尝试写入或反复询问用户，把机制性问题变成对话噪音。

#### Scenario: agent 创建目标
- **WHEN** 模型调用 `create_goal`
- **THEN** 工具结果返回成功状态与新建任务的标识，模型据此继续对话

#### Scenario: 用户随后撤销
- **WHEN** 用户在界面上撤销该改动
- **THEN** 撤销发生在人与界面之间，模型不被通知、也不需要补偿动作

#### Scenario: 会话历史与最终数据不一致
- **WHEN** 回看会话，其中包含一条后来被撤销的改动
- **THEN** 这是预期状态：会话记录"当时提议了什么"，数据表达"最终确认了什么"，两者不需要一致

### Requirement: 用户可编辑技能

内置工作流 SHALL 首次补齐到 `~/.goal/skills/<name>/SKILL.md`。已有文件 MUST NOT 被启动或升级覆盖。每次加载技能及后续模型轮次 SHALL 读取当前文件；无效、缺失或过大的文件 MUST 给出可定位的错误，MUST NOT 静默忽略用户修改。技能仅定义工作方法，工具白名单、参数校验、事实读取和提案确认由代码控制。默认新建的 `coach` skill 使用英语内容。

#### Scenario: 对话内切换到季度分析
- **WHEN** 用户在日计划 Coach 中要求分析一个季度
- **THEN** LLM 可调用 `load_skill` 加载 `period-analysis`，再用 `get_period_context` 获取明确起止日期内的数据
- **AND** 周期选中态不限制分析范围；分析技能不提供计划写工具，未开放的工具调用由后端拒绝

#### Scenario: 修改个人工作流
- **WHEN** 用户修改 `daily-planning/SKILL.md`
- **THEN** 下一次加载该技能即使用新内容，无需重启应用

#### Scenario: 默认 Coach 技能的语言
- **WHEN** 应用首次创建缺失的 `coach/SKILL.md`
- **THEN** 文件使用英语默认提示，后续加载遵循用户对该文件的修改
- **AND** 创建默认文件不改写已有其他 skill 文件

### Requirement: 可配置的会话空闲过期

Coach 全局上下文保留时间 SHALL 在设置中配置为 1–1440 整数分钟，默认 15。计时从最近完成的对话回合起算，读取会话不续期。截止时间由后端依据同一配置计算；前端按截止时间自动重取并清空显示。后端在读取、恢复及发起新回合前清除已过期消息、技能与错误；正在进行的回合不被中途清除。任务、提案和复盘数据独立于会话，不随清空删除。

#### Scenario: 调整过期时长
- **WHEN** 用户保存新的分钟数
- **THEN** 后续读取立即使用新时长；已打开 Coach 更新截止时间，若新截止时间已过去则清空

#### Scenario: 过期后发送
- **WHEN** 用户在截止时间之后发送新消息或从休眠恢复
- **THEN** 旧上下文不再发送给模型，新问题从空会话和技能选择开始

#### Scenario: 在途回合不被清理
- **WHEN** 全局回合正在执行且达到闲置 TTL
- **THEN** 消息、工具调用和回合状态保持到回合结束；完成后重新开始 TTL 计时
- **AND** 页面浏览不会延长或中断该回合

### Requirement: 时段事实范围

季度、半年、年度或自选日期分析 SHALL 使用自然日期起止边界，MUST NOT 使用 28 天的产品月外推。数据包括与请求范围相交的长期、周、日计划，保留归档计划；分别呈现各层任务与当前归属构成。边界外、无日期、已删除的数据限制必须可解释。MUST NOT 把周任务与其每日步骤相加为完成成果，或把当前完成状态冒充期末状态。范围过大时显式要求缩小，禁止静默截断后给出完整性结论。

#### Scenario: 读取自然日期范围的当前快照
- **WHEN** period-analysis 使用有序的 `start_date`、`end_date` 调用 `get_period_context`
- **THEN** 结果包含与日期范围相交的长期、周、日周期（包括已归档周期），排除无日期周期，并分别返回各周期的任务与 work mix
- **AND** 结果明确这是当前任务/归属快照，不把周任务与日步骤相加，也不声称拥有历史完成事件或按日期归因的专注时长

#### Scenario: 拒绝无效或过大的分析范围
- **WHEN** 日期格式无效、结束日期早于开始日期、范围超过十年，或快照序列化后超过上下文上限
- **THEN** 后端返回可定位的 `invalid_analysis_period` 或 `analysis_context_too_large` 错误，并要求调整范围
- **AND** 不静默截断数据后返回完整性结论

### Requirement: 工具过程聚合展示

Coach SHALL 将同一回合内连续工具消息收敛为默认折叠的一行摘要；调用和结果按 tool_call_id 合并，每项最多一行。用户与助手正文不折叠。完成的历史 MUST NOT 残留“正在执行”状态；失败或未完成在摘要可见。点击可展开或收起具体过程，键盘可操作且暴露 aria-expanded。

#### Scenario: 多次读取上下文
- **WHEN** 一轮调用读取一次周期和三项任务详情，并收到四项结果
- **THEN** 默认只显示“已读取 4 项资料”，展开后为四项已完成步骤，不展示八行开始/结束记录

### Requirement: AI 回复的结构化 Markdown 呈现

Coach SHALL 使用支持 GFM 的 Markdown 渲染组件，正确呈现表格、标题、强调、列表、任务列表、引用和代码块。宽表格和代码 SHALL 在消息内部横向滚动，键盘可达，MUST NOT 撑大面板。配色和字号复用 App 主题。原始 HTML 不执行，模型图片不自动加载；候选回复协议先剥离，保留独立点击和快捷键行为。

#### Scenario: 回复包含计划表格
- **WHEN** 模型返回带表头和分隔行的有效 Markdown 表格
- **THEN** 显示语义化表头和单元格；三列内容在面板内阅读或横向滚动，不显示成管道符段落

#### Scenario: Markdown 分段补全
- **WHEN** 渲染器收到进行中的部分内容，随后收到追加和完成内容
- **THEN** 不完整强调与表格能更新成完整结构，同一内容不重复；最终正文不被模拟打字延迟

### Requirement: Coach 动效与阅读位置

等待状态 SHALL 来自真实请求状态，MUST NOT 用演示计时器模拟步骤或回复。工具过程可平滑展开和收起，折叠后不可聚焦且对辅助技术隐藏。新消息轻量进入，减少动态效果设置下停用动效。输入区提供发送按钮及 Enter / Shift+Enter；请求中阻止重复发送，失败保留草稿。

#### Scenario: 阅读旧消息时收到更新
- **WHEN** 用户主动上滚且会话收到新内容
- **THEN** 保持阅读位置并提供回到最新消息入口，不能自动拉回底部


### Requirement: 回答风格可独立配置
系统 SHALL 首次创建 `~/.goal/persona.md`，默认以简洁、有用的秘书／助理风格回复。已有文件 MUST NOT 被覆盖。Coach 与时段分析 SHALL 在生成时重新读取该文件；persona MUST NOT 扩大工具权限或取消确认要求。

#### Scenario: 用户修改回答风格
- **WHEN** 用户修改 persona.md 并发起下一次请求
- **THEN** 新请求使用最新风格，不要求重启或重新配置 provider

### Requirement: 决策回执属于聊天记录
成功应用或放弃改动后，系统 SHALL 在全局会话中保存应用侧回执，并在消息滚动区按回合顺序显示；MUST NOT 把成功回执持久固定在输入框上方。待确认项 SHALL 通过现有待确认入口跨页面可见，并始终引用原提案目标。

#### Scenario: 跨页面确认计划
- **WHEN** 模型在周页面创建任务提案，用户随后切到 Calendar 或另一个周期
- **THEN** 待确认入口仍可见；用户确认或放弃时作用于原提案目标，不按当前页面重定向
- **AND** 操作回执写入全局聊天历史

#### Scenario: 确认计划后继续聊天
- **WHEN** 用户确认任务提案后发送下一条消息
- **THEN** 回执保留在原消息位置，后续模型上下文知道该提案已经处理
- **AND** 输入框上方只保留尚未处理的确认项

#### Scenario: 页面变化不重写待确认目标
- **WHEN** 待确认项存在时用户切换长期、周、日选择或 Workspace/Calendar
- **THEN** 待确认项的目标 id、来源周期和确认状态不变，不生成新的用户消息

### Requirement: 按技能提供有限的业务工具
系统 SHALL 提供文档化的工具数量、分类、参数和确认策略，当前目录为 24 项。分析与问题诊断技能 MUST NOT 提供写工具。模型 MUST NOT 获得通用 IPC、SQL、shell、凭据或确认自身提案的工具。具体清单见 docs/coach-tools.md。

#### Scenario: 模型修改设置
- **WHEN** 用户要求修改某项设置
- **THEN** 模型读取当前值，提出带参数的单项操作卡
- **AND** 用户 GUI 确认前，原设置保持不变
- **AND** 确认后重新执行设置校验并刷新界面

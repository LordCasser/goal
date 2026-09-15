## Purpose

定义 AI 陪练的会话模型。每个规划周期最多一条 conversation，会话由「回合（turn）」驱动，每次回合把上下文喂给 LLM、执行工具调用、把结果落库，并通过一个可选的 `active_skill` 决定系统提示与行为流程。

## Requirements

### Requirement: 每周期一条会话

系统 SHALL 按规划周期持有会话，会话记录 `active_turn_id`、`revision`、`last_error` 与 `active_skill`。同一周期内 MUST 复用同一条会话，而不是每次对话新建。

#### Scenario: 打开某周期的 agent
- **WHEN** 用户在某个 Long-term 周期点击 "Plan with AI"
- **THEN** 系统返回该周期已有的会话；不存在则创建

#### Scenario: 切换到另一个周期
- **WHEN** 用户切到另一个周期并再次使用 agent
- **THEN** 系统使用该周期自己的会话，历史互不串台

### Requirement: 消息模型

会话中的每条消息 SHALL 带 `sequence_number`（会话内唯一且递增）与五选一的 `message_type`：`user`、`model_text`、`model_function_call`、`function_result`、`app_tool_result`。消息内容以 JSON 形式存于 `payload_json`。

#### Scenario: 一次工具调用的落库
- **WHEN** 模型决定调用 `update_goal_breakdown`
- **THEN** 依次写入 `model_function_call` 与（执行后的）`function_result`，序号连续

#### Scenario: 会话被删除
- **WHEN** 所属周期被删除
- **THEN** 该会话与其全部消息级联删除

### Requirement: 技能激活

系统 SHALL 向模型提供技能目录和 `load_skill` 工具，由 LLM 根据当前用户问题自行选择、加载和切换工作流。技能覆盖目标澄清、长期规划、周规划、日规划、优先级、单周期复盘、时段分析、计划问题诊断；无技能时为 Coach 引导。前端入口可提供初始规划范围，但 MUST NOT 将该范围固化为整段对话的技能。技能状态由工具结果推导并持久化。

#### Scenario: 启动长周期规划
- **WHEN** agent 调用 `start_planning` 且当前页面是 month 周期
- **THEN** `active_skill` 置为 `long_term_planning`，同一回合的下一轮模型调用立即使用对应系统提示和工具集合

#### Scenario: 启动周/日规划
- **WHEN** agent 调用 `start_planning` 且当前页面是 week 或 day 周期
- **THEN** 分别激活 `weekly_planning` 或 `daily_planning`，读取各自独立的技能文件

#### Scenario: 对专注块启动规划
- **WHEN** agent 在 session 周期上调用 `start_planning`
- **THEN** 返回 `unsupported_cycle_type`，且系统提示明确 "Agent mutations are not supported for session cycles."

#### Scenario: 未激活技能时的引导
- **WHEN** 会话处于 `none` 技能状态
- **THEN** 系统提示提供技能目录，模型先调用 `load_skill` 或明确的初始化工具，再进入相应流程

### Requirement: 回合执行与错误可见

每次回合 SHALL 被执行器的状态所约束：同一会话同一时刻最多一个进行中的回合；回合失败时错误写入 `last_error` 并可被前端读取。工具结果 SHALL 通过 `app_tool_result` 类型回灌给前端用于即时渲染。

#### Scenario: 回合进行中重复触发
- **WHEN** 前端在回合未结束时再次发送消息
- **THEN** 系统拒绝或排队，不产生并发写入

#### Scenario: 供应商报错
- **WHEN** LLM 调用失败
- **THEN** 错误归一化为 `AgentError` 写入 `last_error`，前端显示可读错误且会话保留

### Requirement: 上下文注入

每次回合 SHALL 注入机器可读的上下文，格式为带标签的文本块，包含当前周期元数据与当前时间。上下文 MUST 只含当前权限范围内可见的数据。

已验证的上下文格式（`<cycle>` 为必需块）：

```
<context>
  <cycle>
    <cycle_key>long-term:2026-09-14:2026-12-07</cycle_key>
    <parent_cycle_key>null</parent_cycle_key>
    <cycle_type>long_term</cycle_type>
    <cycle_length>3 months</cycle_length>
    <starts_on>2026-09-14</starts_on>
    <ends_on>2026-12-07</ends_on>
  </cycle>
  <current_date_and_time>2026-09-14T04:48:32.754004+00:00</current_date_and_time>
</context>
```

要点：

- `cycle_type` 的取值是**面向产品的词**（`long_term` / `short_term` / `day` 等），不是数据库的 `month`/`week`。数据库类型与模型看到的词汇是两套。
- `cycle_length` 是人类可读的时长标签（如 `3 months`），由时长换算得出。
- `parent_cycle_key` 无父时为字面量 `null`。
- 当前时间 MUST 注入，否则模型无法判断"今天"与剩余时间。

#### Scenario: 注入周期上下文
- **WHEN** 回合开始
- **THEN** 提示中包含周期键、父周期键、面向产品的周期类型、人类可读时长、起止日期

#### Scenario: 父周期不存在
- **WHEN** 当前周期没有父周期
- **THEN** `parent_cycle_key` 为 `null`，系统提示要求模型不要去取父周期上下文

#### Scenario: 注入当前时间
- **WHEN** 回合开始
- **THEN** 注入当前日期时间，使模型能相对"今天"推理

#### Scenario: 规划技能额外注入
- **WHEN** agent 处于规划类技能
- **THEN** 上下文额外包含父周期键与子周期键（区分已结束/当前/未来）与该周期的任务列表

#### Scenario: 单任务澄清时的上下文
- **WHEN** agent 处于 `goal_setting` 且已通过 `start_goal_setting` 选中某任务
- **THEN** 提示包含该任务的 `goal_breakdown`、`missing_fields`、`needs_refinement`、`needs_breakdown`

### Requirement: 应用侧工具调用

除了模型发起的工具调用，系统 SHALL 支持**应用侧**发起的工具调用，类型标记为 `app_tool_result`。这类调用用于在用户进入某个界面时初始化技能，不需要模型决定。

#### Scenario: 进入规划界面
- **WHEN** 用户在一个周期上启动规划
- **THEN** 应用侧调用 `start_planning`（无参数），结果包含激活的技能与完整周期上下文
- **AND** Plan with AI 的应用命令在激活后立即发起模型回合，使用该周期既有会话；用户不需要再点击发送才获得规划协助

#### Scenario: 应用侧结果对模型可见
- **WHEN** 应用侧工具调用完成
- **THEN** 其结果以 `app_tool_result` 消息写入会话，模型在后续回合能看到它

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

内置工作流 SHALL 首次补齐到 `~/.goal/skills/<name>/SKILL.md`。已有文件 MUST NOT 被启动或升级覆盖。每次加载技能及后续模型轮次 SHALL 读取当前文件；无效、缺失或过大的文件 MUST 给出可定位的错误，MUST NOT 静默忽略用户修改。技能仅定义工作方法，工具白名单、参数校验、事实读取和提案确认由代码控制。

#### Scenario: 对话内切换到季度分析
- **WHEN** 用户在日计划 Coach 中要求分析一个季度
- **THEN** LLM 可调用 `load_skill` 加载 `period-analysis`，再用 `get_period_context` 获取明确起止日期内的数据
- **AND** 周期选中态不限制分析范围；分析技能不提供计划写工具，未开放的工具调用由后端拒绝

#### Scenario: 修改个人工作流
- **WHEN** 用户修改 `daily-planning/SKILL.md`
- **THEN** 下一次加载该技能即使用新内容，无需重启应用

### Requirement: 可配置的会话空闲过期

Coach 上下文保留时间 SHALL 在设置中配置为 1–1440 整数分钟，默认 15。计时从最近完成的对话回合起算，读取会话不续期。截止时间由后端依据同一配置计算；前端按截止时间自动重取并清空显示。后端在读取、恢复及发起新回合前清除已过期消息、技能与错误；正在进行的回合不被中途清除。任务、提案和复盘数据独立于会话，不随清空删除。

#### Scenario: 调整过期时长
- **WHEN** 用户保存新的分钟数
- **THEN** 后续读取立即使用新时长；已打开 Coach 更新截止时间，若新截止时间已过去则清空

#### Scenario: 过期后发送
- **WHEN** 用户在截止时间之后发送新消息或从休眠恢复
- **THEN** 旧上下文不再发送给模型，新问题从空会话和技能选择开始

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
成功应用或放弃改动后，系统 SHALL 在当前会话中保存应用侧回执，并在消息滚动区按顺序显示；MUST NOT 把成功回执持久固定在输入框上方。

#### Scenario: 确认计划后继续聊天
- **WHEN** 用户确认任务提案后发送下一条消息
- **THEN** 回执保留在原消息位置，后续模型上下文知道该提案已经处理
- **AND** 输入框上方只保留尚未处理的确认项

### Requirement: 按技能提供有限的业务工具
系统 SHALL 提供文档化的工具数量、分类、参数和确认策略，当前目录为 24 项。分析与问题诊断技能 MUST NOT 提供写工具。模型 MUST NOT 获得通用 IPC、SQL、shell、凭据或确认自身提案的工具。具体清单见 docs/coach-tools.md。

#### Scenario: 模型修改设置
- **WHEN** 用户要求修改某项设置
- **THEN** 模型读取当前值，提出带参数的单项操作卡
- **AND** 用户 GUI 确认前，原设置保持不变
- **AND** 确认后重新执行设置校验并刷新界面

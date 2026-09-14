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

系统 SHALL 用四个技能之一驱动 agent 行为：`goal_setting`、`long_term_planning`、`short_term_planning`、`prioritization`；无技能时为 `none` 回退块。技能 MUST 由工具调用结果推导并持久化，而不是由前端直接指定。

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

## Why

`openspec/specs/` 里已有四个能力的规范描述产品最核心的智能行为——`agent-conversation`（会话与技能）、`goal-clarification`（把目标问清楚）、`prioritization`（取舍）、`planning-issues`（边写边审）。`rebuild-baseline` 只实现了它们的数据表（`agent_conversations` / `agent_messages`），没有任何行为。

这四个能力是同一条链路的四个环节，构成产品的**后端核心**：AI 审查发现问题 → 用户在某个目标上启动澄清 → 澄清产出可执行步骤 → 在候选变多时做取舍。规范已存在且经验证，本变更把它们变成可运行的后端。

## What Changes

- **LLM 抽象层**：`LLMProvider` 边界、供应商解析、带工具调用的对话与结构化输出两条通路
- **Agent 回合**：系统提示按技能选择、上下文注入、工具调用循环、消息落库（`agent_messages`，五种消息类型，序号唯一）
- **五个技能的系统提示**：`none` / `goal_setting` / `long_term_planning` / `short_term_planning` / `prioritization`，以及技能由 `start_*` 工具结果推导并持久化
- **工具集**：`start_planning` / `start_goal_setting` / `start_prioritization` / `get_cycle_context` / `get_task_details` / `create_goal` / `delete_goal` / `update_goal` / `update_goal_breakdown` / `update_prioritization_breakdown` / `move_goal`（全部写入走 `agent-proposals` 的预览层）
- **GoalBreakdown 引擎**：四部分结构、缺失字段计算、清晰度标记推导、标题精炼的四种情形
- **优先级引擎**：五桶结构、分类理由、增量合并与校验
- **计划审查**：边写边审的触发与去抖、问题类型识别、忽略的持久化

## Capabilities

### New Capabilities

无。四个能力的规范已在 `openspec/specs/` 中存在，本变更是实现而非改需求。

## Impact

- 后端：新增 `ai/agent`（回合、提示、工具、会话持久化）、`ai/goal_breakdown`、`ai/prioritization`、`review`（计划审查）；`commands` 增加 agent 相关命令
- 数据：无新表（表已由 `rebuild-baseline` 建好）；`agent_conversations.active_skill` 开始被写入
- 前端：agent 侧栏、澄清面板、优先级面板、问题提示（前端为独立任务，本变更为其提供后端）
- 依赖：需要 `rebuild-baseline`（数据层、预览机制、事件）与 `add-ai-access-and-voice`（凭据与供应商解析）。二者未完成时，本变更的 LLM 层可用假 provider 测试
- 顺序：本变更之后才能做 `add-review-retrospective` 的复盘技能（它复用本变更的技能机制）

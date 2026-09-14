## 1. LLM 抽象层

- [ ] 1.1 `ai/llm/mod.rs`：`LlmProvider` trait（`generate_agent` / `generate_json`）、`AgentRequest` / `AgentResponse` / `LlmRequest` 类型
- [ ] 1.2 工具定义类型：名称、描述、参数 JSON Schema；一个 assembler 收集全部工具定义
- [ ] 1.3 `ai/llm/resolved.rs`：按设置解析当前 provider（`hyperfocus` / `openrouter` / `local`）
- [ ] 1.4 `ai/llm/fake.rs`：`FakeProvider`，按脚本返回预置工具调用与文本，供测试使用（`#[cfg(test)]` 或 test 专用 feature）
- [ ] 1.5 测试：请求/响应序列化往返；provider 解析在三种设置下的结果

## 2. 会话与回合

- [ ] 2.1 `ai/agent/conversation.rs`：按 `cycle_id` 取或建会话（每周期唯一）
- [ ] 2.2 `ai/agent/service.rs`：回合执行器——组装请求 → 调用 provider → 执行工具 → 回灌结果 → 循环（上限 8 轮）
- [ ] 2.3 消息落库：回合结束后按最终顺序一次性写入 `agent_messages`，`sequence_number` 连续；失败时只写 `last_error`
- [ ] 2.4 并发保护：同一会话同时只允许一个进行中的回合，重复触发返回明确错误
- [ ] 2.5 测试：单回合无工具调用；单回合含一次工具调用；多轮工具调用；达到轮数上限；回合失败后会话可继续且消息不乱序

## 3. 技能与提示

- [ ] 3.1 `ai/agent/prompt.rs`：`build_system_instruction(ActiveAgentSkill)`，返回五套提示之一
- [ ] 3.2 五个技能的系统提示文本（`none` / `goal_setting` / `long_term_planning` / `short_term_planning` / `prioritization`），行为约束照 `openspec/specs/` 的对应场景
- [ ] 3.3 技能激活：`start_*` 工具结果携带 `activated_skill`，由回合执行器写入 `agent_conversations.active_skill`；前端无写入路径
- [ ] 3.4 `start_planning` 的类型分派：`month` → `long_term_planning`；`day|week` → `short_term_planning`；`session` → `unsupported_cycle_type`
- [ ] 3.5 测试：四个技能各自的激活路径；session 被拒；`active_skill` 只在会话行上被写、且值合法

## 4. 上下文注入与裁剪

- [ ] 4.1 `ai/agent/context.rs`：构造 `CycleContextPayload`（周期元数据 + 父子键 + 任务快照）
- [ ] 4.2 三档优先级与裁剪函数（纯函数，可单测）
- [ ] 4.3 父周期不存在时 `parent_cycle_key` 为 null，且提示要求模型不要取父上下文
- [ ] 4.4 子任务以 Markdown 渲染
- [ ] 4.5 测试：裁剪在各预算下的结果；无父周期；空任务列表；子任务渲染

## 5. 工具集

- [ ] 5.1 `get_cycle_context` / `get_task_details`：只读工具，参数 `cycle_key` / `task_id`
- [ ] 5.2 `start_planning` / `start_goal_setting` / `start_prioritization`：激活类工具，返回上下文与激活的技能
- [ ] 5.3 写工具：`create_goal` / `delete_goal` / `update_goal` / `move_goal`——**全部经预览层**，带 `rationale`
- [ ] 5.4 `create_goal` 的空行复用：列表尾部存在空任务行时复用之
- [ ] 5.5 `move_goal` 的校验：源/目标可用性、周期已结束、任务不在源等，返回规范里的错误码
- [ ] 5.6 工具参数严格反序列化：未知字段或类型不符时返回明确错误
- [ ] 5.7 测试：每个工具一例；写工具调用后确认无正式数据变化、只有预览行；预览计数事件被发射

## 6. GoalBreakdown 引擎

- [ ] 6.1 四部分类型与序列化（`context` / `output` / `outcome` / `scope`）
- [ ] 6.2 合并语义：`null` 清除、缺省保留；空更新返回 `empty_update`
- [ ] 6.3 缺失字段计算（至少覆盖 `context.clarification` / `output.value` / `outcome.value` / `outcome.verification_method`）
- [ ] 6.4 清晰度标记推导：`needs_refinement` 与 `needs_breakdown` 的清除条件分开
- [ ] 6.5 `update_goal_breakdown` 工具 + 返回 `TaskContextSnapshot`
- [ ] 6.6 测试：部分更新不清空其他字段；显式 `null` 清除；空更新被拒；四种标题精炼情形各一例

## 7. 优先级引擎

- [ ] 7.1 五桶类型与持久化（`cycles.prioritization_breakdown`）
- [ ] 7.2 增量合并：未提交的桶保留，除非显式清空；结构校验失败则拒绝写入
- [ ] 7.3 `start_prioritization` 返回持久化 breakdown + 尚未分类的候选
- [ ] 7.4 `update_prioritization_breakdown` 工具：每个落桶项必须带用户给出的理由
- [ ] 7.5 模型可读渲染（XML 转义）
- [ ] 7.6 测试：只更新一个桶时其他桶保留；非法结构被拒；转义；无候选任务时的引导

## 8. 计划审查（边写边审）

- [ ] 8.1 触发：内容变更后去抖（默认 800 ms），内容哈希未变则用缓存
- [ ] 8.2 问题识别：`too_many_goals` / `too_many_tasks` / `too_much_work` / `not_sure_what_to_do_next` / `missing_something` / `not_useful_for_needs`
- [ ] 8.3 问题报告：按周期返回完整清单，支持周期级与任务级
- [ ] 8.4 忽略的读写（复用 `planning_issue_dismissals`），忽略后不再出现在报告与就地提示中
- [ ] 8.5 审查不可用（无供应商/额度用尽）时静默跳过，不阻断编辑、不弹错误
- [ ] 8.6 测试：去抖行为；缓存命中；忽略后过滤；不可用时的降级；审查不阻塞编辑路径

## 9. IPC 与事件

- [ ] 9.1 命令：`start_agent_conversation` / `send_agent_message` / `get_agent_conversation` / `get_previous_agent_conversation` / `start_planning` / `start_goal_setting` / `start_prioritization`
- [ ] 9.2 命令：`get_planning_issue_report` / `dismiss_planning_issue` / `get_planning_issue_dismissals`
- [ ] 9.3 事件：`agent:conversation_updated`（携带会话 id 与 revision，只做失效通知）
- [ ] 9.4 错误映射：`unsupported_cycle_type` / `empty_update` / `provider_required` / `task_not_found` 等 `code` 与规范场景对应
- [ ] 9.5 测试：命令级主路径；错误 code；事件在回合结束后发射一次

## 10. 验收

- [ ] 10.1 `cargo test` 全绿，且四个能力的测试全部用 `FakeProvider` 离线运行
- [ ] 10.2 手工（配好真实供应商）：在一个长目标上走完澄清——agent 一次问一个问题、标题被精炼、步骤经确认后写回
- [ ] 10.3 手工：agent 的每一次写入都出现在待确认列表，Revert 后数据完全还原
- [ ] 10.4 手工：故意写一个模糊目标，确认就地出现审查提示且不打断输入
- [ ] 10.5 手工：关掉 AI 供应商，确认审查静默跳过、其余功能正常
- [ ] 10.6 更新 `docs/architecture.md` 的 IPC 契约与模块图

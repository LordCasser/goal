## Context

四个能力的规范已经写完并通过校验，本文只记录**怎么实现**的取舍。

要实现的规范：
- `openspec/specs/agent-conversation/spec.md`
- `openspec/specs/goal-clarification/spec.md`
- `openspec/specs/prioritization/spec.md`
- `openspec/specs/planning-issues/spec.md`

已由 `rebuild-baseline` 提供：`agent_conversations` / `agent_messages` 两张表、预览层（`task_preview_originals` + `tasks.agent_proposal`）、事件发射器、`cycles` / `tasks` 的领域与仓储。

## Goals / Non-Goals

**Goals:**

- 让四个能力的行为可运行、可测试，且**测试不依赖真实 LLM**
- 保持上游已经验证过的关键设计：技能由工具结果推导、写入一律走预览、清晰度由计算得出
- 上下文注入有明确的裁剪策略，避免超出模型窗口后行为不可预测

**Non-Goals:**

- 不做前端界面（前端是独立任务）
- 不做复盘技能（`add-review-retrospective`）与计划问题之外的扩展
- 不做向量检索 / 长期记忆 / 多会话并发

## Decisions

### D1：LLM 抽象只要求两个方法

```rust
#[async_trait]
trait LlmProvider: Send + Sync {
    /// 带工具定义的对话，返回文本 + 工具调用
    async fn generate_agent(&self, req: AgentRequest) -> Result<AgentResponse>;
    /// 结构化输出（用于清晰度评估等不需要工具的场景）
    async fn generate_json(&self, req: LlmRequest) -> Result<serde_json::Value>;
}
```

上游实现里 `generate_agent_once` / `generate_json_once` 是必需方法，`generate_agent` / `generate_json` 是默认实现（内部包一层）。我们直接合并为两个方法——那一层包装在上游没有可见的用途。

**测试策略**：`FakeProvider` 按脚本返回预置的工具调用序列，覆盖回合循环、技能激活、错误路径。这样四个能力的测试完全离线且确定。

### D2：技能由工具结果推导，不由前端指定

`active_skill` 的写入路径只有一条：

```
工具被调用 → 工具返回结果（含 activated_skill）→ 回合执行器写 agent_conversations.active_skill
```

前端不能直接设置它。这是上游验证过的设计：技能的真相在工具层，前端只是渲染者。若允许前端设置，就会出现"界面说在澄清、实际按规划提示跑"的错位。

`start_planning` 按周期类型分派：`month` → `long_term_planning`，`day|week` → `short_term_planning`，`session` → `unsupported_cycle_type` 错误。

### D3：写入工具一律调用预览层，工具层不感知

工具实现里**不写**"是否预览"的判断逻辑，而是全部经 `service::proposals` 的同一入口。这样：

- 新增工具时不会漏掉预览
- 预览机制的语义只有一处（`agent-proposals` 规范）
- 测试可以断言"任何工具调用后 `agent_proposal` 非空的行数与预期一致"

### D4：上下文裁剪按重要性分层

注入内容分三档，超出预算时从低档开始丢：

| 档 | 内容 | 丢弃顺序 |
| --- | --- | --- |
| 必需 | 当前周期元数据、当前任务快照、`missing_fields` | 永不丢弃 |
| 重要 | 当前周期的任务列表（标题 + 标记） | 2 |
| 可选 | 父周期上下文、子周期键、历史复盘结论 | 1（先丢） |

理由：澄清问答只需要"当前这个目标 + 它在哪个周期"，历史与远端周期是锦上添花。本地模型窗口小，这个策略决定它在本地能否可用。

### D5：审查去抖 + 只审可审项

`planning-issues` 要求"边写边审"。朴素做法是每次编辑都发请求，代价是打字过程中反复触发。

选择：**停止输入后去抖触发**（默认 800 ms），且只对"文本有实质变化"的行重审（用内容哈希比对）。审查结果是幂等的替换，不是累加。

`review` 能力不可用时静默跳过，不阻断编辑——这是规范里明确要求的。

### D6：优先级 breakdown 用 JSON 列而不是拆表

`PrioritizationBreakdown` 五桶存在 `cycles.prioritization_breakdown` 一列里，而不是五张关系表。

理由：它总是整体读写（一次排序会话产出完整结论），没有按桶查询的需求。拆表会带来 join 与部分更新的一致性负担。代价是失去 SQL 层对桶内结构的约束——用 Rust 侧的反序列化 + 校验补上。

## Risks / Trade-offs

| 风险 | 影响 | 处理 |
| --- | --- | --- |
| 假 provider 通过但真实模型行为不同 | 测试给了虚假信心 | 保留一个可选的真实 provider 冒烟测试（标记为 ignored，需凭据时手动跑）；工具参数 schema 用严格反序列化，模型给错参数时明确报错而非静默忽略 |
| 上下文裁剪逻辑复杂易错 | 模型拿到残缺上下文，行为诡异 | 裁剪函数是纯函数并有单测；开发模式下把实际注入内容落日志便于排查 |
| 回合中途失败留下半截会话 | 用户看到断裂的对话 | 消息在回合结束后一次性按序落库（沿用流式不落库的原则）；失败时写 `last_error` 且不产生半截消息 |
| 工具调用循环无终止 | 死循环烧 token | 设最大工具调用轮数（默认 8），超出后以错误结束并保留已产生的消息 |
| 审查请求与用户输入竞争 | 界面闪烁 | 审查结果按 `task_id + 内容哈希` 缓存；内容未变时直接用缓存 |

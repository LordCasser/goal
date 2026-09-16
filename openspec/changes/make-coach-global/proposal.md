## Why

当前 Coach 按规划周期分割会话，用户切换周、日、长期计划或 Workspace/Calendar 时会失去正在阅读的上下文、草稿和在途回合，待确认改动也容易脱离原页面。Coach 需要成为一个跨页面、跨选中计划的连续工作流，同时让每次模型请求仍以发送瞬间的页面选择为事实快照，避免把浏览动作误当成对话输入。

## What Changes

- 将 Coach 改为全局唯一会话：一条历史、一份草稿和阅读滚动状态跨长期、周、日、Workspace、Calendar 保留，页面切换不重建会话或取消在途回复。
- 每次发送 LLM 请求时快照当前页面状态，向上下文注入 `view`、`long_term_cycle_id`、`week_cycle_id`、`day_cycle_id`、`week_starts_on`、`selected_date`；选择为空时明确表达无计划，浏览或选中变化不单独写入用户消息或触发请求。
- 在在途回合中固定该次请求的默认工具 scope；页面切换只影响后续发送，不改变已经发出的工具调用目标。
- 让待确认操作跨页面持续可见，确认/放弃始终使用原提案目标；应用回执进入全局聊天历史并按回合顺序保存。
- 以迁移 14 移除 `agent_conversations` 到 `cycles` 的外键，按 `turn_id` 保序合并既有消息且不丢失；使用固定主键和 CHECK 约束保证单例，删除计划不删除 Coach 历史；迁移完成后清空无法在重启后续用的进行中回合标记。
- 保留现有 1–1440 分钟闲置 TTL、正在执行回合不清理及失败可见语义；不新增独立会话实体或额外 UI。沿用现有英文 skill 和根据界面语言设置生成输出语言要求的机制，不修改 skill 文件。

## Capabilities

### New Capabilities

无。本变更把现有会话能力从周期作用域收敛为全局作用域。

### Modified Capabilities

- `agent-conversation`：将每周期会话改为全局单例，增加页面选择快照、固定在途 scope、全局历史与跨页面确认回执，并保留技能、工具、TTL 和回合语义。
- `planner-workspace`：Coach 侧栏、入口、待确认入口和页面切换改为共享全局会话，保留草稿、滚动位置和在途回复。
- `local-persistence`：迁移 14 保全并合并已有会话消息，解除周期外键；计划删除不再级联删除聊天记录。

## Impact

- SQLite `agent_conversations`、`agent_messages` 及迁移序列需要把已有周期会话合并为单例，并用固定主键和 CHECK 约束防止多行；迁移必须保留 turn 顺序、消息内容、技能/错误，清空无法续用的进行中回合标记。
- Rust agent repository/service、上下文构造、工具默认 scope、IPC 入参/返回和失效事件需要从 `cycle_id` 作用域改为全局会话加发送时页面快照。
- React Coach 面板、工作台与日历页面需要提升会话状态的持有范围，跨页面保留草稿、滚动位置、在途状态和待确认项；不增加新的会话实体或独立 UI。
- 需要覆盖全局单例、迁移保全、无计划选择、浏览不触发请求、页面切换期间在途工具 scope、跨页面确认以及闲置 TTL 的后端、前端和集成验收。
- 前置依赖为 `rebuild-baseline` 的本地迁移/事件纪律、`add-ai-planning-core` 的回合执行器和 `add-review-retrospective` 已建立的技能/回执扩展；本变更不改现有 skill 文件代码。

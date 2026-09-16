## Context

当前 `agent_conversations` 以 `cycle_id` 关联规划周期，工作台入口因此把 Coach 视为当前周期的页面状态。前端切换页面会重新选择会话，后端工具也从当前页面读取默认范围。目标和约束见 [proposal.md](proposal.md)；本设计只解决如何把既有会话收敛为一条全局会话，同时保留现有的提案确认、闲置 TTL 与数据库单一真相源。

## Goals / Non-Goals

**Goals:**

- 让 `agent_conversations` 现有表承载唯一的 `coach` 会话，消息序号、回合状态和技能状态均以该会话为范围。
- 在发送时把页面状态复制成不可变快照，供 prompt、默认工具 scope、应用侧工具和确认回执使用；页面浏览不产生模型副作用。
- 通过一次原子迁移把旧周期会话按回合顺序合并，保留每一条消息及其原始 `turn_id`。
- 让全局会话状态在应用 shell 级别存活，满足切页、切周期、滚动、草稿和在途回合的连续性。

**Non-Goals:**

- 不引入独立的 Later 或 conversation 实体，也不实现消息与计划关系的历史重建。
- 不改变普通计划移动、技能工具白名单、提案预览/Keep 语义或既有闲置 TTL 数值规则。
- 不把页面浏览、恢复滚动或打开无目标 Coach 转化为用户消息、`start_planning` 或其他请求。
- 不承诺旧数据库结构、旧客户端或旧会话接口继续并行工作；迁移完成后以全局契约为唯一读写路径。

## Decisions

### 1. 用固定主键表达单例，不增加 guard 实体

迁移 14 将 `agent_conversations` 重建为保留现有字段的表，移除 `cycle_id` 及其外键，并增加 `CHECK (id = 'coach')`。`agent_messages.conversation_id` 仍引用该表，`UNIQUE (conversation_id, sequence_number)` 继续保证全局序号。Repository 只通过 `get_or_create_conversation` 取得 id 为 `coach` 的这一行；初始化在短 `BEGIN IMMEDIATE` 事务中插入或读取，依靠主键和 CHECK 抵御并发入口。闲置清理和确认回执的独立写事务同样先取得写锁，避免 WAL 读快照升级时发生锁冲突；已有调用方事务继续复用原边界。

选择固定主键而不是另建 `conversation_guard` 或新增实体，是因为单例约束属于既有会话表的数据不变量；这样不会把 UI 状态或数据库中的另一种会话身份引入架构。

### 2. 迁移按回合拼接并重新编号

迁移在现有 SQLite 迁移事务中读取所有旧周期会话和消息，再建立新的单例表。每个旧会话按 `turn_id` 聚合；按 `turn_created_at`、旧 `conversation_id`、`turn_first_sequence`、`turn_id`、旧消息序号排序。按该顺序把所有消息写入 `conversation_id='coach'`，重新分配从 1 开始的连续序号，保留原 `turn_id`、`message_type`、`payload_json` 和创建时间。

全局行的 `revision` 取旧行修订号的最大值，`active_skill` 和 `last_error` 取 `updated_at` 最近的一整行（包括该行的 `null` 值）；迁移完成后清空 `active_turn_id`，因为进程重启后不能续用旧执行锁，旧回合的所有已落库消息仍然保留。实际排序键为 `turn_created_at`、旧 `conversation_id`、`turn_first_sequence`、`turn_id`、旧消息序号，随后以窗口行号重新编号。迁移使用现有 SQLite 迁移事务完成，不额外假设迁移锁级别。

选择“按回合排序后重编号”而不是直接拼接每个周期的序号，是为了避免周期内序号相同导致历史顺序依赖遍历顺序；选择保留原 payload 而不是重写为新格式，是为了不丢失旧回执和工具参数。

### 3. 把页面选择建模为发送快照

AgentPanel 由应用 shell 稳定挂载；消息、草稿和滚动位置继续由现有 React state/ref 管理，后端 `ConversationView` 只报告全局会话和 `active_turn_id`。请求的 `pageContext` 使用 `long_term_cycle_id`、`week_cycle_id`、`day_cycle_id` 表达选择；每次显式发送或明确的 AI 入口捕获当前值，并在 `<page_state>` 块中呈现 `active_cycle_id`、`view`、`week_starts_on`、`selected_date` 以及带标题和起止日期的 `selected_long_term`、`selected_week`、`selected_day`。不存在的计划或选择使用 `null`。没有计划时允许直接打开全局 Coach；计划专用入口必须携带目标周期。

后端在接受发送命令时捕获并冻结这份上下文；`load_turn_context` 只在回合开始加载一次计划事实，生成 `<page_state>`、解析默认工具 scope 并记录应用侧结果。执行器只使用已加载的回合上下文，不在后续工具调用中重新读取前端当前选择。切页只更新下一次发送可用的页面状态，不触碰全局回合。

选择发送快照而不是让每个工具查询“当前页面”，是为了在异步 LLM/tool 链路中消除竞态；选择在后端重新生成 prompt 上下文而不是信任前端拼接，是为了保持权限、日期与周期层级校验集中在后端。

### 4. 保留既有应用侧入口和确认模型

打开全局 Coach、切换 Workspace/Calendar、选中周期和滚动均是纯页面操作。只有显式发送、计划区的 Plan with AI、目标澄清入口或确认操作才可产生 `user`/`app_tool_result` 或启动回合。无计划时 Coach 可打开并发送普通问题，但不会隐式创建计划；Plan with AI 入口仍需一个具体目标，`start_planning` 的目标由后端依据入口快照解析。

待确认项继续由现有提案/顶栏入口承载。任务预览确认按 `task_id` 与 approve/reject 定位；非任务 `agent_actions` 使用 `cycle_id`、`action_id` 与 approve，动作保留原始 `source_cycle_id` 供后端校验。两类确认都不使用确认时的浏览 scope；成功或放弃的 `app_tool_result` 作为全局消息落库，即使用户已经切到别的页面也归属于原回合。

确认回执沿用全局回合并发边界：同步确认在同一事务中 claim 全局回合、执行动作、写入回执并 finish；异步确认在执行前取得全局回合 guard，直到回执完成才释放。guard 只是代码作用域内的小型 RAII 对象，不新增数据库实体。忙时先拒绝确认，不修改计划；失败路径由持有 owner token 的 guard 负责释放，避免留下占用状态。

### 5. 在应用 shell 提升会话状态，沿用 TTL 与事件失效

AgentPanel 的挂载位置提升到工作台 shell，页面组件只订阅全局会话查询和事件失效，不按周期卸载会话。`active_turn_id` 在 `ConversationView` 与后端读模型中可见，使页面切换期间仍显示后台忙状态；事件只携带全局会话的失效标识，业务消息仍通过查询读取。

闲置 TTL 仍以最近完成回合计算，读取、滚动、切页和打开侧栏不续期；读取和新回合前清理过期上下文，进行中的回合由 `active_turn_id` 保护。这样只改变会话作用域，不新增“页面活跃”或“前端保活”规则。

## Risks / Trade-offs

- **[旧历史的时间并列]** 不同周期会话可能共享时间戳。→ 使用迁移 SQL 的 `turn_created_at`、`old_conversation_id`、`turn_first_sequence`、`turn_id`、`old_sequence_number` 五个排序键，并把合并顺序固定在迁移测试中。
- **[并发初始化或发送]** 多入口可能同时创建会话或发送回合。→ 固定主键/CHECK、数据库事务和 `active_turn_id` 并发守卫共同拒绝第二个会话或并发 turn。
- **[切页造成错误 scope]** 异步工具若重新读页面会改动原目标。→ 工具执行器只接受发送快照，确认只按 taskId 与原 `source_cycle_id` 校验。
- **[无计划上下文过度推断]** 页面没有周期时模型可能尝试写入计划。→ 快照明确使用 `null`，后端对缺少目标的计划工具拒绝，普通 Coach 仍可对话。
- **[全局历史增长]** 单一历史比周期会话更长。→ 继续使用现有 TTL 清理规则和服务端过期判定，不新增第二套自动归档策略。

## Migration Plan

1. 增加版本 14：在迁移事务中读取旧表，创建带单例 CHECK 的新表，按回合合并消息并连续编号，替换旧表并移除 `cycle_id` 外键；迁移测试验证完整性、序号连续性和历史保留。
2. 更新 Rust repository/service/IPC 的读写入口为固定 `coach` 会话，所有发送和应用侧工具命令都捕获页面上下文；同步全局事件与 `ConversationView.active_turn_id`。
3. 将 AgentPanel 挂载位置提升到应用 shell，接入既有待确认查询和事件失效；页面选择只更新下一次请求的 pageContext，不触发发送。
4. 以迁移数据库、并发初始化、跨页面在途回合、无计划发送、确认原目标、TTL 与删除计划保留历史为集成验收边界；再用 GUI 验证全局历史、草稿、滚动、忙状态和待确认入口。

迁移是版本化顺序中的前向变更。事务失败时保留未成功状态并在下次启动重试；不提供旧 schema 与新 schema 并行读写路径。

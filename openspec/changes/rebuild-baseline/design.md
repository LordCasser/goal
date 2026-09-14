## Context

本变更把 `openspec/specs/` 里已经写好的行为规范变成可运行的软件。模块边界、数据模型与 IPC 契约记录在 `docs/architecture.md`（权威来源），本文只记录本变更的取舍与风险。

约束来源：

- 目标行为由 12 个能力规范定义，其中本变更实现 `planning-cycles`、`task-graph`、`agent-proposals` 的数据与界面部分。
- 视觉与交互要还原被拆解应用的设计原则（等宽紧凑、单一强调色、直角、细线分隔、空状态即说明书、键盘优先），但**前端代码必须原创**：原应用的前端 bundle 是其版权作品，只作为行为与视觉语言的参考，不复制其源码或样式表。
- 原应用的技术选型（Tauri + WebView + 本地 SQLite）值得沿用；前端框架按用户要求换成 React。

## Goals / Non-Goals

**Goals:**

- 一条可运行、可测试的核心链路：创建长周期 → 添加目标 → 周计划 → 日任务 → 专注块
- 把「规则写在数据库里」这件事真正做到位（触发器 + CHECK + 部分唯一索引）
- 把 agent 提议机制（预览/快照/Keep-Revert）的**数据层与交互**先建好，AI 接入留到后续变更
- 为四个后续扩展留出干净的落点，不需要重做迁移

**Non-Goals:**

- 不实现 LLM 调用、agent 对话循环、goal breakdown 计算（`agent-conversation`、`goal-clarification`、`prioritization`）
- 不实现语音、授权/试用、引导清单、退出调查、自动更新
- 不做暗色主题、多窗口、跨设备同步
- 不追求与原应用像素级一致

## Decisions

### D1：SQLite 访问用 `rusqlite`（bundled）而不是 `sqlx`

`rusqlite` 同步、无异步运行时耦合，`bundled` 特性把 SQLite 编进产物，避免用户机器上的 SQLite 版本差异。`sqlx` 的编译期查询校验在这个规模下收益有限，代价是编译时间与 `async` 传染到整个 service 层。

代价：需要在 Tauri 的同步命令上下文里访问连接池；用 `r2d2` 管理，命令保持同步即可。

### D2：手写版本化迁移，不用 ORM 迁移工具

规范要求「带版本号与校验和的迁移序列，按序执行，不重复执行，失败可重试」。自建 `schema_migrations(version, description, checksum, applied_at)` + 一个 40 行的执行器就够，且校验和逻辑（迁移文件内容变了要报错）必须自己控制。

代价：迁移要手写；收益：行为完全可预测，测试可以直接断言。

### D3：不变量下沉到 schema

`root_color_key`、`calendar_key` 唯一性、生命周期单调这些规则写成触发器与部分索引，而不是在 service 里写 `if`。

理由：这些是**数据完整性**约束，任何写入路径（包括未来接入的 agent、批量导入、备份恢复）都必须满足。放在 service 里迟早被绕过。

代价：违反时错误来自 SQLite（`RAISE(ABORT, ...)`），需要在 `error.rs` 把 `SQLITE_CONSTRAINT_TRIGGER` 映射成带 `code` 的 `Conflict`，否则前端只能显示原始英文。

### D4：事件只做失效通知，不携带业务数据

事件载荷只有 `{ cycle_ids: [...] }`。前端收到就失效对应 react-query 查询重新拉取。

替代方案是事件里带上变更后的实体，省一次往返。不采用的理由：那等于维护一份前端缓存真相，离线和并发场景下很容易与数据库不一致；本地 SQLite 查询代价极低，没必要省这一跳。

### D5：`week` 的 `calendar_key` 固定用 ISO 周编号

设置里的 `week_start_day` 只影响界面如何排布一周，不影响键的生成。若让键随设置变化，用户改一次设置就会产生重复的周周期（旧键孤儿 + 新键重复）。

### D6：前端只有拖拽排序做乐观更新

其余操作等事件回来刷新。拖拽如果等往返会明显卡顿，而排序失败可以整表重取回滚，风险可控。

### D7：`agent_conversations` / `agent_messages` 在本变更建表但不写入

后续 `agent-conversation` 变更不需要新增迁移就能开工。空表不产生行为，也不违反「不为未确认需求增加实体」——这两个实体已由既有规范确认。

## Risks / Trade-offs

| 风险 | 影响 | 处理 |
| --- | --- | --- |
| SQLite 触发器错误信息翻译不全 | 用户看到英文原句或 `UNKNOWN` | `error.rs` 里对每个 `RAISE(ABORT, ...)` 文本建立映射表；映射缺失时降级为通用文案并记录 |
| `r2d2` 池在写事务上竞争 | 长时间写入阻塞读 | 事务保持短小；`busy_timeout` 设为 5s；禁止在事务内做耗时计算或网络调用 |
| 手写 SQL 无编译期校验 | 字段改名后运行时才发现 | 每个 repository 函数配一个针对临时库的集成测试 |
| 事件风暴（一次操作发多个事件） | 前端重复拉取 | 事件在 service 层按 `cycle_id` 聚合去重后再发射 |
| 前端原创实现与原应用视觉有偏差 | 用户觉得"不像" | 以 `analysis/reports/design-system.md` 的 token 表为设计输入，按原则重画而不是逐像素复刻；偏差在提交说明里如实记录 |
| 日期处理集中在 `domain/calendar.rs` | 一旦有 bug 影响面大 | 该模块配最密的单元测试（跨月、跨年、ISO 周边界、夏令时） |

## Migration Plan

无既有实现、无存量数据，不需要数据迁移。首次启动创建库并顺序应用全部迁移。开发期允许删除本地库重建。

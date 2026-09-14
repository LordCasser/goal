# 架构（自有实现）

> 本文是模块边界、依赖方向与数据所有权的权威来源。
> 需求行为以 `openspec/specs/` 为准；需求从何而来见 `analysis/reports/hyperfocus-deconstruction.md`。
> 本文描述的是**我们自己的实现**，技术栈与原应用不同（React 而非 SolidJS），前端代码为原创。

## 技术栈

| 层 | 选型 | 理由 |
| --- | --- | --- |
| 外壳 | Tauri 2 | 小体积、原生集成（钥匙串/通知/菜单/更新），且已验证适合这类本地优先应用 |
| 后端 | Rust（单进程） | 与 SQLite、系统 API 同侧；领域不变量可编译期约束 |
| 前端 | React 19 + TypeScript + Vite | 生态最成熟，组件库与工具链选择多 |
| 样式 | Tailwind CSS | 设计语言以 token 为主，工具类足够 |
| 数据库 | SQLite（`rusqlite`，`bundled`） | 本地单用户；bundled 避免依赖系统 SQLite 版本 |
| 连接池 | `r2d2` + `r2d2_sqlite` | 读并发 + 写串行；避免把 `Connection` 手动塞进全局锁 |
| 前端状态 | `@tanstack/react-query` + Tauri 事件 | 后端是唯一真相源，事件驱动失效缓存 |

不引入：异步运行时耦合的 ORM（sqlx/diesel）。迁移与查询手写 SQL，理由见"决策"。

## 分层与依赖方向

```
┌──────────────────────────────────────────────────────────┐
│ src/ (React)                                              │
│   features/*  按能力组织 UI                               │
│   lib/ipc.ts  唯一的 invoke 封装，命令名集中声明           │
│   lib/events.ts 订阅 tauri 事件 → 失效 react-query 缓存    │
└────────────────────────┬─────────────────────────────────┘
                         │ invoke / event（唯一跨进程边界）
┌────────────────────────┴─────────────────────────────────┐
│ src-tauri/src/commands/*    IPC 边界：参数校验 + 调用 service │
│         │  唯一能被前端调用的层；不含业务规则               │
├─────────┴─────────────────────────────────────────────────┤
│ service/*    用例编排：事务边界、跨聚合协作、事件发射        │
│         │                                                 │
├─────────┴─────────────────────────────────────────────────┤
│ repository/*  单一聚合的 SQL 读写；不做跨聚合编排           │
│         │                                                 │
├─────────┴─────────────────────────────────────────────────┤
│ domain/*     纯类型 + 不变量函数；无 IO、无 tauri、无 SQL   │
│         │                                                 │
├─────────┴─────────────────────────────────────────────────┤
│ db/*  pool / migrations / backup                          │
└──────────────────────────────────────────────────────────┘
```

依赖只向下。`domain` 不依赖任何上层；`commands` 不直接写 SQL。

## 目录结构

```
src-tauri/src/
├── main.rs            启动、窗口、菜单、命令注册
├── error.rs           AppError（可序列化）+ 与领域错误映射
├── db/
│   ├── mod.rs         pool 初始化、PRAGMA（WAL/foreign_keys/busy_timeout）
│   ├── migrations.rs  版本化迁移执行器 + MIGRATIONS 常量
│   └── backup.rs      备份导出
├── domain/
│   ├── cycle.rs       CycleType、生命周期状态、日历键
│   ├── task.rs        Task、TaskTree、清晰度标记
│   ├── proposal.rs    预览态与快照语义
│   └── calendar.rs    日期/周键计算（唯一处理日期的地方）
├── repository/
│   ├── cycles.rs
│   ├── tasks.rs
│   └── proposals.rs
├── service/
│   ├── cycles.rs      创建/启动/结束/复制/删除守卫
│   ├── tasks.rs       增删改、移动、链接、着色
│   └── proposals.rs   预览写入、Keep/Revert
├── events.rs          CycleEvent / TaskEvent 发射器
└── commands/
    ├── mod.rs         命令注册清单
    ├── cycles.rs
    ├── tasks.rs
    └── proposals.rs

src/
├── main.tsx / App.tsx
├── lib/ipc.ts         命令封装（唯一 invoke 出口）
├── lib/events.ts      事件订阅
├── features/planner/  工作台：横向周期列
├── features/later/    Do Later 侧栏
├── features/proposals/待确认改动底栏
└── ui/                基础组件（Button/Input/Dialog/...）
```

## 数据模型

### 表

```sql
cycles(
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  type TEXT NOT NULL CHECK (type IN ('session','day','week','month')),
  parent_id TEXT REFERENCES cycles(id) ON DELETE CASCADE,
  position INTEGER NOT NULL DEFAULT 0,
  archived INTEGER NOT NULL DEFAULT 0,
  started INTEGER NOT NULL DEFAULT 0,
  finished INTEGER NOT NULL DEFAULT 0,
  started_at INTEGER, finished_at INTEGER,
  duration INTEGER,              -- 毫秒；NULL = 未定时长（Do Later）
  focused_time INTEGER NOT NULL DEFAULT 0,  -- 毫秒
  repeat_id TEXT REFERENCES repeats(id),    -- 由哪个重复模板生成；模板移除时置空而非级联删除
  starts_on TEXT, ends_on TEXT,  -- 'YYYY-MM-DD'
  calendar_key TEXT,             -- 见"日历键"
  created_at INTEGER NOT NULL,
  CHECK (NOT (started = 0 AND finished = 1))
);
CREATE UNIQUE INDEX ux_cycles_calendar_key
  ON cycles(calendar_key) WHERE calendar_key IS NOT NULL;
CREATE INDEX ix_cycles_parent ON cycles(parent_id);

tasks(
  id TEXT PRIMARY KEY,
  cycle_id TEXT NOT NULL REFERENCES cycles(id) ON DELETE CASCADE,
  parent_id TEXT REFERENCES tasks(id) ON DELETE CASCADE,
  title TEXT NOT NULL,
  subtasks TEXT NOT NULL DEFAULT '[]',   -- JSON: [{title, completed}]
  position INTEGER NOT NULL DEFAULT 0,
  completed INTEGER NOT NULL DEFAULT 0,
  goal_breakdown TEXT,                   -- JSON，结构见 goal-clarification 规范
  needs_refinement INTEGER,
  needs_breakdown INTEGER,
  root_color_key TEXT,
  copied_from_task_id TEXT,
  proposal TEXT CHECK (proposal IN ('upsert','delete')),
  created_at INTEGER NOT NULL
);
CREATE INDEX ix_tasks_cycle_visible ON tasks(cycle_id, proposal, position);
CREATE INDEX ix_tasks_parent ON tasks(parent_id);

task_preview_originals(
  task_id TEXT PRIMARY KEY REFERENCES tasks(id) ON DELETE CASCADE,
  cycle_id TEXT NOT NULL REFERENCES cycles(id) ON DELETE CASCADE,
  original_exists INTEGER NOT NULL,
  title TEXT, completed INTEGER, subtasks TEXT, position INTEGER,
  goal_breakdown TEXT, parent_id TEXT, root_color_key TEXT, created_at INTEGER,
  needs_refinement INTEGER, needs_breakdown INTEGER
);

repeats(
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  duration INTEGER NOT NULL CHECK (duration >= 0),
  position INTEGER NOT NULL DEFAULT 0,
  archived INTEGER NOT NULL DEFAULT 0
);

agent_conversations(
  id TEXT PRIMARY KEY,
  cycle_id TEXT NOT NULL REFERENCES cycles(id) ON DELETE CASCADE,
  active_turn_id TEXT, revision INTEGER NOT NULL DEFAULT 0,
  active_skill TEXT CHECK (active_skill IS NULL OR active_skill IN
    ('goal_setting','long_term_planning','short_term_planning','prioritization')),
  last_error TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);

agent_messages(
  id TEXT PRIMARY KEY,
  conversation_id TEXT NOT NULL REFERENCES agent_conversations(id) ON DELETE CASCADE,
  turn_id TEXT NOT NULL,
  sequence_number INTEGER NOT NULL CHECK (sequence_number > 0),
  message_type TEXT NOT NULL CHECK (message_type IN
    ('user','model_text','model_function_call','function_result','app_tool_result')),
  payload_json TEXT NOT NULL,
  created_at TEXT NOT NULL,
  UNIQUE (conversation_id, sequence_number)
);

app_settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);  -- 见"设置"
```

与拆解结论一致：`agent_conversations` / `agent_messages` 表在本次变更中**建表但不由本变更写入**（`agent-conversation` 能力由后续变更实现），避免为将来重做迁移。

### 数据库级不变量

规则放 schema 里，不放应用代码（与拆解结论一致，理由见 `design.md`）：

1. `cycles.type` 四选一；`NOT (started=0 AND finished=1)`
2. `calendar_key` 全局唯一（部分索引）
3. `root_color_key` 只能出现在 `type='month'` 周期的任务上（`BEFORE INSERT` / `BEFORE UPDATE` 触发器）
4. 含 root color 的周期不能被改成非 `month`（`BEFORE UPDATE OF type` 触发器）

### 日历键

`domain/calendar.rs` 是唯一处理日期的地方。**格式已由运行时数据坐实**（见 `analysis/evidence/samples/`）：

| 周期类型 | `calendar_key` | 例 |
| --- | --- | --- |
| `day` | `day:{YYYY-MM-DD}` | `day:2026-09-14` |
| `week` | `week:{周首日}` | `week:2026-09-14` |
| `month`（长周期） | `long-term:{starts_on}:{ends_on}` | `long-term:2026-09-14:2026-12-07` |
| `session` | `NULL` | — |
| Do Later 容器 | `NULL` | — |

三点容易搞错：

1. **不是裸的日期串，是带类型前缀的复合键**。前缀用 `long-term`（带连字符）而不是 `month`——数据库类型是 `month`，但用户可见与上下文里用的名字是 "Long-term"。
2. **周键用周首日而不是 ISO 周编号**（不是 `2026-W38`）。所以它天然随 `week_start_day` 设置变化——这与我先前"键固定用 ISO 周以避免漂移"的推断相反。上游实际做法是键跟着设置走。
3. `session` 与 Do Later **没有日历键**（同一天可有多个专注块）。

Do Later 容器固定 `id = 'later'`，`type='month'`，`duration = 0`，无日期。**注意 `duration` 是 `0` 而不是 `NULL`**——上游的种子数据就是这么写的（迁移 25 的 insert 里 duration 位是 `0`）；`NULL` 虽然在迁移 45 之后被允许，但 `later` 实例用的是 0。所有任务都必须属于某条周期（不引入"无周期任务"这个平行状态）。

### 时间表示

- 墙钟时间戳：`INTEGER`，毫秒 epoch（与 `focused_time`、`duration` 同单位）
- 日历日期：`TEXT`，`YYYY-MM-DD`（本地日期，不用 UTC，避免跨时区偏移导致"哪一天"漂移）
- 消息时间：`TEXT` ISO8601

### 周期时长（毫秒，各类型固定）

`duration` 是**毫秒**。上游证据：`2419200000 -- 28 days in milliseconds`、`86400000 -- 24 hours`、`604800000 -- 7 days`。

| 类型 | 时长 | 毫秒 | 说明 |
| --- | --- | --- | --- |
| `month`（长周期） | 28 / 84 / 168 天 | `2419200000` / `7257600000` / `145152000000` | 即 1 / 3 / 6 × 28 天。**按周计算而非日历月**——3 个月 = 84 天 = 整 12 周，与创建界面承诺的 "12 weeks left" 一致 |
| `week` | 固定 7 天 | `604800000` | 上游文案 `Weekly planning cycles use a fixed 7-day duration.` |
| `day` | 固定 24 小时 | `86400000` | 上游文案 `extend onboarding day to 24 hours` |
| `session` | 用户指定 | — | 专注块，未定时长可为 `NULL` |
| Do Later 容器 | `NULL` | — | 无日期无时长 |

### 生命周期相关不变量

除数据库的 `CHECK (NOT (started = 0 AND finished = 1))` 外，应用层必须保证：

1. **启动前必须有时长**：`Cycle duration must be set before starting`（上游原文）。未定时长的周期不能被启动。
2. **周周期必须有父周期**：`Weekly planning cycles require a parent.`
3. **已结束的长周期下不能再建周周期**：`Cannot create a weekly planning cycle under an ended long-term cycle.`
4. **长周期时长只允许 1 / 3 / 6 × 28 天**（`validate_long_term_duration`）。
5. **日期边界由时长推导**：`calculate_ends_on` 与 `dated_cycle_bounds` 共同决定 `starts_on` / `ends_on`，且 `calendar_key` 由 `start_of_week` 与本地日期格式化派生。

## IPC 契约

命令名用 `snake_case` 动词短语。错误统一序列化为 `{ "code": "...", "message": "..." }`。

本变更实现的命令：

| 组 | 命令 |
| --- | --- |
| 周期 | `get_planner_state` `create_planning_cycle` `update_cycle` `delete_planning_cycle` `get_cycle_deletion_preview` `start_cycle` `finish_cycle` `add_session` `reorder_sessions` `copy_uncompleted_from_previous` |
| 任务 | `add_task` `update_task` `patch_task` `delete_task` `move_task` `reorder_tasks` `set_task_parent_link` `set_task_root_color` |
| 编辑态 | `get_editor_workspace` `get_editor_workspaces_by_cycle_ids` |
| 重复日程 | `add_repeat` `update_repeat` `stop_repeat` |
| 预览 | `get_preview_summary` `keep_task_preview` `undo_task_preview` `keep_all_previews` `undo_all_previews` |
| Do Later | `add_later_goal` `promote_later_goal` |
| 设置 | `get_settings` `set_week_start_day` `get_telemetry_settings` `set_telemetry_enabled` |
| 维护 | `export_backup` `get_schema_version` `export_diagnostics` |

事件（Rust → 前端）：

| 事件 | 载荷 | 前端动作 |
| --- | --- | --- |
| `cycles:changed` | `{ cycle_ids: string[] }` | 失效周期相关查询 |
| `tasks:changed` | `{ cycle_ids: string[] }` | 失效任务相关查询 |
| `proposals:changed` | `{ cycle_id: string }` | 失效待确认摘要 |

事件只做"失效通知"，载荷不带业务数据——避免出现第二份真相。

## 错误语义

```rust
enum AppError {
  Validation { code: String, message: String },   // 参数/状态非法
  NotFound { entity: String, id: String },
  Conflict { code: String, message: String },     // 唯一约束、删除守卫
  Db(String),                                      // SQLite 层，透传 message
  Internal(String),
}
```

删除守卫用 `Conflict`，其 `code` 与规范里的场景一一对应（`past_cycle`、`has_started_session`、`not_latest_n`、`unsupported_cycle_type` 等），前端按 `code` 决定文案。

## 前端数据流

```
用户操作 → lib/ipc.ts 调用命令 → service 事务提交 → 发射事件
   ↓                                                   ↓
乐观更新（仅本地顺序调整）                        lib/events.ts 收到
   ↓                                                   ↓
react-query 缓存 ←────────── 失效并重取 ←───────────────┘
```

- 只有**拖拽排序**做乐观更新（低风险、可回滚）；其余等事件回来刷新，避免自造第二份真相。
- 预览态由后端 `proposal` 字段驱动，前端不维护"哪些是待确认"的独立集合。

## 设置

`app_settings` 表存键值（`week_start_day`、`telemetry_enabled`、`last_seen_version` 等非敏感项）。凭据类数据走系统钥匙串，**不进数据库、不进配置文件、不进日志**。

## 派生优先于物化

以下两项在上游被实现为「物化的状态」，后来都被证明是多余的，本实现一律用**查询派生**：

| 概念 | 上游做法 | 本实现 |
| --- | --- | --- |
| 「当前周期」 | 曾加 `active` 布尔列 + 唯一索引保证每层只有一个，并用三段自连接 SQL 回填 | 查询时派生（按 `created_at` 取每层最新）。不引入 `is_active` 列——物化会带来「何时更新」的一致性问题 |
| 「哪些是待确认改动」 | 曾用 `preview_state` 三态枚举 + 内容修订号 + 最后修改修订号 | 只用 `tasks.agent_proposal`（`upsert`/`delete`）+ `task_preview_originals` 快照表 |

同理**不引入修订号 / 版本向量 / 乐观并发控制**：单用户本地应用不存在并发写入，这些字段只增加复杂度。理由与证据见 `analysis/reports/10-requirement-evolution.md` §0x03、§0x05。

## 遥测边界

遥测是唯一允许的非必要出网流量（AI 除外），受 `openspec/specs/telemetry/spec.md` 约束：

- 开关缺失即视为**未启用**
- 事件载荷只允许枚举值、计数、时长；**不得包含任何自由文本**（目标标题、任务标题、笔记、周期名）
- 匿名标识独立于设备指纹与授权标识
- 上报不在业务关键路径同步等待，失败静默丢弃
- 新增事件或字段时按"是否可能携带用户内容"审查

关闭遥测 + 不使用 AI 时，应用在离线状态下应当零出网。

## 验证入口

| 层 | 入口 | 覆盖 |
| --- | --- | --- |
| 领域 | `cargo test --lib` | 日历键计算、生命周期状态机、树操作 |
| 持久化 | `cargo test --test invariants`（临时库） | 触发器与 CHECK、迁移幂等、级联删除、备份导出 |
| IPC | `cargo test --test commands` | 命令级主路径 + 错误 code |
| 前端 | `npm run build` + `npm run test` | 类型检查、组件测试 |
| 手工 | `npm run tauri dev` | 工作台、Do Later、Keep/Revert 全链路 |

核心验证必须打到**真实入口**（通过 Tauri 命令调用路径或等价 service 公共 API），不能只测私有 helper。

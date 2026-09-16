# 架构（自有实现）

> 本文是模块边界、依赖方向与数据所有权的权威来源。
> 需求行为以 `openspec/specs/` 为准；本文只记录实现边界，不把内部研究材料当成发布依赖。
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
│   ├── settings.rs    app_settings 键值
│   └── proposals.rs
├── service/
│   ├── cycles.rs      创建/启动/结束/复制/删除守卫
│   ├── tasks.rs       增删改、移动、链接、着色
│   ├── settings.rs    主题/日志级别/一次性提示等设置用例
│   ├── editor.rs      编辑态工作区（树 + Markdown 渲染）
│   └── proposals.rs   预览写入、Keep/Revert
├── logging/           统一本地调试日志（级别/滚动/脱敏；纯本地零出网）
├── providers/         BYOK 供应商配置（providers.json）与钥匙串凭据
├── sampling/          三协议采样客户端（统一事件流 + 错误分类）
├── ai/                AI 规划能力（add-ai-planning-core）
│   ├── llm/           LLM 抽象（解析 BYOK 供应商 → 采样请求；FakeProvider 测试）
│   ├── agent/         技能提示、<context> 注入、回合工具循环
│   ├── tools.rs       11 个 agent 工具（写工具一律经预览层）
│   ├── breakdown.rs   GoalBreakdown 引擎（合并/缺字段/清晰度/标题精炼）
│   ├── prioritization.rs 五桶优先级引擎（JSON 列持久化）
│   └── review.rs      计划审查（结构+语义两层、缓存、忽略）
├── events.rs          CycleEvent / TaskEvent 发射器
└── commands/
    ├── mod.rs         命令注册清单
    ├── cycles.rs / tasks.rs / editor.rs / later.rs / repeats.rs
    ├── proposals.rs / settings.rs / maintenance.rs
    └── ai_settings.rs BYOK 供应商设置命令

src/
├── main.tsx / App.tsx 外壳：平台初始化、面板布局与 Later 快捷键
├── features/desktop/ WindowBar / useDesktopWindow：窗口状态、控制与安全区
├── lib/platform.ts    编译目标与主修饰键规则（Command / Control）
├── lib/ipc.ts         命令封装（唯一 invoke 出口，类型从 Rust 推导）
├── lib/events.ts      事件订阅 → react-query 失效（qk key 工厂）
├── lib/theme.ts       白底/灰底双主题运行时（data-theme 切换）
├── features/planner/  工作台：横向周期列、时长弹窗、专注块、任务编辑
├── features/later/    Do Later 侧栏（一次性说明卡经 app flag 持久化）
├── features/proposals/Coach 确认卡与工具栏导航入口
├── features/settings/ 设置弹窗（主题/周起始日/日志级别/日志目录）
└── ui/                基础组件（Button/Input/Checkbox/Dialog/Popover/EmptyState/ProgressDot）
```

## 桌面平台边界

`scripts/desktop-platform.mjs` 归一 Tauri 目标，Vite 将其编译为 `__DESKTOP_PLATFORM__` 并写出 `dist/desktop-build.json`。原生 dev/build/bundle hooks 检查目标和版本；独立 Web 预览使用 `web`。平台不是用户设置，也不由 UA 推断原生外壳。`scripts/desktop-config.mjs` 按 JSON Merge Patch 检查公共配置与目标覆盖，窗口数组完整保留 main、1280×800 和 960×600。

macOS 使用原生红绿灯与 Overlay 标题栏。Windows 的 `platform/windows.rs` 管理启动握手和 3 秒降级期限，`windows_frame.rs` 在主窗口线程管理最大化及空白拖动区的原生子窗口，保留 tao/wry 消息链。`geometry.rs` 校验 DOM 客户区几何和修订号，再转换为物理坐标；缩放/最小化期间的旧测量隐藏命中区，等待新布局。无任意 HWND 或消息号可从前端传入。鼠标最大化走原生路径，键盘走 Tauri API，避免重复执行。

`desktop_shell_state/ready/regions` 仅 Windows 注册，仅本地 main 窗口获得 capability；Linux/Web 不请求这些操作。窗口几何、激活态、全屏态不写入业务数据库，不暴露给 Coach。macOS、Windows、Linux 分别编译 keyring 的 apple-native、windows-native、sync-secret-service 后端，没有明文或内存凭据回退。

`.github/workflows/release.yml` 是六目标发布合同：Linux `x86_64-unknown-linux-gnu` / `aarch64-unknown-linux-gnu`、Windows `x86_64-pc-windows-msvc` / `aarch64-pc-windows-msvc`、macOS `x86_64-apple-darwin` / `aarch64-apple-darwin`。各目标在对应原生 runner 构建；macOS app 经过固定自签名后生成 DMG，当前不公证，Windows 包未签名。手动运行只上传 workflow artifact，版本一致的 `v<version>` tag 才触发完整 Release。环境准备、签名边界与产物筛选见[桌面构建说明](desktop-build.md)。其他平台的静态或组件检查不能代替其原生验收。

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
  task_id TEXT REFERENCES tasks(id) ON DELETE CASCADE, -- 专注块可选关联同日日任务
  repeat_id TEXT REFERENCES repeats(id),    -- 由哪个重复模板生成；模板移除时置空而非级联删除
  starts_on TEXT, ends_on TEXT,  -- 'YYYY-MM-DD'
  progress_check TEXT,           -- 可空 JSON；长期周期的 once/repeat 检查安排
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
  id TEXT PRIMARY KEY NOT NULL CHECK (id = 'coach'),
  active_turn_id TEXT, revision INTEGER NOT NULL DEFAULT 0,
  active_skill TEXT CHECK (active_skill IS NULL OR active_skill IN
    ('goal_setting','long_term_planning','short_term_planning','weekly_planning','daily_planning','prioritization','review','period_analysis','planning_issues')),
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

`agent_conversations` 只有 id 为 `coach` 的全局会话；`agent_messages` 的序号在该会话内全局递增。迁移 14 从旧周期会话暂存消息，按 `turn_created_at`、旧会话 id、回合首序号、`turn_id`、旧消息序号排序后重新编号，移除会话到 `cycles` 的外键；技能和错误取 `updated_at` 最近的旧会话行，无法在重启后续用的 `active_turn_id` 清空。删除周期不再级联 Coach 历史。

### 数据库级不变量

规则放 schema 里，不放应用代码：

1. `cycles.type` 四选一；`NOT (started=0 AND finished=1)`
2. `calendar_key` 全局唯一（部分索引）
3. `root_color_key` 只能出现在 `type='month'` 周期的任务上（`BEFORE INSERT` / `BEFORE UPDATE` 触发器）
4. 含 root color 的周期不能被改成非 `month`（`BEFORE UPDATE OF type` 触发器）

### 日历键

`domain/calendar.rs` 是唯一处理日期的地方。格式由领域类型和本地持久化规范共同定义：

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

Later 中的任务使用 `tasks.later_plan_type` 保存移入前的 `month/week/day` 层级。字段只在 Later 内允许非空（迁移 13 的 CHECK），新建收纳项写入 `month`；旧 `NULL` 按长期处理，不猜测来源。编辑树和同层移动子树在 Later 内按此类型分界，避免已关联的周项与长期目标因进入同一容器而被合并。`promote_later_goal` 的目标 id 可省略：周项在 Rust 中按本地今天与 `week_start_day` 解析本周，日项解析今天；长期项仍由前端选择周期。目标创建、同层子树移动、类型清空及失效外部父关联清理在同一事务内完成，成功后通过既有周期/任务事件失效缓存。


### 时间表示

- 墙钟时间戳：`INTEGER`，毫秒 epoch（与 `focused_time`、`duration` 同单位）
- 日历日期：`TEXT`，`YYYY-MM-DD`（本地日期，不用 UTC，避免跨时区偏移导致"哪一天"漂移）
- 消息时间：`TEXT` ISO8601

### 周期时长（毫秒）

`duration` 是**毫秒**。上游证据：`2419200000 -- 28 days in milliseconds`、`86400000 -- 24 hours`、`604800000 -- 7 days`。

| 类型 | 时长 | 毫秒 | 说明 |
| --- | --- | --- | --- |
| `month`（长周期） | 预设 28 / 84 / 168 天，或自定义日期差 | 预设 `2419200000` / `7257600000` / `14515200000` | 预设按产品月（4 周）计算；自定义允许非整周，以 date-only 日期差计算 |
| `week` | 固定 7 天 | `604800000` | 上游文案 `Weekly planning cycles use a fixed 7-day duration.` |
| `day` | 固定 24 小时 | `86400000` | 上游文案 `extend onboarding day to 24 hours` |
| `session` | 用户指定 | — | 专注块，未定时长可为 `NULL` |
| Do Later 容器 | `NULL` | — | 无日期无时长 |

### 生命周期相关不变量

除数据库的 `CHECK (NOT (started = 0 AND finished = 1))` 外，应用层必须保证：

1. **启动前必须有时长**：`Cycle duration must be set before starting`（上游原文）。未定时长的周期不能被启动。
2. **周、日容器允许独立**：日期身份保持唯一；目标归属由 `tasks.parent_id` 表达。2026-09-15 用户明确放开原版“周必须有父级”的约束。
3. **已结束的长周期下不能再建周周期**：`Cannot create a weekly planning cycle under an ended long-term cycle.`
4. **长周期预设为 1 / 3 / 6 × 28 天**；自定义传 `starts_on` / `ends_on`，两种参数互斥，结束日期必须晚于开始日期。
5. **日期边界保持本地日期语义**：预设由时长推导，自定义直接校验日期边界，`ends_on` 为排他边界。

长期检查安排由迁移 11 在 `cycles.progress_check` 可空 JSON 列持久化，封闭枚举为 `{kind:"once",date}` 或 `{kind:"repeat",every_days}`。一次检查须位于 `[starts_on, ends_on)`；重复间隔为正整数天，从开始日期推进，仅派生结束前的检查日。前后端按整天计算，预览只生成有限日期和总数，不物化检查实例，不新增通知调度器。预设保存中点检查，旧记录空值仅在展示时推导中点。

删除影响由 `service/deletion.rs` 统一计算：先遍历周期子树，再沿任务父链递归遍历跨周期后代。任务删除沿任务树并纳入显式 `task_id` 关联的专注块，不推断同日独立专注块关联。GUI 预览返回影响计数和 token，删除事务重新计算 token，变化时要求再次确认；同一事务清理后代提醒及预览快照，并使所有受影响周期的查询失效。Coach 保留已有单一确认入口。

专注关联由迁移 12 的可空 `cycles.task_id` 表达，仅 session 可填写；创建服务验证任务真实、有标题、已确认并属于同一天。日历整体合并按事务共同移动任务与专注，故同日约束在业务事务边界验证。任务单独跨日移动会解除关联，原日专注历史保留；重复实例不继承关联。`TaskNode.focused_time` 在编辑态从关联专注的已累计时长派生，任务不保存第二份计数。结束专注沿周期父链只累计一次；递归删除先从幸存祖先扣除相关累计，再由外键删除记录，并统一清理提醒和失效查询。界面只在创建专注时提供可选关联，不增加常驻任务统计。

## IPC 契约

命令名用 `snake_case` 动词短语。错误统一序列化为 `{ "code": "...", "message": "..." }`。

Tauri 命令的顶层参数使用默认 `camelCase`，例如 `get_editor_workspace` 的调用负载是 `{ cycleId }`。`args`、`patch`、`config` 内的 serde 结构继续使用 `snake_case`。2026-09-15 的真实 GUI 测试发现旧前端错误地把所有顶层参数写成了 `snake_case`；修复覆盖现有 IPC 入口，测试应断言真实 wire contract，不能把 Rust 函数参数拼写直接当成 JSON 键。

本变更实现的命令：

| 组 | 命令 |
| --- | --- |
| 周期 | `get_planner_state` `list_sessions` `create_planning_cycle` `update_cycle` `delete_planning_cycle` `get_cycle_deletion_preview` `start_cycle` `finish_cycle` `add_session` `reorder_sessions` `copy_uncompleted_from_previous` `ensure_day` |
| 任务 | `add_task` `update_task` `patch_task` `get_task_deletion_preview` `delete_task` `move_task` `reorder_tasks` `set_task_parent_link` `set_task_root_color` |
| 编辑态 | `get_editor_workspace` `get_editor_workspaces_by_cycle_ids` |
| 重复日程 | `add_repeat` `update_repeat` `stop_repeat` |
| 预览 | `get_preview_summary` `keep_task_preview` `undo_task_preview` `keep_all_previews` `undo_all_previews` |
| Do Later | `add_later_goal` `promote_later_goal` |
| 设置 | `get_settings` `set_week_start_day` `set_theme` `set_log_level` `get_app_flag` `set_app_flag` |
| 维护 | `export_backup` `get_schema_version` `get_debug_log_dir` |
| AI 设置 | `get_ai_settings` `save_provider` `delete_provider` `set_active_provider` `save_provider_api_key` `remove_provider_api_key` `test_provider_connection` |
| Agent | `start_agent_conversation` `send_agent_message` `get_agent_conversation` `get_previous_agent_conversation` `start_planning` `start_goal_setting` `start_prioritization` |
| 计划问题 | `get_planning_issue_report` `dismiss_planning_issue` `get_planning_issue_dismissals` |

事件（Rust → 前端）：

| 事件 | 载荷 | 前端动作 |
| --- | --- | --- |
| `cycles:changed` | `{ cycle_ids: string[] }` | 失效周期相关查询 |
| `tasks:changed` | `{ cycle_ids: string[] }` | 失效任务相关查询 |
| `proposals:changed` | `{ cycle_id: string }` | 失效待确认摘要 |
| `agent:conversation_updated` | `{ conversation_id, revision }` | 失效全局 Coach 会话 |

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

`app_settings` 表存键值（`week_start_day`、`theme`、`log_level`、`hint.*` 一次性提示等非敏感项）。凭据类数据走系统钥匙串，**不进数据库、不进配置文件、不进日志**。主题经 `set_theme` 持久化，前端以 localStorage 作首帧缓存、app_settings 为持久真相。

### 应用语言

`app_settings.locale` 是语言偏好的唯一持久真相，仅接受 `en` / `zh-CN`。首次读取按系统语言匹配并保存，不支持的语言使用英语；前端 `goal.locale` 缓存只解决首帧。设置写入成功后更新 i18next 和 `html.lang`，失败保留原语言。界面通过 `useTranslation` 订阅语言变化，禁止在模块初始化阶段计算文案或通过页面重载切换。

`src/lib/i18n/locales` 打包离线资源，按职责分 namespace；`Intl` 仅负责显示，日期身份与时区语义不变。错误根据稳定 code 映射，未知错误保留本地化说明和诊断；规则问题和确认内容使用结构化 key/参数，避免把某种语言的句子当业务状态。原生通知与 AI 请求从同一偏好读取；内置 SKILL 和默认 persona 使用英语编写，默认回复语言允许用户明确覆盖，历史消息和自定义指令文件不自动翻译。

新增语言应补齐全部 namespace/key/插值参数，并通过 `src/lib/i18n/i18n.test.tsx`。`scripts/i18n-coverage.test.mjs` 防止新的可见 JSX 文案和无障碍标签绕过语言资源。详见 `openspec/changes/add-application-i18n` 的行为场景与验收任务。

## 派生优先于物化

以下两项在上游被实现为「物化的状态」，后来都被证明是多余的，本实现一律用**查询派生**：

| 概念 | 上游做法 | 本实现 |
| --- | --- | --- |
| 「当前周期」 | 曾加 `active` 布尔列 + 唯一索引保证每层只有一个，并用三段自连接 SQL 回填 | 查询时派生（按 `created_at` 取每层最新）。不引入 `is_active` 列——物化会带来「何时更新」的一致性问题 |
| 「哪些是待确认改动」 | 曾用 `preview_state` 三态枚举 + 内容修订号 + 最后修改修订号 | 只用 `tasks.agent_proposal`（`upsert`/`delete`）+ `task_preview_originals` 快照表 |

同理**不引入修订号 / 版本向量 / 乐观并发控制**：单用户本地应用不存在并发写入，这些字段只增加复杂度。

## 本地调试日志

应用不含任何遥测（该能力于 2026-09-14 删除，规范 `openspec/specs/telemetry/spec.md` 已于 2026-09-15 移除）。替代能力是 `openspec/specs/local-logging/spec.md` 定义的统一本地调试日志：

- 全部模块经统一日志桩写本机日志文件；级别 error/warn/info/debug 可调
- 日志纯本地：没有任何上报通道。除用户配置的 AI 供应商请求外，应用零出网
- 凭据（API Key、令牌）绝不入日志；完整计划内容仅 debug 级别
- 日志按大小滚动淘汰；写入失败静默，不影响业务
- 设置页提供打开日志目录的入口（`get_debug_log_dir`）

## 验证入口

| 层 | 入口 | 覆盖 |
| --- | --- | --- |
| 领域 | `cargo test --lib` | 日历键计算、生命周期状态机、树操作 |
| 持久化 | `cargo test --test invariants`（临时库） | 触发器与 CHECK、迁移幂等、级联删除、备份导出 |
| IPC | `cargo test --test commands` | 命令级主路径 + 错误 code |
| 前端 | `npm run build` + `npm run test` | 类型检查、组件测试 |
| 手工 | `npm run tauri dev` | 工作台、Do Later、Coach 预览确认/拒绝全链路 |

核心验证必须打到**真实入口**（通过 Tauri 命令调用路径或等价 service 公共 API），不能只测私有 helper。


## 2026-09-15 独立事务与统计边界

周/日日期容器和任务目标链分开：前端创建自然周不传长期父周期，`get_or_create_day_in_tx` 同时供打开日期及日历移动使用，不再重复实现长期父级推导。显式传父周期时仍检查层级与结束状态，独立日保持 `NULL` 父级。复制与复盘延续按相邻日期查找，周/日不再受长期容器分支限制。

`EditorWorkspace.work_mix` 是实时派生的计数 DTO，不新增表或持久化分数。同一计算函数供编辑器及 AI context 使用，任务父链变化会使下层编辑查询失效。周期内按顶层承诺计数、空行与提议不计入、子步骤不重复计数；未解析关系单列。当前上下文必须明确统计范围和当前关系快照，跨季度/半年/年度的聚合与历史归属不是现有统计的能力。

独立记录的后续事项：旧周期仍保留原有可选父级；既有周期/任务删除级联契约需要结合独立事务另行审计（既有 A01），本轮不自动迁移或删除用户数据。按任务分配专注时长、长期历史归属快照与跨期 AI 报告应分别设计，不能用前端估算来填补数据。


## 按需技能与模型选择（2026-09-15）

供应商元数据和当前激活的 `provider_id + model_id` 组合继续存于 providers.json，不新增模型选择实体。原子保存选中组合；解析器只取指定模型。配置测试、凭据更新和激活操作由同一异步互斥锁串行化；测试失败不覆盖旧配置。计划区的可用性检查只读配置，不因浏览页面访问钥匙串。

供应商 `extra_headers` 只存名称数组，值通过 `save_provider` 的独立 `header_values` 参数单向提交，在系统凭据 `provider-headers:{id}` 中保存。留空保留同名值、删除名称移除值；统一 `providers::headers` 校验后逐模型测试，再提交配置和凭据，失败执行补偿。运行时解析和连接测试共用相同值；采样客户端统一按名称覆盖默认 Header 并脱敏错误，拒绝传输层保留名称和重定向转发。前端、普通配置和 Coach 上下文均不获得已存值。

供应商 `connection` 与采样请求共用 `network::ConnectionSettings`（auto/direct/proxy）。`network.rs` 集中 URL 校验、回环识别与 reqwest 代理构建；protocol adapter 只处理请求与事件格式。最终目标为回环地址时始终禁用代理；非回环目标按供应商选择自动、直连或单一显式代理，失败不改路由。该层不写环境变量、注册表或系统代理；本次无代理认证与 PAC/WPAD。代理 URL 不能携带账号密码、路径、query 或 fragment，普通配置与 Debug 不暴露代理凭据。

`ai/skills.rs` 管理内置文件、首次补齐和运行时读取；`ai/agent/prompt.rs` 只组合不可变契约、技能目录和当前工作流。模型调用 `load_skill` 选择技能，执行器在同一回合的下一轮更新系统指令与工具集合。长期、周、日规划分别读取文件；时段分析和问题诊断只有读取工具与技能切换工具，执行器验证调用是否属于当前集合。技能正文不可扩大代码层权限。

`ai/period_analysis.rs` 负责校验自然日期范围和构造一致快照，复用现有任务、周期、Work mix，不新增“季度计划”容器。范围与周期区间求交（周期 ends_on 为排他边界），包含归档，排除 Later、Focus Block 容器与无日期计划，并报告数据限制。Coach 在时段技能下调用 `get_period_context` 读取；另有只读 `analyze_planning_period` IPC 可供独立报表入口复用。数量按层级展示，不把周目标与其日步骤相加。超过上下文上限显式失败。

会话超时配置复用 app_settings 的 `ai.context-idle-minutes`。默认 15，后端验证 1–1440 整数。全局 Coach 最后完成回合的 updated_at 决定到期时间；查询不续期。读取和新回合前事务清除空闲过期消息/技能/错误，前端依据服务端 expires_at 定时重取，并在设置更新后失效查询。进行中的回合由 `active_turn_id` 保护，完成后重新计时。应用关闭期间无需运行清理进程，恢复后的首次读取即执行失效，不会向模型重放过期历史。

迁移 9 扩展已有 active_skill 约束；迁移 14 重建会话表前暂存消息，避免外键级联误删历史，随后把旧周期会话合并为 `coach` 单例。

Coach 使用固定的 `coach` 会话身份；Rust repository 通过 `get_or_create_conversation` 读写这一行，AgentPanel 在应用 shell 中稳定挂载，消息、草稿和滚动位置仍由现有 React state/ref 管理。发送命令捕获 `pageContext`（`long_term_cycle_id`、`week_cycle_id`、`day_cycle_id`、`view`、`week_starts_on`、`selected_date`），`load_turn_context` 仅在回合开始加载一次事实并生成 `<page_state>`；后续工具沿用这份快照，切换页面不会追加消息或重读浏览 scope。无计划时 `active_cycle_id` 为 null，Coach 可直接聊天，计划专用入口仍需目标周期。

后续独立处理：周期末历史归属/完成事件账本、按日期归因的实际专注时长、范围分析缓存与分层汇总。当前范围分析明确提供“目前数据库中的计划快照”，不伪造过去状态。原 App 专注块内部行动清单仍是单独功能差距。

## Coach 展示层边界（2026-09-15）

`ChatMarkdown.tsx` 将 Streamdown 的 GFM / 增量解析与应用主题隔离，复用现有消息正文，不引入第二套聊天状态或 AI SDK。显式关闭 raw HTML 插件、远程图片以及默认工具栏；表格和代码使用局部滚动容器。`parseNextSteps` 仍先于 Markdown 解析，协议标记不会进入正文。

Halaska 仅适配等待指示器和相关视觉/动效模式，MIT 声明保留在 `public/licenses/halaska-ui.txt` 并随前端产物打包。未导入整份演示站、外部字体和模拟会话定时器。消息来源、busy、工具结果、错误、技能状态仍由既有查询和 IPC 决定。工具过程按调用 ID 聚合，收起区域 inert / aria-hidden，展开采用自然内容高度。

真实 token streaming 仍未接入前端：当前 `sendAgentMessage` 等待整个回合完成，随后更新持久化会话。今后需单独定义增量事件的顺序、回合 ID、取消、恢复和最终消息去重；当前渲染器的 `isStreaming` 能力及增量测试不等于这些传输契约已经实现。


## Coach 工具与确认边界（2026-09-15）

工具目录收敛为 24 项、按技能提供 6–19 项，详见 [工具目录](coach-tools.md)。`ai/tools.rs` 保留目标内容预览与技能路由；`ai/tool_catalog.rs` 定义封闭参数和按技能提供的工具；`ai/actions.rs` 保存及执行 GUI 确认后的非任务行操作。只新增一张通用 `agent_actions` 表（迁移 10），没有为每种工具建立实体。设置按单项确认；provider/model 只暴露白名单元数据，模型不能接触凭据。

工具创建提案与 GUI 确认是不同入口，模型目录没有批准命令。移动事务复用 service::tasks::move_task，原 ID 与同周期子任务保留，跨层关联的周/日事务保留在原周期；替代此前复制和删除分开确认的移动工具。任务内容预览沿用原始快照。任务预览按 `task_id` 与 approve/reject 处理；非任务 `agent_actions` 通过 `cycle_id`、`action_id` 与 approve 处理，并保留动作的 `source_cycle_id` 供原目标校验。提交和 `app_tool_result` 回执写入同一事务。同步和异步确认都在代码作用域内取得全局回合 guard，直到 claim/动作/回执/finish 完成；忙时先拒绝且不改计划，失败由 owner token 释放，不新增数据库实体。确认入口跨页面可见，不按当前浏览 scope 重定向；会话过期清理可复用调用方事务。回执进入全局聊天历史，可供后续模型读取。

工具回合及确认后统一失效 workspace、calendar、任务、设置、提醒、模型选择等查询。包括工具成功后模型最终回复失败的情形，界面仍重新读取已保存的提案。

`ai/persona.rs` 首次补齐 `~/.goal/persona.md`，不覆盖已有文件；每次生成重新读取。Persona 管表达方式，SKILL 管工作流，代码管可用工具、确认和数据契约。Coach 与时段分析共用 persona。

架构债务：旧进程若在回合中崩溃，持久化的 `active_turn_id` 没有启动恢复机制；迁移 14 只清理迁移时无法续用的旧标记，本次不扩大为进程恢复设计。


## 任务预览投影与写入锁（2026-09-15）

编辑器读取含 proposal 的任务再构建同一棵树，Workspace 和 Calendar 共享此投影，避免前端把提案追加到树尾。业务统计仍过滤预览。父项删除通过现有外键影响关联后代，编辑投影派生其待删除状态，不生成额外提案；确认卡同步展示实际影响名单。

任务服务以 task ID 检查预览锁和待删除祖先；删除 / 移动等结构操作还检查后代提案。页面结束、删除、跨日移动不能绕过待确认任务锁。不存在全周期编辑锁，未受影响项照常修改。确认或拒绝复用现有快照提交/还原事务，回执携带 target_kind、target_id、decision；前端匹配工具结果之后的首条同目标回执，避免历史确认误解锁同一任务的后续提案。


### 计划检查的显式请求与报告

`get_planning_issue_report(cycle_id, refresh)` 返回报告 DTO：当前范围、待办/待确认/忽略数量、带来源和任务名的问题，以及 AI 检查状态、时间、模型和检查项数。`refresh=false` 只读当前规则与匹配缓存；`refresh=true` 解析已验证模型，加载 planning-issues SKILL 和 persona，向当前周期未完成且无提案的任务发起一次只读结构化请求。失败返回 AppError，不生成空成功报告。

语义响应严格校验顶层结构、问题枚举、提供的 task ID 和内容长度，最多 8 条可行动建议；范围超过 100 项或 64 KB 明确提示缩小范围。计划完整快照、模型标识与生成语言用于内存缓存，任务详情、归属或周期变化即失效；请求完成前再次读取快照，变更则返回冲突。规则与 AI 同任务同类型的问题由具体 AI 建议优先，沿用现有忽略记录过滤。没有新增持久化实体；重启后 AI 状态回到未检查。

前端任务/周期事件和模型选择事件使报告查询失效。Issues → Coach 只传草稿和 focused_task_id，不自动发起会话回合；任务定位交给 TaskList 在自身查询完成后滚动/聚焦，避免并行 workspace 查询的渲染竞态。


## 跨平台适配边界

平台识别仅来自构建常量；业务页面不解析 UA、不拼操作系统路径，也不为三个平台复制服务。窗口能力由 `useDesktopWindow` 与 Rust `platform` 模块封装，Windows SDK 和凭据后端只在目标依赖中启用。数据库和供应商元数据通过 Tauri app data 目录定位；用户可编辑的 persona/SKILL 使用系统 home 下 `.goal`，两类目录职责不同，均通过 Path 拼接而非手写分隔符。

应用内 Select 统一使用 Radix primitive 的 popper 定位、客户区避让、滚动和焦点管理，样式由公共 token 提供。业务调用方只负责选项与保存回调；平台原生文件、凭据和权限 UI 保留系统职责。Popover 中菜单与表单使用不同键盘语义，避免 Windows/WebKit 原生时间与数值控件的按键被菜单导航截获。具体排查与待验证范围见 [跨平台复核](cross-platform-review.md)。

# Backend implementation brief

> 交接对象：coder。范围：`openspec/changes/rebuild-baseline/tasks.md` 的第 3–7 章（领域层、仓储层、服务层、IPC 边界、前端占位之外的后端部分）。
> 不含：前端界面（独立任务）、agent 循环 / goal_breakdown / prioritization（属 `add-ai-planning-core`）、授权与语音（属 `add-ai-access-and-voice`）。

## 0. 权威来源（按优先级）

1. **行为** → `openspec/specs/*/spec.md`。本批次涉及：
   `planning-cycles`、`task-graph`、`agent-proposals`、`local-persistence`、`session-repeats`、`planner-workspace`（仅数据结构部分）
2. **架构与契约** → `docs/architecture.md`（模块分层、表结构、IPC 命令名、错误语义、时间表示）
3. **任务清单** → `openspec/changes/rebuild-baseline/tasks.md` 第 3–7 章 + 第 8/10 章
4. **上游语义证据** → `analysis/reports/00-technical-report.md`、`hyperfocus-deconstruction.md`

冲突时按 1 → 2 → 3 定序；发现规范与架构文档矛盾时**停下来报告**，不要自行选一个。

## 1. 已就绪的部分（不要重写）

| 文件 | 状态 |
| --- | --- |
| `src-tauri/src/db/migrations.rs` | 迁移执行器完成（SHA-256 校验和、顺序应用、幂等）。已有 `0001_init` / `0002_invariants` / `0003_seed_later`。**需要追加 `0004_repeats`**（见任务 2.3 的说明） |
| `src-tauri/src/db/mod.rs` | 连接池 + PRAGMA + `data_dir` 完成 |
| `src-tauri/src/db/backup.rs` | 备份导出完成（WAL checkpoint + integrity_check） |
| `src-tauri/src/error.rs` | `AppError`（`Validation`/`NotFound`/`Conflict`/`Db`/`Internal`）与 IPC 序列化为 `{code, message}` 完成 |
| `src-tauri/src/events.rs` | 三个事件发射器与 `CycleIdSet` 去重完成 |
| `src-tauri/src/domain/cycle.rs` | `CycleType`、时长常量、`is_valid_long_term_duration` **已完成，是冻结契约** |
| `src-tauri/src/domain/{task,proposal}.rs` | 仅类型骨架，需要补全 |
| `src-tauri/src/domain/calendar.rs` | **TODO 占位，需要实现** |
| `src-tauri/src/repository/mod.rs`、`service/mod.rs` | 空，需要建立子模块 |

## 2. 必须遵守的不变量（违反即验收失败）

从上游二进制恢复的确切语义，容易搞错，逐条列出：

1. **`duration` 单位是毫秒**，不是秒。`day` = `86400000`，`week` = `604800000`，长周期 = `28/84/168 天 × 86400000`。
2. **一个"月"是 28 天**，不是日历月。3 个月 = 84 天 = 整 12 周。不允许按日历月加月运算（会得到 91 天，破坏 "N weeks left" 的整周承诺）。
3. **启动周期前必须有时长**：错误码 `cycle_duration_required`，文案对应上游 `Cycle duration must be set before starting`。
4. **周周期必须有父周期**：`weekly_requires_parent`。
5. **已结束的长周期下不能建周周期**：`parent_cycle_ended`。
6. **`root_color_key` 只能挂在 `type='month'` 周期的任务上** —— 已由数据库触发器强制，但 service 层要把 `RAISE(ABORT)` 文本映射成带 `code` 的 `Conflict`（见第 4 节）。
7. **`calendar_key` 唯一** —— `day` 用 `YYYY-MM-DD`，`week` 用 ISO 周 `YYYY-Www`（**不随 `week_start_day` 设置变化**，否则改设置会产生重复周周期），`month`/`session` 为 `NULL`。
8. **`repeats` 移除模板是解除关联，不是级联删除**：`UPDATE cycles SET repeat_id = NULL WHERE repeat_id = ?`，实例与其 `focused_time` 必须保留。
9. **改重复模板只作用于未来实例**；无法判定未来范围时明确失败（错误码 `repeat_future_unknown`）。
10. **agent 写入一律先落预览态**（`tasks.agent_proposal` + `task_preview_originals`），Keep 才成为正式数据。本批次要实现机制本身；触发方（agent）不在本批次。

## 3. 分层规则（硬约束）

```
commands/*   仅参数校验 + 调 service。禁止出现 SQL。
service/*    事务边界、跨聚合规则、事件发射。
repository/* 单聚合的 SQL。禁止跨聚合编排、禁止发事件。
domain/*     纯类型与不变量函数。禁止 IO / SQL / tauri 依赖。
db/*         连接池、迁移、备份。
```

- `domain` 的单元测试不得起数据库。
- `repository`/`service` 的测试用临时库文件（`tempfile`），每个测试独立建库。
- 命令层不做业务判断（例如"周周期必须有父周期"是 service 的规则，不是命令层的 `if`）。

## 4. 错误码映射（必须实现）

`error.rs` 需要新增一个函数，把 SQLite 的约束错误映射成稳定的 `code`。触发器用 `RAISE(ABORT, '<token>')` 抛出，token 就是 `code`：

| 触发器/CHECK 抛出的 token | 归入 | 前端文案方向 |
| --- | --- | --- |
| `root_color_key_requires_long_term_cycle` | `Conflict` | 「颜色只能用于长期目标」 |
| `cycle_with_root_colors_must_stay_long_term` | `Conflict` | 「该周期含彩色目标，不能改成其他类型」 |
| `NOT (started = 0 AND finished = 1)` | `Conflict` → `invalid_lifecycle` | 内部错误，不面向用户 |
| 唯一索引冲突（`calendar_key`） | `Conflict` → `calendar_key_taken` | 复用已有周期 |

映射缺失时降级为 `Conflict { code: "constraint_violation" }` 并保留原始 message。

## 5. 本批次要交付的模块与命令

### 模块

```
domain/calendar.rs   本地日期解析/格式化、ISO 周键、start_of_week、
                     calculate_ends_on、dated_cycle_bounds、剩余周数
domain/cycle.rs      （已有）补 Lifecycle 状态迁移函数与非法迁移拒绝
domain/task.rs       Task、TaskTree（parent_id 组装 + position 稳定排序）、
                     ClarityFlags 三态语义
domain/proposal.rs   预览态、快照相等性判断
repository/cycles.rs repository/tasks.rs repository/proposals.rs repository/repeats.rs
service/cycles.rs    service/tasks.rs service/proposals.rs service/repeats.rs
commands/{cycles,tasks,proposals,repeats,editor}.rs
```

### 命令（名称固定，来自 `docs/architecture.md`）

| 组 | 命令 |
| --- | --- |
| 周期 | `get_planner_state` `create_planning_cycle` `update_cycle` `delete_planning_cycle` `get_cycle_deletion_preview` `start_cycle` `finish_cycle` `add_session` `reorder_sessions` `copy_uncompleted_from_previous` |
| 任务 | `add_task` `update_task` `patch_task` `delete_task` `move_task` `reorder_tasks` `set_task_parent_link` `set_task_root_color` |
| 编辑态 | `get_editor_workspace` `get_editor_workspaces_by_cycle_ids` |
| 重复 | `add_repeat` `update_repeat` `stop_repeat` |
| 预览 | `get_preview_summary` `keep_task_preview` `undo_task_preview` `keep_all_previews` `undo_all_previews` |
| Do Later | `add_later_goal` `promote_later_goal` |
| 设置 | `get_settings` `set_week_start_day` |
| 维护 | `export_backup` `get_schema_version`（后者已实现） |

命令注册在 `commands/mod.rs` 的 `generate_handler!` 里。

## 6. 必测场景（不是"测试覆盖率"，是这些具体行为）

每条对应规范里的场景，用 `#[test]` 或集成测试覆盖：

**calendar（纯单测，最密集）**
- 跨月 / 跨年 / ISO 周边界（周一 vs 周日为周首）
- 闰年 2 月 29 日
- `calculate_ends_on(from, 84 days)` = `from + 84`
- 剩余时间计算为整周；`is_whole_weeks` 对 28/84/168 天为真，对 91 天为假
- `start_of_week` 在不同 `week_start_day` 下正确，但 `calendar_key` 不随之改变

**cycles（集成，临时库）**
- 创建 1/3/6 个月长周期 → 时长分别为 `2419200000`/`7257600000`/`14515200000`，`ends_on` = `starts_on + N 天`
- 非允许时长被拒；不是整周的天数被拒
- 未设时长时 `start_cycle` 被拒（`cycle_duration_required`）
- 周周期无父被拒（`weekly_requires_parent`）
- 在已结束的长周期下建周被拒（`parent_cycle_ended`）
- `calendar_key` 重复被映射为 `calendar_key_taken`
- 删除守卫：过去周期 / 含已启动专注块 / 只有最新 N 个可删 —— 三种各一例
- `copy_uncompleted_from_previous` 写入 `copied_from_task_id`；上一周期全完成时返回空

**tasks（集成）**
- 树组装与 `position` 稳定排序
- 跨层链接只允许相邻层级（日→周、周→长周期）；日→长周期被拒
- `root_color_key` 在周周期任务上被拒（触发器映射生效）
- 移动任务到已结束周期被拒

**proposals（集成）**
- agent 写入落预览态，正式查询看不到（除非带 `agent_proposal`）
- `keep_task_preview` 后快照被清除、数据成为正式
- `undo_task_preview` 还原到快照；`original_exists=false` 时删除该行
- `keep_all_previews` / `undo_all_previews` 的批量语义
- `create_goal` 时空行复用：列表尾部有空任务行时替换而非追加
- 快照含 `needs_refinement` / `needs_breakdown` / `root_color_key` / `parent_id`

**repeats（集成）**
- `add_repeat` → 模板 + 首个实例关联
- 次日生成实例
- `stop_repeat` = 归档：历史实例保留，来源关联清空
- 改模板不动历史实例
- 单独删除某天实例不影响模板

**editor（集成）**
- `get_editor_workspace` 与批量版结果一致
- 不存在的周期返回空而非错误
- 子任务 Markdown 渲染：层级与顺序一致；空列表产出空串

## 7. Verification Plan

| 项 | 命令 | 期望 |
| --- | --- | --- |
| 编译 | `cd src-tauri && cargo check --all-targets` | 无 error |
| 单测 + 集成 | `cargo test` | 全绿；新增测试数 ≥ 40 |
| 领域纯函数 | `cargo test --lib` | calendar 相关用例 ≥ 12 |
| 迁移幂等 | 集成测试内断言 | 连续 `apply` 两次不报错，版本不变 |
| 前端不受影响 | `npm run build` | 通过 |

**验收口径**：以规范场景为准，不以"代码写完"为准。每条第 6 节的场景必须有一个能失败的测试（能捕获该回归）。

## 8. 允许的改动面

- ✅ `src-tauri/src/{domain,repository,service,commands}/**`、`src-tauri/src/error.rs`（只加映射函数）、`src-tauri/src/db/migrations.rs`（只追加 `0004_repeats`）、`src-tauri/src/lib.rs`（命令注册）
- ❌ 不改 `openspec/specs/**` 的任何需求（发现需求问题停下来报告）
- ❌ 不改 `docs/architecture.md` 的既有决策（同上）
- ❌ 不动 `src/**` 前端（除 `lib/ipc.ts` 里补命令名常量）
- ❌ 不引入新的数据库表（除 `0004_repeats`），不引入 ORM

## 9. 依赖与顺序

无外部依赖，可立即开始。内部顺序建议：

```
domain/calendar + cycle(Lifecycle) + task + proposal   ← 先做，纯函数好测
  → migrations 0004_repeats + repository/*
  → service/*
  → commands/* + 注册
```

若时间紧张，**优先级**：domain > repository + migrations > service（cycles/tasks）> proposals > commands > repeats > editor。

## 10. 报告要求

完成后按 Coder Result 回复：Status、Implemented Change（含文件）、Verification Evidence（实际命令与结果）、Contract/Documentation Impact、Residual Risks、Git State。**不要**声称跑过没跑的测试。

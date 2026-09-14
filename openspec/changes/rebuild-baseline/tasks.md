## 1. 工程脚手架

- [x] 1.1 初始化 Tauri 2 + React 19 + TypeScript + Vite 工程（`package.json`、`vite.config.ts`、`tsconfig.json`、`src-tauri/`），配置 Tailwind
- [x] 1.2 配置 Rust crate（`Cargo.toml`）：`tauri`、`rusqlite`（`bundled`）、`r2d2`、`r2d2_sqlite`、`serde`、`thiserror`、`uuid`、`time`/`chrono`、`sha2`
- [x] 1.3 建立 `docs/architecture.md` 里定义的目录骨架与模块声明，未实现的模块留空文件而不是空目录
- [x] 1.4 定下 Tauri 窗口参数（最小尺寸、标题栏）与 `npm run dev` / `npm run tauri dev` 脚本

## 2. 数据库层

- [x] 2.1 `db/mod.rs`：连接池初始化，PRAGMA `journal_mode=WAL`、`foreign_keys=ON`、`busy_timeout=5000`、`synchronous=NORMAL`；应用数据目录解析
- [x] 2.2 `db/migrations.rs`：`schema_migrations(version, description, checksum, applied_at)` + 顺序执行器；校验和用 SHA-256，已应用迁移校验和不匹配时报错
- [x] 2.3 迁移 `0001_init`：`cycles` / `tasks` / `task_preview_originals` / `agent_conversations` / `agent_messages` / `repeats` / `app_settings` 七张表与全部索引（`cycles.repeat_id` 外键指向 `repeats`，`ON DELETE SET NULL` 语义需在 service 层显式处理）
- [x] 2.4 迁移 `0002_invariants`：三条触发器（root_color_key 的 insert/update 校验、周期类型变更校验）
- [x] 2.5 迁移 `0003_seed_later`：插入固定的 `id='later'` 容器周期
- [x] 2.6 `db/backup.rs`：WAL checkpoint 后复制数据库到用户指定路径；导出结果做一次 `PRAGMA integrity_check`
- [x] 2.7 测试：迁移幂等（连续跑两次）、校验和漂移报错、从空库到最新版本的完整路径

## 3. 领域层

- [x] 3.1 `domain/cycle.rs`：`CycleType`、`Lifecycle`（含 `start`/`finish` 状态迁移函数与非法迁移的拒绝）、`Cycle` 结构
- [x] 3.2 `domain/calendar.rs`：本地日期解析/格式化、ISO 周键、长周期 `ends_on` 计算（按月加 N 个月并回退到最近的有效日期）、周起始日排布
- [x] 3.3 `domain/task.rs`：`Task`、`TaskTree`（按 `parent_id` 组装、按 `position` 稳定排序）、清晰度标记的三态语义（`NULL`/`true`/`false` 含义不同）
- [x] 3.4 `domain/proposal.rs`：预览态定义、快照相等性判断（用于判定"改动是否被用户实际接受"）
- [x] 3.5 测试：`calendar` 覆盖跨月/跨年/ISO 周边界/2 月 29 日；`cycle` 覆盖全部非法状态迁移

## 4. 仓储层

- [x] 4.1 `repository/cycles.rs`：CRUD、按 `calendar_key` 查找、子树查询、`focused_time` 累加、`position` 重排
- [x] 4.2 `repository/tasks.rs`：CRUD、按周期列出可见任务（含 `proposal` 过滤）、树查询、`position` 重排、批量跨周期移动
- [x] 4.3 `repository/proposals.rs`：写入预览、读快照、Keep 单条/全部、Revert 单条/全部
- [x] 4.4 `error.rs`：`AppError` 定义与 `serde` 序列化；SQLite 约束错误（含触发器 `RAISE(ABORT)` 文案）到 `Conflict { code }` 的映射表
- [x] 4.5 测试：每个仓储函数一个针对临时库的集成测试；断言触发器与 CHECK 真的会拒绝非法写入

## 5. 服务层与事件

- [x] 5.1 `service/cycles.rs`：创建长周期（校验时长属于 {1,3,6} 月）、创建/复用带日历键的周与日周期、启动/结束、复制上一周期未完成任务（写 `copied_from_task_id`）、删除守卫（过去周期 / 含已启动专注块 / 只有最新 N 个可删 / 先返回影响预览）
- [x] 5.2 `service/tasks.rs`：增删改、移动（校验源与目标可用性）、跨层链接（只允许相邻层级）、着色（仅长周期）、清晰度标记读写
- [x] 5.3 `service/proposals.rs`：把一次 agent 写入包装成"写快照 + 写预览"的原子操作；新建目标时若列表尾部存在空任务行则复用该行
- [x] 5.4 `events.rs`：`cycles:changed` / `tasks:changed` / `proposals:changed` 三个发射器，按 `cycle_id` 去重
- [x] 5.5 测试：删除守卫的四种拒绝路径各一个用例；复制血缘；空行复用；Keep/Revert 后数据与快照的终态

## 6. IPC 边界

- [x] 6.1 `commands/mod.rs`：按 `docs/architecture.md` 的契约注册全部命令
- [x] 6.2 周期与任务命令：参数校验放在命令层，业务规则放 service；禁止命令直接访问 SQL
- [x] 6.3 预览命令、Do Later 命令、设置命令、`export_backup` / `get_schema_version`
- [x] 6.4 测试：通过 `tauri::test` 或直接调用命令函数，验证主路径 + 错误 `code` 与规范场景一一对应

## 7. 前端

- [ ] 7.1 设计 token：按 `analysis/reports/design-system.md` 的原则定义我们自己的颜色/字阶/间距/描边（**重新取值，不复制原样式表**）
- [ ] 7.2 `lib/ipc.ts` 与 `lib/events.ts`：命令封装与事件订阅 → 失效 react-query
- [ ] 7.3 `ui/`：Button（primary/secondary/ghost）、Input、Checkbox、Dialog、Popover、EmptyState（虚线框 + 图标 + 大写标题 + 一句解释 + 主按钮）、ProgressDot
- [ ] 7.4 `features/planner/`：横向可滚的周期列；列为空时显示 EmptyState 与唯一主按钮；列头显示剩余时间与选项菜单
- [ ] 7.5 长周期时长选择弹窗：1/3/6 月三选一，右侧实时显示推导出的时间线（设置日 → 推进期 → 复盘日）
- [ ] 7.6 `features/later/`：Do Later 侧栏，含一次性说明卡（关闭状态持久化）
- [ ] 7.7 `features/proposals/`：待确认改动的视觉区分（高亮底色）与底栏 `You have N pending edits by agent` + Revert/Keep
- [ ] 7.8 键盘：`⌘⇧L` 打开 Do Later；拖拽排序用乐观更新
- [ ] 7.9 前端测试：EmptyState 分支、待确认计数、时长选择后的时间线推导

## 8. 重复日程

对应规范：`openspec/specs/session-repeats/spec.md`

- [x] 8.1 `repeats` 表与领域类型：标题、时长、排序位置、归档标记
- [x] 8.2 `repository/repeats.rs`：CRUD、按位置排序、归档、按 id 取
- [x] 8.3 `service/repeats.rs`：把专注块保存为模板（记录标题与时长并建立首个实例关联）；为某一天生成实例；修改模板只作用于未来实例；无法判定未来范围时明确失败
- [x] 8.4 解除关联而非级联删除：移除模板时把实例的 `repeat_id` 置空，实例与其时间记录全部保留
- [x] 8.5 单独修改/删除某个实例不改动模板与其他实例
- [x] 8.6 命令：`add_repeat` / `update_repeat`（含 `repeat_id`）/ `stop_repeat`
- [x] 8.7 测试：保存为重复；次日生成实例；停止重复后历史保留且来源关联已清空；改模板不动历史实例；单独删除某天实例不影响模板；模板实例遵守既有删除守卫

## 9. 本地调试日志

> 遥测已按产品决定整体删除（2026-09-14，含已实现的模块；`openspec/specs/telemetry/spec.md` 已于 2026-09-15 移除）。
> 本章实现其替代能力：`openspec/specs/local-logging/spec.md`——纯本地的统一调试日志渠道。

对应规范：`openspec/specs/local-logging/spec.md`

- [ ] 9.1 `logging` 统一日志桩：全部模块经它写日志；级别 error/warn/info/debug 可调；每条带模块标识与级别
- [ ] 9.2 文件 sink：应用数据目录下的日志文件，按大小滚动淘汰（上限与保留份数为固定常量）；写入失败静默，不阻塞业务、不向用户报错
- [ ] 9.3 敏感边界：凭据（API Key、令牌）绝不入日志（含错误链路）；完整计划内容只允许 debug 级别
- [ ] 9.4 命令 `get_debug_log_dir`：返回日志目录路径（前端设置页提供「打开日志目录」入口）
- [ ] 9.5 测试：级别过滤；滚动淘汰；写失败不 panic；以包含敏感串的凭据断言日志文件无明文

## 10. 编辑态后端契约

对应规范：`openspec/specs/task-graph/spec.md` 的「任务内容的编辑态」与「子任务的结构化存储与文本渲染」

- [x] 10.1 命令 `get_editor_workspace(cycle_id)` 与 `get_editor_workspaces_by_cycle_ids(cycle_ids)`（批量返回，键为周期标识）
- [x] 10.2 不存在或不可见的周期返回空结果而非错误
- [x] 10.3 子任务结构化存储 ↔ Markdown 渲染：层级与顺序一致；空列表产出空串；标题含 Markdown 元字符时不破坏结构
- [x] 10.4 测试：单周期与批量入口一致；空周期；渲染往返；元字符转义

## 11. 验收

- [ ] 11.1 `cargo test` 全绿；`npm run build` 与 `npm run test` 通过
- [ ] 11.2 手工走通：创建长周期 → 添加目标 → 建立周计划 → 建立日任务 → 启动专注块 → 结束专注块后 `focused_time` 累加
- [ ] 11.3 手工验证 Keep/Revert：制造两条待确认改动，分别 Keep 与 Revert，确认数据与快照终态
- [ ] 11.4 手工验证删除守卫的四种拒绝，确认提示文案与 `code` 对应
- [ ] 11.5 手工验证重复日程：保存一个专注块为每日重复 → 次日出现实例 → 改模板确认历史不变 → 停止重复确认历史保留
- [ ] 11.6 手工验证本地日志：制造一次可排障事件后能在日志目录看到记录；含凭据的操作不出现在日志中；断网状态下应用零外部出网且日志照常写入
- [ ] 11.7 导出备份并在全新数据目录恢复，确认数据一致
- [ ] 11.8 更新 `docs/architecture.md` 中与实现不一致的部分（若有）

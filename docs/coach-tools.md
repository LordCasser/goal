# Coach 工具目录

当前共 **24 个模型可见工具**。按用户意图组织，不把每个 GUI 按钮映射成工具，也不提供通用 `invoke`、SQL、文件或 shell 工具。实现入口为 `ai/tools.rs`、`ai/tool_catalog.rs`；非任务行操作由 `ai/actions.rs` 暂存，用户确认后调用现有业务服务。

## 分类与参数

| 分类 | 数量 | 工具 | 主要参数 / 语义 |
| --- | ---: | --- | --- |
| 技能路由 | 1 | `load_skill` | `name`：9 个内置技能之一；读取用户修改后的 SKILL.md |
| 查询 | 10 | `list_cycles` | 可选 `start_date` / `end_date`，必须同时提供；可选 `cycle_type`；返回 ID、日期和类型 |
| | | `get_cycle_context` | 可选 `cycle_id`；省略表示当前聚焦周期 |
| | | `get_task_details` | `task_id`；完整任务与执行步骤 |
| | | `get_calendar` | `start_date`、`end_date`；最多 62 天，含日任务、专注安排和时间预算 |
| | | `get_period_context` | 精确日期范围；季度、半年、年度的事实数据，不推算不存在的历史 |
| | | `get_planning_issues` | 当前周期的问题诊断，只读 |
| | | `get_pending_changes` | 当前周期待确认任务和该会话提出的操作 |
| | | `get_settings` | 常用设置及已验证 provider/model 选择；不返回密钥、URL、请求头 |
| | | `list_reminders` | 可选 `cycle_id`；`status=pending/fired/all` |
| | | `list_repeats` | 活跃的每日重复模板 |
| 事务编辑与复盘 | 6 | `create_goal` | `title`、`rationale`；可选目标 `cycle_id`；支持 Later、长期、周、日周期 |
| | | `update_goal` | `task_id`、`rationale`；可选 `title`、`completed`、`subtasks` |
| | | `delete_goal` | `task_id`、`rationale` |
| | | `update_goal_breakdown` | `task_id`、结构化 `update`、`rationale` |
| | | `update_prioritization_breakdown` | 五类优先级的结构化 `update`、`rationale`；只在 prioritization 技能提供 |
| | | `start_review` | 读取当前周期复盘事实并进入复盘；只在 cycle-review 技能提供 |
| 周期与安排 | 4 | `propose_cycle` | `change.operation=create/start/finish/delete/copy_uncompleted`，各分支有独立参数 |
| | | `propose_focus_block` | `create/update/schedule/reorder`；使用 day/session ID；时长为分钟；`schedule.starts_at` 为 RFC 3339 时间，传 `null` 移回未安排并保留时长 |
| | | `propose_task_organization` | `move/link/color/reorder`；保留任务 ID；归属、颜色可显式置 null |
| | | `propose_day_move` | `cycle_id`、`target_date`、`strategy`；null 遇占用停止，merge/swap 必须有明确意图 |
| 提醒与重复 | 2 | `propose_reminder` | `create/update/delete`；目标 kind + ID；带时区的提醒时间；明确是否遵循免打扰 |
| | | `propose_repeat` | `create/update/stop`；已有专注块 ID 或模板 ID；更新仅影响未来实例 |
| 设置 | 1 | `propose_settings` | 一次一项 `change.setting`；所有修改等待 GUI 确认 |

`propose_*` 的顶层统一为 `{change, rationale}`。`change` 使用封闭的判别联合，各分支只接受与其相关的字段；不是可执行任意命令的字符串。参数中没有 `approved`、`auto_apply` 或跳过确认开关。

## 设置范围

`propose_settings` 支持 9 项：

- `theme`：white / gray。
- `week_start_day`：1–7，周一为 1。
- `coach_idle_minutes`：1–1440 分钟。
- `plan_with_ai`：boolean。
- `daily_capacity_minutes`：1–1440，null 清除。
- `daily_reminder`：本地 HH:MM，null 关闭。
- `quiet_hours`：start/end 两个 HH:MM，或同时为 null。
- `log_level`：error / warn / info / debug。
- `active_model`：`provider_id + model_id`，只能选择已经保存并测通的精确组合；确认时重新校验。

Provider 的增删、endpoint、请求头和密钥编辑继续使用设置页；模型不能读取、保存或测试任意凭据。系统通知权限、更新安装、文件导出路径、日志文件以及窗口布局操作也保留 GUI。上述边界不会伪装成已经完成的工具能力。

## 按需提供

| 当前技能 | 可见数量 | 范围 |
| --- | ---: | --- |
| coach | 7 | 路由、基础查询、设置读写 |
| 长期 / 周 / 日规划、目标澄清 | 18 | 查询、任务预览、周期、专注、归属、提醒和重复 |
| prioritization / cycle-review | 19 | 规划工具加各自的专用入口 |
| period-analysis | 7 | 基础查询、时段数据、日历；无写工具 |
| planning-issues | 6 | 基础查询、问题诊断；无写工具 |

每轮调用前校验当前技能实际提供的工具。切换技能后重新构建工具集合和系统指令。GUI 的 start_planning / start_goal_setting / start_prioritization 仍是应用内部入口，不在模型目录里重复占位。确认与放弃的 IPC 命令永远不进入模型目录。

## 参数约定

先查再写，使用真实 ID；标题和列表位置不是身份。省略可选修改字段表示保留，显式 null 只用于允许清除的字段。`completed` 是期望状态，不是 toggle；`subtasks` 是完整替换，空数组才表示清空。列表查询最多返回 100 条并明确 `truncated`，日历查询有日期上限。

日期采用 YYYY-MM-DD；提醒、专注排期采用带明确时区的 RFC3339，避免秒 / 毫秒和时区混淆。专注块时长使用整数分钟。长期周期沿用现有产品的 1 / 3 / 6 × 28 天承诺；创建周 / 长期容器从当前本地日期开始，不能通过工具伪造其他起始日。

## 确认与聊天回执

任务内容编辑继续使用任务预览和原始快照。Coach 显示真实修改前后内容，主体原位预览并锁定受影响项，确认 / 拒绝后解除锁定；跨周期预览显示各自所属周期。只在 Coach 保留确认控件。确认与会话回执在同一 SQLite 事务中保存，回执随后进入消息列表，并被后续模型上下文读取。

非任务行操作共用 `agent_actions`，只存枚举参数、理由、明确的操作描述和处理状态。提出时不改业务数据，拒绝时不执行业务服务，确认时重新执行 GUI 的校验。改变了的描述先刷新并要求重新核对。重复提出相同的待确认操作复用同一条记录。应用中意外退出的记录不自动重试，用户核对实际结果后可关闭记录。

输入区上方只保留未处理的确认项；“已添加／已更新／已删除／已放弃”具体结果进入聊天历史，随消息滚动，不形成永久横条。模型只能收到 proposed 或实际错误，不能自行确认。优先级分类写入同样先进入 `agent_actions` 待确认队列；用户在 Coach 中确认后才保存整份周期结论。它不会提交任务完成态或改变任务归属。

## 回答风格

`~/.goal/persona.md` 与 `~/.goal/skills/` 分开管理。首次安装默认文件，已有文件不覆盖；每次生成读取最新 UTF-8 内容。默认风格为简洁、可靠的秘书／助理：先结果和下一步，少复述，一次只问关键问题。Coach 和时段分析共用此风格；权限、数据真实性及 GUI 确认要求由代码保留。

## 单独跟踪的边界

本轮不加入通用脚本执行、任意文件编辑、历史关系账本或自主定时 Agent。完整的引导式复盘答案提交、issue 忽略理由、反馈与更新管理尚未作为模型工具开放。它们需要分别设计可审阅的操作对象，避免把 GUI 命令列表直接扩张成模型权限。

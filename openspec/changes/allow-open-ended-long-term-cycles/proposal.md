## Why

当前长期周期必须有结束日，并自动生成进度检查。持续性目标无法自然表示，用户也不能只关闭检查安排。结束日期和检查安排应独立选择。

## What Changes

- 长期周期可选择预设时长、自定义结束日或无固定结束日；无固定结束日保留开始日，仍可手动开始和完成。
- 进度检查可选择无、指定一次或定期检查，与结束日期独立；无结束日的定期检查可持续推导，指定一次仍需有效日期。
- 工作台、日历、周期生命周期和 AI 提议按开放式日期边界处理；无结束日不显示剩余周数或虚构的复盘日。
- 新建迁移放宽 `progress_check` 约束，保留现有周期及检查数据。

## Capabilities

### New Capabilities

无。

### Modified Capabilities

- `planning-cycles`: 长期周期允许无结束日，进度检查可为空，相关日历身份和生命周期规则随之调整。
- `planner-workspace`: 长期周期创建与列头展示无结束日、无检查状态。

## Impact

依赖 `rebuild-baseline`、`refine-planning-cycle-controls`、`add-reminders-notifications`。涉及 `cycles` SQLite 约束与迁移、周期创建服务和 AI 操作校验、日历范围查询、长期周期创建弹窗、工作台与日历的周期选择和列头展示。既有有期限周期的数据与行为保留。

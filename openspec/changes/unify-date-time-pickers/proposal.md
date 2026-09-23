## Why

长期周期、提醒和日程编辑使用浏览器原生日期/时间弹窗，外观与工作台不一致，也会随系统平台变化。已有日历日期网格具备所需的基础交互，可以统一这些选择体验。

## What Changes

- 表单中的日期、时间及日期时间输入改为与应用主题一致的控件；保留原有日期、时区和精度语义。
- 长期周期的开始、结束和单次检查日期共用日历选择；继续遵守各字段的有效范围。
- 提醒与日程的时间选择共用时分选择；日历中的移动日期网格使用相同的日历视觉与键盘规则。
- 日期/时间控件在中英文、白底/灰底主题、键盘与窄窗口下保持可用。

## Capabilities

### New Capabilities

- `date-time-controls`：应用内日期、时间及日期时间选择的交互与可访问行为。

### Modified Capabilities

无。

## Impact

依赖 `rebuild-baseline`、`add-calendar-time-view`、`add-reminders-notifications`、`allow-open-ended-long-term-cycles`。影响 React UI 控件、长期周期创建、提醒编辑/设置和日历日程编辑及对应前端测试；不修改 IPC、数据库或日期/时间持久化格式。受影响的既有规范为 `openspec/specs/planner-workspace/spec.md`、`openspec/changes/add-calendar-time-view/specs/calendar-view/spec.md`、`openspec/changes/add-reminders-notifications/specs/reminders/spec.md`，其业务要求不变。

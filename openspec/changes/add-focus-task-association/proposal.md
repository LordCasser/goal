## Why

专注块属于某一天，但目前无法表达这一段专注服务于哪项日任务。需要在添加专注块时允许用户可选关联当天的任务，并按关联记录任务投入；杂事专注仍然可以独立记录。

## What Changes

- 在添加专注块表单中提供可选任务选择，每个专注块最多关联一项同日、已确认、有标题的真实任务，默认不关联。
- 日任务的专注时间由关联区块汇总并随编辑态数据返回，独立区块只计入日及其周期祖先；不新增常驻统计展示。
- 递归删除任务或目标时，预览和确认覆盖关联专注块，并清理计时累计与提醒。
- 单独跨天移动任务时解除关联，保留原日的专注记录；整体移动或合并日内容时保留关联。
- 重复模板生成的新区块不继承某一天的任务关联。

## Capabilities

### New Capabilities
- `focus-task-association`: 专注与日任务的可选关联、派生累计及关联的生命周期。

### Modified Capabilities
无；新增能力补充现有周期与任务行为。

## Impact

前置依赖：`rebuild-baseline`、`add-calendar-time-view`、`refine-planning-cycle-controls`。关联现有规范：`openspec/specs/planning-cycles/spec.md`、`openspec/specs/task-graph/spec.md`、`openspec/specs/session-repeats/spec.md`，以及日历与提醒变更中的规范。

影响 SQLite 迁移、Rust 周期创建/任务投影/递归删除/任务移动/日历合并服务，以及前端添加专注块表单和删除确认。使用现有数据实体与 IPC，不增加任务行专注入口、独立计时器或持久化任务累计字段。

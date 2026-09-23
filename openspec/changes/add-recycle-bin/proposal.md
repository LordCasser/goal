## Why

目前目标和计划一旦确认删除就从本地数据中消失，误删长期目标、稍后事项或整个计划后无法回到原来的安排。用户需要一个与「稍后」同样容易找到的回收站，让日常删除可恢复，只有在回收站内再次确认才永久移除。

## What Changes

- 新增回收站能力：一次删除及其关联影响组成一个可恢复条目，记录来源、标题和删除时间。
- 工作台工具栏在「稍后」旁增加「回收站」入口；侧栏提供查看、逐项恢复、多选和确认后永久删除。
- 目标、Later 事项以及长期、周、日计划的用户删除操作默认移入回收站；关联子目标、跨计划子任务、专注块和提醒随同一条目处理。
- 恢复时回到删除前的周期、父项和顺序；原位置不可用或身份冲突时保留回收站条目并解释失败原因。
- 已确认的 Coach 删除遵循相同规则；未确认的提议与撤销仍沿用既有预览语义。

## Capabilities

### New Capabilities

- `recycle-bin`: 回收站条目、原位恢复、冲突处理与永久删除的用户行为。

### Modified Capabilities

- `planner-workspace`: 工具栏与左侧抽屉增加回收站入口和多选操作。
- `planning-cycles`: 规划周期的用户删除改为可恢复移除，原有删除守卫和影响预览保留。
- `task-graph`: 目标及其跨计划关联子项的用户删除改为整组可恢复移除。
- `agent-proposals`: 用户确认 Coach 删除后进入回收站，待确认提议仍独立存在。

## Impact

前置依赖：`rebuild-baseline`；关联 `add-ai-planning-core`、`add-review-retrospective`、`add-reminders-notifications` 和 `add-focus-task-association` 的现有数据关系。

受影响的既有规范：`openspec/specs/planner-workspace/spec.md`、`openspec/specs/planning-cycles/spec.md`、`openspec/specs/task-graph/spec.md`、`openspec/specs/agent-proposals/spec.md`。实现将涉及 SQLite 迁移、Rust 删除与恢复事务、Tauri 命令和失效事件、React 侧栏及中英文文案。无需外部服务或新的运行时依赖。

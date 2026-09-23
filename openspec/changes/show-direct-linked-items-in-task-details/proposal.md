## Why

长期目标和周事项的详情目前只显示备注，用户无法从这里确认它们直接关联了哪些下级事项。工作台一次只加载当前可见的计划，因而不能完整列出其他周、其他日期中的关联项。

## What Changes

- 双击长期目标或周事项时，在详情底部展示其直接关联的一级子项，包含事项标题、所属计划层级和日期。
- 长期目标只列出直接关联的周、日事项；周事项只列出直接关联的日事项。同周期子步骤、间接后代、空白输入行和未确认的提议不计入。
- 没有关联子项时显示明确空状态；关联变化后重新打开详情可看到最新结果。

## Capabilities

### Modified Capabilities

- `task-graph`：在既有事项详情中增加直接关联子项的只读视图。

## Impact

前置依赖：`rebuild-baseline`、`refine-task-interactions-and-continuity`。受影响的既有规范为 `openspec/specs/task-graph/spec.md`。实现涉及只读任务查询、Tauri 命令、详情弹窗和相关测试；不修改任务关系或数据库结构。

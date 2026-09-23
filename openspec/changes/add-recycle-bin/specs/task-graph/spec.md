## MODIFIED Requirements

### Requirement: 任务的自引用树

任务 SHALL 支持 `parent_id` 自引用形成树，同级顺序由 `position` 决定；子任务可以以 Markdown/JSON 形式存于 `subtasks` 字段。用户删除父任务时，其子任务 SHALL 与父任务作为同一回收站条目从活动列表移出；恢复时 SHALL 保留原有父子关系和顺序。

#### Scenario: 添加子任务
- **WHEN** 用户在某个任务下添加子任务
- **THEN** 新任务 `parent_id` 指向该任务，并出现在其下方

#### Scenario: 删除父任务
- **WHEN** 用户删除一个带有子任务的任务
- **THEN** 其子任务一并从活动列表移出，相关预览不再作为待确认内容显示，回收站出现可恢复条目

#### Scenario: 恢复父任务
- **WHEN** 用户恢复该父任务且原计划仍存在
- **THEN** 父任务与子任务回到原计划，父子关系和同级顺序与删除前一致

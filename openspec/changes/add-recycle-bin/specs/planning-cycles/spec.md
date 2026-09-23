## MODIFIED Requirements

### Requirement: 周期类型与层级

系统 SHALL 用一张自引用的表承载全部时间层级，类型限定为 `session` / `day` / `week` / `month`，其中 `month` 在产品语义上即 Long-term cycle。用户删除父周期时，其下级周期和任务 SHALL 随父周期一起移入回收站，并在恢复父周期时重建原层级。

#### Scenario: 创建子周期
- **WHEN** 用户在某个 week 周期下创建 day 周期
- **THEN** 新周期 `parent_id` 指向该 week 周期，且查询 week 的子树能返回该 day

#### Scenario: 删除父周期
- **WHEN** 用户删除一个有子周期的 week 周期
- **THEN** 其下所有 day 与 session 周期及其任务从活动计划移出，并与该 week 一起进入回收站

#### Scenario: 恢复父周期
- **WHEN** 用户从回收站恢复该 week 周期，原上级周期仍存在
- **THEN** 其下 day、session 和任务回到删除前的层级及日期

#### Scenario: 拒绝未知类型
- **WHEN** 尝试写入 `type` 不在四个取值内的周期
- **THEN** 数据库 CHECK 约束拒绝写入

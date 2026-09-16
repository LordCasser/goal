## MODIFIED Requirements

### Requirement: Do Later 收纳容器

系统 SHALL 提供一个无起止日期的 Long-term 容器（标题 "Later"），用于暂存尚未承诺的目标。该容器 MUST NOT 参与常规周期删除。Later 中的任务 SHALL 通过 `later_plan_type` 记录用户准备安排的周期类型，取值为 `month`、`week`、`day`；直接新增的想法和缺少该字段的旧数据按 `month` 解释，不得猜测旧来源。

#### Scenario: 暂存一个想法
- **WHEN** 用户在 Do Later 侧栏输入一条目标
- **THEN** 该目标以 `type='month'` 的 Later 周期为 `cycle_id` 创建，且不出现在任何带日期的周期列中
- **AND** 目标的 `later_plan_type` 为 `month`

#### Scenario: 保留来源计划类型
- **WHEN** 用户把长期、周或日计划中的任务移入 Later
- **THEN** 任务进入 Later 容器并分别记录 `month`、`week` 或 `day` 的 `later_plan_type`
- **AND** 同一来源周期内随根任务移动的子步骤保留同一类型

#### Scenario: 把长期想法拉进目标周期
- **WHEN** 用户对 `later_plan_type='month'` 的 Later 目标选择提升并选定一个长期周期
- **THEN** 目标 `cycle_id` 更新为目标周期，`later_plan_type` 清空，原 Later 中不再显示

#### Scenario: 周想法默认安排到本周
- **WHEN** 用户对 `later_plan_type='week'` 的 Later 目标选择安排到本周
- **THEN** 系统按用户的 `week_start_day` 和当前本地日期确定本周，并将目标移入该周
- **AND** 已存在该周周期时复用它，不要求用户再次选择长期周期

#### Scenario: 日想法默认安排到今天
- **WHEN** 用户对 `later_plan_type='day'` 的 Later 目标选择安排到今天
- **THEN** 系统按当前本地日期确定今天的日周期，并将目标移入该日
- **AND** 已存在该日周期时复用它

#### Scenario: 默认目标周期不存在
- **WHEN** 本周或今天尚无对应周期，用户确认把周或日 Later 目标安排到默认周期
- **THEN** 系统在同一操作中创建所需周期并移动目标
- **AND** 周期创建或移动任一步骤失败时整体回滚，不留下没有任务的孤立周期

#### Scenario: 重复提交提升操作
- **WHEN** 用户在一次安排操作尚未完成时重复点击按钮，或重试同一请求
- **THEN** 系统最多完成一次有效移动，返回当前结果或明确的可见错误
- **AND** 不产生重复周期、重复任务或空周期

#### Scenario: 提升失败
- **WHEN** 目标周期已结束、目标类型不匹配或数据库操作失败
- **THEN** Later 项保留在原容器及原 `later_plan_type`，界面显示可理解的错误
- **AND** 不显示成功状态，不留下部分移动结果

## Purpose

定义应用的骨架对象：时间容器（planning cycle）。所有目标、任务、专注块都必须挂在某个周期下，周与日周期允许独立存在；可选的 `parent_id` 表达时间收纳，专注块必须属于某一天，并各自带生命周期、日历身份与删除守卫。

## Requirements

### Requirement: 周期类型与层级

系统 SHALL 用一张自引用的表承载全部时间层级，类型限定为 `session` / `day` / `week` / `month`，其中 `month` 在产品语义上即 Long-term cycle。子周期的删除 SHALL 级联到其后代。

#### Scenario: 创建子周期
- **WHEN** 用户在某个 week 周期下创建 day 周期
- **THEN** 新周期 `parent_id` 指向该 week 周期，且查询 week 的子树能返回该 day

#### Scenario: 删除父周期
- **WHEN** 删除一个有子周期的 week 周期
- **THEN** 其下所有 day 与 session 周期及其任务一并删除

#### Scenario: 拒绝未知类型
- **WHEN** 尝试写入 `type` 不在四个取值内的周期
- **THEN** 数据库 CHECK 约束拒绝写入

### Requirement: 周期生命周期单调

周期 SHALL 有 `started` / `finished` 两个布尔状态与对应时间戳。系统 MUST NOT 出现「未开始但已结束」的状态。

#### Scenario: 启动专注块
- **WHEN** 用户启动一个 session 周期
- **THEN** `started=1` 且写入 `started_at`

#### Scenario: 非法状态写入
- **WHEN** 尝试写入 `started=0, finished=1`
- **THEN** 数据库 CHECK 约束 `NOT (started = 0 AND finished = 1)` 拒绝写入

### Requirement: 日历身份唯一

带日历含义的周期 SHALL 通过 `calendar_key` 唯一标识，系统 MUST NOT 允许同一个 `calendar_key` 存在两条周期。键 SHALL 是带类型前缀的复合串，且 MUST 与用户可见的周期命名一致。

键的构成（已验证的运行时格式）：

| 类型 | 键 |
| --- | --- |
| `day` | `day:{本地日期}` |
| `week` | `week:{该周首日}` |
| `month`（Long-term） | `long-term:{starts_on}:{ends_on}` |
| `session` / Do Later | 无键 |

周的键使用**该周首日**，因此会随用户设置的周起始日变化。日期边界由 `starts_on` / `ends_on` 表达。

#### Scenario: 重复创建同一天
- **WHEN** 对同一日期再次创建 day 周期
- **THEN** 唯一约束拒绝，调用方应改为复用已存在的周期

#### Scenario: 长周期的键体现起止
- **WHEN** 创建一个从某日开始的 3 个月长周期
- **THEN** 其 `calendar_key` 同时包含起始日与结束日，因此同一起始日的不同时长周期不会互相冲突

#### Scenario: 周键随起始日设置变化
- **WHEN** 用户更改周起始日设置
- **THEN** 之后创建的周周期使用新的周首日生成键；既有周周期不受影响

#### Scenario: 首次确定周起始日
- **WHEN** 用户首次打开需要周起始日的界面
- **THEN** 系统按系统区域推断并持久化 `week_start_day`，之后不再变更

### Requirement: 长周期时长受限选项

Long-term cycle 的时长 SHALL 只从三档中选取，且各档 MUST 按 **28 天为一个月**计算而非日历月：1 个月 = 28 天、3 个月 = 84 天（整 12 周）、6 个月 = 168 天（整 24 周）。创建时即确定 `starts_on` 与 `ends_on`。

按整周计算的理由：产品对用户承诺的是周数（列头显示 "11 weeks left" 这类信息），若按日历月计算会得到非整周数，导致承诺与实际不符。

#### Scenario: 选择 3 个月
- **WHEN** 用户在周期选择界面选中 3 个月并确认
- **THEN** 系统创建 `type='month'` 周期，时长为 84 天，`starts_on` 为今天，`ends_on` 为 `starts_on` 加 84 天，列头显示的剩余周数为整数

#### Scenario: 三档时长均落在整周上
- **WHEN** 用户分别选择 1 / 3 / 6 个月
- **THEN** 对应时长分别为 28 / 84 / 168 天，全部是 7 的整数倍

#### Scenario: 时长超出允许集合
- **WHEN** 调用方提交不在允许集合内的时长
- **THEN** 系统拒绝并返回校验错误

### Requirement: 周期的时长用于计算日期边界

系统 SHALL 由周期时长推导 `ends_on`，且 MUST NOT 依赖日历月加月运算（那会产生非整周边界）。引用相同日期的两个周期 MUST 得到相同的日期边界，不因创建时刻或时区而漂移。

#### Scenario: 由时长推导结束日
- **WHEN** 创建一个从某日开始的 3 个月长周期
- **THEN** `ends_on` 等于该日加 84 天

#### Scenario: 不随时区漂移
- **WHEN** 同一日期在不同时区下创建周期
- **THEN** `starts_on` / `ends_on` 表示的是本地日期，不因 UTC 偏移而前后移动一天

#### Scenario: 传入的时长无法被整周表达
- **WHEN** 调用方提交一个不是 7 的整数倍的天数
- **THEN** 系统拒绝，避免产生非整周的周期边界

### Requirement: 启动周期前必须有时长

系统 MUST NOT 允许启动一个没有设定时长的周期。未定时长的周期只能作为容器存在（例如暂存区），不能进入开始/结束的生命周期。

#### Scenario: 无时长时启动被拒
- **WHEN** 用户尝试启动一个未设定时长的周期
- **THEN** 系统拒绝并返回明确错误

#### Scenario: 有时长时正常启动
- **WHEN** 用户启动一个已设定时长的周期
- **THEN** 写入 `started=1` 与 `started_at`

### Requirement: 子周期创建的层级要求

周、日周期 SHALL 允许没有父周期。若调用方显式指定父周期，系统 MUST 校验相邻层级及父周期未结束。创建日历日期 MUST NOT 依赖长期目标存在；每周、每日可包含多条独立或已关联的事务。

#### Scenario: 周周期缺少父周期
- **WHEN** 用户尚无长期目标并创建本周事务
- **THEN** 创建独立的自然周容器，允许添加多条周事务

#### Scenario: 独立日周期
- **WHEN** 用户创建没有父周期的日计划
- **THEN** 按日期保存，重复打开复用该日，不强制补父周期

#### Scenario: 打开或移动到无长期周期覆盖的日期
- **WHEN** 日历需要该日期的日容器
- **THEN** 复用既有日期身份，必要时建立独立周与日，不因缺少长期周期而拒绝

#### Scenario: 父周期已结束
- **WHEN** 尝试在一个已结束的长周期下创建周周期
- **THEN** 系统拒绝并返回明确错误

#### Scenario: 父周期仍在进行
- **WHEN** 在一个未结束的长周期下创建周周期
- **THEN** 创建成功并建立父子关系

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

### Requirement: 从上一周期复制未完成项

系统 SHALL 支持把上一周期中未完成的任务复制到当前周期，并记录复制血缘。

#### Scenario: 复制上一周未完成任务
- **WHEN** 用户在当前周选择 "Copy from previous week"
- **THEN** 上一周所有未完成任务在当前周生成副本，副本的 `copied_from_task_id` 指向原任务

#### Scenario: 上一周期无未完成项
- **WHEN** 上一周期所有任务都已完成
- **THEN** 复制操作返回空结果，不产生任何任务

### Requirement: 时长是创建时的一次性承诺

已创建周期的时长 SHALL 不可修改，系统 MUST NOT 提供重命名或修改时长的入口。周期级操作 SHALL 只包含删除（删除前给出影响预览）。

设计意图：周期是一段**承诺**。若允许随时改短，用户会在每次进展不顺时收缩承诺，机制失效。用户想调整节奏的正确做法是结束当前周期并新建一个。

#### Scenario: 查看周期选项
- **WHEN** 用户打开某个周期的选项菜单
- **THEN** 只显示删除（及删除影响预览），不显示重命名或修改时长

#### Scenario: 想改变节奏
- **WHEN** 用户希望换一个更短或更长的周期
- **THEN** 结束当前周期并新建一个，而不是修改既有周期的时长

### Requirement: 剩余时间读数为整数

周期列 SHALL 显示剩余时间的读数（周或天），且该读数 SHALL 是整数。系统 MUST NOT 显示小数形式的剩余周数。

这也是时长必须落在整周上的原因之一：非整周时长会让"剩余周数"无法用整数表达。

#### Scenario: 长周期剩余时间
- **WHEN** 一个 84 天的长周期过去了 21 天
- **THEN** 列头显示剩余 9 周，而不是 9.0 或 8.9 周

#### Scenario: 短周期剩余时间
- **WHEN** 一个周周期只剩不到一天
- **THEN** 以天或小时表达剩余，不使用小数周

#### Scenario: 已结束的周期
- **WHEN** 周期已结束
- **THEN** 不再显示剩余时间读数

### Requirement: 规划周期删除守卫

系统 SHALL 在删除规划容器前评估其可否删除，并 MUST 拒绝会导致数据丢失或语义破坏的删除。单个 Focus block 是可删除的专注记录，即使它已经开始或完成。

#### Scenario: 删除过去规划周期
- **WHEN** 用户尝试删除一个已结束的月、周或日规划周期
- **THEN** 系统拒绝并提示 "Past cycles can't be deleted."

#### Scenario: 删除历史专注块
- **WHEN** 用户删除一个已经完成的单个 Focus block
- **THEN** 系统允许删除该 Focus block
- **AND** 从其父级日、周、月的 focused_time 聚合中扣除该记录的已计时间，并将聚合值限制为不小于 0
- **AND** 删除该 Focus block 的排期提醒

#### Scenario: 删除运行中的专注块
- **WHEN** 用户删除一个已经开始但尚未完成的单个 Focus block
- **THEN** 系统允许删除该 Focus block 及其排期提醒
- **AND** 不向任何父级 focused_time 聚合追加时间

#### Scenario: 删除包含已启动专注块的周期
- **WHEN** 目标月、周或日规划周期下存在已经启动的 session
- **THEN** 系统拒绝并说明该周期包含已开始的专注块

#### Scenario: 超出可删范围
- **WHEN** 用户尝试删除非最近 N 个的周期
- **THEN** 系统拒绝并提示只有最新 N 个可删除

#### Scenario: 删除前的预览
- **WHEN** 用户请求删除某个周期
- **THEN** 系统先返回删除影响预览（受影响的子周期与任务），用户确认后才执行

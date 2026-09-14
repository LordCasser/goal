## Purpose

定义应用退出、反馈与自我更新这三件「产品外围但影响留存」的行为。退出不是静默消失，而是一次有门槛、可忽略、结果可上报的对话；同时应用要能自我更新并在专注块结束时通知用户。

## Requirements

### Requirement: 退出调查

系统 SHALL 在满足条件时于退出前弹出一次调查，且 SHALL 用持久化时间戳保证同一用户不会反复被打扰。调查 SHALL 支持「继续使用」与「确认退出」两条出路。

#### Scenario: 满足弹出的条件
- **WHEN** 用户触发退出且满足弹出条件（如使用时长与历史可见记录符合要求）
- **THEN** 弹出调查，并记录已展示时间

#### Scenario: 用户选择留下
- **WHEN** 用户在调查中选择继续
- **THEN** 应用不退出，正常回到主界面

#### Scenario: 用户确认退出
- **WHEN** 用户提交调查并确认退出
- **THEN** 调查结果上报后应用退出

#### Scenario: 用户直接关闭
- **WHEN** 用户关闭调查面板
- **THEN** 记录一次忽略并正常退出，不再重复弹出

#### Scenario: 列表未就绪
- **WHEN** 前端尚未准备好接收调查事件
- **THEN** 系统等待前端就绪信号后才推送，不丢失调查

### Requirement: 反馈与联系作者

系统 SHALL 提供反馈入口，并可引导用户到与作者沟通的渠道。反馈内容 MUST 携带足够的诊断上下文（应用版本、系统版本、匿名标识）。

#### Scenario: 提交反馈
- **WHEN** 用户提交一段文字反馈
- **THEN** 系统带上版本与匿名标识上报，并提示 "Thanks — your feedback was received."

#### Scenario: 空反馈
- **WHEN** 用户提交空白内容
- **THEN** 提示 "Please enter your feedback."，不上报

#### Scenario: 上报失败
- **WHEN** 网络失败
- **THEN** 提示 "Couldn't send feedback. Please try again."，内容不丢失

#### Scenario: 复制支持 ID
- **WHEN** 用户选择复制支持 ID
- **THEN** 匿名标识进入剪贴板并提示已复制

### Requirement: 自动更新

系统 SHALL 能检查、下载并安装新版本，且 MUST 在安装前校验更新包来源可信。检查与安装失败 SHALL 各自可提示且不阻断使用。

#### Scenario: 存在新版本
- **WHEN** 检查到新版本
- **THEN** 提示 "Update available:" 并允许用户下载安装

#### Scenario: 已是最新
- **WHEN** 版本没有更新
- **THEN** 提示 "No updates available."，不干扰当前操作

#### Scenario: 检查失败
- **WHEN** 更新端点不可达
- **THEN** 记录失败并提示，应用继续正常使用

### Requirement: 专注块结束通知

系统 SHALL 在专注块的计划时长到达时发系统通知，且 MUST 在发送前确保通知权限已被授予。

#### Scenario: 专注块自然结束
- **WHEN** 一个设置了时长的专注块到达结束时间而应用未在前台
- **THEN** 发送系统通知

#### Scenario: 权限未授予
- **WHEN** 通知权限未授予
- **THEN** 系统请求权限；被拒绝时静默跳过通知，不影响专注块状态

#### Scenario: 无时长的专注块
- **WHEN** 专注块未设置时长
- **THEN** 不产生到期通知

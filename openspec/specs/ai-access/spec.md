## Purpose

定义 AI 能力的授权与供应商选择。规划器本身永久免费、无订阅；AI 只有「内置额度（限时）+ 自带凭据」两种状态，两种模式在 agent、语音等上层能力上必须表现一致。

## Requirements

### Requirement: 无订阅的授权模型

系统 MUST NOT 提供可购买的订阅、付费档位或 AI 额度包。授权的唯一作用是**控制内置 AI 额度的可用性**，MUST NOT 限制规划器本身的功能。

#### Scenario: 授权无效时的本地能力
- **WHEN** 内置额度已用尽且未配置任何自带凭据
- **THEN** 周期、任务、专注块、工作台、链接可视化等全部本地功能照常可用，不出现付费提示

#### Scenario: 不存在购买入口
- **WHEN** 用户查看设置
- **THEN** 只有「内置 AI」与「自带凭据」两类配置项，没有购买、升级或额度包入口

#### Scenario: 额度用尽不阻断已达成的数据
- **WHEN** 内置额度用尽
- **THEN** 既有数据、未确认改动与历史会话全部保留且可继续操作

### Requirement: 设备绑定的授权令牌

系统 SHALL 使用一个自包含的授权令牌表达权限状态，令牌 MUST 绑定设备指纹，包含签发时间、过期时间、状态与试用到期时间，状态取值限定为 `trial` / `paid` / `blocked`。设备指纹 SHALL 由硬件标识派生并加盐，MUST NOT 明文暴露原始硬件序列号。

#### Scenario: 首次启动
- **WHEN** 应用首次运行且本地无令牌
- **THEN** 系统以设备指纹换取一条 `trial` 状态令牌并持久化

#### Scenario: 令牌校验失败
- **WHEN** 令牌的设备指纹与当前设备不匹配
- **THEN** 判定为无效，界面提示 "Could not verify access. Check your connection and try again."

#### Scenario: 离线启动
- **WHEN** 应用启动时无法连接授权服务且本地存在未过期令牌
- **THEN** 应用按本地令牌状态继续运行，不阻断主流程

### Requirement: 供应商选择

系统 SHALL 在两个 AI 供应商之间切换：内置托管（`hyperfocus`）与用户自带 OpenRouter（`openrouter`）。切换后上层 agent 行为 MUST 保持一致。

#### Scenario: 默认使用内置 AI
- **WHEN** 用户在试用期内且未配置自有凭据
- **THEN** 所有 agent 请求走托管后端

#### Scenario: 切换到自有凭据
- **WHEN** 用户选择 OpenRouter 并成功保存 API key
- **THEN** 后续 agent 请求走 OpenRouter，设置界面标记该选项为已选

#### Scenario: 供应商状态不可读
- **WHEN** 读取供应商配置失败
- **THEN** 界面降级为未知状态并记录错误，不崩溃

### Requirement: 凭据安全存储

用户凭据 SHALL 存入系统钥匙串，MUST NOT 写入明文配置文件或日志。凭据的读取与删除 SHALL 有独立命令。

#### Scenario: 保存 key
- **WHEN** 用户提交 OpenRouter API key
- **THEN** key 写入钥匙串，界面提示保存成功，配置文件不出现该 key

#### Scenario: 移除 key
- **WHEN** 用户移除凭据
- **THEN** 钥匙串条目被删除且供应商切回可用状态

#### Scenario: 通过 OAuth 连接
- **WHEN** 用户选择通过 OpenRouter 账号授权
- **THEN** 系统走 PKCE 授权码流程（本地回调 + 取消支持），成功后把换取到的凭据写入钥匙串

### Requirement: 试用结束后的降级行为

试用到期后，系统 SHALL 保留本地规划能力，仅停用依赖 AI 的能力，并给出明确的可恢复路径。

#### Scenario: 试用过期后使用 agent
- **WHEN** 令牌状态不再是有效试用且未配置自有凭据
- **THEN** agent 入口提示 "Included AI access has ended. Connect AI provider in Settings to continue."

#### Scenario: 试用过期后的语音
- **WHEN** 试用已结束
- **THEN** 语音听写入口停用并提示 "Voice dictation is unavailable after included AI access ends."

#### Scenario: 本地数据不受影响
- **WHEN** 授权无效或服务不可达
- **THEN** 周期、任务、专注块等本地能力全部可用，数据不丢失

### Requirement: 托管后端的凭据隔离

使用托管后端时，第三方服务凭据（如语音转写）SHALL 由后端换取短期 token 下发，客户端 MUST NOT 持有第三方长期密钥。

#### Scenario: 获取语音 token
- **WHEN** 用户开始语音听写且使用托管后端
- **THEN** 客户端向后端申请一次性 token 与有效期，再用该 token 连接转写服务

#### Scenario: 使用自有 OpenRouter
- **WHEN** 用户切换到 OpenRouter 且尝试语音听写
- **THEN** 系统按无托管凭据处理并给出提示，不尝试使用用户的 LLM key 去换语音服务

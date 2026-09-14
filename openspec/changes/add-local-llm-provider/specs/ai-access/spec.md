## MODIFIED Requirements

### Requirement: 供应商选择

系统 SHALL 在三种 AI 供应商之间切换：内置托管（`hyperfocus`）、用户自带 OpenRouter（`openrouter`）、本机运行时（`local`）。切换后上层 agent 行为 MUST 保持一致；差异只在延迟、流式表现与可用能力上，且这些差异必须对用户可见。

#### Scenario: 默认使用内置 AI
- **WHEN** 用户在试用期内且未配置自有凭据
- **THEN** 所有 agent 请求走托管后端

#### Scenario: 切换到自有凭据
- **WHEN** 用户选择 OpenRouter 并成功保存 API key
- **THEN** 后续 agent 请求走 OpenRouter，设置界面标记该选项为已选

#### Scenario: 切换到本机运行时
- **WHEN** 用户选择本地供应商并选定一个已探测到的模型
- **THEN** 后续 agent 请求发往该本地端点，且不再产生任何出网请求

#### Scenario: 供应商状态不可读
- **WHEN** 读取供应商配置失败
- **THEN** 界面降级为未知状态并记录错误，不崩溃

### Requirement: 试用结束后的降级行为

试用到期后，系统 SHALL 保留本地规划能力，仅停用依赖厂商托管的 AI 能力；使用用户自有凭据或本机运行时的 AI 能力 MUST 保持可用，并给出明确的可恢复路径。

#### Scenario: 试用过期后使用 agent
- **WHEN** 令牌状态不再是有效试用，且未配置自有凭据或本地供应商
- **THEN** agent 入口提示 "Included AI access has ended. Connect AI provider in Settings to continue."

#### Scenario: 试用过期但已配置本地供应商
- **WHEN** 试用已结束且当前供应商为本地
- **THEN** agent 全部可用，不出现任何授权提示

#### Scenario: 试用过期但已配置 OpenRouter
- **WHEN** 试用已结束且用户已保存 OpenRouter 凭据
- **THEN** agent 走该凭据，不受试用状态影响

#### Scenario: 试用过期后的语音
- **WHEN** 试用已结束
- **THEN** 语音听写入口停用并提示 "Voice dictation is unavailable after included AI access ends."

#### Scenario: 本地数据不受影响
- **WHEN** 授权无效或服务不可达
- **THEN** 周期、任务、专注块等本地能力全部可用，数据不丢失

## ADDED Requirements

### Requirement: 本机运行时探测

系统 SHALL 能探测本机是否存在可用的模型运行时，MUST NOT 要求用户手工输入端点才能开始。探测 MUST 校验响应确实来自兼容服务，而不是仅凭端口可连接。用户 SHALL 仍可手工指定端点。

#### Scenario: 探测到运行中的运行时
- **WHEN** 用户选择本地供应商
- **THEN** 系统检查若干常见默认端点，发现可用运行时后列出其模型

#### Scenario: 端口被其他服务占用
- **WHEN** 某默认端点有服务响应但不是兼容的模型服务
- **THEN** 判定为不可用，不把它列为候选项

#### Scenario: 没有可用运行时
- **WHEN** 所有默认端点都不可用
- **THEN** 界面说明需要先启动本地模型运行时，并提供手工填写端点的入口

#### Scenario: 手工端点
- **WHEN** 用户填写自定义端点
- **THEN** 系统按同一套校验流程验证后再允许保存

### Requirement: 模型能力探测与显式降级

系统 SHALL 在选择模型时探测其是否支持工具调用，并 MUST 把结果持久化。运行时不支持工具调用时，agent SHALL 降级为「只能对话、不能提议改动」，且该降级 MUST 在界面上可见。

#### Scenario: 支持工具调用
- **WHEN** 探测显示模型能正确返回工具调用
- **THEN** agent 具有完整的澄清与提议能力

#### Scenario: 不支持工具调用
- **WHEN** 探测显示模型只能返回纯文本
- **THEN** agent 可以对话，但不产生任何待确认改动；侧栏显示当前模型不支持改动的说明

#### Scenario: 能力未知
- **WHEN** 探测请求失败或返回不明确
- **THEN** 按不支持处理，并允许用户重新探测

#### Scenario: 切换模型使探测失效
- **WHEN** 用户更换本地模型
- **THEN** 已缓存的能力标记失效并重新探测

### Requirement: 本地推理的超时与流式输出

本地推理 SHALL 使用分层超时：端点探测使用短超时，内容生成使用可配置的长超时。模型文本 SHALL 以流式方式呈现。流式内容 MUST NOT 直接落库，落库只在回合结束后按序进行一次。

#### Scenario: 探测超时
- **WHEN** 某端点在该次探测的超时时间内没有正确响应
- **THEN** 判定不可用并继续探测下一个候选，不阻塞界面

#### Scenario: 生成超时
- **WHEN** 模型在配置的超时时间内没有完成回合
- **THEN** 回合以超时错误结束，错误可读，会话与既有数据不受影响

#### Scenario: 流式呈现
- **WHEN** 模型开始返回文本
- **THEN** 文本逐步出现在侧栏，用户可以看到生成过程

#### Scenario: 落库顺序
- **WHEN** 回合结束
- **THEN** 该回合的消息按最终顺序一次性写入，序号连续，不出现重复或乱序

### Requirement: 本地供应商不产生出网请求

当供应商为本机运行时，系统 MUST NOT 向任何外部服务发起 AI 相关请求，包括授权服务的 AI token 换取。

#### Scenario: 本地回合
- **WHEN** 一个回合使用本地供应商完成
- **THEN** 期间没有对厂商托管端点的调用

#### Scenario: 授权服务不可达
- **WHEN** 网络完全不可用且供应商为本机运行时
- **THEN** agent 仍然可用

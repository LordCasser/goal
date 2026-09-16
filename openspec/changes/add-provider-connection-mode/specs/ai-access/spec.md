## MODIFIED Requirements

### Requirement: 供应商配置

系统 SHALL 允许用户配置任意数量的自定义供应商，每条包含名称、Base URL、API 格式、API Key（可为空）、模型列表与连接设置。API 格式 SHALL 限定为 Anthropic Messages（`/v1/messages`）、Chat Completions（`/v1/chat/completions`）、Responses（`/responses`）三种之一。同一配置模型 MUST 同时覆盖云端与本地端点，MUST NOT 为特定厂商或本地运行时设置专用接入路径。连接设置 SHALL 使用 `ConnectionSettings` 的三种形态：`{mode:"auto"}`、`{mode:"direct"}` 或 `{mode:"proxy",url:string}`；缺省连接设置 SHALL 等同于 `auto`。

#### Scenario: 添加供应商
- **WHEN** 用户填写名称、Base URL、API 格式、连接设置并至少添加一个模型后保存
- **THEN** 系统先使用该草稿的地址、协议、凭据和连接设置逐个测试模型，全部通过后才持久化并出现在供应商列表

#### Scenario: 本地运行时即普通供应商
- **WHEN** 用户把 Base URL 指向本机端点（如 Ollama / LM Studio）并留空 API Key
- **THEN** 测试通过后保存，且该供应商在其他方面与云端供应商行为一致

#### Scenario: 缺省连接方式
- **WHEN** 供应商草稿未指定连接设置，或读取的既有配置没有该字段
- **THEN** 系统按 `auto` 处理，并在后续保存时使用 `auto` 的路由语义

#### Scenario: 校验失败
- **WHEN** Base URL 不是 http(s)、代理连接设置缺少有效代理 URL，或供应商没有任何模型
- **THEN** 保存被拒并给出行内原因，且旧配置保持不变

#### Scenario: 连接测试失败
- **WHEN** 草稿中任一模型无法按所选连接设置连接或认证失败
- **THEN** 不保存草稿配置或新凭据，已有配置保持原状；表单保留草稿并说明原因

#### Scenario: 验证结果持久化
- **WHEN** 当前配置通过真实连接测试
- **THEN** 后端记录该配置的验证时间，重启后仍可读取，不由前端自行指定已验证
- **AND** 修改配置、模型、凭据或连接设置必须重新测试才能保存；移除凭据或已保存配置重测失败会清除验证状态

### Requirement: AI 失败不伤及本地

AI 请求失败、供应商不可达或配置缺失 MUST NOT 影响本地功能与数据完整性。错误 SHALL 以稳定 code 加可读消息呈现，密钥 MUST NOT 出现在错误信息中。

#### Scenario: 请求失败
- **WHEN** 一次采样请求失败或超时
- **THEN** 既有数据不变、会话可继续、错误信息可读

#### Scenario: 断网时本地端点可用
- **WHEN** 设备离线且激活供应商指向本机端点
- **THEN** AI 能力照常可用，不产生外部网络请求

#### Scenario: 本机端点不依赖桌面代理环境
- **WHEN** 供应商最终请求地址为 localhost、IPv4 回环地址或 IPv6 回环地址，桌面进程没有继承 NO_PROXY 且系统代理不可用
- **THEN** 连接测试和实际采样均直接访问本机端点；自动连接模式的非回环供应商继续使用既有系统代理行为，应用不修改进程或系统的代理配置

#### Scenario: 认证失败
- **WHEN** 供应商返回认证失败
- **THEN** 错误信息提示检查凭据，且不包含 Key 明文

#### Scenario: 删除当前模型
- **WHEN** 用户保存的模型列表不再包含当前模型 ID
- **THEN** 清空激活组合，不自动切换到该供应商的其他模型

#### Scenario: 同供应商多个模型
- **WHEN** 用户切换到同一供应商的另一个已测通模型
- **THEN** 不需要重新添加配置或输入 Key，之后新请求使用明确选定的模型

## ADDED Requirements

### Requirement: 供应商连接路由

系统 SHALL 将供应商的连接设置传递给连接测试和实际采样请求，二者 MUST 使用相同的路由规则。`auto` 对非回环地址沿用 reqwest 默认代理行为；`direct` 忽略系统代理并直接连接；`proxy` 关闭系统代理且只使用指定代理。指定代理失败时系统 MUST 返回失败，不得静默回退到直连或系统代理。

#### Scenario: 自动模式访问远程地址
- **WHEN** 供应商使用 `auto` 且目标地址不是回环地址
- **THEN** 连接测试和实际采样均遵循现有 reqwest 默认代理环境

#### Scenario: 回环地址强制直连
- **WHEN** 目标主机是大小写不敏感且允许末尾点的 `localhost`、`127.0.0.0/8`、`::1` 或 IPv4-mapped loopback
- **THEN** 无论连接设置为 `auto`、`direct` 还是 `proxy`，连接测试和实际采样均直接访问目标地址

#### Scenario: 直接模式忽略代理
- **WHEN** 供应商使用 `direct` 且目标地址不是回环地址，系统代理环境可用
- **THEN** 请求不经过系统代理，并将目标连接失败作为原始连接失败返回

#### Scenario: 指定代理不回退
- **WHEN** 供应商使用 `proxy` 且指定代理不可达或拒绝请求
- **THEN** 连接测试和实际采样均报告该代理路径的失败，不尝试直连或系统代理

#### Scenario: 测试与采样路由一致
- **WHEN** 同一供应商分别执行连接测试和三种 API 格式的实际采样
- **THEN** 每次请求均使用该供应商相同的连接设置和回环判定结果

### Requirement: 代理地址验证与凭据边界

系统 SHALL 只接受无需认证的代理地址。代理 URL 的 scheme SHALL 为 `http`、`https`、`socks5` 或 `socks5h`，MUST 包含主机和显式有效端口（`1..65535`，包含 `80` 与 `443`），路径只能为空或 `/`，且不得包含 query、fragment、用户名、密码或空白字符。系统 MUST NOT 将代理认证凭据写入配置、钥匙串或日志。

#### Scenario: 接受有效代理 URL
- **WHEN** 用户填写支持的代理 scheme、主机和 `1..65535` 的显式端口，且 URL 只有空路径或根路径
- **THEN** 代理设置通过校验并可用于 `proxy` 模式

#### Scenario: 拒绝无效代理 URL
- **WHEN** 代理 URL 缺少主机或显式端口、端口超出 `1..65535`、使用其他 scheme、含非根路径、query、fragment 或空白
- **THEN** 系统拒绝保存并给出可定位到代理地址的校验错误

#### Scenario: 拒绝代理认证信息
- **WHEN** 代理 URL 含用户名或密码
- **THEN** 系统拒绝保存，不发起网络请求，且任何错误、配置返回和日志均不包含该凭据

### Requirement: 连接设置表单

系统 SHALL 在供应商高级设置中提供连接模式选择和按需显示的代理 URL 输入，且提供中文与英文文案。新供应商默认折叠高级设置；已有自定义连接设置的供应商默认展开。连接设置的修改 SHALL 标记表单为 dirty，并贯通连接测试与保存状态。

#### Scenario: 选择连接模式
- **WHEN** 用户在高级设置中选择 `auto`、`direct` 或 `proxy`
- **THEN** 表单显示对应模式的说明，只有 `proxy` 模式显示并要求代理 URL，且修改状态标记为 dirty

#### Scenario: 测试并保存草稿
- **WHEN** 用户修改连接设置后执行测试并保存
- **THEN** 操作使用当前草稿的连接设置；成功后清除 dirty 并显示已验证状态，失败后保留草稿、dirty 状态和行内错误；dirty 时禁用对已保存配置的独立测试

#### Scenario: 展开状态与语言
- **WHEN** 用户打开新供应商或已有自定义连接设置的供应商表单，并切换应用语言
- **THEN** 高级设置分别按默认折叠或展开规则呈现，模式、代理 URL、校验、测试和保存状态均显示对应的中英文文案

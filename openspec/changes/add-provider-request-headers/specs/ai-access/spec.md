## ADDED Requirements

### Requirement: 供应商请求 Header 编辑

系统 SHALL 在添加和编辑供应商时提供可增删的 Header 名称/值行，同一供应商的所有模型共用这些设置。可选区域 SHALL 保持紧凑；新增行聚焦名称，名称和值有可访问标签，窄窗口保持可操作。系统 SHALL 提供中英文文案。

#### Scenario: 新增与删除
- **WHEN** 用户添加 Header 行并填写名称和值，或删除已有行
- **THEN** 草稿反映变更，测试并保存成功后才影响该供应商；不自动改变激活模型

#### Scenario: 供应商导航
- **WHEN** 用户管理供应商或添加首个供应商
- **THEN** 导航按内容高度展示轻量列表，空列表不占侧栏；添加入口位于管理标题旁，窄窗口导航位于表单上方

#### Scenario: 已保存值
- **WHEN** 用户重新打开供应商
- **THEN** 仅显示 Header 名称和已保存提示，值不回传；输入新值可替换，留空保留同名已有值，删除该行则移除

#### Scenario: 无效输入
- **WHEN** 名称非法、名称大小写重复、值含换行/控制字符、新增行缺值或尝试覆盖传输层 Header
- **THEN** 行内说明原因，禁止提交；后端也拒绝同样的无效输入，错误不包含值

### Requirement: Header 测试与运行一致

系统 MUST 在三种协议的逐模型连接测试、Coach、规划和检查请求中使用相同 Header 合并规则。自定义 Header SHALL 按不区分大小写的名称替换同名协议默认 Header，包括认证 Header。系统 MUST 保留对 `host`、`content-length`、`transfer-encoding`、`connection`、`keep-alive`、`te`、`trailer`、`upgrade`、`proxy-authorization`、`proxy-authenticate`、`content-type` 和 `accept` 的管理权。

#### Scenario: 带 Header 的测试和运行
- **WHEN** 服务端要求用户配置的网关 Header
- **THEN** 测试草稿及保存后的真实 AI 请求均发送该 Header；同名覆盖只发送一个有效值

#### Scenario: 测试失败
- **WHEN** 修改 Header 后任一模型测试失败
- **THEN** 原有配置和凭据不变，草稿保留可继续编辑

### Requirement: Header 值的凭据保护

系统 MUST 将全部自定义 Header 值存入系统凭据存储，配置文件、设置返回、LLM 上下文和日志不得包含这些值。删除供应商 SHALL 一并清理 Header 凭据。

#### Scenario: 保存并重开
- **WHEN** Header 测通并保存后重启应用
- **THEN** 配置只包含 Header 名称，请求时从凭据存储恢复值，设置界面不回显旧值

#### Scenario: 服务端回显
- **WHEN** 服务端错误中回显请求 Header 值
- **THEN** 面向用户的错误和日志隐藏这些值

#### Scenario: 移除
- **WHEN** 用户删除所有 Header 或删除供应商
- **THEN** 对应 Header 凭据被清理，后续请求不再携带

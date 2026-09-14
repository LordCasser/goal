## Why

`ai-access` 是所有 AI 行为的平台层：没有它，`add-ai-planning-core` 无法解析出可用的供应商。本变更把这条路径落地，并按 2026-09-14 的产品决定收敛范围：

1. **语音移出本变更。** 语音管线（AudioWorklet 采集、转写 token 换取）整体删除；`openspec/specs/voice-input/spec.md` 保留为规范文档，但其实现不再由本变更承载，待另行决定实现载体或废弃。
2. **OpenRouter 专用路径移除，泛化为 BYOK。** 与其为 OpenRouter 一家做品牌化接入（专用 key 存储命名、PKCE 授权、固定供应商枚举），不如泛化为「任意自定义供应商」：用户填名称、Base URL、API Key、API 格式即可接入任何兼容服务。实现形态参考 `grow` 项目的 BYOK 采样层（`crates/codegen/sampler`，见 `design.md`）。
3. **三种 API 格式**：Anthropic Messages（`/v1/messages`）、Chat Completions（`/v1/chat/completions`）、Responses（`/responses`）。一个供应商绑定一种格式，同一套内部事件抽象对上层屏蔽协议差异。
4. **并入 `add-local-llm-provider` 并将其删除。** 本地模型（Ollama / LM Studio 等）不再有专门路径：它们只是 BYOK 供应商的普通取值——Base URL 指向本机端点、Key 可为空、格式选 Chat Completions。原变更中仍然成立的三点被吸收为本变更的通用能力：模型级的**工具调用能力元数据**（agent 写操作依赖它，不支持则显式降级为对话模式）、**流式输出与超时分层**、**自带凭据不受授权闸门限制**。被丢弃的只有本地特化部分：运行时端口探测、`/v1/models` 模型发现、专用设置界面。
5. **订阅机制整体移除，规范已同步清理。** 设备指纹、授权令牌（`trial|paid|blocked`）、内置托管 AI 通道与额度判定全部废弃——本应用没有订阅、试用或托管额度，AI 的唯一来源是 BYOK。`openspec/specs/ai-access/spec.md` 已按同一决定重写（无订阅与托管额度、供应商配置、切换与解析、模型元数据与显式降级、凭据安全存储、AI 失败不伤及本地）。

## What Changes

- **BYOK 供应商配置层**：多供应商配置（名称 / Base URL / API 格式 / 模型列表及其元数据）存本机配置文件；激活供应商的选择与确定性解析；非敏感元数据与凭据严格分离
- **凭据存储**：每个供应商的 API Key 独立存系统钥匙串（不落数据库/配置/日志），保存/读取/删除，失败与「未配置」区分；本地端点允许空 Key
- **三协议采样客户端**：每种 API 格式一个适配器（请求构造、认证头、SSE 流解析），统一输出内部采样事件；协议坏响应与瞬态网络错误分类，前者不重试；流式增量输出；连接/生成超时分层
- **模型元数据**：上下文窗口、最大输出 Token、输入/输出类型（文本必选）、工具调用支持——由用户在配置时声明，agent 层据此路由与降级，不做运行时探测
- **设置页 GUI**（布局参考 zcode「模型设置」页，样式按本仓库 `design.md`）：左栏供应商列表（状态点、添加入口），右栏添加/编辑表单与模型列表，「添加模型」对话框，连接测试
- **删除**：订阅/试用/托管额度机制（设备指纹、授权令牌、托管通道）、OpenRouter PKCE 授权与专用 key 命令、语音管线与转写 token、本地运行时探测与模型发现

## Capabilities

### Modified Capabilities

- `ai-access`：规范已随本变更直接重写（2026-09-14）——移除订阅/授权机制与语音条目，供应商模型泛化为 BYOK 三格式。本变更 `skip_specs`，规范修订不经 delta 流程。

## Impact

- 后端：`credentials`（钥匙串读写，按供应商存取）、`providers`（配置存储与解析，新增）、`sampling`（三协议客户端，新增）
- 依赖新增：钥匙串访问（`keyring`）、HTTP 客户端与 SSE 解析（`reqwest` 系）——当前后端没有任何出网依赖，这是第一处，集中在一个模块内
- 数据：无新表；供应商元数据存本机配置文件（非敏感），Key 只进钥匙串
- 前端：设置页新增「模型供应商」管理区（布局见 `design.md`；界面为独立任务）
- 平台：macOS 钥匙串访问；**不需要**麦克风权限、本地回环 OAuth 端口、授权服务与设备指纹
- 依赖关系：建立在 `rebuild-baseline` 之上；被 `add-ai-planning-core` 依赖（agent 循环、上下文裁剪与回合落库属于该变更，不在这里）
- 变更账本：`openspec/changes/add-local-llm-provider/` 已删除，其意图由本变更吸收（见 Why 第 4 条）

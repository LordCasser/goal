## Why

`ai-access` 与 `voice-input` 的规范已写好，但没有任何实现。这两个能力是**所有 AI 行为的平台层**：没有它，`add-ai-planning-core` 无法解析出可用的供应商，语音入口也无法取得短期凭据。

`add-local-llm-provider` 只修改了 `ai-access` 的供应商选择与降级语义，它假设"内置供应商"这条路径已经存在——实际上没有。本变更补上这条路径。

## What Changes

- **设备指纹与授权令牌**：硬件标识派生 + 加盐哈希、`hft1` 前缀令牌的签发/缓存/校验、`trial|paid|blocked` 状态、离线容忍
- **托管 AI 通道**：向自建 worker 换取访问凭据，并在额度用尽时给出明确的可恢复路径
- **凭据存储**：OpenRouter key 存系统钥匙串（不落数据库/配置/日志），PKCE 授权码流程（本地回调 + 取消）
- **语音管线**：AudioWorklet 采集（48 kHz → 16 kHz、`linear16`、80 ms/1280 采样）、向后端换取一次性转写 token、连接与权限错误的区分处理

## Capabilities

### New Capabilities

无。`ai-access` 与 `voice-input` 的规范已存在，本变更是实现。

## Impact

- 后端：新增 `entitlement`（设备指纹、令牌、服务）、`credentials`（钥匙串读写）、`openrouter_oauth`（PKCE + 本地回调服务器）、`voice`（token 换取、错误映射）
- 数据：无新表；`config.toml` 或设置表记录非敏感的授权状态与指纹
- 前端：设置页的 AI 访问区、语音按钮与错误提示（前端为独立任务）
- 平台：需要 macOS 钥匙串访问、麦克风权限声明、本地回环端口（OAuth 回调）
- 依赖：`rebuild-baseline`（错误类型、设置存储）。被 `add-ai-planning-core` 依赖
- 与 `add-local-llm-provider` 的关系：本变更实现内置与 OpenRouter 两条路径，后者在其上增加第三条；两者的供应商解析必须落在同一处

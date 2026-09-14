## Why

基线只有两条 AI 路径：内置托管试用，以及自带 OpenRouter 凭据。两者都需要联网、都需要外部账号，且试用结束后应用的核心能力就被闸门关掉。

用户已经在本地跑模型（Ollama / LM Studio 这类运行时），把它们接进来可以一次性解决三件事：AI 能力不再有授权闸门（内置额度用完也不必依赖第三方账号）、推理不出本机、完全离线可用。

注意这里**不是"绕过付费"**——上游产品本身免费，没有订阅或额度包（见 `analysis/reports/20-website-and-goal-linking.md` §0x04）。真正的动机是隐私自持与离线可用。

## What Changes

- 供应商集合从 `hyperfocus | openrouter` 扩展为 `hyperfocus | openrouter | local`
- **统一按 OpenAI 兼容协议接入**：Ollama、LM Studio、llama.cpp server、vLLM 都提供 `/v1/chat/completions` 与 `/v1/models`，一个客户端覆盖全部
- 本机运行时探测与模型发现：给出默认端点候选，探测到可用运行时后列出模型供选择
- **工具调用能力探测**：agent 的写操作依赖 function calling。运行时不支持时，agent 退化为「只能对话、不能提议改动」，并明确告知用户，而不是静默失败或改用正则解析文本
- 显式超时与流式输出：本地模型首 token 慢，需要可配置超时与逐字输出
- **本地供应商不受授权闸门限制**：试用结束后本地模型照常可用

## Capabilities

### Modified Capabilities

- `ai-access`: 供应商集合扩展；新增本地供应商的探测、选择、能力探测与失败语义；修改试用结束后的降级行为（本地供应商仍然可用）。

## Impact

- 后端：`ai/llm` 新增 OpenAI 兼容客户端与能力探测；`resolved_client` 增加 `local` 分支；设置模型增加 `local_base_url` / `local_model` / `local_supports_tools` 与超时参数。
- 前端：设置页 AI 区新增第三个选项、端点与模型选择界面、连接测试结果反馈。
- 数据：无新表；设置项扩展（键值表）。
- 与 `voice-input` 的关系：本地供应商不提供转写服务，语音仍依赖托管授权或保持不可用（本变更不改变语音行为）。
- 依赖关系：建立在 `rebuild-baseline` 之上；与 `add-review-retrospective` 无耦合（复盘技能对供应商无感知）。

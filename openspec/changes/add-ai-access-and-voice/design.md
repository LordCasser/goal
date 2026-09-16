## Context

本变更把 `ai-access` 落地为「纯 BYOK 自带凭据」接入，范围经过四次收敛（2026-09-14）：删除语音、删除 OpenRouter 特化路径、并入并删除 `add-local-llm-provider`（本地模型是 BYOK 的普通取值，不是第三条专用路径）、删除订阅机制（设备指纹、授权令牌、试用/托管额度）。

三类输入：

1. **行为规范** → `openspec/specs/ai-access/spec.md`，已按同一批产品决定直接重写：无订阅与托管额度、供应商配置、切换与解析、模型元数据与显式降级、凭据安全存储、AI 失败不伤及本地。
2. **实现参考** → `grow` 项目的 BYOK 采样层（只参考形态，不复制代码）：
   - `crates/codegen/sampling-types/src/types.rs`：`ApiBackend` 三值枚举（ChatCompletions 默认 / Responses / Messages）
   - `crates/codegen/sampler/src/config.rs`：`SamplerConfig`（api_key、base_url、model、api_backend、`auth_scheme: Bearer | XApiKey`、extra_headers、context_window、output_limit）
   - `crates/codegen/sampler/src/stream/{messages,chat_completions,responses}.rs`：每个后端独立的流式 transform，输出统一的 `SamplingEvent`；协议坏响应走 `protocol_failure`（请求失败，不进瞬态重试）
   - `crates/codegen/tools/src/types/api_key_provider.rs`：`ApiKeyProvider` trait（同步缓存读 + 按请求异步解析）
   - `crates/codegen/shell/docs/architecture/byok-sampling.md`：采样偏好的所有权——内部请求不设 temperature/top_p/max_output/reasoning_effort；解析顺序为「请求显式值 > 采样配置 > 上游默认」；Messages 适配器在协议必需时补 `max_tokens`
3. **界面参考** → zcode 的「模型设置」页截图（仅取布局；样式、组件、色彩一律按本仓库根目录 `design.md` 的规范执行）。

## Goals / Non-Goals

**Goals:**

- 一个供应商配置模型覆盖所有接入方式：云厂商、网关、本地运行时
- 三协议对上层（`add-ai-planning-core` 的 agent 循环）呈现同一套采样事件
- 凭据零落盘（钥匙串之外搜不到明文）
- 能力不满足时显式降级，不静默失败

**Non-Goals:**

- 不做本地运行时的端口探测、`/v1/models` 模型发现（原 `add-local-llm-provider` 的特化自动化，随合并放弃；用户手工填写）
- 不在应用内管理模型下载、量化或运行时生命周期
- 不做供应商自动路由（按任务在供应商之间切换）
- 不做语音；不提供订阅、试用、托管额度、设备授权或任何购买入口
- agent 回合执行、上下文裁剪、会话落库属于 `add-ai-planning-core`，本变更只交付「给定供应商配置发一次采样请求」的能力

## Decisions

### D1：供应商与模型的两层配置模型

```text
ProviderConfig {
  id: String (uuid，稳定不变),
  name: String,
  base_url: String,            // 含 scheme；路径前缀，如 https://api.example.com/v1
  api_format: anthropic_messages | openai_chat_completions | openai_responses,
  extra_headers: [],           // 可选；非敏感，存配置文件
  models: [ModelConfig],
  created_at, archived: bool
}
ModelConfig {
  model_id: String,            // 发给 API 的模型标识
  context_window: u64,         // 信息性元数据：compaction 决策用，采样器不强制（沿用 grow 语义）
  max_output_tokens: u64,
  input_types: [text|image|video|pdf],   // text 锁定必选
  output_types: [text],                  // text 锁定必选
  supports_tools: bool,        // 默认 true；agent 写操作依赖它
}
```

- 照搬 grow 的职责切分：采样请求 = base_url + api_format + model + api_key，其余都是元数据
- `supports_tools` 吸收自 `add-local-llm-provider` 的能力探测，改为**用户声明**而非运行时探测：探测要花一次真实计费请求且有假阴性，而 BYOK 用户对自己的供应商有判断；agent 层拿到 `false` 时显式降级为对话模式并在界面可见（沿用其 D2 原则：不静默降级）
- 添加模型对话框因此比参考截图多一个「工具调用」复选项（默认勾选）——这是对参考截图的唯一字段级偏差

### D2：api_format 决定端点、认证头与流解析

| api_format | 端点（相对 base_url） | 认证 | 备注 |
| --- | --- | --- | --- |
| `anthropic_messages` | `/messages` | `x-api-key` + 固定 `anthropic-version` 头 | `max_tokens` 为协议必需：请求与模型配置都没有时由适配器补默认值（byok-sampling.md） |
| `openai_chat_completions` | `/chat/completions` | `Bearer` | 本地运行时（Ollama / LM Studio / llama.cpp / vLLM）的主格式 |
| `openai_responses` | `/responses` | `Bearer` | 结构化输出可原生携带 schema；Messages 则不能与工具调用并存 |

三者的 SSE chunk 形状不同，各自实现独立的流式 transform，输出统一的内部采样事件（文本增量、工具调用、用量、终止原因、失败）。工具调用在流结束后统一解析，不消费半截 JSON（吸收自原变更 D3）。

### D3：错误分类决定重试与文案

| 类别 | 触发 | 处理 |
| --- | --- | --- |
| `auth_failed` | HTTP 401/403 | 不重试；提示检查 Key |
| `invalid_request` | HTTP 4xx（非认证） | 不重试；展示服务端 message（截断） |
| `provider_unreachable` / `timeout` | 连接失败 / 超时 | 瞬态，可重试 |
| `rate_limited` | HTTP 429 | 按重试-退避策略 |
| `protocol_error` | 响应/SSE 不符合协议 | **不重试**（grow `protocol_failure` 语义），请求即失败 |
| `no_active_provider` / `credentials_missing` | 配置缺失 | 不发请求；引导到设置 |

全部映射到 `AppError { code, message }`；错误消息绝不包含 Key 或完整请求头。

### D4：存储切分——元数据进配置文件，Key 进钥匙串

- `providers.json`（应用数据目录，与 `planner.db` 同级）：供应商/模型元数据，非敏感；损坏时按空配置可写回，读写受互斥保护（`local-persistence` 规范的场景）
- 钥匙串：service 固定为应用标识，account 为 `provider:{id}`；读取失败与「未配置」区分；本地端点允许无 Key（此时不写钥匙串条目）
- 激活供应商 id 存配置文件；解析是确定性的：使用激活项，无激活项时 AI 不可用并引导到设置（规范要求不得自动挑选替代供应商）。BYOK 是唯一路径，不存在授权闸门

### D5：流式与超时分层

- 采样客户端只走 SSE 流式：文本增量实时上报（agent 层再决定推送节奏），工具调用流结束后统一交付
- 超时分两层：连接/空闲超时（短，默认 30s）与生成超时（长，默认 300s，可配置）——吸收自原变更 D3（本地大模型首 token 慢）

### D6：设置页 GUI——布局取自 zcode，样式按 `design.md`

布局（参考截图，三层结构不变）：

```text
模型设置
说明行：当前接入与费用来源先说明（design.md §9.3）          [刷新/连接测试]
┌───────────────┬──────────────────────────────────────┐
│ 自定义供应商    │  选中供应商详情 / 「添加模型供应商」表单   │
│   ● provider A │  名称 / Base URL / API Key /           │
│   ○ provider B │  API 格式（三选下拉，显示端点路径）        │
│   [+ 添加供应商]│  模型列表（空态说明 + 「添加模型」按钮）   │
│                │  底部：行内校验提示 + 主按钮               │
└───────────────┴──────────────────────────────────────┘
添加模型对话框（约 420px）：模型 ID / 上下文窗口 / 最大输出 Token /
  输入类型（文本🔒 图片 视频 PDF）/ 输出类型（文本🔒）/
  工具调用 ✓ / 取消·保存
```

样式映射（全部来自根目录 `design.md`，不引入截图的深色主题）：

- 表面：页面底 `ink-50`，左右两栏为白色面板，1px `ink-100`/`ink-200` 细边界，默认方角（控件 2–4px 圆角），无阴影
- 字阶：页面标题 18/24 600，分区标题 14/20 600，表单正文 14/22，辅助与路径说明 13/20 `ink-600`
- 间距：4px 主节奏（表单行 12–16px，区块 24px）；主按钮深底白字高 36px，次按钮白底描边
- 状态点：已连通 `accent-500`、未配置/失败 `ink-400` + 文字说明（不只靠颜色）
- API Key 输入框为密码型；保存后**不回显明文、不显示尾片段**，只显示「已配置」状态与移除入口（design.md §9.3：密钥不出现在普通状态）
- 保存前校验：名称非空、Base URL 为 http(s)（本地 http 合法）、至少一个模型——不满足时主按钮禁用并在底部行内说明（与参考截图的底部提示一致）
- 删除供应商用删除确认弹窗（对象名称 + 影响说明：钥匙串条目一并清除）
- 键盘、焦点、Escape 顺序按 design.md §10；列表/表单数据经事件失效刷新（沿用既有 IPC 事件边界）

### D7：连接测试是一条最小真实请求

`test_provider_connection` 向所选供应商发一次极小采样请求（固定一句、max_tokens 收紧），返回延迟或分类错误。它验证 Base URL / Key / 格式三者组合真实可用，替代原变更的「探测 + 二次确认」两步法。

## Risks / Trade-offs

| 风险 | 影响 | 处理 |
| --- | --- | --- |
| 三协议 SSE 语义差异（工具调用增量形状不同） | 解析错漏难以肉眼发现 | 每协议配本地假服务器的**事件序列回放测试**；工具调用流结束后统一解析 |
| Responses 格式供应商面窄 | 用户选了不支持的服务 | 不禁用；格式下拉中显示端点路径，连接测试给出可读失败 |
| 手工声明 `supports_tools` 可与实际不符 | agent 写操作失败 | 失败按 `protocol_error`/`invalid_request` 显式暴露；用户可回改开关 |
| 首次引入出网依赖（reqwest/keyring） | 依赖面扩大 | 集中在 `sampling`/`credentials` 两个模块；除用户配置的供应商外应用保持零出网 |
| 钥匙串不可用 | 凭据无法保存 | 明确失败并提示，**不降级明文存储**（`local-persistence` 规范红线） |

## Migration Plan

无存量数据。首次运行时 `providers.json` 不存在即为空配置，应用以未配置状态引导到设置。原 `add-local-llm-provider` 变更目录删除，无实现代码需要迁移。

## 该变更目录内的文档效力

`proposal.md`（范围与决策记录）、本文（决策）、`tasks.md`（交付清单）。`openspec/specs/ai-access/spec.md` 已随本变更直接重写，实现与规范以同一份为准。

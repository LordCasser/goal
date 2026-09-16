# 发布前审查发现（2026-09-15）

本记录只保留可定位的发布与安全发现；审查未读取系统钥匙串中的密钥、未运行 Cargo、未操作 UI，也未输出任何凭据值。

## macOS 凭据授权与签名身份

- `src-tauri/src/providers/credentials.rs:19-27,40-67,72-88` 使用固定的 Keychain service `dev.lordcasser.planner` 和 `provider:{provider_id}` account。provider 配置更新不会随机生成新的 provider ID，`src-tauri/src/ai/llm/resolved.rs:29-40` 每次 AI 解析都会重新读取同一条凭据。因此重复授权的主要变量是应用代码身份，而不是 provider account 或 service 名称。
- `src-tauri/tauri.conf.json` 的 bundle identifier 已固定；`src-tauri/tauri.macos.conf.json` 没有 Developer ID、entitlements 或公证配置。当前发布 workflow 已导入固定的自签名证书并对 macOS app 签名，但这仍不是 Developer ID/公证发布；从旧 ad-hoc 身份迁移到该稳定身份时，已有钥匙串项目仍可能需要一次用户授权。
- `scripts/macos-signing.mjs` 用固定证书指纹和 designated requirement（精确 identifier + certificate root）签名，并将临时签名钥匙串加入当前用户的 search list。`prepare` 只通过 `GITHUB_ENV` 输出 `GOAL_SIGNING_STATE_FILE`，不再输出 `APPLE_SIGNING_IDENTITY`；它不安装系统 trust anchor，也不改变用户的 trust 设置。`cleanup` 会恢复原 search list 并删除临时钥匙串。连续性 smoke test 已用不同 CDHash 验证同一签名身份可读、错误 identifier 和 ad hoc 签名被拒，且没有系统弹窗。

参考：[Apple Code Signing Tasks](https://developer.apple.com/library/archive/documentation/Security/Conceptual/CodeSigningGuide/Procedures/Procedures.html)、[Apple Code Signing Requirement Language](https://developer.apple.com/library/archive/documentation/Security/Conceptual/CodeSigningGuide/RequirementLang/RequirementLang.html)、[Apple TN2206](https://developer.apple.com/library/archive/technotes/tn2206/)、[Tauri macOS signing](https://tauri.app/distribute/sign/macos/)。

## 发布矩阵

- 当前 `.github/workflows/release.yml` 已声明六个目标：macOS x86_64/arm64、Windows x86_64/arm64、Linux x86_64/arm64，并在版本 tag 的完整构建通过后创建 GitHub Release；手动运行只保留 Actions artifacts。矩阵覆盖和产物校验不等于各目标的原生安装与 GUI 验收。
- macOS 产物使用自签名证书、未公证；Windows 产物未签名。发布说明必须保留这两个限制，不能把构建成功描述为 Gatekeeper 或 Windows 信任已解决。

## AI 写入边界

- 旧实现的 `src-tauri/src/ai/tools.rs:637-660` 曾直接调用 `prioritization::store_for_cycle`，绕过 `agent_actions`；模型调用后即可改变周期优先级文档，与 `src-tauri/src/ai/agent/prompt.rs:24-27` 的 GUI approval 约束及 `openspec/changes/add-ai-planning-core/proposal.md` 的“全部写入走预览层”冲突。
- 该边界现已收敛到 `ai::actions::PrioritizationAction`：工具只校验并创建 pending action，拒绝不写入，只有通用 GUI approval 的 `actions::apply` 才合并并保存周期文档；定向测试覆盖直接调用、拒绝、确认和空理由错误。

## 凭据落盘与请求路径

未发现 API key 明文落盘路径：credentials backend 走系统 keyring，sampling 请求和错误日志会注册并脱敏 key（`src-tauri/src/providers/credentials.rs:40-67`、`src-tauri/src/sampling/client.rs:110-125,149-153`）。`ProviderConfig.extra_headers` 是 providers.json 中的非敏感元数据，当前 sampling request 不会将其自动带到 wire；不要把它当作密钥备份通道。

## active changes 的未勾任务与发布限制

截至 2026-09-15，8 个 active change 的 `tasks.md` 共 319 项，286 项已勾选，33 项仍未勾选。以下按未勾任务的实际性质归类；OpenSpec 格式校验通过只说明文档格式正确，不能替代这些任务要求的原生或用户流程证据。

| change | 状态 | 未勾任务 | 对 v0.1.0 的含义 |
| --- | --- | --- | --- |
| `adapt-desktop-platform-shell` | 12/25 | 2.4–2.6、3.2–3.4、3.6–3.7、4.1–4.5 | Windows 原生外壳/消息与降级、跨平台布局及 macOS/Windows/Linux 安装、DPI、窗口和数据链路验收仍缺。六目标构建矩阵不能替代这些证据；本次发布必须保留原生 GUI 验收有限的说明，不能把这些条目计为已完成。 |
| `add-ai-access-and-voice` | 34/37 | 6.2、6.3、6.5 | 真实供应商、离线本地端点和删除供应商后的钥匙串清理只剩手工验证。自动测试已覆盖本地假服务器；只有在 v0.1.0 对外承诺这些运行时路径时才是发布阻断。 |
| `add-ai-planning-core` | 54/56 | 10.2、10.4 | 真实供应商上的澄清写回、模糊目标的就地审查提示尚缺手工确认；FakeProvider 测试不能证明真实端到端体验。 |
| `add-calendar-time-view` | 26/29 | 6.2–6.4 | 日历拖动/合并、重叠排期和视图切换的用户验收未完成；实现与自动测试已勾选，属于功能发布前的 GUI 验收缺口。 |
| `add-onboarding-and-lifecycle` | 40/46 | 8.2–8.7 | 全新目录引导、跳过与重启、退出调查、反馈重试和一分钟专注通知尚缺手工证据；属于 v0.1 生命周期验收缺口。 |
| `add-reminders-notifications` | 26/30 | 6.2–6.5 | 到期通知、退出后错过摘要、免打扰和系统权限拒绝场景尚缺手工证据；属于真实系统通知验收缺口。 |
| `add-review-retrospective` | 31/33 | 8.2–8.3 | 长周期复盘去向、后续上下文及历史快照不变尚缺手工证据；属于复盘功能验收缺口。 |
| `rebuild-baseline` | 63/63 | 无 | 没有未勾任务。 |

“voice”不是当前 v0.1.0 的遗漏实现：`add-ai-access-and-voice/proposal.md` 明确将语音管线移出本 change，保留的 `voice-input` 规范待另行决定实现载体或废弃。因此它应归入后续能力，不能用它解释当前 33 项未勾任务，也不应在本版本发布说明中暗示已交付语音输入。

v0.1.0 按用户要求提供三平台原生构建，并在公开文档中明确原生 GUI 和部分系统服务验收仍有限。这些未勾任务继续保留，不用构建成功代替人工验收。自动化门禁覆盖编译、测试、产物架构、签名和完整安装包集合；后续对应环境中的人工验收仍须补充。

## 最终发布状态记录（2026-09-15）

- 公开仓库为 [LordCasser/goal](https://github.com/LordCasser/goal)。发布使用干净的公开 checkout `.artifacts/release-repo`；原始 root 研究目录及其 Git 历史不推送到公开仓库。
- 应用公开名称已统一为 Goal：全平台 product/window/html 标题和可执行文件均使用 Goal；稳定的 bundle identifier、Keychain service/account 约定和数据目录保持不变。
- CI run `34983953058` 暴露提醒调度器的真实边界：空队列会睡眠最多 60 秒，而提醒创建和其他 mutation 提交后只发事件、不唤醒 scheduler。修复已接到 `commands/reminders.rs` 的提醒事件边界和 `commands/mod.rs` 的 mutation 事件边界；调度器线程测试现在先确认首次空队列 pass，再显式 wake，并保留 10 秒投递上限。
- 签名清理异常的两条 Vitest 路径已通过；本机 `prepare → continuity → cleanup` 正常通过，未修改系统 trust 设置。公开说明仍保留自签名、未公证和 Windows 未签名限制。
- 前端全量结果为 50 个文件、332 个测试通过，前端 build 通过。Rust/原生 runner 和最终发布流程仍以 CI 为准。
- 当前 tag `v0.1.0` 的 head 为 `f14b8ea`，最终 CI run `34985952687` 仍在构建；截至本记录不能写成已发布。此前列出的原生 GUI、安装、DPI、真实通知和真实供应商等未验收限制继续有效。

# hyperfocus 0.15.0 技术与设计报告

> 对象：`hyperfocus_0.15.0_mac_universal.dmg`（Developer ID: DMITRII TCEPELEV / A3HNYL86C9，已公证）
> 方法：静态反汇编 + 符号还原 + 运行时数据目录 + 前端资源解压 + UI 驱动截图
> 详细的证据与命令见附录：`hyperfocus-deconstruction.md`（需求与架构）、`backend-internals.md`（后端细节）、`design-system.md`（设计 token 全表）

---

## 0x00 一页速览

| 维度 | 事实 |
| --- | --- |
| 产品定位 | ADHD 友好的四级时间规划器：Long-term(1/3/6 月) → Week → Day → Session(专注块)，带 AI 陪练 |
| 商业模型 | 规划器**永久免费**（无订阅、无付费档）；AI 为「内置 14 天 + 之后自带 OpenRouter」两态，作者不抽成。据官网 FAQ 更正，见 `20-website-and-goal-linking.md` §0x04 |
| 外壳 | **Tauri 2.10.2**（Rust + WKWebView），非 Electron |
| 前端 | **SolidJS** + Vite，Tailwind v3 + Lightning CSS，TipTap/ProseMirror 富文本 |
| 数据库 | **SQLite**（`sqlx`），本地单文件，无账号体系 |
| AI | 自建 worker（`workers.hyperfocus.in`）+ OpenRouter + Gemini 系模型 |
| 语音 | **Deepgram** 实时转写，48 kHz 采集 → 16 kHz linear16 送云端 |
| 音频反馈 | `rodio` |
| 可观测 | **PostHog**（含退出调查 = PostHog Survey） |
| 更新 | `tauri-plugin-updater` + S3（minisign 公钥校验） |
| 二进制 | 66,130,800 字节（x86_64 + arm64 通用），Resource 目录里只有 `icon.icns` |
| 前端资源 | 解压后约 1.2 MB（含 322 KB 自托管字体），以 brotli 内嵌在二进制中 |
| 数据库 | 57 条迁移，10 张表，3 条强制不变量触发器 |
| IPC | 约 60+ 个命令（符号表里 `commands::*` 相关符号 124 个） |

---

## 0x01 技术栈选型

按层给出：选了什么、还有什么可选、为什么这么选、代价是什么。代价一栏是这份报告里最有价值的部分——它决定了**同一个选择在你的项目里是否还成立**。

### 1.1 外壳：Tauri 2

| 候选 | 相对代价 |
| --- | --- |
| Electron | 产物 150–250 MB 起步，每个应用带一份 Chromium 与 Node；内存占用高 |
| 原生 SwiftUI | 体积最小、体验最原生，但 UI 迭代慢，富文本编辑器要自己写 |
| Flutter | 渲染自绘，桌面端仍带一份引擎；社区桌面生态弱于 Web |
| **Tauri 2** | 复用系统 WKWebView，产物小、能直接进 Keychain / 通知 / 菜单 / 更新 |

**代价**：UI 永远受系统 WebView 版本约束（macOS 上意味着 Safari 的坑都要踩）；前端资源需要自己处理内嵌与自定义协议；Rust 与 JS 之间有序列化边界，跨边界传大对象要谨慎。

从产物看这个选择是成功的：63 MB 里绝大部分是 Rust 标准库 + 静态链接的依赖，而同等功能的 Electron 应用通常 200 MB 起。

### 1.2 前端：SolidJS + Vite

选 SolidJS 而不是 React，关键理由是**这个界面状态多而更新碎**：一屏里同时有多个周期列、每列若干任务、每任务若干子任务，还有 agent 侧栏的流式消息和待确认改动计数。Solid 的细粒度响应式没有虚拟 DOM，状态变更直接落到具体 DOM 节点。

**代价**：生态比 React 小一个量级。它引的两个"重"依赖（TipTap、tippy.js）恰好都是有 React 版本的库，说明 Solid 生态里缺现成实现时只能自己适配。`vendor-solid-*.js` 只有 15 KB，这也说明它用到的框架能力并不深——**换句话说这个应用选 Solid 的收益主要在渲染效率，而不是生态**。

对我们自己的重建（React 19）而言，这一条是可替换的：状态复杂度是否真的需要细粒度响应式，取决于实现方式。

### 1.3 富文本：TipTap（ProseMirror 封装）

两个 chunk：`editor-prosemirror`（216 KB，内核）+ `editor-tiptap`（108 KB，封装层）。说明任务/目标带富文本描述。

**代价**：这两个 chunk 加起来 324 KB，占前端 JS 的近 40%。对"写目标"这种场景，ProseMirror 是重武器。

### 1.4 数据库：SQLite + sqlx

本地单用户、无同步、无账号。这种形态下 SQLite 几乎是唯一合理选择。用 `sqlx` 而非 `rusqlite` 的差别主要在于异步与编译期查询校验。

**值得注意的做法**：他们把**业务规则写进 schema**（触发器 + CHECK + 部分唯一索引），而不是只写在 Rust 里。见 §2.4。

### 1.5 AI：自建 worker + OpenRouter 双路径

```
用户 ──► 应用 ──┬──► workers.hyperfocus.in   /v1/llm/generate-turn
                │                             /v1/llm/generate-json
                │                             /v1/transcription/deepgram-token
                └──► OpenRouter（用户自带 key，PKCE OAuth）
```

选双路径的理由很直接：试用期不能让用户先注册第三方账号（流失），长期又不能让开发者承担全部推理成本。自带 key 是这类产品的常见解法。

**代价**：两条路径意味着两套错误语义、两套模型能力假设，都必须在上层 agent 里归一化。他们用 `LLMProvider` trait + `resolved_client` 处理，但也带来了 §2.7 里提到的"应用层完全没有重试/退避/超时"这个问题。

**模型**：`google/gemini-3.7-flash`、`google/gemini-3.1-flash-lite`、`google/gemini-2.5-flash`。全是 flash 档，说明交互式对话的延迟优先级高于推理深度。

### 1.6 语音：Deepgram

内嵌一个 `/audio/deepgram-pcm-worklet.js`，48 kHz 采集 → 16 kHz linear16 → 80 ms 一帧。**关键设计是凭据不下发客户端**：应用向后端要一次性 token（`/v1/transcription/deepgram-token`），拿到 `{access_token, expires_in}` 再去连 Deepgram。

**代价**：语音能力与托管授权强绑定，试用结束后语音直接不可用（文案明说）。

### 1.7 授权：自签 JWT + 设备指纹

无账号体系下要收费，就得有别的身份锚点：

- 设备指纹：`ioreg -c IOPlatformExpertDevice` 取硬件标识，加盐 `hyperfocus-device-fingerprint-v1` 后哈希
- 令牌：`hft1` 前缀 + JWT 形状，claims 含 `device_fingerprint` / `issued_at` / `expires_at` / `status` / `trial_expires_at`，`status ∈ trial|paid|blocked`
- 校验：链接了 ring 的 `ECDSA_P256_SHA256_*`，算法大概率 ES256（二进制里没有 `ES256` 字面量，无法从静态分析坐实）

**代价**：换机器要重新授权；指纹存在 `config.toml` 明文里；离线时只能靠本地令牌的有效期做判断。

### 1.8 可观测：PostHog

`posthog-rs 0.3.7`，端点 `https://us.i.posthog.com/i/v0/e/`。**退出调查直接用了 PostHog Survey**（8 个 code），而不是自建表单——把"用户为什么走"这件事外包给了现成的调研基础设施。

### 1.9 选型上的一致偏好

把上面串起来看，选型有三条一以贯之的偏好：

1. **产物要小、启动要快**：Tauri 而非 Electron、Solid 而非 React、自托管字体而非 CDN。
2. **能用系统能力就不自己写**：Keychain、通知、菜单、更新器、WebView 全部走系统或成熟插件。
3. **收费点放在服务端**：AI 与语音都经过自己的 worker，客户端拿不到任何第三方长期密钥。

---

## 0x02 技术细节

### 2.1 产物形态与资源内嵌

`.app` 里只有三个东西：`MacOS/hyperfocus`（66 MB 通用二进制）、`Resources/icon.icns`、`Info.plist`。**前端不在磁盘上**。

Tauri 的 codegen 把前端资源以 brotli 压缩块内嵌进 `__TEXT,__const`，布局规律是：

```
[资源路径字符串][压缩数据][下一个资源路径字符串][压缩数据]...
```

按这条规律可以完整还原前端。还原结果：14 个 JS/CSS + 3 个 woff2 字体 + 1 个 HTML + 1 个 audio worklet。

| 文件 | 解压后 |
| --- | --- |
| `index-BZ_MKiTS.js`（主 bundle） | 205,630 B |
| `editor-prosemirror-*.js` | 215,874 B |
| `DismissibleHint-*.js` | 121,770 B |
| `editor-tiptap-*.js` | 107,519 B |
| `AgentConversation-*.js` | 101,884 B |
| `index-*.css` | 64,576 B |
| `vendor-ui-*.js` | 35,097 B |
| `vendor-solid-*.js` | 15,227 B |
| 其余（图标、小 chunk、worklet） | ~30 KB |
| 字体（Plex Sans var + Mono ×2） | 322,692 B |

**代价**：前端无法热更，每次改动都要重新打包发布；也没法用"检查元素"看源码（需要额外开 Web Inspector）。收益是单文件分发、无法被简单篡改。

### 2.2 进程与运行时模型

单进程。Rust 主线程持有窗口与 WKWebView，前端通过 `tauri://localhost` 自定义协议从 Rust 侧取资源（不是文件系统）。业务逻辑全部在 Rust，前端只做渲染与交互。

数据流是**单向真相源**：

```
前端 ──invoke──► commands::* ──► ai / service / repository ──► SQLite
  ▲                                                                │
  └──────────── tauri emit（events::{tasks,cycles,agent}） ◄────────┘
```

事件只出不进，前端订阅后刷新。DB 是唯一真相源。

### 2.3 IPC 契约

符号表里 `hyperfocus::commands::*` 共 124 个符号，前端 bundle 里能提取出约 60 个命令字符串。分组：

| 组 | 代表命令 |
| --- | --- |
| 周期 | `create_planning_cycle` `delete_planning_cycle` `get_planning_cycle_deletion_preview` `start_cycle` `finish_cycle` `copy_uncompleted_tasks_from_previous_cycle` `reorder_sessions` |
| 任务 | `upsert_task` `patch_task` `move_task` `set_task_parent_link` `set_task_root_color` `set_task_clarity_data` |
| 预览/撤销 | `get_preview_summary` `keep_task_preview` `undo_task_preview` `keep_all_preview_changes` `undo_all_preview_changes` |
| Agent | `start_agent_conversation` `send_agent_message` `start_goal_setting` `start_planning` `start_prioritization` |
| AI 设置 | `set_ai_provider` `save_openrouter_api_key` `connect_openrouter` `get_deepgram_transcription_token` |
| 计划问题 | `get_planning_issue_report` `dismiss_planning_issue` |
| 引导 / 提示 | `reconcile_getting_started_guide` `skip_getting_started_guide` `dismiss_hint` |
| 退出 / 反馈 | `acknowledge_exit_poll_shown` `submit_exit_poll` `send_feedback` `get_talk_to_founder_eligibility` |

命名一律是**动词短语 + 名词**，且**意图明确的复合操作**（`copy_uncompleted_tasks_from_previous_cycle`）而不是通用的 `update`。

事件名：`goal_breakdown_update_status`、`task_added`、`task_removed`、`cycle_ended`。

### 2.4 数据模型与数据库级不变量

十张表：`cycles` `tasks` `repeats` `task_preview_originals` `agent_conversations` `agent_messages` `planning_issue_dismissals` `scores`（已删）`checkins`（已删）+ `_sqlx_migrations`。

`cycles` 是自引用的时间容器：

```sql
type CHECK (type IN ('session','day','week','month')),   -- 'month' 即产品语义的 Long-term
parent_id REFERENCES cycles_table(id) ON DELETE CASCADE,
started, finished, started_at, finished_at,
focused_time,                    -- 累计专注时长（迁移 6 改为毫秒）
duration, prioritization_breakdown,
starts_on, ends_on, calendar_key,
CHECK (NOT (started = 0 AND finished = 1))
```

**把规则写进 schema 是这个项目最好的工程判断**。三条触发器都是产品语义的硬约束：

```sql
-- 着色只能挂在 Long-term 周期上
CREATE TRIGGER reject_task_root_color_key_non_long_term_insert ...
  WHEN NEW.root_color_key IS NOT NULL
   AND NOT EXISTS (SELECT 1 FROM cycles_table WHERE id = NEW.cycle_id AND type = 'month')
  BEGIN SELECT RAISE(ABORT, 'root_color_key requires a Long-term cycle'); END;

-- 已着色的周期不能被降级成非 Long-term
CREATE TRIGGER reject_cycle_type_change_with_root_colors ...

-- 同一天/同一周只可能有一条周期
CREATE UNIQUE INDEX idx_unique_cycle_calendar_key
  ON cycles_table(calendar_key) WHERE calendar_key IS NOT NULL;
```

这么做的好处是**任何写入路径都绕不过去**——包括后来的 agent 工具。代价是错误只能通过 `RAISE(ABORT, ...)` 的文本向上传递，应用层需要做一层翻译，否则用户看到的是英文原句。

`tasks_table` 里有三个字段专门服务 agent：

- `agent_proposal TEXT CHECK (agent_proposal IN ('upsert','delete'))` —— 待确认的改动
- `copied_from_task_id` —— 跨周期复制的血缘
- `needs_refinement` / `needs_breakdown` —— 清晰度标记（三态：`NULL` 表示从未评估）

以及一条专门给 agent 用的索引：`idx_tasks_cycle_agent_proposal_visibility ON (cycle_id, agent_proposal, position)`——说明"按周期取可见任务（含待确认）"是最高频的查询。

### 2.5 迁移演进读出的产品史

57 条迁移。最值得看的是 `clarity`（清晰度）这条线：

```
12  clarity breakdown migration
13  goal evaluation schema
14  reset clarity breakdown for granular issues
15  replace execution clarity with structural type
17  add goal breakdown
21  drop clarity breakdown            ← 整套删掉
26  add prioritization breakdown
49  add clarity flags to tasks        ← 换成两个布尔值回来
51  drop legacy clarity columns
52  clear stale goal breakdown state
```

读法：**"如何让用户把目标说清楚"被反复重设计了至少五轮**，从独立表 → 结构化类型 → 整体删除 → 最后收敛成两个布尔 flag + 一份 JSON。这条曲线说明原始需求不是"做一个澄清表单"，而是"让人在不被表单烦到的情况下把话说清楚"，最后选了最轻的实现。

另外两条：`40 rename periods to cycles`（概念改名反映了"周期"这个抽象比"时间段"更准）；`45 allow null cycle duration`（为 Do Later 这种没有终点的容器开路）。

### 2.6 Agent 管线

**回合制**。每个规划周期一条 conversation，每次交互是一个 turn：

```
用户消息 → 组装系统提示（按 active_skill 选） + 上下文（周期 + 任务快照）
        → LLM（带工具定义）
        → 模型返回工具调用 → 执行工具（写库，但走预览层）
        → 结果回灌 → 可能继续循环
        → 消息按序落库（agent_messages，5 种 message_type，序号唯一）
```

**五个技能块**，由工具调用结果推导并持久化到 `agent_conversations.active_skill`：

| 技能 | 触发 | 提示词形态 |
| --- | --- | --- |
| `goal_setting` | `start_goal_setting` | UNDERSTAND → COLLECT → REFINE_TITLE → BREAK_DOWN → FINISH |
| `long_term_planning` | `start_planning`（month 周期） | 长周期规划流程 |
| `short_term_planning` | `start_planning`（week/day 周期） | 短周期规划流程 |
| `prioritization` | `start_prioritization` | 五桶分类流程 |
| `none` | 初始 | 要求模型先选 `start_*` 工具 |

session 周期显式拒绝：`Agent mutations are not supported for session cycles.`

**`goal_setting` 提示词的核心逻辑**（近原文还原）：

- 一次只问一个靶向问题，优先澄清 **context**，因为"it drives everything else"
- 用 `missing_fields` 做提问引导，但允许跳过不相关字段
- 标题精炼按四种情形分支：`OriginalOnly` / `OutputOnly` / `OutcomeOnly` / `OutputToOutcome`，后者要求把被动产出改成动作（`"signed agreement"` → `"Sign agreement"`）
- 明确禁止编造：*"Use only user answers and tool results. Do not invent metrics, dates, stakeholders, products, or scope."*
- 结果不由用户控制时（`controlled_by_user=false`），要求把目标改写成用户可控的表述

**工具集**（10 个）：`start_planning` / `start_prioritization` / `start_goal_setting` / `get_cycle_context` / `get_task_details` / `create_goal` / `delete_goal` / `update_goal` / `update_goal_breakdown` / `update_prioritization_breakdown` / `move_goal`。

注意每个工具都带 `rationale` 参数（"Brief explanation of why this goal should be created"）——**让模型解释意图，界面展示给用户**，这是提议机制的一部分。

**GoalBreakdown 结构**（4 部分）：

```
context   { level: low|high, clarification }
output    { value }
outcome   { value, requires_verification, verification_method, controlled_by_user, expected_date }
scope     { work_size(6 档), decomposition[], fully_decomposed }
```

由 `calculate_clarity_flags` / `calculate_missing_fields` 投影成两个布尔 flag。**"目标清不清晰"是一个计算量，不是模型的自由判断**——这是整套设计里最值得借鉴的一点。

### 2.7 LLM 抽象

```rust
trait LLMProvider {
    async fn generate_agent_once(...);   // 带工具调用的对话
    async fn generate_json_once(...);    // 结构化输出
    // generate_agent / generate_json 是默认实现，内部包一层
}
```

实现：`OpenRouterClient`、`WorkerLlmClient`、`ResolvedLlmClient`、`ResolvedAgentLlmClient`。

**一个显著缺口**：应用层没有任何重试、退避或超时常量，只有 reqwest 的默认值。对交互式应用来说，一次网络抖动就会让整个回合以底层错误结束。

### 2.8 授权与凭据

| 项 | 做法 |
| --- | --- |
| 设备指纹 | `ioreg -c IOPlatformExpertDevice` + 固定盐 + 哈希，存 `config.toml` |
| 令牌 | `hft1` 前缀 + JWT 形状，ES256（推断） |
| 状态 | `trial` / `paid` / `blocked` |
| 令牌生命周期 | `refresh_or_start`，`fresh_hosted_ai_token` 在需要时换取 |
| OpenRouter key | macOS Keychain（service `com.hyperfocus.desktop.openrouter`，account `api-key`） |
| OpenRouter 连接 | PKCE 授权码流程，回调由内置本地 HTTP server 处理（有独立回调页 HTML + 取消支持） |

### 2.9 音频与语音

| 环节 | 参数 |
| --- | --- |
| 采集 | Web Audio AudioWorklet，48 kHz |
| 下行 | 重采样到 **16 kHz** |
| 编码 | `linear16` 小端 |
| 帧长 | 80 ms / **1280 采样** |
| 传输 | 零拷贝 buffer 转移 |
| 转写 | Deepgram，token 由后端下发 |
| 本地反馈音 | `rodio`（`init_audio_system` / `play_audio_feedback` / `decode_audio`） |

### 2.10 可观测性

`AnalyticsService` + `AwaitableAnalyticsCapture`（可等待的上报，说明关键事件要保证送达）。事件覆盖引导、退出调查、邀请作者、授权状态。属性里有 `surface`、`desktop_app`、`uuid` 这类上下文。

### 2.11 平台集成清单

| 能力 | 实现 |
| --- | --- |
| 更新 | `tauri-plugin-updater`，S3 `app.hyperfocus/update.json`，minisign 校验 |
| 通知 | `tauri-plugin-notification`，`session_expiry_worker` 到期提醒 |
| 对话框 | `tauri-plugin-dialog`（备份导出、文件选择） |
| 打开链接 | `tauri-plugin-opener` |
| 菜单 | Tauri 菜单（含 `exit_poll` 专用菜单项） |
| 数据库备份 | WAL checkpoint + 复制，导出前 `integrity_check` |
| 窗口 | 单窗口，`show_window_when_ready` 控制首帧显示时机 |

### 2.12 后期补获的三块后端能力

以下几块在首轮拆解中被模块树与符号表漏掉，是后来按"每个后端模块都要有对应规范"逐项核对时补上的。记录在此以保证需求与证据的对应关系完整。

#### 重复日程（session repeats）

`repeats_table` 在首轮 schema 抓取时被看到，但没人注意它的语义。实际是一套完整的**专注块模板机制**：

```sql
repeats_table (
  id, title, duration,          -- 模板 = 标题 + 时长
  position,                     -- 顺序
  archived                      -- 停止重复 = 归档而非删除
)
-- 实例侧
cycles_table.repeat_id TEXT REFERENCES repeats_table(id)
```

从 SQL 片段能读出三条关键语义：

| SQL | 含义 |
| --- | --- |
| `UPDATE cycles_table SET repeat_id = NULL WHERE repeat_id = ?` | 移除模板时**解除关联而非级联删除**——用户已经做过的专注块及其时长记录必须保留 |
| `UPDATE repeats_table ... archived` | 「Stop repeating」是归档，不是硬删除 |
| 文案 `Failed to update future session repeats` | 改模板只作用于**未来**实例，无法判定未来范围时明确失败 |

迁移史 `38 add tasks to repeats` → `55 drop repeat tasks` 说明模板早期能携带预设任务清单，后来被砍掉，收敛成纯粹的"标题 + 时长"形状。

对外文案：`Repeat daily`、`Stop repeating`、`Repeat updated`。命令：`add_repeat` / `update_repeat`（带 `repeat_id`）/ `stop_repeat`。

#### 遥测边界

`AnalyticsService` + PostHog 在首轮已经还原（§2.10），但**没有转成需求**。这在 local-first 产品上是个漏洞：应用对外承诺"data stored locally"，同时把使用行为发往 `us.i.posthog.com`。两者不矛盾，但必须显式契约化，否则重建时要么无意引入遥测，要么无意破坏承诺。

值得注意的实现细节：`AwaitableAnalyticsCapture`（可等待的上报）说明部分关键事件会等待送达，而日志里有一句 `Analytics service initialized and registered` —— 遥测是一个被显式注册的服务，不是散落的埋点调用。

#### 编辑态后端契约

`get_editor_workspace(cycle_id)` 与 `get_editor_workspaces_by_cycle_ids(cycle_ids)` 是**后端命令**（在命令注册表里），返回 `HashMap<周期标识, EditorWorkspace>` 形态。

前端引了 TipTap + ProseMirror（§1.3），所以编辑器在客户端；但后端要按周期提供编辑所需内容，并且子任务要以 Markdown 渲染（`subtasks_markdown`）供 AI 上下文使用。

`EditorWorkspace` 的完整字段在静态分析中未还原——这是一个**已知的证据缺口**，规范按可观察行为写，字段级 schema 留待实现时补。

---

## 0x03 设计

### 3.1 信息架构

```
                    ┌── Do Later（暂存容器，无日期）
Long-term cycle ────┤
  (1/3/6 月)        └── Week cycle ──► Day cycle ──► Session（专注块）
```

四级链的每一级都是同一种对象（`cycles` 的一条记录），靠 `type` 与 `parent_id` 区分角色。**这个统一是整套设计的支点**：因为同构，所以可以在一屏里并排展示、可以跨级移动、可以递归复制、可以用同一套 UI 渲染。

### 3.2 视觉语言

技术上是 Tailwind，但配置被完全重写。关键 token（完整表见 `design-system.md`）：

| 维度 | 取值 |
| --- | --- |
| 画布 / 面板 | `#fbfbfb` / `#ffffff`（只差一点点，靠 1px 描边区分） |
| 中性色 | 自定义 12 级灰 `#fbfbfb` → `#1e1e1e`（不用 Tailwind 默认灰） |
| 强调色 | 唯一一个：`amber-500 #ed5c2d`（focus 边框、radio、状态点）；链接用 `amber-600 #bc4721` |
| 语义色 | 8 组任务链接色对（每组一个浅色 + 一个深色） |
| 字体 | 自托管 `IBM Plex Sans` 可变字重（用到 400/450/700）+ `IBM Plex Mono` |
| 字阶 | 标题 18/16/14/12px **全部大写 + 700**；正文 14/13px 400；标签 13/12px |
| 圆角 | ≈ 0（只有 3 处 2px，圆点用 full） |
| 阴影 | 全表只有 1 处 `.shadow`；层次靠 1px 细线 + `bg-gray-950/40` 遮罩 |
| 动效 | 统一 `.15s cubic-bezier(.4,0,.2,1)`；引导用更慢的 `cubic-bezier(.22,1,.36,1)` |
| 焦点 | `focus-visible:outline-1 outline-gray-700`（1px 实线环）；表单用 `amber-500` 边框 + ring |
| 顶部高度 | `--app-header-height: 40px` |
| 侧栏宽度 | 424px（agent）、440px |

三条可以概括它的视觉哲学：

1. **明度差极小，全靠描边**。画布 `#fbfbfb` 与面板 `#fff` 的差别几乎不可见，但 1px 灰线把结构说清楚了。
2. **唯一强调色，且用得极窄**。橙色只出现在"当前焦点"和"选中状态"上，所以它一出现就一定是重要的。
3. **直角 + 无阴影**。整个界面像排版品而不是仪表盘，和 IBM Plex 的工程感一致。

**故意的可访问性取舍**：正文对比度是 AAA（16.11），但占位符、禁用项、已完成任务用了 2.10 / 2.11 的低对比度——用可读性换"弱化"的视觉效果。这是一个明确的设计决策，不是疏忽，实现时应当在测试快照或说明里固定下来。

### 3.3 交互语法

六条反复出现的模式，这是这个应用真正值得学的地方：

**① 选择 → 后果预览**
选长周期长度（1/3/6 月）时，右侧不是说明文字，而是实时算出的时间线：`September 14. Set 1-3 long-term goals` / `Next 3 months. Make weekly progress` / `December 7. Review results`。改选项，时间线立刻变。比任何 tooltip 都更能让人理解"3 个月"意味着什么。

**② Agent 侧栏会话**
```
                    [Help me set my long-term goals]   ← 用户消息右对齐带底色
 ✎ Planning skill activated                            ← 技能激活是可见状态
 I'm here to help you set 1–3 clear outcomes...
 By the end of this cycle, what 1–3 results
 would matter most to you? You can start messy,
 we'll clarify things as we go.                        ← 一次一个焦点问题
──────────────────────────────────────────────────────
 [⌘1 Publish online articles] [⌘2 Write a book draft]  ← 候选回答带键盘编号
 [Enter your answer...]
```
要点：技能激活可见、一次只问一个、永远给候选答案、候选答案本身是"如何描述目标"的示范、允许烂输入（"You can start messy"）。

**③ 提议 → 人工确认**（最重要的一条）
Agent 往计划里写东西时不是直接写：目标以**高亮底色**插入，底栏出现 `✎ You have 1 pending edit by agent` + `[Revert] [Keep]`。后端对应 `tasks.agent_proposal` + `task_preview_originals` 快照表 + `preview_execution` 模块。

这条解决了 AI 产品最大的信任问题：**AI 的写入天然可逆**。有了它，用户才敢让 AI 多说话；有了 `You have N pending edits` 这个显式计数，用户又始终知道有多少东西没被消化。

**④ 键盘优先**
`⌘⇧L` = Do later、`⌘1..n` = 选择第 n 个候选回答、`⌘D` = 听写。快捷方式直接写在文案里（`Do later (⌘+⇧+L)`、`Start dictation (⌘D)`）。

**⑤ 即时收纳**
Do Later 抽屉随时可开，里面就一个输入框。想法先扔进去，不打断当前规划；第一次打开时有一条可关闭的说明卡解释它的用途——**功能自己教自己怎么用**。

**⑥ 把"用户不会用"当一等公民**
`planning_issue_report` 主动列出计划的问题（目标太多、任务太多、工作量过大、不清楚下一步、缺少必要的东西、对当前需求没用），每条可忽略且忽略被持久化。可选原因里包括 `"Planning felt like too much work"` 和 `"Not sure how to use it"`——把产品自身的可用性问题做成可收集的信号。

### 3.4 文案语气

从 382 条用户可见文案里能读出统一的语气：

- **直陈，不客套**：`"Past cycles can't be deleted."`、`"Finish other focus block first"`
- **给边界而不是给要求**：`"Aim for 1-3 clear outcomes"`、`"Aim for up to 5 tasks"`
- **允许不完美**：`"You can start messy, we'll clarify things as we go."`、`"Just exploring"`
- **空状态即说明书**：虚线框 + 图标 + 大写标题 + 一句用途 + 一个按钮

---

## 0x04 对做技术选型的启示

**值得直接借鉴的：**

1. 规则下沉到数据库 schema。这是唯一能保证"任何写入路径都不破坏语义"的做法，而且换语言换框架都不失效。
2. AI 写入默认进预览态 + 快照表。实现成本不高（一张表 + 一组命令），但它是 AI 功能能不能被信任的分水岭。
3. 把"清晰度"做成计算量而不是模型判断。可测试、可解释、可迭代。
4. 同构的数据模型换取 UI 的灵活性。四级时间层级用一张表，才有后面所有的并排展示与跨级操作。
5. 单一强调色 + 直角 + 无阴影。一套克制的 token 比一套丰富的 token 更容易保持一致。

**是它的特定约束产物、不一定适用于你：**

1. 前端资源 brotli 内嵌进二进制 —— 只有当"单文件分发 + 防篡改"是硬需求时才值得。代价是失去前端热更。
2. SolidJS —— 收益在渲染效率，代价在生态。如果你的状态没有那么碎，React 更划算。
3. 自签设备指纹令牌 —— 是"无账号体系还想收费"的解法。有账号体系就不需要。
4. PostHog Survey 当退出调查 —— 省事，但把用户反馈数据放在了第三方。
5. 双 AI 路径（托管 + BYOK）—— 两套错误语义与能力假设都要在 agent 层归一化，复杂度不低。

**它明显没做好的：**

1. 应用层没有重试/退避/超时，一次网络抖动就毁掉整个回合。
2. 语音与授权强绑定，试用结束就没了。
3. 没有复盘界面——承诺了 "Review results" 但没实现（这正是扩展方向之一）。
4. 前端 JS 里 TipTap + ProseMirror 占近 40%，对"写目标"这个场景偏重。

**另外三处需要在重建时留意的定位信息**（来自官网，见 `20-website-and-goal-linking.md`）：

1. **AI 的第一卖点是"边写边审"（Grammarly 式），对话助手才是第二卖点。** 这与"重点做 agent 会话"的直觉相反，`planning-issues` 应当优先做扎实。
2. **不要求所有任务都挂在目标下**——演示数据里刻意留了一条无色标的个人事务。
3. **"local-first + planner works offline" 是对外承诺**，所以本地能力不能被授权状态或网络影响。

---

## 0x05 证据与局限

**本仓库的报告索引**

| 报告 | 内容 |
| --- | --- |
| `00-technical-report.md`（本文） | 技术栈选型、技术细节、设计；面向"怎么照着做" |
| `20-website-and-goal-linking.md` | 官网分析：产品定位与自我陈述、**目标拆解链接 UX 的完整实现推导**、商业模型澄清 |
| `40-frontend-ux-deconstruction.md` | 前端产物拆解：chunk 职责、交互状态契约、拖拽/浮层/动效/可访问性 |
| `hyperfocus-deconstruction.md` | 需求反推（R1–R14）、领域模型、分层架构、关键决策与代价 |
| `backend-internals.md` | 后端细节：五个 agent 系统提示、工具 schema、GoalBreakdown、授权、语音、可观测 |
| `design-system.md` | 设计 token 全表（颜色/字阶/间距/动效/组件签名） |

**证据位置**

| 内容 | 文件 |
| --- | --- |
| 原始数据库 schema（含 3 个触发器） | `analysis/evidence/db/schema.sql` |
| 57 条迁移历史 | `analysis/evidence/db/migrations.txt` |
| Rust 模块树 | `analysis/evidence/binary/module_tree.txt` |
| 前端资源清单 | `analysis/evidence/binary/assets_manifest.txt` |
| 还原出的 19 个前端文件 | `analysis/evidence/web/` |
| 153 条组件模板骨架 + 382 条文案 | `analysis/evidence/ui/` |
| UI 交互截图 | `analysis/evidence/screenshots/` |
| 官网 DOM（含可交互 mock 全结构） | `analysis/evidence/website/site-index.html` |
| 官网 mock 样式表（55 KB，674 行 mock 规则） | `analysis/evidence/website/site-mock-css.txt` |
| 官网 hero 规划板截屏（连线与色标可见） | `analysis/evidence/website/hf-mock.png` |

**局限（不要把推测当结论）**

- **ES256 是推断**：二进制里没有 `ES256` 字面量，只有 ring 的 `ECDSA_P256_SHA256_*` 校验器被链接。算法无法从静态分析坐实。
- **`hft1.` 的点分隔符是推断**：`hft1` 四字节前缀在 x86_64 切片里已证实。
- **托管后端的行为未验证**：只看到端点与模型 id，没实际发请求。
- **富文本编辑器的实际能力未验证**：前端引了 TipTap/ProseMirror，但没在 UI 里找到入口。
- **周期删除的具体阈值未提取**：文案是 `Only the latest ${n} can be deleted.`，`n` 的取值在 `cycles::deletion::evaluator` 里，未提取。
- **`update_goal` 的第 7 个参数名未提取**：已知它有 7 个字段，其中 6 个确认。
- **音效的 `sound_type` 取值未提取**。

复现方法见 `hyperfocus-deconstruction.md` §0x01，脚本在 `analysis/scripts/`。

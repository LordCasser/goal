# hyperfocus 0.15.0 拆解报告

> 对象：`/Users/lordcasser/Downloads/hyperfocus_0.15.0_mac_universal.dmg`
> 目标：把已成型的产品还原成原始需求，再拆清它的架构与设计，作为后续扩展的地基。
> 本报告的每条结论都绑定可复现的证据；无法直接验证的部分单独标注。

---

## 0x00 这是什么

一个 macOS 原生桌面应用（**不是** Electron 套壳），核心是把「几个月后的目标」和「今天做什么」用一条连续的链子接起来，中间每一层都有 AI 陪练，但**最终改动必须由人确认**。

产品分层很硬：

```
Long-term cycle（1/3/6 个月）  →  LONG-TERM GOALS
        ↓ 拆解/连接
Week cycle                     →  WEEKLY PLAN
        ↓ 拆解/连接
Day cycle                      →  DAILY PLAN
        ↓
Session cycle（专注块）        →  focus block + 计时
```

另有一个旁路容器 **Do Later**（数据库里就是一条 `type='month'`、`title='Later'` 的周期）：用来暂存不想忘记、但还没准备好承诺的东西。

身份信息：

| 项 | 值 |
| --- | --- |
| `CFBundleIdentifier` | `com.hyperfocus.desktop` |
| 版本 | `0.15.0`（`CFBundleShortVersionString` = `CFBundleVersion`） |
| 签名 | Developer ID Application: DMITRII TCEPELEV (`A3HNYL86C9`)，Runtime 26.5.0，已公证（Notarization Ticket=stapled） |
| 分类 | `public.app-category.productivity` |
| 最低系统 | 10.15 |

商业模型（据官网 FAQ 更正，原文见 `20-website-and-goal-linking.md` §0x04）：**规划器永久免费，没有 Pro 订阅、没有付费档、没有 AI 额度包**。只有 AI 是"内置 14 天 + 之后自带 OpenRouter 凭据"两态，作者声明不从用户的 AI 用量中抽成。

所以二进制里的 `entitlement` 模块不是在卖订阅，而是在管**内置 AI 试用额度**。令牌 `status` 的 `trial`/`paid`/`blocked` 三态里，`trial` 是真实业务状态，`paid`/`blocked` 更像协议预留位（`blocked` 的实用场景是滥用防护）。这一条纠正了我此前的推断——原来写的"或订阅"是错的，规范里不应出现可购买的订阅实体。

---

## 0x01 拆解路径（怎么拿到这些结论的）

按证据强度排序，每一步都能复现：

1. **挂载 DMG**。`hdiutil attach -nobrowse -readonly`。包体只有 `Contents/MacOS/hyperfocus`（66 MB 通用二进制）、`icon.icns`、`Info.plist`——Resources 里没有前端资源，说明前端被内嵌进了二进制。
2. **读 Info.plist**。`NSMicrophoneUsageDescription` = "Hyperfocus uses the microphone to transcribe your spoken Agent messages."，第一次暴露「Agent」和语音输入的存在；`NSAppTransportSecurity` 开了不安全 HTTP 例外，说明有本地 HTTP 端点（后来确认是 OpenRouter OAuth 回调服务器）。
3. **看二进制链接**。`otool -L` 出 `WebKit` / `JavaScriptCore` / `Carbon` / `OSAKit`，`nm` 里出现 `$serde_core`、`cargo/registry` 路径 → 这是 **Rust + Wry/WKWebView**，即 Tauri。
4. **符号反重整**。用自写脚本解析 legacy mangled name，还原出 `hyperfocus::*` 的完整模块树（见 `evidence/binary/module_tree.txt`）。
5. **读运行中应用的数据目录**。`~/Library/Application Support/com.hyperfocus.desktop/` 下有 `config.toml` 和 `local.db`（SQLite）。`_sqlx_migrations` 表暴露了 **57 条迁移**，等于一份需求演进史。
6. **还原前端**。Tauri 把资源以 brotli 压缩块内嵌在 `__TEXT,__const`，布局规律是 **资源路径字符串后面紧跟它的压缩数据**。按这个规律批量解压，得到 19 个文件（SolidJS bundle、Tailwind CSS、自托管字体、Deepgram audio worklet）。
7. **读前端 bundle**。拿到全部 IPC 命令名、领域枚举、用户可见文案、HTML 模板骨架。
8. **驱动 UI 截图**。自写 Swift 工具用 `CGWindowList` 定位窗口、`CGEvent` 注入点击/按键，配合 `screencapture -l` 抓图，按真实交互路径走了一遍。

有意思的一点：**`strings` 在这个二进制里会把相邻的 Rust 字符串字面量粘成一行**（Rust 不写 NUL 分隔符），所以看到 `skip_getting_started_guiderecord_talk_to_founder_openedclose_talk_to_founder...` 这种连读的字符串反而是正常现象，不是脚本坏了。

---

## 0x02 反推原始需求

这一节的写法是：先给出可观察到的产品行为（事实），再反推它要解决的问题（需求）。行为和需求的对应关系都写出来了，方便检查推理跳跃。

### 核心矛盾

通用待办工具解决的是「别忘事」，但它不解决「**这件事值不值得做**」和「**为什么现在做**」。反过来，目标管理方法论（OKR 之类）解决「方向」，但不解决「今天下午两点坐在电脑前该点开哪个文件」。

Hyperfocus 赌的是：这两端之间的断层是可以被产品化的，而且断层里**最适合放一个会问问题的 AI**，而不是一个自动排计划的 AI。

从 UI 文案能直接读出它瞄准的人群：

- `"You can start messy, we'll clarify things as we go."` —— 允许用户输入是烂的
- `"Aim for 1-3 clear outcomes worth a few months of effort to you."` —— 主动限制数量
- `"Choose how long you want to focus on a few goals before reviewing results."` —— 预设周期，降低选择成本
- `"Planning felt like too much work"`（作为可选的反馈选项）—— 承认「规划本身」就是负担
- `"Not sure how to use it"` / `"Seems like a task, not a goal"` —— 把「用户不会用」当成一等公民来处理

### 需求清单（R1–R14）

| # | 需求 | 支撑证据 |
| --- | --- | --- |
| R1 | **四级时间容器**：长周期 → 周 → 日 → 专注块，父子可级联删除 | `cycles_table.type IN ('session','day','week','month')` + `parent_id REFERENCES cycles_table ON DELETE CASCADE` |
| R2 | **单一工作台**：所有层级并排、横向可滚，不切页面 | 截图：`Cycles`/`LONG-TERM GOALS`/`Weeks`/`WEEKLY PLAN` 同屏并排；前端只有 `#root` 一个挂载点，无路由库 |
| R3 | **目标质量闸门**：目标必须澄清 what/why/context/success-measure，且可分解 | `tasks_table.needs_refinement` / `needs_breakdown`；文案 `Miss clear what` / `Miss clear why` / `Miss context` / `Miss success measure` / `Breakdown missing` / `Break down` |
| R4 | **AI 是陪练，不是生成器**：一次只问一个焦点问题，附可选答案 | 截图：agent 首问「what 1–3 results would matter most to you? You can start messy」；回答后追问「What kind of writing…and what would success look like by December?」 |
| R5 | **AI 的写入必须可逆且需人确认** | 截图：目标以高亮态插入 + 底栏 `You have 1 pending edit by agent` + `Revert`/`Keep`；DB 有 `tasks_table.agent_proposal` 和 `task_preview_originals` 快照表 |
| R6 | **降低规划的意志力成本**：模板化周期、复制上一周期、重复日程 | 周期选择只有 1/3/6 月三项；`commands::cycles::copy_previous`；`repeats_table`；文案 `Copy from previous cycle/day/week`、`Repeat daily` |
| R7 | **计划健康检查**：主动指出计划的问题，而不是等它崩 | `planning_issue_dismissals` 表；文案 `Too many goals` / `Too many tasks` / `Didn't feel useful for what I need` / `Something I needed was missing` |
| R8 | **收纳区（Do Later）**：不过滤灵感，但也不污染当前计划 | `later` 周期；左侧抽屉 `DO LATER` + 「Type your goal here...」；文案 `Add goal to "Do Later"`、`Toggle Later sidebar` |
| R9 | **进度必须可感知**：剩余时间、完成步数、累计专注时长 | 列头 `12 weeks left`；`Getting started 0 of 5`；`cycles_table.focused_time`（毫秒） |
| R10 | **退出是产品的一部分**：退出时问一句为什么，并给一条联系作者的通道 | `exit_poll::*` 模块 + 6 个命令；文案 `Could you tell us more?` / `I'll be back` / `Just exploring`；`talk_to_founder` + `Dmitri, maker of Hyperfocus` |
| R11 | **付费闸门绑定设备**：无账号体系也要能控权限 | `entitlement` 模块；`config.toml` 里 `device_fingerprint` + ES256 JWT（`hft1.` 前缀，payload 含 `status`/`trial_expires_at`）；文案 `Active trial or paid plan required`、`Included AI access has ended. Connect AI provider in Settings to continue.` |
| R12 | **本地优先**：数据不出机器，只有 AI 请求上云 | 全部业务数据在 `local.db`；无登录、无同步表；`database::backup` |
| R13 | **语音输入**：长文用说的 | `NSMicrophoneUsageDescription`；内嵌 `/audio/deepgram-pcm-worklet.js`；IPC `get_deepgram_transcription_token`；文案 `Speak to type...`、`Start dictation (⌘D)` |
| R14 | **产品可观测** | `analytics` 模块；`skip_getting_started_guide`、`record_talk_to_founder_opened`、`get_entitlement_summary` 等事件名 |

### 从迁移史读到的需求变迁

57 条迁移里有一条很清晰的曲线，说明哪个概念是产品的痛点：

```
12  clarity breakdown migration
13  goal evaluation schema
14  reset clarity breakdown for granular issues
15  replace execution clarity with structural type
17  add goal breakdown
18  add is refined to tasks
21  drop clarity breakdown            ← 整个删掉
26  add prioritization breakdown
49  add clarity flags to tasks        ← 换成两个布尔 flag 回来
50  add clarity flags to preview originals
51  drop legacy clarity columns
52  clear stale goal breakdown state
```

结论：**「如何让用户把目标说清楚」被反复重设计**。从独立表 → 结构类型 → 删掉 → 最终收敛成 `needs_refinement` / `needs_breakdown` 两个布尔值 + 一份 JSON 形式的 `goal_breakdown`。这说明原始需求不是「做一个澄清表单」，而是「让人在不被表单烦到的情况下把话说清楚」——最终选了最轻的实现。

另外 `40 rename periods to cycles` 说明这个「时间容器」概念早期叫 period，后来才叫 cycle。`45 allow null cycle duration` 说明周期时长一开始是必填，后来允许为空（对应 `Later` 这种没有终点的容器）。

---

## 0x03 领域模型

### 表结构

```
repeats_table          日程模板：title, duration, position, archived
cycles_table           时间容器（见下）
tasks_table            任务/目标，自引用树
task_preview_originals 预览态快照：agent 改动被 Keep 之前，原始值存这里
agent_conversations    每个 cycle 一条对话，记 active_turn_id / revision / active_skill
agent_messages         对话消息，序号唯一，5 种类型
planning_issue_dismissals  周期内的计划问题「已忽略」记录
```

`cycles_table` 的字段值得单独看：

```sql
id, title,
type CHECK (type IN ('session','day','week','month')),
started, finished, started_at, finished_at,   -- 生命周期
parent_id REFERENCES cycles_table ON DELETE CASCADE,
repeat_id REFERENCES repeats_table(id),
focused_time,                                 -- 累计专注时长（迁移 6 改成毫秒）
prioritization_breakdown, duration,
starts_on, ends_on, calendar_key,
CHECK (NOT (started = 0 AND finished = 1))     -- 没开始不能已结束
```

### 数据库层强制的不变量

这是整个设计里最硬的部分——规则不写在应用代码里，写在 schema 里：

1. **`root_color_key` 只能挂在 Long-term 周期上**。两条 `BEFORE INSERT/UPDATE` 触发器直接 `RAISE(ABORT, 'root_color_key requires a Long-term cycle')`。
2. **一旦有任务带 root color，该周期不能改成非 Long-term**。第三条触发器 `reject_cycle_type_change_with_root_colors`。
3. **`calendar_key` 全局唯一**（`WHERE calendar_key IS NOT NULL`）：保证「同一天」「同一周」在库里只可能有一条实例。
4. **`type='month'` 在 UI 层就是 Long-term**。触发器报错文案直接写 "Long-term cycle"，把数据库类型和产品概念对上。
5. **状态的单调性**：`NOT (started = 0 AND finished = 1)`。
6. **agent 改动的类型只有两种**：`tasks_table.agent_proposal IN ('upsert','delete')`。
7. **task 有三条索引专门服务 agent**，其中 `idx_tasks_cycle_agent_proposal_visibility ON tasks_table(cycle_id, agent_proposal, position)` 说明「按周期查可见任务」是最高频查询，而且可见性跟 `agent_proposal` 直接相关——待确认的改动是列表的一部分。

### 层级与生命周期

```
Later (month, 无日期)  ──┐
                        │ 用户主动 commit
Long-term (month, 1/3/6 月, starts_on/ends_on/calendar_key)
   └── Week  (week,  calendar_key = ISO 周)
         └── Day   (day, calendar_key = 日期)
               └── Session (session, focused_time 累计)
```

`copied_from_task_id` 记录「从上一周期复制过来」的血缘，`copy_previous` 命令负责这个动作。

---

## 0x04 架构

### 进程与运行模型

```
┌──────────────────────────────────────────────┐
│ 单进程：hyperfocus (Rust, universal binary)   │
│                                               │
│  ┌─────────────────┐   invoke / event        │
│  │ WKWebView        │◄──────────────────────►│
│  │ tauri://localhost│   (tauri:// 自定义协议) │
│  │ SolidJS + TipTap │                         │
│  └─────────────────┘                          │
│                                               │
│  commands::*  ← IPC 边界，唯一对外入口        │
│      │                                        │
│  ai::* / events::* / entitlement::* / ...     │
│      │                                        │
│  sqlx ──► SQLite (local.db)                   │
│  reqwest ──► OpenRouter / Gemini / 自有后端   │
└──────────────────────────────────────────────┘
```

前端不是从磁盘读的——资源经 `tauri://localhost` 自定义协议由 Rust 侧应答，所以抓不到独立资源目录。

### 模块树（从符号还原，节选）

```
hyperfocus
├── commands::{cycles,tasks,agent,onboarding,settings,exit_poll,
│              feedback,getting_started_guide,talk_to_founder,utils}
├── ai
│   ├── agent
│   │   ├── conversation::{lifecycle,snapshot,storage}
│   │   ├── service::{turn,parsing}
│   │   ├── prompt                      + ActiveAgentSkill
│   │   ├── prioritization::{repository,workflow}
│   │   └── tools
│   │       ├── create_goal / delete_goal / update_goal
│   │       ├── update_goal_breakdown::{args,execute,merge}
│   │       ├── start_planning / start_prioritization
│   │       ├── update_prioritization_breakdown
│   │       ├── get_cycle_context::schema
│   │       └── shared::{activation,cycle_context,cycle_metadata,
│   │                   cycle_mutability,cycle_resolution,
│   │                   preview_execution,task_context}
│   ├── goal_breakdown
│   │   ├── types::breakdown::{GoalBreakdown,ScopeBreakdown,
│   │   │   OutputBreakdown,ContextBreakdown,OutcomeBreakdown,
│   │   │   WorkSize,ContextType}
│   │   ├── prompt::{extraction,parent_matching,shared}
│   │   ├── gathering_plan  ← calculate_clarity_flags / missing_fields
│   │   └── service
│   └── llm
│       ├── traits::LLMProvider
│       ├── openrouter_client / openrouter_mapping
│       ├── gemini_mapping::{requests,responses,content}
│       ├── worker_client::WorkerLlmClient   ← 自有后端
│       ├── resolved_client / schema_mapping
│       └── types::{request,agent_request,agent_response}
├── entitlement::{device,service,verification}   + token
├── openrouter_oauth   (PKCE)
├── credentials::openrouter   (Keychain)
├── analytics::AnalyticsService
├── events::{tasks,cycles,agent}
├── exit_poll::{eligibility,lifecycle,macos_menu}
├── session_expiry_worker
├── database::{backup,dialog}
└── settings::Settings
```

### 依赖方向

很清楚的三层，没有逆流：

```
commands  →  ai / entitlement / settings / database  →  sqlx / reqwest / tauri
   ↑
events（单向推给前端）
```

- **`commands` 是唯一 IPC 入口**，命名是动词短语（`create_planning_cycle`、`patch_task`、`keep_all_preview_changes`）。
- **`events` 只出不进**，三个 emitter（tasks / cycles / agent）。前端订阅事件拿增量更新，所以 DB 是唯一真相源。
- **`ai::llm` 用 trait 隔离供应商**（`LLMProvider`），上层 agent 逻辑不感知是 OpenRouter 还是 Gemini 还是自有 worker。
- **`goal_breakdown` 是独立子系统**，有自己的 types / prompt / service，说明「把目标拆成子目标」被当成核心能力而不是 agent 的一个工具。

### AI 层

**四个 skill**（`agent_conversations.active_skill`）：

| skill | 触发时机 |
| --- | --- |
| `goal_setting` | 新建 Long-term 周期时 |
| `long_term_planning` | 规划长周期 |
| `short_term_planning` | 规划周/日 |
| `prioritization` | 排优先级（有独立的 `prioritization::{repository,workflow}`） |

**Agent 工具集**：`create_goal` / `delete_goal` / `update_goal` / `update_goal_breakdown` / `update_prioritization_breakdown` / `start_planning` / `start_prioritization` / `get_cycle_context`。

注意 `tools::shared::preview_execution` —— **「预览执行」是一等公民，和 `cycle_mutability`（可变性检查）、`cycle_resolution`（周期解析）并列**。这就是 Keep/Revert 机制在后端的落点。

**GoalBreakdown 的数据形状**（这是产品的差异点）：

```
GoalBreakdown
├── ScopeBreakdown     (scope_missing_fields)
├── OutcomeBreakdown   (outcome_missing_fields)
├── OutputBreakdown    (output_missing_fields)
└── ContextBreakdown   (context_missing_fields)
    WorkSize / ContextType 枚举
```

配合 `calculate_clarity_flags` / `calculate_missing_fields` / `calculate_refinement_missing_fields` 三个计算函数，最终投影成 UI 上的 `needs_refinement` / `needs_breakdown`。也就是说：**「目标清不清晰」是一个可计算的量，不是让 AI 随口判断**。

**供应商解析**：`resolved_client` 决定走哪个后端，`ai_provider` 只有两个取值：`hyperfocus`（自有 hosted）和 `openrouter`（自带 key）。

- **Hosted**：`https://workers.hyperfocus.in`，两个端点 `/v1/llm/generate-turn`（对话）和 `/v1/llm/generate-json`（结构化输出）。模型是 `google/gemini-3.7-flash`、`google/gemini-3.1-flash-lite`、`google/gemini-2.5-flash`。鉴权用 `EntitlementService::hosted_ai_token` 换来的 token。**语音的 Deepgram key 也从这里换**（`/v1/transcription/deepgram-token`，返回 `{access_token, expires_in}`），客户端拿不到服务商 API key。
- **OpenRouter**：PKCE OAuth（`generate_pkce_verifier` / `compute_pkce_challenge` / `build_authorization_url` / `parse_callback_request`），回调由应用内置的本地 HTTP server 处理，key 存 macOS Keychain（service `com.hyperfocus.desktop.openrouter`，account `api-key`）。

`LLMProvider` trait 只强制两个方法：`generate_agent_once` 和 `generate_json_once`；`generate_agent` / `generate_json` 是默认实现。实现类：`OpenRouterClient`、`WorkerLlmClient`、`ResolvedLlmClient`、`ResolvedAgentLlmClient`。

值得注意的一个**缺席**：应用层没有任何重试/退避/超时常量，只有 reqwest 自己的默认值。

### IPC 契约（从前端 bundle 提取，共 60+ 命令）

按功能分组：

| 组 | 命令 |
| --- | --- |
| 周期 | `create_planning_cycle` `delete_planning_cycle` `get_planning_cycle_deletion_preview` `start_cycle` `finish_cycle` `update_cycle` `add_session` `reorder_sessions` `add_repeat` `update_repeat` `stop_repeat` `copy_uncompleted_tasks_from_previous_cycle` `get_current_short_term_cycle_ids` `get_week_start_day` `get_or_initialize_week_start_day` |
| 任务 | `add_task` `upsert_task` `patch_task` `delete_task` `move_task` `set_task_attrs` `set_task_parent_link` `set_task_root_color` `set_task_clarity_data` `reset_task_clarity` `get_task_by_id` `get_tasks_by_cycle_ids` `get_parent_link_candidates` `select_task` |
| 预览/撤销 | `get_preview_summary` `keep_task_preview` `undo_task_preview` `keep_all_preview_changes` `undo_all_preview_changes` |
| Agent | `start_agent_conversation` `send_agent_message` `get_agent_conversation` `get_previous_agent_conversation` `start_goal_setting` `start_planning` `start_prioritization` |
| AI 设置 | `get_ai_settings` `set_ai_provider` `save_openrouter_api_key` `remove_openrouter_api_key` `connect_openrouter` `cancel_openrouter` `get_deepgram_transcription_token` |
| 计划问题 | `get_planning_issue_report` `dismiss_planning_issue` `get_planning_issue_dismissals` |
| 引导 | `get_onboarding` `prepare_onboarding` `complete_onboarding` `reconcile_getting_started_guide` `skip_getting_started_guide` |
| 提示 | `dismiss_hint` `get_dismissed_hints` |
| 退出调查 | `mark_exit_poll_listener_ready` `acknowledge_exit_poll_shown` `submit_exit_poll` `dismiss_exit_poll` `continue_after_exit_poll` `exit_after_exit_poll` |
| 反馈 | `send_feedback` `get_talk_to_founder_eligibility` `record_talk_to_founder_opened` `close_talk_to_founder` |
| 编辑器 | `get_editor_workspace` `get_editor_workspaces_by_cycle_ids` `task_editor_transaction_source` `track_changes` |
| 权限 | `get_entitlement_summary` `get_entitlement_support_id` |
| 其他 | `play_audio` `show_window_when_ready` |

**事件名**：`goal_breakdown_update_status` `task_added` `task_removed` `cycle_ended`。

**领域枚举**（前端硬编码，等于后端契约的一部分）：

- 消息类型：`user` `model_text` `model_function_call` `function_result` `app_tool_result`
- 计划问题：`too_many_goals` `too_many_tasks` `too_much_work` `not_sure_what_to_do_next` `missing_something` `not_useful_for_needs`
- 优先级：`low_priority` `neutral_gray`
- 目标状态：`in_flight` `awaiting_input` `done_for_now` `maybe_later` `just_exploring` `no_thanks`
- 周期动作：`daily_alignment` `weekly_alignment` `view_agenda` `started_focus_block`
- 删除拒绝原因：`ended_descendant` `newer_sibling`

### 平台集成

| 能力 | 实现 |
| --- | --- |
| 更新 | `tauri-plugin-updater`，端点 `//s3.eu-central-1.amazonaws.com/app.hyperfocus/update.js` |
| 通知 | `tauri-plugin-notification`（`session_expiry_worker` 在专注块结束时发通知） |
| 音频播放 | `rodio`（`commands::utils::init_audio_system` / `play_audio_feedback` / `decode_audio`） |
| 语音转写 | Deepgram（内嵌 PCM worklet，`get_deepgram_transcription_token` 换临时 token） |
| 密钥 | macOS Keychain（`credentials::openrouter`，`PROTECTED_STORE_INITIALIZATION`） |
| 退出流程 | `exit_poll::{eligibility,lifecycle,macos_menu}` + `ExitPollCoordinator` |
| 数据库备份 | `database::backup` + `database::dialog`（导出/选择路径） |

---

## 0x05 交互设计

这是最值得抄的部分。有几个反复出现的语法。

### 布局语法

```
┌─ 40px app header ────────────────────────────────────────┐
│ [clock] Getting started ▓▓░░░ 0 of 5      AI included · 14 days left [gear] │
├──────────┬──────────────────────────────┬────────────────┤
│ DO LATER │  横向滚动的周期列             │ Agent 侧栏 424px│
│ 抽屉     │  Cycles│LONG-TERM│Weeks│WEEK  │ (fixed right)   │
│ (可切换) │        │ GOALS   │     │ PLAN │                 │
└──────────┴──────────────────────────────┴────────────────┘
```

- header 固定 40px（`--app-header-height`）
- agent 侧栏 `<aside>` 固定右侧 424px，`top` 从 header 下方开始
- 主区是横向滚动的列，**每一列都是一层时间容器**，不是不同页面
- 空状态统一是虚线框（`border-dashed border-gray-300`）+ 图标 + 大写标题 + 一句解释 + 一个主按钮

### 语法一：选择 → 后果预览

选长周期长度时（1/3/6 月），右侧不是说明文字，而是**实时算出来的时间线**：

```
○ September 14. Set 1-3 long-term goals
│
○ Next 3 months. Make weekly progress
│
○ December 7. Review results
```

改选项，时间线立刻跟着变。这比任何 tooltip 都更能让人理解「3 个月」到底意味着什么。

### 语法二：Agent 侧栏

截图里完整可见的结构：

```
Hyperfocus Agent                                    [×]
────────────────────────────────────────────────────
                    [Help me set my long-term goals]   ← 用户气泡右对齐
 ✎ Planning skill activated                            ← 状态徽章
 I'm here to help you set 1–3 clear outcomes...
 By the end of this cycle, what 1–3 results
 would matter most to you? You can start messy,
 we'll clarify things as we go.                        ← 一次只问一个焦点问题
────────────────────────────────────────────────────
 [⌘1 Publish online articles] [⌘2 Write a book draft]
 [⌘3 Improve work writing]
 [Enter your answer...]                                 ← 输入框 + ⌘N 快捷回复
```

要点：
1. **skill 激活是可见的状态**（`Planning skill activated`），不是隐形的。
2. **一次一个焦点问题**，问题里明确给出决策边界（"1–3"、"by December"）。
3. **永远给候选答案**，并且带 `⌘1`/`⌘2`/`⌘3` 快捷键——降低打字成本。候选答案本身是「示例」，在教用户怎么描述目标。
4. **允许低质量输入**："You can start messy"。

### 语法三：提议 → 确认（最重要的一个）

Agent 往计划里写东西时，不是直接写：

```
LONG-TERM GOALS
 ┌────────────────────────────────────┐
 │ ☐ Write a book draft               │  ← 高亮底色的待确认态
 └────────────────────────────────────┘

──────────────────────────────────────────────────────
 ✎ You have 1 pending edit by agent    [Revert] [Keep]
```

对应的后端机制：`tasks_table.agent_proposal`（值为 `upsert`/`delete`）+ `task_preview_originals`（存原值快照）+ `preview_execution` 模块 + 4 个 `*_preview_changes` 命令。

**这条设计解决了 AI 产品最大的信任问题**：AI 的写入天然可逆，用户可以放心让它多发几次言。而且 `You have N pending edits` 把它做成一个显式的计数，用户始终知道「有多少东西还没被消化」。

### 语法四：键盘优先

- `⌘+⇧+L` → Do later（文案里直接标注 `Do later (⌘+⇧+L)`）
- `⌘1`…`⌘n` → 选择第 n 个候选回答
- `⌘D` → 开始/停止语音听写（`Start dictation (⌘D)`）

### 语法五：即时收纳

左侧 `DO LATER` 抽屉任何时候都能打开，里面就是一个输入框「Type your goal here...」。想法进来先扔这里，不打断当前规划。第一次打开会出现一条可关闭的提示卡解释它的用途——**功能自己在教自己怎么用**。

### 语法六：把「用户不会用」当成功能

`planning_issue_report` 会主动列出计划的问题，每一项都能「dismiss」且被持久化（`planning_issue_dismissals`，且有 `UNIQUE(cycle_id, issue_type, task_id)` 和针对 `task_id IS NULL` 的部分唯一索引，说明有些问题是**整周期级**的、有些是**单任务级**的）。可选的原因文案包括 `"Planning felt like too much work"`、`"Not sure how to use it"`——把产品自身的可用性问题做成可收集的信号。

---

## 0x06 设计语言

完整 token 表在 `analysis/reports/design-system.md`（513 行，由子代理产出）。这里只留结论：

技术上是 **Tailwind v3 + Lightning CSS**，但配置被改成了完全非默认的样子：

- **没有暗色主题**。整个 CSS 里 `prefers-color-scheme` 和 `dark:` 出现 0 次。
- **中性色是自定义的 12 级灰**，不是 Tailwind 默认：`gray-25 #fbfbfb`（画布）到 `gray-950 #1e1e1e`（正文）。画布 `#fbfbfb` 和卡片 `#fff` 之间只差一点点，靠 1px 描边区分。
- **唯一品牌色是橙 `amber-500 #ed5c2d`**，用途极窄：focus 边框/ring、radio、状态点。文本链接用 `amber-600 #bc4721`。
- **圆角≈0**。按钮、输入框、弹窗、菜单全是直角；只有 3 处 `rounded-sm`（2px）和点/环用 `full`。
- **几乎没有阴影**。整张表就 1 处 `.shadow`；层次靠 1px 细线 + `bg-gray-950/40` 遮罩。
- **字号小、字重高、标题全大写**：标题 18/16/14/12px 且 `text-transform: uppercase`；正文 14/13px；行高紧凑。
- **字体自托管**：`IBM Plex Sans var`（weight 100–900，用到 400/450/700 三个）+ `IBM Plex Mono`。
- **悬停才出现的行内控件**（`opacity:0; pointer-events:none` → `li:hover` 时显示）——保持静态时画面干净。
- 动效统一 `.15s cubic-bezier(.4,0,.2,1)`；onboarding 用更慢的 `cubic-bezier(.22,1,.36,1)`。

可访问性上有两处刻意的取舍：正文对比度是 AAA（`#1e1e1e` on `#fbfbfb` = 16.11），但 **placeholder、禁用态、已完成任务用了低对比度**（2.10 / 2.11）——是用可读性换「弱化」的视觉效果。另外 `motion-reduce` 覆盖了弹窗入场，但**没有覆盖无限循环的 agent 指示器**。

---

## 0x07 关键设计决策与代价

| 决策 | 它解决了什么 | 代价 |
| --- | --- | --- |
| Tauri 而不是 Electron | 二进制 66MB、内存低、能直接进 Keychain/通知/菜单 | 前端只能用系统 WebView，必须自建资源内嵌与协议 |
| 前端资源 brotli 内嵌进二进制 | 单文件分发，无外部资源目录 | 无法热更前端；我们这次也正是靠这个规律还原出前端的 |
| 规则写进 SQLite trigger/CHECK | 应用层换语言、换框架都不会破坏数据一致性 | 改规则要写迁移；错误信息只能靠 `RAISE(ABORT, ...)` 传给用户 |
| AI 提议 + 人工 Keep/Revert | AI 写入可逆，降低信任门槛，用户保有所有权 | 需要 `task_preview_originals` 快照表和一整套预览命令；复杂度不低 |
| 澄清逻辑收敛成两个布尔 flag | 早期版本用独立表和结构类型，太重 | `goal_breakdown` JSON 变成半结构化数据，查询能力弱 |
| 无账号、设备指纹绑定授权 | 不用注册就能收费，隐私门槛低 | 换机器要重新授权；`device_fingerprint` 进了 `config.toml` 明文 |
| 单一工作台横向滚动 | 长期目标和今天任务在同一视线内 | 状态多、上下文切换成本靠 UI 承担 |

---

## 0x08 存疑与待验证

写下来避免把推测当结论。（下面标 ✅ 的条目是第二轮子代理独立验证后又补上来的，不再存疑。）

1. ✅ **Hosted AI 后端已定位**：`https://workers.hyperfocus.in`，端点 `/v1/llm/generate-turn`、`/v1/llm/generate-json`、`/v1/transcription/deepgram-token`；模型为 Gemini 系。细节见 `backend-internals.md` §1.4。
2. ✅ **Analytics 是 PostHog**：`https://us.i.posthog.com/i/v0/e/`，用 `posthog-rs 0.3.7`。退出调查不是自建表单，而是 **PostHog Survey**（8 个 code），和前端 bundle 里的文案能对上。
3. ✅ **授权 token 的确切格式**：`hft1` 4 字节前缀已证实（x86_64 侧 `0x45aced`），`. ` 分隔符是推断。Header 是 `{alg,typ,ver}`，Claims 是 `{version, device_fingerprint, issued_at, expires_at, status, trial_expires_at}`，`status ∈ trial|paid|blocked`。设备指纹来自 `ioreg -c IOPlatformExpertDevice` 加盐 `hyperfocus-device-fingerprint-v1`。**ES256 仍然只是推断**——二进制里没有 `ES256` 字面量，只有 ring 的 `ECDSA_P256_SHA256_*` 校验器被链接进来，实际算法无法从静态分析坐实。
4. ✅ **Agent 写入是纯预览，不是提交**：`tools::shared::preview_execution` 把每一次变更都路由到 `task_preview_originals` + `agent_proposal='upsert'`。另外一个反直觉的细节：**agent 新建目标时会替换「最后一个空任务行」而不是追加**——这是为了让空输入框自然变成目标。
5. ✅ **优先级模型**：`PrioritizationBreakdown{big_wins, bottlenecks, non_negotiables, deprioritized, pending_review}`，存在 `cycles_table.prioritization_breakdown` 列里；发给模型时用 XML 表示（`hydrate_breakdown_xml`，会做 XML 转义）。系统提示里用 ⭐️ 标 big wins、💣 标 non-negotiables。
6. ✅ **语音管线规格**：48 kHz 采集 → 16 kHz 输出、80 ms / 1280 采样一帧、`linear16` 小端、零拷贝 buffer 转移。
7. ⬜ **`update_goal` 的第 7 个参数名没提取到**。已知它有 7 个字段，其中 6 个（`title`/`subtasks`/`parent_link`/`rationale`/`needs_refinement`/`needs_breakdown`）确认，第 7 个只能从 `update_goal::resolved_clear_only_flag` 推测是个「只清不设」的 flag。
8. ⬜ **`sound_type` 的具体取值没提取到**。只知道有音频反馈（`play_audio` + `rodio`）。
9. ⬜ **`2026` 这个年份标签的来源**：截图中 Cycles 列上方显示 `2026`，但 schema 里没有 year 类型，推测是日历年份分组头，未确认。
10. ⬜ **周期删除规则的具体阈值**：文案有 `Only the latest ${n} can be deleted.` 和 `Past cycles can't be deleted.`，`commands::cycles::deletion::evaluator` 里应该有常量，未提取。
11. ⬜ **`get_editor_workspace` / TipTap 的实际能力未验证**。前端引了 `editor-tiptap` + `editor-prosemirror` 两个 chunk，说明任务描述是富文本，但没进 UI 实测（没找到入口）。推测是任务详情里的富文本备注。

---

## 0x09 证据索引

```
analysis/evidence/
├── binary/
│   ├── module_tree.txt       Rust 模块树（符号还原）
│   ├── commands.txt          commands::* 全部符号路径
│   ├── assets_manifest.txt   内嵌前端资源清单
│   └── strings_arm64.txt     二进制字符串（注意字面量会粘连）
├── db/
│   ├── schema.sql            完整 schema（含 3 个触发器）
│   ├── migrations.txt        57 条迁移历史
│   └── local.db              运行时数据库副本
├── web/                      还原出的 19 个前端文件
│   ├── _index.html
│   ├── _assets_index-BZ_MKiTS.js        主 bundle（205KB）
│   ├── _assets_AgentConversation-*.js   agent 会话 UI（102KB）
│   ├── _assets_editor-tiptap-*.js       富文本编辑器
│   ├── _assets_vendor-solid-*.js        SolidJS
│   ├── _assets_index-Bk3EgsqR.css       设计系统（64KB）
│   ├── _assets_fonts_*.woff2            IBM Plex
│   └── _audio_deepgram-pcm-worklet.js   语音采集 worklet
└── ui/
    ├── templates.html       153 条组件模板骨架（class 全保留）
    └── copy.txt             382 条用户可见文案，按 bundle 分组
analysis/reports/
├── design-system.md         设计系统完整提取
└── backend-internals.md     后端内部机制（LLM/agent/工具/权限/音频）
```

复现命令见 `analysis/scripts/`。挂载 DMG 后按 0x01 的步骤 1→8 可重跑全流程。

## Why

`openspec/specs/` 已经用 12 个能力、62 条需求描述了要交付的系统（由 hyperfocus 0.15.0 拆解而来）。这些需求目前没有任何实现——仓库里只有拆解证据与规范。本变更把规范变成可运行的软件：一个我们自己的、结构可控的本地优先规划应用。

原应用闭源且已公证，无法在其上修改；因此"扩展"的唯一可行路径是先拥有一份自己的实现。本变更是那四个扩展变更（回顾、本地模型、日历视图、提醒）的前置。

## What Changes

从零建立应用骨架与核心规划链路：

- Tauri 2 + React 19 + TypeScript + SQLite 的工程脚手架，可构建、可运行
- 本地数据库层：WAL、版本化迁移、备份导出
- 领域层：周期（session/day/week/month）与任务（自引用树、跨层链接、着色、清晰度标记）
- IPC 边界：周期与任务的完整命令集，以及 Rust → 前端的事件推送
- 数据库级不变量：把 `root_color_key`、`calendar_key` 唯一性、生命周期单调等规则放在 schema 里而不是应用代码里
- Agent 提议机制的数据层：预览态、原始值快照、Keep/Revert/Keep all/Undo all
- 最小可用界面：横向周期列工作台、Do Later 侧栏、待确认改动底栏

不在本次范围内：LLM 调用与 agent 对话循环（`agent-conversation`、`goal-clarification`、`prioritization` 三个能力）、语音输入、授权与试用、引导清单、退出一致性流程。它们的规范已存在，由后续变更实现。

## Impact

- 新增：`src-tauri/`（Rust 后端）、`src/`（React 前端）、构建与测试配置
- 新增：本机数据库文件与应用配置文件（非仓库内容）
- 依赖：Tauri 2、React 19、Vite、Tailwind、rusqlite（bundled）、r2d2
- 不受影响：`openspec/specs/` 下的既有规范（本变更不修改任何需求）
- 后续：`add-review-retrospective`、`add-local-llm-provider`、`add-calendar-time-view`、`add-reminders-notifications` 四个变更都建立在本变更之上

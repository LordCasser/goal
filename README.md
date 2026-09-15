# planner

一个本地优先的个人规划应用：把「几个月后的目标」和「今天做什么」用一条连续的链子接起来。

需求来自对 macOS 应用 **hyperfocus 0.15.0** 的拆解还原；本仓库当前阶段交付**规范化需求**与**拆解报告**，代码骨架已就位但尚未实现业务逻辑。

本轮重做需求的入口是 [核心 UX 候选基线](analysis/reports/30-core-ux-requirements.md)：35 条需求、70 个验收场景，附来源与优先级。[实际使用与证据复核](analysis/reports/31-product-use-and-evidence.md) 保存了同版本复测与资源身份校验；[待决策与缺口](analysis/reports/32-rebuild-decisions-and-gaps.md) 独立记录 12 个产品决策、4 项架构债务。[官网 UX/UI 复核](analysis/reports/33-website-ux-ui-review.md) 补充设计对照与需求差异，其中 **UXR-033「点击条目持续展示关联」是用户确认的额外需求**。候选需求尚未自动同步到既有 OpenSpec，重做前应先处理记录中的行为冲突。

应用前端设计统一从 [design.md](design.md) 进入：依据原 App 与官网中的 App 界面展示，整理工作台布局、视觉规范、组件状态、交互规则和验收场景，不包含官网页面自身的设计。

---

## 这份仓库里有什么

| 目录 | 内容 | 权威性 |
| --- | --- | --- |
| `openspec/specs/` | **既有需求规范**，15 个能力 | 已整理的重建行为；本轮复测差异见 `32-rebuild-decisions-and-gaps.md` |
| `openspec/changes/` | 变更提案（8 个：1 个重建 + 3 个补齐基线 + 4 个扩展） | 待实施 |
| `docs/architecture.md` | 模块边界、依赖方向、数据所有权、IPC 契约 | 架构的唯一权威来源 |
| `design.md` | **应用前端设计入口**：布局、视觉、组件状态、交互与验收映射 | 区分原 App / 官网 App 展示、首版建议和待决策项 |
| `analysis/reports/30-core-ux-requirements.md` | **重做 UX 候选基线**：35 条需求、70 个验收场景 | 用于加入新想法、确定最终产品行为 |
| `analysis/reports/31-product-use-and-evidence.md` | **实际产品复测**：24 组记录、20 张界面截图、19 个资源逐字节校验 | 本轮事实与未验证边界 |
| `analysis/reports/32-rebuild-decisions-and-gaps.md` | **待决策与缺口**：产品规则、规范矛盾、独立架构债务 | 重做前需处理的差异 |
| `analysis/reports/33-website-ux-ui-review.md` | **官网 UX/UI 复核**：证据范围、设计对照、点击关系扩展与需求补充 | 官网声明、产品事实与自有决策分别标注 |
| `analysis/reports/00-technical-report.md` | **主报告**：技术栈选型、技术细节、设计 | 拆解结论与证据 |
| `analysis/reports/20-website-and-goal-linking.md` | **官网分析**：产品定位、方法论、目标链接演示的机制与边界 | 定位与演示参考，复核见 33 |
| `analysis/reports/40-frontend-ux-deconstruction.md` | **前端 UX 交互拆解**：产物切分、交互状态契约、拖拽/浮层/动效/可访问性 | 前端重建的对照材料 |
| `analysis/reports/hyperfocus-deconstruction.md` | 需求反推、领域模型、架构分层、关键决策 | 需求来源 |
| `analysis/reports/backend-internals.md` | 后端细节：agent 提示词、工具 schema、授权、语音、可观测 | 后端细节 |
| `analysis/reports/design-system.md` | 设计 token 全表 | 设计细节 |
| `analysis/evidence/` | 原始证据：数据库 schema、迁移史、还原出的前端、官网 DOM/CSS、UI 截图、符号表 | 未加工的原始材料 |
| `src-tauri/` `src/` | 可编译的工程骨架（Tauri 2 + React 19） | 仅骨架，未实现业务逻辑 |

### 材料的分工

- **要重建什么行为** → `openspec/specs/`（每条需求带可证伪的场景）
- **技术怎么选、怎么搭** → `00-technical-report.md` + `docs/architecture.md`
- **应用界面怎么设计和交互** → [design.md](design.md)；原始依据保留在 `40-frontend-ux-deconstruction.md` 与 `analysis/reports/design-system.md`

---

## 需求目录怎么用

### 结构

```
openspec/specs/
├── planning-cycles/        周期层级、生命周期、日历身份、Do Later、删除守卫
├── task-graph/             任务树、跨层链接、着色、清晰度标记、移动与度量
├── planner-workspace/      工作台布局与交互契约（列、空状态、侧栏、键盘、视觉约束）
├── agent-conversation/     会话模型、回合执行、技能激活、上下文注入
├── agent-proposals/        AI 写入的预览态、快照、Keep/Revert
├── goal-clarification/     目标澄清：breakdown 结构、缺失字段、清晰度推导、why→what→how
├── prioritization/         优先级分类：五桶结构、分类理由、增量合并
├── planning-issues/        计划健康检查与忽略
├── ai-access/              授权令牌、供应商选择、凭据存储、试用降级
├── voice-input/            语音采集格式、转写凭据、权限与错误
├── onboarding-guidance/    五步引导、行为驱动进度、一次性提示
├── app-lifecycle/          退出调查、反馈、自动更新、专注块通知
└── local-persistence/      本地数据源、版本化迁移、备份、密钥分离
```

每条需求是 `### Requirement:`，每个场景是 `#### Scenario:` + `WHEN/THEN`。场景即验收条件。

### 重建时的依赖顺序

规范之间存在真实的实现依赖，建议按下面顺序推进（也是 `rebuild-baseline` 的任务顺序）：

```
local-persistence ─┐
planning-cycles ───┼─→ task-graph ─→ planner-workspace
                   │                      │
                   └─→ agent-proposals ←──┘
                              │
              agent-conversation ─→ goal-clarification ─→ prioritization
                              │
                       planning-issues
                              │
                  ai-access ──┴── voice-input
                              │
                 onboarding-guidance / app-lifecycle
```

`planner-workspace` 依赖前三个能力提供的数据结构，但它的交互契约（空状态解剖、选择即预览后果、侧栏外壳）可以独立先做。

### 校验

```bash
openspec validate --specs      # 15 个能力
openspec list                  # 变更与任务进度
openspec status --change rebuild-baseline
```

---

## 变更提案与能力覆盖

每个能力都有对应的实现变更，可以按依赖顺序推进。

| 变更 | 内容 | 任务数 | 依赖 |
| --- | --- | --- | --- |
| `rebuild-baseline` | 从零实现骨架与本地数据层、本地调试日志（`skip_specs`；遥测章节已按产品决定替换为 `local-logging`） | 63 | — |
| `add-ai-access-and-voice` | BYOK 供应商（三种 API 格式：Anthropic Messages / Chat Completions / Responses）、钥匙串凭据、模型设置页（`skip_specs`；已并入原 `add-local-llm-provider`，语音已移出范围） | 37 | baseline |
| `add-ai-planning-core` | agent 回合与五个技能、工具集、GoalBreakdown、优先级、边写边审（`skip_specs`） | 62 | baseline + ai-access |
| `add-onboarding-and-lifecycle` | 五步引导、一次性提示、退出调查、反馈、自动更新、专注块通知（`skip_specs`） | 39 | baseline |
| `add-review-retrospective` | 周期复盘、未完成项去向、跨周期汇总、`review` 技能 | 33 | ai-planning-core |
| `add-calendar-time-view` | 日历网格、单日时间轴、跨日期移动、时间预算 | 27 | baseline |
| `add-reminders-notifications` | 任务/日/周期提醒、免打扰、启动补偿 | 30 | baseline |

**能力 → 实现变更**

| 能力 | 实现者 | 修改者 |
| --- | --- | --- |
| `planning-cycles` | rebuild-baseline | — |
| `task-graph` | rebuild-baseline | — |
| `session-repeats` | rebuild-baseline | — |
| `agent-proposals` | rebuild-baseline | — |
| `local-persistence` | rebuild-baseline | — |
| `planner-workspace` | rebuild-baseline | add-review-retrospective、add-calendar-time-view |
| `agent-conversation` | add-ai-planning-core | add-review-retrospective |
| `goal-clarification` | add-ai-planning-core | — |
| `prioritization` | add-ai-planning-core | — |
| `planning-issues` | add-ai-planning-core | — |
| `ai-access` | add-ai-access-and-voice（2026-09-14 规范重写：纯 BYOK，无订阅/试用/托管） | — |
| `voice-input` | —（已移出实现范围） | — |
| `onboarding-guidance` | add-onboarding-and-lifecycle | — |
| `app-lifecycle` | add-onboarding-and-lifecycle | add-reminders-notifications（专注块通知收敛） |

**建议顺序**：`rebuild-baseline` → `add-ai-access-and-voice` → `add-ai-planning-core` → 其余四个扩展可并行。

四个变更声明了 `skip_specs: true`——它们实现既有规范而不改变需求，因此不产生 delta 文件。其余变更修改既有需求，在 `specs/<capability>/spec.md` 给出**完整替换块**。

---

## 工程骨架现状

**已验证**：`cargo test`（312 个测试）与 `npm run build` / `npm run test`（67 个测试）全绿。

已实现（按变更）：

- `rebuild-baseline`（63/63）：本地数据层、双主题工作台前端、统一本地日志、编辑态、Do Later、待确认改动、专注块与重复日程
- `add-ai-access-and-voice`：BYOK 供应商配置（providers.json）、钥匙串凭据、三协议采样客户端、模型设置页
- `add-ai-planning-core`：LLM 抽象、回合工具循环、五个技能、工具集（写工具经预览层）、GoalBreakdown、优先级、计划审查（结构+语义）
- 其余四个扩展（复盘 / 日历视图 / 引导生命周期 / 提醒通知）按各自 tasks.md 推进中

```bash
```bash
npm install
npm run build                       # 前端类型检查 + 构建
cd src-tauri && cargo check          # 后端编译检查
npm run tauri dev                    # 启动应用
```

> 前端从 [design.md](design.md) 进入：白底/灰底双主题、统一窗口栏、横向周期列、Do Later、待确认底栏与设置（主题/日志/模型）均已落地。

---

## 需要注意的边界

- **原应用闭源**。`analysis/evidence/web/` 里的前端文件是从其二进制中还原出来的，**属于原作者版权作品**，仅作为理解行为与视觉语言的证据保留，不得复制进本项目的实现。`src/` 下的前端必须是原创。
- 视觉与交互可以对齐拆解出的设计原则，但不要逐像素复刻其样式表。

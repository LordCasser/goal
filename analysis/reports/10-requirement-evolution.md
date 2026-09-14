# 需求演进史：从 57 条迁移反推真实需求

> 证据：`analysis/evidence/db/migrations-sql.txt`（从二进制提取的全部迁移 SQL **原文**，含开发者注释）
> 方法：迁移是需求变更的**执行痕迹**。与规范不同，它不撒谎——被删掉的列说明需求死了，被反复重建的表说明需求没想清楚。
> 价值：这是最深一层反向需求提取。不是"它要做什么"，而是"它试过什么、为什么放弃、最后收敛成什么"。

---

## 0x00 最重要的发现：这是一次重写，不是初版

第一条迁移的注释直接写着：

```sql
-- Initial schema migration matching the existing Drizzle schema
```

第二条：

```sql
-- Insert Month Tasks (exact replication from generateOnboardingData.ts lines 23-52)
```

**所以 hyperfocus 原本是一个 TypeScript + Drizzle(ORM) 的应用，后来被重写成 Rust + Tauri。** 这解释了：

- 为什么 `_sqlx_migrations` 是 sqlx 的、但 `0001` 的注释在说 Drizzle
- 为什么 `tasks_table` 有个 `migrated_from_task_id` 列（迁移 19 才删）
- 为什么 `focused_time` 一开始是 `REAL`（秒），后来花了一整条迁移（M6）改成毫秒整数——TS 侧用浮点秒，Rust 侧统一毫秒
- 为什么 `is_refined` 是 `INTEGER 0/1` 而不是 `BOOLEAN`——TS 侧没有布尔类型

**对重建的启示**：这份 schema 不是从零设计的，它背着一次跨语言重写的包袱。有些别扭之处（如 `INTEGER` 当布尔、`tasks` 与 `cycles` 同构）是历史原因，不是设计意图。我们自己的实现应当直接用原生类型，不需要兼容这段历史。

---

## 0x01 试过又放弃的三个功能

这一节是这份报告最独特的价值——这些功能在**最终版本的界面和规范里完全看不到**，只有迁移史记得它们。

### ① PMF 问卷（M1 建表 → M3 删除）

```sql
CREATE TABLE checkins_table (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    pmf TEXT CHECK (pmf IN ('very_disappointed', 'somewhat_disappointed', 'not_disappointed')),
    created_at INTEGER NOT NULL DEFAULT (unixepoch() * 1000)
);
-- M3: DROP TABLE IF EXISTS checkins_table;
```

这是 **Sean Ellis 的 PMF 测试**（"如果这个产品明天消失了，你会多失望？"）。作者把它做进了应用内，两三条迁移之后就删了。

**反推出的需求**：「验证产品是否值得继续做」。后来这个需求被移出产品本身，改成官网 FAQ 和退出调查——**自测工具不该占用用户界面**。

### ② 「计划与现实吻合度」评分（M1 有列 → M4 删除）

```sql
plan_reality_rating INTEGER,   -- periods_table 上的列
-- M4: ALTER TABLE periods_table DROP COLUMN plan_reality_rating;
```

**反推出的需求**：「让用户回顾计划是否靠谱」。被删掉后，直到 0.15.0 都**没有替代实现**——这正是我们扩展方向里「回顾与复盘」要补的洞。产品承诺了 `Review results`，但机制被删掉后没重建。

### ③ 「和 Pro 选手对赌」（M5/M7/M9 建 → M16 部分删 → M48 全删）

这是被砍掉的最大一块功能。完整还原：

```sql
-- M5
-- Add pro_id column to periods_table
-- This allows week periods to store a reference to their selected Pro benchmark
ALTER TABLE periods_table ADD COLUMN pro_id TEXT;

-- M7
-- Add scores table for tracking daily strategy and execution scores
-- Each day period gets score rows (one per score_type: strategy, execution)
CREATE TABLE scores_table (
  id TEXT PRIMARY KEY,
  period_id TEXT NOT NULL REFERENCES periods_table(id),
  score_type TEXT NOT NULL CHECK(score_type IN ('strategy', 'execution')),
  player_score INTEGER NOT NULL,
  pro_score INTEGER NOT NULL,
  pro_id TEXT NOT NULL,
  player_won BOOLEAN NOT NULL,  -- True if player_score >= pro_score
  score_data TEXT NOT NULL,     -- JSON: ScoreData enum (Strategy or Execution)
  UNIQUE(period_id, score_type)
);

-- M9：Pro 一开始是具名的人
UPDATE periods_table SET pro_id = CASE pro_id
  WHEN 'sarah_chen' THEN '1'
  WHEN 'david_lee' THEN '2'
  WHEN 'marcus_rivera' THEN '3'
  WHEN 'isabella_rossi' THEN '4'
  ...
```

**还原出的玩法**：用户规划自己的周/日计划，系统拿「一个具名专家的计划」做对照，给出**策略分**与**执行分**两个维度，判定 `player_won`。这是一种把规划行为游戏化的设计——像和高手下棋一样学规划。

**它为什么死了**（从演进痕迹推断）：

1. **从具名人改成数字 id**（M9）——维护一组「人格化的专家」成本高，且具名会带来真实人物的一致性问题
2. **先摘掉对手，再摘掉分数**（M16 删 `pro_id`/`pro_score`/`player_won`，留下 `score_type`/`player_score`）——先放弃"和谁比"，再放弃"打分"
3. **M48 整表删除**——连自己的分数也不要了

**留下的教训，直接体现在最终产品里**：0.15.0 不再给用户打分，而是**指出具体问题**（`planning_issue_dismissals` 的六类问题）。同一个需求（"我的计划好不好"）从**评分**转向了**可执行的诊断**。

这条判断对我们的扩展很重要：**不要给用户打分，要告诉他哪里不清楚。** 打分是这条路上被验证失败的方案。

---

## 0x02 被重设计六次的「目标清晰度」

这是全项目改动最密集的概念。完整时间线：

| 迁移 | 结构 | 设计意图（注释原文） |
| --- | --- | --- |
| M8 | `clarity_level` INTEGER 0-100 + `clarity_breakdown` JSON | `JSON: SMARTBreakdown structure` |
| M10 | 加 `parent_id` | `supports the new clarity evaluation system with parent goal linking` |
| M12 | 结构改为 `clarification_needed` | `from actionable/achievable to clarification_needed` |
| M13 | **GoalEvaluation** 8 维 | `{atomicity, verifiability, agency, orientation, ambiguity, primary_issue, feedback, parent_id}` → `{goal_singularity, result_verifiability, amount_of_control, result_type, execution_plan_clarity, primary_issue, feedback, parent_id}` |
| M14 | 问题类型细粒度化 | `from ambiguous issue types ("result_verifiability") to granular, self-describing variants ("result_verifiability_none", "result_verifiability_subjective")` |
| M15 | `execution_plan_clarity` → `structural_type` | `for improved LLM evaluation reliability`（task/project） |
| M17 | 换成 `goal_breakdown` 列 | `single source of truth for both clarity calculation and Copilot conversations. The cached breakdown eliminates redundant AI calls during Copilot startup` |
| M21 | 删 `clarity_breakdown` | `replaced by goal_breakdown` |
| M49–M51 | 收敛为两个布尔 | `needs_refinement` / `needs_breakdown`，删掉 `clarity_level` 与 `is_refined` |

**读法**：六个阶段对应三次认知跃迁。

1. **从「打分」到「列维度」**（M8 SMART → M13 GoalEvaluation 8 维）：一开始想给 0-100 分，发现分数不可执行，改成列出具体的评估维度。
2. **从「抽象维度」到「自描述问题」**（M14）：`result_verifiability` 这种抽象标签模型不稳定，改成 `result_verifiability_none` 这种**自带答案的问题**。这条注释是关键——它说明了为什么：模型对抽象标签的判断不可靠。
3. **从「评估」到「结构化提取」**（M15/M17）：`execution_plan_clarity` 有 `ambiguous/needs_discovery/actionable` 三态，模型判不准；换成 `structural_type`（是任务还是项目）这种客观二分。最终 M17 转向 `goal_breakdown`——**不再让模型判断"清不清晰"，而是让模型提取结构，清晰度由结构算出来**。

最终形态（M49）：`needs_refinement` + `needs_breakdown` 两个布尔。

**这条演进链是整个产品最有价值的设计经验**：

> 不要让模型做「评价」，让模型做「提取」，评价交给确定性代码。

我们的 `goal-clarification` 规范已经体现了这一点（`GoalBreakdown` → `missing_fields` → 两个布尔），但**规范里没写这条原则的由来**。这是从失败中得来的，值得在报告里显式记录。

**另一条附带发现**：M17 的注释里出现了 **"Copilot"** ——agent 最初叫 Copilot，后来才改名。同时 `clarity_level` 时代就有 `feedback` 字段，说明 AI 反馈从一开始就是这套设计的一部分。

---

## 0x03 提议机制的收敛：从修订号到两个值

「AI 写的东西要能被撤销」这条需求的实现演进：

| 迁移 | 机制 |
| --- | --- |
| M28 | `preview_state IN ('created','updated','deleted')` + `last_modified_revision` + `content_revision`（周期级） |
| M29 | 改为 `is_preview BOOLEAN` + `deleted_at`（墓碑） |
| M30/M31 | 删 `preview_state`、删全部 revision 元数据 |
| M33 | 改为 `agent_proposal IN ('upsert','delete')` |

M33 的注释解释了最后一步的取舍：

```sql
-- Committed tombstones are a legacy storage detail, not unresolved proposals.
-- Clear any conversation pointers before we physically remove those rows.
```

意思是：**墓碑（tombstone）把两件事混在了一起**——「已提交的删除」是存储细节，「未处理的提议」是用户待办。两者用同一个 `deleted_at` 表达，导致查询和语义都混乱。拆开后：

- 已确认的删除 → 物理删除该行
- 未确认的提议 → `agent_proposal` 非空

**反推出的原则**：**区分「状态」和「待办」**。修订号（revision）也一并删掉了，说明乐观并发控制在这个单用户本地应用里没有实际需求——加了复杂度但没有收益。

对照我们的 `agent-proposals` 规范：已经写成了 `agent_proposal` + `task_preview_originals` 的形态，方向一致。但规范里没有「为什么不保留修订号」的依据，重建时可能有人"顺手加回来"。

---

## 0x04 Later 收纳桶：从三层到一层

```sql
-- M23: 建三个
('later-month', 'Later Month', 'month'), ('later-week', 'Later Week', 'week'), ('later-day', 'Later Day', 'day')

-- M25: 合并为一
-- Collapse three legacy Later buckets (later-month, later-week, later-day)
-- into a single unified `later` period.
--    Merge order: later-month tasks first, then later-week, then later-day.
```

M25 的合并用了窗口函数保证顺序：

```sql
ROW_NUMBER() OVER (
    ORDER BY CASE t2.period_id
        WHEN 'later-month' THEN 0
        WHEN 'later-week'  THEN 1
        WHEN 'later-day'   THEN 2
    END, t2.position ASC, t2.rowid ASC
) - 1
```

**反推出的需求**：一开始认为「收纳」也要分层（长期收纳 vs 本周等会做 vs 今天等会做），后来发现**用户不区分这个**——收纳就是收纳，分三层只是增加决策成本。合并时按「层级从高到低」排序，保留了原有的优先级直觉。

**对我们的启示**：我们的 `planning-cycles` 规范已经写成单一 `later` 容器（正确），但值得记住这层设计是删繁就简的结果，不要"为了完整"再加回分层。

---

## 0x05 「当前周期」：从物化标记到派生

```sql
-- M22: 加标记列 + 唯一索引
ALTER TABLE periods_table ADD COLUMN active BOOLEAN NOT NULL DEFAULT 0;
-- Enforce uniqueness: at most one active period per type (excluding sessions)
CREATE UNIQUE INDEX idx_unique_active_period
ON periods_table(type) WHERE active = 1 AND type != 'session';

-- 回填用了三段自连接 SQL，按 created_at DESC LIMIT 1 逐层找出当前周期

-- M54: 整个删掉
DROP INDEX IF EXISTS idx_unique_active_cycle;
ALTER TABLE cycles_table DROP COLUMN active;
```

**反推出的需求**：「哪一个是当前周期」。方案 A 是物化成 `active` 列（带唯一索引保证每层只有一个）；方案 B 是查询时派生（`ORDER BY created_at DESC LIMIT 1`）。

最终选了 B。M22 的回填 SQL 本身就是证据——**要找出"当前"周期，用的逻辑就是 `ORDER BY created_at DESC LIMIT 1`**，那存一个标记列只是把这个查询结果缓存下来，还带来"什么时候更新标记"的一致性问题。

**对我们的启示**：`planning-cycles` 规范里不要引入 `active` 列。当前周期由查询派生。这条我需要在架构文档里显式写明，否则实现时很容易顺手加一个 `is_active`。

---

## 0x06 Agent 技能：从 3 个到 4 个，以及一次数据迁移

```sql
-- M37: 最初三个
active_skill IN ('goal_setting', 'planning', 'prioritization')

-- M46: 拆成四个，并做了数据迁移
active_skill IN ('goal_setting', 'long_term_planning', 'short_term_planning', 'prioritization')
...
CASE WHEN active_skill = 'planning' THEN NULL ELSE active_skill END
```

**反推出的需求**：`planning` 这个技能粒度太粗——长期规划和短期规划需要的提问方式、时间尺度、上下文完全不同，一个提示词覆盖不了。拆成两个。

**注意那次数据迁移的写法**：`planning` → `NULL`（回退到"无技能"）而不是随便映射到其中一个。这是**宁可让用户重来，也不给一个可能错的技能**的选择。

另有一个更激进的信号：M52 与 M53 **两次清空了全部会话数据**：

```sql
DELETE FROM agent_messages;
DELETE FROM agent_conversations;
```

为了改 schema 直接丢掉所有历史对话。这说明**在 launch 之前，agent 数据被当作可丢弃的**——它是缓存，不是用户资产。

**对我们的启示**：`active_skill` 的最终四值是正确目标；但"agent 会话是否属于用户资产"这个问题，上游的答案是"不是"。我们的规范把它当持久数据（`local-persistence` 要求迁移不丢数据），**这与上游态度不同**——这是一个需要显式决策的点，不能默默继承。

---

## 0x07 反复出现的「数据保全纪律」

这是迁移史里最容易被忽略、但对重建最有约束力的部分。作者在删除/清理时有一套固定做法：

**① 用完整签名匹配，绝不模糊删除**（M24）

```sql
-- Match full seed signatures so we don't delete user-created tasks that
-- happen to share onboarding titles/periods, and we keep seeded rows users modified.
DELETE FROM tasks_table
WHERE (id = '...' AND period_id = '...' AND title = '...') OR ...
```

三条条件（id + 所属周期 + 标题）同时匹配才删。理由是明确的：**用户可能创建了同名任务**。

**② 用守卫表保证"只在全新安装上执行"**（M36）

```sql
CREATE TEMP TABLE cleanup_untouched_onboarding_children_guard AS
SELECT 1 AS can_cleanup
WHERE EXISTS (... started = 0 AND finished = 0 AND active = 1 ...)
  AND NOT EXISTS (... 用户加了别的子周期 ...);
-- 每条 DELETE 都带 AND EXISTS (SELECT 1 FROM ..._guard)
```

只有"所有引导数据都还没被碰过"时才清理。

**③ 重建表时显式三步**（M34/M41/M46/M53）

```sql
-- no-transaction
PRAGMA foreign_keys = OFF;
CREATE TABLE x_new (...);
INSERT INTO x_new SELECT ... FROM x;
DROP TABLE x; ALTER TABLE x_new RENAME TO x;
PRAGMA foreign_key_check;
PRAGMA foreign_keys = ON;
```

`-- no-transaction` 与 `foreign_key_check` 是固定搭配，说明作者知道 SQLite 在重建表时外键会出问题。

**④ 物理删除前先解引用**（M33）

```sql
-- Clear any conversation pointers before we physically remove those rows.
UPDATE agent_conversations SET task_id = NULL WHERE task_id IN (...);
DELETE FROM tasks_table WHERE ...;
```

**对我们的启示**：`local-persistence` 规范里写了「迁移失败可重试」，但**没有写这些数据保全纪律**。重建时如果按"删掉旧数据就行"的直觉做迁移，会违反上游已经踩过的坑。这需要补进规范。

---

## 0x08 引导数据的完整演进

onboarding 种子数据被反复调整，暴露了「新用户第一次看到什么」的迭代：

| 迁移 | 改动 |
| --- | --- |
| M2 | 建立：Month 1 (30 天) / Week 1 / Day 1 (20 小时) / Morning focus (90min) / Afternoon cleanup (45min) |
| M11/M20 | 把引导任务标为「最高清晰度」「已精炼」 |
| M24 | 删除未被动过的引导任务 |
| M27 | Day 从 20 小时 → 24 小时 |
| M35 | Month 从 30 天 → 28 天 |
| M36 | 删掉未被碰过的子周期，只留月周期 |
| M39 | 改名：`Long-term`(12 周!) / `Short-term` / `Today` |
| M44 | **连月周期也删掉** —— 全新安装从空开始 |

**读出的两条需求变化**：

1. **文案从"教学式"转向"命名式"**。早期种子数据在教操作：
   - `Type your first goal here...`
   - `Use [Enter] to create new tasks`
   - `Break them into smaller tasks using [Tab] and [Shift+Tab]`
   
   后期（M39）只给周期起名 `Long-term` / `Short-term` / `Today`，并把它们的时长设成真实值（12 周 / 7 天 / 24 小时）。
   
   **反推**：新手教学从「塞示例数据」改成「引导清单 + 空状态自解释」。这与我们在 `onboarding-guidance` 规范里写的五步清单一致——**上游已经试过"预置数据"这条路，放弃了**。

2. **从"给一个已规划好的示例"到"给一个空的工作台"**（M44）。预置数据会被误当成自己的数据，且难以清理干净（看 M24/M36/M44 三条迁移都在收拾它）。

**顺带确认了时长语义**：M39 把长周期设成 `7257600000 -- 12 weeks in milliseconds`。这与官网演示的 "12 weeks left" 完全一致，坐实了「3 个月 = 12 周 = 84 天」而非日历月。

---

## 0x09 完整时间线（按产品阶段重排）

把 57 条迁移按认知阶段归组，得到这个产品的真实开发史：

```
阶段 0  重写         TS/Drizzle → Rust/Tauri，1:1 复刻 schema        M1–M2
阶段 1  验证         PMF 问卷 + 计划吻合度评分                       M3–M4
阶段 2  游戏化       和具名 Pro 对照打分（策略分/执行分）            M5–M7, M9
阶段 3  清晰度探索   SMART → GoalEvaluation 8维 → 细粒度问题        M8, M10–M15
阶段 4  放弃游戏化   摘掉 Pro、摘掉分数、删表                        M16, M48
阶段 5  转向原则     提取优先于评价：goal_breakdown 成为真相源      M17–M21
阶段 6  简化当前     尝试物化 active，后改回派生                     M22, M54
阶段 7  收纳         Later 三层 → 一层                              M23, M25
阶段 8  提议机制     修订号 → 墓碑 → 两值 proposal                  M28–M33
阶段 9  Agent        Copilot 会话、3 技能 → 4 技能、推倒重建两次    M32–M37, M46, M52–M53
阶段 10 数据保全     用签名匹配 + 守卫表做清理                       M24, M36, M44
阶段 11 概念改名     period → cycle                                  M40
阶段 12 日历身份     starts_on / ends_on / calendar_key              M47
阶段 13 最终收敛     两个布尔 flag 取代全部清晰度机制                 M49–M51
阶段 14 计划诊断     用具体问题取代评分                              M56
阶段 15 重复日程     摘掉模板里的任务清单，只留标题+时长              M38, M55
```

**最后一行的含义值得单独说**：M38 给 `repeats_table` 加了 `tasks` 列（模板可以带整套任务清单），M55 又把它删了。**同一件事，上线前后各做了一次相反的判断。**

反推：模板带任务清单看起来更强（"我的晨间专注固定做这三件事"），但实践中可能是——子任务的完成状态在重复时如何结转无法自洽，不如让模板只管时长与标题这个最小骨架。这是「能力做减法」的又一个实例。

---

## 0x0A 对重建的直接约束

从这份演进史能提炼出**不该重犯的错**和**不该丢的原则**：

### 不要重犯

| 教训 | 依据 | 约束 |
| --- | --- | --- |
| 不要给用户打分 | Pro/scores 功能被整体删除 | 计划质量用「具体问题」表达，不用分数 |
| 不要把「状态」和「待办」混在一个字段 | M33 的墓碑混乱 | 提议态与已提交的删除必须分开 |
| 不要引入修订号做并发控制 | M30/M31 删掉全部 revision | 单用户本地应用不需要乐观并发 |
| 不要物化「当前周期」 | M22 → M54 | 当前周期由查询派生，不加 `active` 列 |
| 不要给收纳分层 | M23 → M25 | `later` 只有一个 |
| 不要让模型做「评价」 | M15/M17 的注释 | 模型提取结构，清晰度由代码算 |
| 不要预置引导数据 | M24/M36/M44 三条迁移在收拾它 | 空工作台 + 引导清单 |
| 不要在模板里塞任务清单 | M38 → M55 | 模板只管标题与时长 |

### 不要丢

| 原则 | 依据 | 要求 |
| --- | --- | --- |
| 迁移必须保全数据 | M24/M33/M36 的注释 | 清理用完整签名匹配；重建表用 `foreign_keys=OFF` + `foreign_key_check` |
| 时长按整周计算 | M35/M39 与官网 "12 weeks" | 1/3/6 个月 = 28/84/168 天 |
| 宁可回退也不要错的状态 | M46 把 `planning` 映射为 `NULL` | 无法确定归属时清空而非猜测 |
| 明确的引导可跳过 | M39/M44 | 引导不预置数据，且可跳过 |

### 需要显式决策的分歧点

**「agent 会话是不是用户资产？」** 上游两次直接 `DELETE FROM agent_conversations` 清空全部历史，说明它被当作可丢弃的缓存。但我们的 `local-persistence` 规范要求迁移不丢数据。

这两者不矛盾（规范约束的是**我们**的迁移，不是上游的行为），但如果不在架构里写清楚，实现时会出现"要不要为会话做数据迁移"的犹豫。**我的判断**：会话对话有用户可见的价值（它记录了"我当时为什么这么定"），应当按用户资产对待，迁移时保全。这是**有意偏离上游**，理由是我们的目标用户会把 AI 对话当决策记录读。

---

## 附：证据索引

| 内容 | 位置 |
| --- | --- |
| 全部 57 条迁移 SQL（含注释，56 KB） | `analysis/evidence/db/migrations-sql.txt` |
| 迁移版本与描述速查 | `analysis/evidence/db/migrations.txt` |
| 最终 schema（含触发器） | `analysis/evidence/db/schema.sql` |
| 提取脚本 | `analysis/scripts/extract_migrations.py` |

复现：挂载 DMG 后运行 `python3 analysis/scripts/extract_migrations.py`。脚本按「描述串后面紧跟 SQL」的规律定位，57/57 全部命中。

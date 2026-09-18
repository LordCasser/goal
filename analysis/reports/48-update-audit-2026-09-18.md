# 2026-09-18 更新审计

今天的改动意图成立。初审发现两处未闭环：日任务直接关联长期目标后，日历目标上下文漏展示；复制后的空输入行位置修复只覆盖了空行先存在的顺序。两项已在后续实现中修复，连线与定位的修复记录见文末。原有 A05 复制子树问题已专项复现，仍单独作为既有债务处理。

审计基线是 `c3a7ba1`，范围为 2026-09-18 工作区中 20 个已跟踪文件的差异，以及新增的 `RelationLayer.test.tsx`。当天没有新的 Git 提交；已跟踪改动的文件修改时间均为当天。意图依据当前差异、规范和代码注释判断，没有推定未写入这些材料的额外需求。初审阶段仅新增这份报告，随后按用户要求继续实现。

**意图与架构**

任务属于哪个日期，与它服务哪个目标，是两种不同的关系。允许日任务直接关联长期目标，可以复用现有 `parent_id`，同时让任务继续处于日计划顶层；无需为了满足层级结构凭空建立周任务。后端链接校验、前端菜单与拖动、颜色继承、Work mix、Later 提升以及 AI 组织任务动作的执行入口，已能接受这条直接关系。

关联线改为读取当前 DOM，并响应异步挂载、节点替换、滚动和尺寸变化；保持单段曲线和横向裁剪，与这次规范一致。在已检查的代码和组件测试范围内，没有发现这部分新增的确定性错误。实际 Tauri WebView 的滚动与动画表现仍未手工验证。

空行尾部位置则需要由“当前列表的根任务范围”决定。跨周期关联项虽然 `parent_id` 非空，在当前周期仍是根。只在复制函数结束时挪动已有空行，不能覆盖后续补行。

**F1 · P2 · 日历遗漏日任务直接关联的长期目标**

位置：[CalendarPlan.tsx:26](/Users/lordcasser/workspace/projects/goal/src/features/calendar/CalendarPlan.tsx:26)。本次扩展了合法关系，但该消费者仍只从 `weeklyTasks` 向上收集 `longTermGoals`。

复现：长期计划中有目标 `Direct monthly goal`；当周任务为空；当天有任务 `Direct daily work`，直接关联该长期目标。打开 Calendar 的 Plan 面板后，日任务正常出现，并继承绿色，但长期目标区域实际显示：

```text
Long-term goalsNo long-term goals linked. Weekly and daily tasks can stay independent.
```

组件测试确认批量查询包含 `day/month/week`，且日任务已继承目标颜色，所以目标数据和关系图均已加载。要求该区域包含 `Direct monthly goal` 的断言失败。若当天直接关联的目标同时被周任务关联，问题会被周任务路径掩盖。

处理边界：在现有日历投影中，把当天任务的实际父链纳入长期目标收集并按 ID 去重，保留原有周目标上下文；补“无周任务、日任务直接关联长期目标”的组件回归。无需增加关系表或缓存。

**F2 · P2 · 空行后创建时，今天的位置修复仍失效**

位置：[cycles.rs:767](/Users/lordcasser/workspace/projects/goal/src-tauri/src/service/cycles.rs:767)。这里只收集复制前已经存在的空行，后续循环也只调整这一集合。

复现使用公共 service API：前一天有 `position=10`、直接关联长期目标的日任务；目标日为空。先调用 `copy_uncompleted_from_previous`，再模拟编辑器首次打开，通过 `add_task` 创建 `title=""`、未指定位置和父项的空行。实际根列表为：

```text
after_copy = [Daily task(position=10, parent=goal_id)]
editor_roots = [<empty>(position=0), Daily task(position=10, parent=goal_id)]
```

这个顺序可由 Coach 对尚未打开的日期执行已确认的复制动作，再打开目标日期触发补行得到：[AI 复制入口](/Users/lordcasser/workspace/projects/goal/src-tauri/src/ai/actions.rs:1134)、[编辑器补行](/Users/lordcasser/workspace/projects/goal/src/features/planner/TaskList.tsx:139)。

根因是 [add_task 默认位置](/Users/lordcasser/workspace/projects/goal/src-tauri/src/service/tasks.rs:127) 仍调用 [max_position](/Users/lordcasser/workspace/projects/goal/src-tauri/src/repository/tasks.rs:166)，其 `parent_id IS NULL` 查询漏掉了跨周期关联的可见根任务。该位置算法与 HEAD 相同，因此这里属于本次修复遗漏的边界，不是今天新引入的位置回归。

处理边界：让顶层追加位置包含跨周期关联的根任务，同周期子步骤仍按父项分组；保留“先补行再复制”和“先复制再补行”两种顺序的回归。不要把后面的子树复制债务混入此项。

**A05 · P2 · 已确认的既有复制子树债务**

原记录：[32-rebuild-decisions-and-gaps.md:108](/Users/lordcasser/workspace/projects/goal/analysis/reports/32-rebuild-decisions-and-gaps.md:108)。本轮把“静态怀疑、尚未专项复测”推进为已复现，原记录未改写。

在源周期建立父任务 `position=10`、子任务 `position=0`。复制后两个任务均为根，子任务失去复制后应有的父关系。源数据保持不变。

原因是 repository 按全周期 `position` 排序，而 [复制循环](/Users/lordcasser/workspace/projects/goal/src-tauri/src/service/cycles.rs:783) 仅在创建完任务后才填入 ID 映射。子项先出现时，代码把“父项尚未遍历”当成“父项没有复制”，将子项提升为根。与 `git show HEAD:src-tauri/src/service/cycles.rs` 对照，核心循环未被今天的改动修改。

单独处理：先确定完整复制范围与 ID 映射，再按父先子后的顺序插入；区分父项未复制、父项待复制和跨周期外部父项。验收需覆盖乱序子树、未复制父项以及外部父关系。此项不作为今天引入的回归。

**验证记录**

| 检查 | 结果 |
| --- | --- |
| `npm exec vitest run src/features/planner` | 15 个文件、146 个测试通过 |
| `npm exec tsc -- --noEmit` | 通过，无类型诊断 |
| `cargo test --test services`（`src-tauri` 目录） | 43 个通过 |
| `cargo test --test later`（`src-tauri` 目录） | 12 个通过 |
| `git diff --check` | 通过 |
| F1 临时 CalendarView 组件复现 | 目标应显示的断言失败；数据加载、日任务和继承色断言通过 |
| F2 临时 service 复现 | 确认空行排在复制任务前 |
| A05 临时 service 复现 | 确认子任务复制成根 |

三个专项复现文件均已删除。Rust 编译存在 4 个既有 warning，无编译错误。没有运行完整 Rust/前端测试矩阵、生产构建或真实 WebView 端到端测试。Atlas 在部分查询中返回旧缓存位置或仅有局部覆盖，因此最终位置与行为证据均以当前源码和测试为准。

**后续实现与复验 · 2026-09-18**

F1 已修复：日历从当周和当天任务的父链共同收集长期目标，并按 ID 去重。无周任务、日任务直接关联长期目标，以及两条路径命中同一目标，均有组件回归。

F2 已修复：`add_task` 在默认追加顶层行时，查询当前周期的可见根任务最大位置，包含跨周期关联项，排除同周期子步骤。新增回归覆盖先复制关联任务、再补空行的顺序；原有先有空行再复制的行为保留。A05 未混入本次修复。

连线专项检查确认三个不同边界：日事项直连长期目标的普通曲线会经过周计划正文；端点纵向越界会使整条线消失；关联栏切换长期周期后立即查询 DOM，目标行尚未挂载时定位丢失。

最终交互按用户反馈及网页对照复验确定：日事项直连长期目标使用整条虚线，在周计划非空正文的左右边界附近以宽 S 弯平滑过渡，不在全跨度中点制造隆起；根据可见正文选择较近的上方或下方。允许部分侵入正文，绕行距离过大时保留直接曲线，空周列不强制绕行。相邻层级保持实线。局部文字遮罩和文字测量试验已移除，没有新增关系实体或绘图库。横向仍自然裁剪，纵向越界改为列表边缘方向标记；目标名称与定位入口保留在关联栏。定位复用编辑器的挂载后 reveal 流程，本地与 Issues 请求分别去重。

| 后续检查 | 结果 |
| --- | --- |
| `npm exec vitest run src/features/planner src/features/calendar` | 18 个文件、211 个测试通过 |
| `RelationLayer.test.tsx` | 14 个测试通过，包含上绕、下绕、空周列、长正文直连、屏外端点和节点替换 |
| `npm run build` | 类型检查及生产构建通过；保留现有大 chunk 提示 |
| `cargo test --test services` | 44 个通过 |
| `cargo test --test later` | 12 个通过 |
| `cargo test reorder_tasks` | 3 个通过 |
| 浏览器真实组件布局，内存 IPC 测试数据 | 平滑曲线、横向裁剪、纵向边缘标记、延迟加载后的跨周期滚动定位均已验证 |

浏览器验证使用真实 `PlannerWorkspace` 与项目样式，没有连接用户数据库；不等同于原生 Tauri WebView 端到端测试。此前临时预览文件已清理；本轮形态对照与真实组件试验保存在被 Git 忽略的 `.artifacts/relation-lab/`，仅含隔离数据。Luna 通过 Computer Use 切换了同高、高差、长正文、空周计划和横向偏移场景，推荐正文边界过渡方案；整条虚线规则由用户最终确定。

整条虚线版本复验：14 个 RelationLayer 测试和类型检查通过，Tauri macOS App 重新构建并通过本地签名校验。Luna 用 Computer Use 复核真实组件页的同高、横向滚动和灰白主题；主 agent 在新 App 的原生 WebView 中确认日事项直连长期目标显示为连续的整条虚线。没有修改任务数据。

**日事项悬浮提示与连线设置 · 2026-09-18**

用户随后确认，小屏无法同时看到日计划与长期计划面板，连线改为默认关闭的可选项。新增 `app_settings.show_relation_lines` 布尔偏好、专用设置命令及通用设置入口，保存失败回滚；现有虚线几何只在用户开启后挂载。没有更改实际关联数据。

按 UI UX Pro Max 的轻量交互建议，日事项整行悬停或键盘聚焦时，使用现有主题、字体、目标色槽与轻阴影显示长期目标卡片。沿真实父链解析直接关联、周事项关联及日子步骤继承，返回实际关联的长期目标条目；不依赖长期面板是否可见。360ms 悬停延迟、180ms 淡入与轻微位移、离开延迟及120ms淡出；卡片自动翻转、约束视口并允许长内容滚动。编辑、菜单、拖拽、工作台滚动、失焦与退场时收起；行内焦点控件具有关联描述，不改变编辑焦点。

验证：19 个相关前端文件共 199 项检查通过，包含真实日事项行的继承目标与焦点描述回归；类型检查通过。Rust设置重开持久化和桌面命令注册2项通过。最终 Tauri macOS测试 App 构建及本地 ad hoc 签名校验通过，保留4个既有Rust警告和既有大chunk提示。

Luna通过Computer Use验视内存IPC真实组件页：直接关联、经由周事项、无关联、长标题、横向滚动、灰白主题与设置开关正常。主agent也在1280×720页面确认长期面板位于屏外时浮层位置与层次正常。网页交互用键盘焦点实测，鼠标悬停生命周期由组件事件测试验证；当前CUA接口未提供hover能力，未将键盘验视宣称为实际鼠标悬停验视。本轮未对用户任务数据做测试写入，也未重新进行原生WebView交互验收。

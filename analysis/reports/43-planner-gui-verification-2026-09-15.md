# Planner UX GUI 验收记录（2026-09-15）

测试对象：`/tmp/Planner UX.app`（`dev.lordcasser.planner.uxaudit`，`localhost:1420`）。只使用 CUA 读取 AX 树、点击、键盘、滚动和截图；未操作正式 Planner 或原版 Hyperfocus，未调用外部 AI，也未填写 API Key。

## 数据与归属

- 新增长期目标 `Design a calm weekly rhythm`，设置颜色为 green。
- 周任务 `Validate the first prototype` 依次切换到新长期目标、解除关联、恢复关联。每一步均以 AX 的 `Help: Linked to ...`、Connections 文案和视觉连线复核；恢复后连线和绿色继承均存在。
- 新建日任务 `Run usability pass`。关联菜单当时把 `Link to Validate the first prototype` 置于选中位置；提交后以任务按钮的 `Help: Linked to Validate the first prototype`、Connections 文案和连线确认归属。菜单候选获得焦点或显示 selected 本身不作为已保存关联的依据。
- 日任务完成复选框由 `Value: 0` 切为 `Value: 1`，任务标题显示删除线；再次点击恢复为 `Value: 0`。切换 Calendar 再回 Workspace 后任务、归属和未完成态仍保留。

## 连接面板与布局

点击日任务条目后，底部 Connections 面板常驻。点击 `← Validate the first prototype` 后，AX 面板切换到周任务并同时显示 `→ Run usability pass`；再点击右向关联按钮，截图确认视图从 Long-term/Weeks 滚动定位到 Week 38/Days，日任务和周任务连线可见。按 Escape 后 Connections 容器消失，视图保持在 Week/Day 位置。

## Workspace、Calendar、Later、Coach、Issues

- Workspace 展示长期、周、日三种计划面及各自时间导航；Calendar 进入独立的 Month/Week 日历视图。Calendar Month 显示 `2026-09-01 – 2026-09-30`，Week 显示 `2026-09-14 – 2026-09-20`，两者 AX 结构与 Workspace 不同。
- Later 打开 `Do Later` 面板（`Value: on`），可见 `LATER`、`PARK NOW, PLAN LATER` 和新建停放目标输入框；Escape 关闭并恢复 `Value: off`。截图中面板约占左侧 406 px，Workspace 卡片向右移，关闭后布局恢复。
- HMR 更新前，Coach 与 Issues 打开后按 Escape 均未关闭，尽管 AX Help 写明 `Close (Esc)`；点击顶部按钮可以关闭。
- HMR 更新后已做回归：Coach 打开后按 Escape，第二次 AX 读取不再包含 Coach 容器；Issues 同样由 expanded 变回 collapsed，第二次 AX 读取不再包含 Plan issues 容器。面板关闭行为在当前回归中通过。

## Settings 与 AI

- Settings 通用页显示当前主题为灰底（灰底 `Value: on`，白底 `Value: off`）。
- AI 页显示没有已连接供应商。打开添加供应商表单，仅浏览字段，未输入任何名称、URL、API Key，也未保存。
- 添加模型弹框可打开；点击取消关闭成功，重新打开后按 Escape 关闭成功。没有新增模型或供应商。
- Settings 的显式 `Close dialog` 和主设置 Escape 均能关闭设置页，AX 恢复主 Workspace。

## 自动化期间的绘制异常（后续复测见下）

1. 初始灰底可见，Workspace 卡片与文字正常。
2. Settings → 通用 → 点击白底后，AX 显示白底 `Value: on`、灰底 `Value: off`，但截图的内容区域变为纯白，只剩顶部 toolbar；AX 仍能读取完整 Workspace 和 Settings DOM。
3. 再点击灰底后 AX 显示灰底 `Value: on`，截图仍为纯白；关闭 Settings、Calendar → Workspace 往返也未恢复。
4. 连续两次截图长度和 JPEG 前缀相同（均 27185 bytes，`FF D8 FF E0 00 10 4A 46`），这证明截图内容没有更新，但单凭相同字节不能排除后台窗口绘制暂停。

阶段结论：当时视觉内容不可见，按 GUI 阻断交给主 agent 复查。不能仅根据 AX 认定通过，也不能仅凭截图相同认定为主题逻辑错误。

## 证据与限制

本轮 CUA 截图均通过 `await app.getScreenshot({emit:false})` 获取并在会话中逐张复核；该子 agent 未执行文件写入，截图未落盘到预定的 `analysis/evidence/gui-review-2026-09-15/after-*.png` 路径。AX 状态和截图观察已完整记录在本报告中，白屏复核使用了上述连续截图字节长度与前缀作为补充证据。

## 主 agent 的独立包复测

对象切换为 `/tmp/Planner UX Review.app`，实际 URL 为 `tauri://localhost`，任务、归属与灰底设置在重新启动后恢复。后台启动时同样出现过只有工具栏的画面；通过 Finder 打开 App、检查 WebView 并关闭检查器后，完整工作台和设置立即恢复。随后实际点击白底，再切灰底，文字、控件、背景和选中态均正常绘制，保存了两套主题的现场截图。这个现象不限定于主题切换；目前证据支持自动化操作时的窗口重绘问题，未修改主题逻辑来掩盖它。

现场证据：`theme-repaint-before.jpg`、`after-settings-white.jpg`、`after-settings-gray.jpg`，均在 `../evidence/gui-review-2026-09-15/`。原先将“连续相同截图”直接推断成主题逻辑故障的结论已更正。

后续 BYOK 实测由主 agent 执行，使用用户明确提供的 DeepSeek Anthropic 兼容地址和 `deepseek-flash`。连接测试返回成功（847 ms），供应商激活成功，Coach 收到 `BYOK OK`；进一步通过 `start_planning` 观察到实际工具调用、技能激活与模型后续回答。密钥只填入安全字段，保存后界面显示“已配置”，没有写入本报告或截图。

真实对话暴露了末尾空输入行进入模型上下文的问题：模型将一条实际任务加空行统计成两条，并追问无标题任务。已在上下文渲染处复用领域空行判定过滤，新增 Rust 回归测试；未改变编辑器真实空行与复用契约。Coach 正文也改为安全 Markdown 排版，使粗体、列表和段落正常显示，原始 HTML 与远程图片不执行/加载。

工具栏打开面板的 Escape、主设置关闭和添加模型取消/Escape 均已复测。真实截图还发现辅助面板关闭 SVG 被普通按钮横向内边距挤小，已增加明确的 icon 尺寸，消除 padding 冲突。

### 最终包与确认结果

最终独立包重新启动后，长期→周、周→日的绿色继承、同色细线和方形端点仍在；归属菜单显示当前父目标，底部关联名称可以滚动定位屏外任务。Coach 历史回复中的段落、粗体、编号与无序列表均正常排版，关闭图标尺寸正常，Escape 关闭后关系查看仍保留。现场证据为 `after-workspace-relations.jpg`、`after-week-day-relations.jpg` 和 `after-coach-markdown-history.jpg`。

Markdown 历史截图使用修复前已经保存的模型回复，里面的“2 tasks”是暴露空输入行问题的旧结果，不能用来证明修复后的任务数量。为确认修复，新周会话另发了一次只读任务计数请求；重新打包后的应用在读取既存测试密钥时触发 macOS 钥匙串确认。当时只读数据库检查显示该会话 revision 为 0、没有 active turn，系统中存在 SecurityAgent；CUA 明确禁止操作 SecurityAgent，因此由用户完成系统确认。

用户确认后，该请求成功结束。主 agent 在真实 Coach 窗口看到回复：`There is 1 task in this cycle: Validate the first prototype`，粗体与列表正常呈现，输入框恢复可用。只读数据库复核得到同一回复，会话 revision 为 1、active_turn_id 和 last_error 均为空。最终截图 `after-byok-real-task-count.jpg` 证明空输入行过滤已通过实际模型调用复核。此前连接测试、BYOK OK 文本回复和规划工具调用也均已成功，本轮 BYOK 验收不再被钥匙串确认阻断。

另记录两个待拆分的 Coach 状态问题：历史工具调用仍使用“正在…”文案，容易和当前运行状态混淆；读取钥匙串期间界面只显示笼统的“正在思考…”，不能解释等待发生在哪一阶段。这些需要与回合状态、取消和持久化一起设计，不在本轮关系 UI 中临时拼接另一套状态。

只读进程采样进一步确认等待栈为 `send_agent_message → build_provider → resolve → Credentials::load → keyring::get_password → SecKeychainFindGenericPassword`，尚未进入 HTTP 采样。因此这次等待不能归因于 DeepSeek 响应速度；没有绕过系统钥匙串确认或导出密钥。

### Later 图标补修

用户后续截图指出 Later 行内提升、删除及提示卡关闭图标过小。这三处仍使用 `compact` 文字按钮尺寸叠加 `w-7 px-0`，实际横向内边距挤压了 SVG。已统一使用现有 `icon` 尺寸（28×28px、无文字内边距），SVG 保持 16×16px 且不收缩。只修改按钮尺寸，不改变提升、删除或提示关闭的业务行为。

Later 现有 11 项测试通过，前端与原生独立包构建成功。更新 `/tmp/Planner UX Review.app` 后，在真实窗口重新打开 Later 并聚焦任务行，提升箭头、删除和提示关闭均清晰可辨。前后截图为 `before-later-action-icons.jpg`、`after-later-action-icons.jpg`。

### Workspace / Calendar 过渡

旧实现通过 `main key={view}` 卸载整页，再从完全透明重新淡入；顶部选中背景直接跳到另一项。改为两页共享同一固定视口：顶部等宽底板用 220ms 滑动，正文用 8px 方向位移与 160ms 淡入淡出衔接。已访问页面保留在各自容器中，往返保留滚动、周期和日历日期，未访问页面仍按需挂载。

切换时立即更新 tab 的选中与焦点状态，旧页立即 inert / aria-hidden；Workspace 的全局 Escape 监听只在当前页启用。动画不决定业务状态，快速反向切换直接响应最后选择；减少动态效果下禁用这组过渡。窗口栏、Later 与 Coach/Issues 不参与正文位移。

新增 3 项 App 回归验证页面状态和滚动保留、快速键盘反向切换及首选页按需挂载；连同已有 Calendar / PlannerWorkspace 测试共 15 项通过，TypeScript / Vite 构建通过。

真实独立包复测确认顶部选中底板滑动，正文往返没有只剩工具栏的空白；Calendar 翻到十月后往返仍停在十月，方向键连续右→左→右最终停在 Calendar，Later 保持打开。页面滚动保留已有组件测试覆盖；本次原生窗口没有成功制造非零水平滚动，因此不将它记作已完成原生验证。截图为 `view-motion-workspace.jpg`、`view-motion-calendar.jpg`。

### Calendar 内容联动

用户明确要求的是内容关联。此前 Calendar 只有专注块统计，缺少 Workspace 中的日任务与目标上下文。本轮直接读取现有 editor workspace，日历格展示实际任务标题、任务完成数与继承色；右侧 Plan 复用 TaskList，显示日任务及所属周、长期目标。Schedule 展示同一天的专注排期。没有新增任务副本、数据库表或同步实体。Calendar 属于本产品扩展，不能作为原 Hyperfocus 的既有功能记录。

原生 GUI 逐项验证：

- 原有 `Test` 在 9 月 15 日格、Daily tasks 和 Workspace 中一致；周目标同名，继承绿色。
- 新建测试日任务 `Calendar content audit`，在 Calendar 改为 `Calendar content verified`，Workspace 随即显示新标题；再在 Workspace 改为 `Workspace content verified`，返回 Calendar 后日期格和编辑器同步更新。
- 测试日任务关联到周目标 `Test` 后，两边继承绿色。Calendar 完成任务，Workspace 复选框为 1；Workspace 取消完成，Calendar 为 0。
- 点击周目标会高亮直接关联任务；Plan / Schedule 共用 9 月 15 日。Week 为周一至周日七列，日期和内容对应。
- GUI 操作只编辑本轮自建测试任务，未修改用户原有 `Test`，也未再次调用 BYOK。

截图为 `calendar-linked-content.jpg`、`calendar-week-content.jpg`、`workspace-linked-content.jpg`。随后视觉检查把空日期中铺满整列的虚线创建框改为顶部轻量入口；保留页面的隐藏任务编辑器不再自动补空行或抢焦点，既有多余纯空输入行只显示一个入口，不删除底层记录，含草稿、子步骤、清单或提议的内容仍保留。

补充回归覆盖无事件投递时的双向改名与完成、失败草稿保留、查询失败不伪装成零任务、父周期范围和颜色、密度偏好不覆盖页面偏好，以及隐藏编辑器与空行键盘导航。完整前端 36 个文件、201 项通过；最后一处键盘导航调整后相关 14 项再次通过。命令使用 `NODE_OPTIONS=--no-experimental-webstorage`，规避已有 Node 26 与 jsdom 的 localStorage 冲突；未改测试框架掩盖失败。TypeScript / Vite 与原生独立包构建通过。

架构债务单独记录：持久化空输入行由各编辑器补齐的既有契约仍可能在异步重叠时产生多条底层空记录。当前限定可见编辑器的副作用并保证单一可见输入入口；后续应统一空行创建的幂等契约，不在本轮引入另一套跨组件锁或自动删除用户数据。

最终包已更新到 `/tmp/Planner UX Review.app` 并启动。真实窗口确认日计划只保留一个空输入入口，空日期不再出现整列虚线框；Plan 方向键切到 Schedule 后日期仍为 9 月 15 日，点击 Plan 可回到同一份任务。点击周目标后日任务和长期父目标高亮。最终截图 `calendar-content-final.jpg` 保存了关系状态，但系统窗口捕获发生缩放，清晰布局参考前面的 `calendar-week-content.jpg`，该较早截图仍包含已修正的重复空输入行与虚线空态。

完成打包后执行 `cargo clean --manifest-path src-tauri/Cargo.toml`，工具报告移除 6548 个文件、3.6 GiB 构建产物，确认 target 目录已不存在。独立测试包约 59 MB，继续可运行；当前磁盘可用空间约 40 GiB。

### 周视图密度、Day / Focus 组合与独立事务

用户进一步明确：同一自然周可以有多个周任务，各自决定是否归属长期目标；日任务可以关联长期目标下的周任务、独立周任务，或完全独立。本轮将日期组织与任务目标归属分开处理，复用现有周期、任务和父链，没有新增“临时计划”实体。新建周、日不再依赖长期周期；Calendar 创建或移动到长期周期范围外的日期使用同一事务入口。已有显式周期父关系仍接受校验。

Week 日期格改为顶部对齐、按内容自然增高；空日期紧凑，周密度不再截断第四条之后的任务，月密度保留摘要。Day Plan 与 Focus Blocks 合成并排的计划面，标题基线对齐、两侧分别滚动，专注块空态及新增入口保持轻量。布局参考原 Hyperfocus，具体原 App 实测见报告 42；本轮没有补齐原 App 专注块内部行动清单，不将它宣称为完整功能对齐。

原生独立包由 Luna 子 agent 逐项操作并结合 AX / 截图确认：

- 周视图中空日期最短、周二两项任务稍高、周三五项任务全部可见且继续增高，任务行没有被固定高度裁切。
- 新建独立周任务 `Audit weekly errand`；`Wed density 1` 关联该周任务，`Wed density 2` 关联已有长期目标下的周任务 `Test`，另外三项日任务保持独立。
- 周三 Work mix 为 1 goal-linked、1 standalone weekly、3 independent daily，80% 没有长期目标关联。按有效顶层任务计数，子步骤不重复；这是当前归属构成，不是耗时或效率评分。
- Day Plan 与 Focus Blocks 并排显示；创建 `25m audit focus` 后显示 25m planned，没有启动计时。
- 跳到超出当前长期周期结束日的 2026-12-09，成功创建日计划和 `Dec 9 audit plan`。显示无周任务、无关联长期目标，Work mix 为 0 / 0 / 1。
- 仅新增和编辑验收任务，没有修改原有 `Test`，没有重复调用 BYOK。

截图：`week-content-sized.jpg`、`day-focus-combination.jpg`、`independent-work-mix.jpg`，位于本报告同级 evidence 的 `gui-review-2026-09-15` 目录。这三张是下述颜色补修之前的版本。

Work mix 同时进入当前周期的 AI 上下文，与 UI 复用同一统计函数。季度、半年度、年度及自选范围总结的周期边界、统计去重、历史关系和时间分配需求已记录到 design.md；这些跨度的总结生成功能属于后续实现，本轮未声称已经完成。周期删除级联等既有架构债务保持拆分记录。

### 归属颜色的可见性补修

用户指出菜单色块和窄色条的强度不一致。根因是色板使用实色，任务行与归属候选仍叠加低透明度。八色统一调整为较高饱和度；长期目标、继承色条、归属候选、日历和关系连线共用同一色值，窄条直接实色填充。只有整行关系选中背景继续浅色处理。颜色菜单增加当前颜色的独立勾选标记，避免把键盘焦点当成已选颜色。

最终包更新到 `/tmp/Planner UX Review.app` 并重新启动。主 agent 在真实窗口确认靛色、绿色目标细条清晰，周任务与日历继承绿色一致。打开绿色目标的菜单，AX 显示 green 下的 Current color / ✓；此时 red 只是菜单焦点。菜单展开时系统截图不可用，因此菜单勾选记为 AX 验证，不伪称取得菜单截图。关闭后保存并查看 `solid-goal-colors.jpg`，日历实色证据为 `solid-calendar-colors.jpg`。

验证结果：完整前端 36 文件、203 项通过；颜色补修后相关 4 文件、24 项再次通过。Rust 库测试 358 项通过、7 项忽略，服务集成 38 项通过；随后补充的“交换具有可选父周期的两个日期，保留各自内容”测试单独通过。TypeScript、Vite 与最终原生独立包构建成功。前端命令继续使用 `NODE_OPTIONS=--no-experimental-webstorage`，已有 Node 26 环境问题未混入本轮改动。

最终构建后执行 `cargo clean --manifest-path src-tauri/Cargo.toml`，移除 4804 个文件、2.0 GiB 产物，并确认 target 已不存在。独立验收 App 保留且可以继续运行。

### Week 阅读宽度与日期定位

用户指出七天等分剩余宽度使计划标题过度换行。本轮将 Week 每日列固定为 280px，列间距 12px，完整周内容横向滚动；高度继续随内容增长，右侧当天详情保持独立。Month 保留紧凑七列。

首次进入 Week 默认定位今天，从 Month 切入则定位当前聚焦日期。中间日期尽量居中，但不超出真实内容边界；按用户补充，周一靠左、周日靠右，首尾仅保留正常 16px 间距。手动滚动、任务刷新和 Workspace 往返不会重新居中；Today 显式回到今天。首次定位没有滑行动画，Today 可平滑滚动，系统减少动态效果时直接定位。

新增两项回归覆盖延迟加载后定位、手动滚动与往返保持、Today 重新定位、减少动态效果。Calendar 与 App 相关 22 项通过；测试 mock 的 TypeScript 签名修正后 CalendarView 11 项再次通过，最终 TypeScript / Vite / 原生构建通过。测试只模拟视口几何与滚动请求，真实宽度和边缘效果另由 GUI 验证。

### AI 配置、按需技能与可配置上下文保留

本轮将添加/编辑供应商模型与激活选择分开。设置顶部按供应商分组选择明确的模型，管理区浏览和保存不隐式激活；后端移除“优先选第一个支持工具的模型”策略。保存前逐个测试草稿中的模型，失败保留旧配置与草稿；重复模型 ID 拒绝，当前模型被移除时清空选择。连接探测验证协议握手/流式事件可读，并不声称已经验证所有工具行为。

长期、周、日计划区加入按 focus/hover 呈现的 Plan with AI。默认开关由设置控制，前提是已选择并测通 BYOK。日历 Week 的完成、标题定位、Focus Block 定位已有独立操作与回归测试。

按用户最新决定，技能由 LLM 自行通过 `load_skill` 按问题选择，不被初始计划类型限制。内置文件首次补齐到 `~/.goal/skills`，不覆盖个人文件，运行时重读。规划、范围分析、Issue 诊断分离；范围分析用真实自然日期和现有计划快照，读权限工具按技能控制。`get_period_context` 可用于季度、半年、年度或自选日期；不伪造历史完成时间、历史归属或区间实际专注耗时。

Coach 设置增加上下文保留时间，默认 15 分钟，可选 1–1440 整数分钟；从最后完成回合起计时，查询不续期，进行中的回合不被打断。服务端读取/恢复/发起前执行失效，窗口按同一到期时间自动重取。修改配置后刷新期限，任务与提案独立保留。

自动化验证：前端 37 文件 / 219 项通过；Rust 库 361 项通过、7 项既有忽略，服务 38 项、持久化 14 项通过。新增覆盖明确模型选择/重载/删除、草稿失败、周/日技能区分、模型加载技能、同一回合切换指令与工具、只读分析范围、归档与季度排他边界、无效输出、技能文件保留与重读、超时配置校验与窗口自动清空、迁移保留消息与外键完整性。TypeScript / Vite / 原生 App 构建通过。

原生验收第一阶段（Luna）：供应商测试连接成功，显示 500 ms；顶部选择 DeepSeek BYOK / deepseek-flash 后显示“已连接”，编辑区“已测通”；入口开关可保存；保留时间从 15 调整为 1 并保存成功。没有密钥输出，没有系统 Keychain 确认。Plan with AI 的 hover 入口寻址耗时过长，只有周计划入口曾在截图中出现，未据此把三个入口全记为已实测。后续转为通过 Coach 的明确 Start planning 入口验证实际回复和动态技能。截图当时仅由子 agent 工具显示，没有落盘，不能列为已有图片证据。

第二阶段真实 BYOK 验收（Luna）已完成：Coach → Start planning 成功加载日计划技能、读取周期与任务并给出回复。随后用粘贴发送完整中文季度请求，模型自行加载 `period-analysis` 并调用时段数据读取，给出 2026-07-01 至 2026-09-30 分析；没有修改计划、没有确认提案。首次 CUA typeText 中文有丢字，改为 paste 后成功，未将输入丢字归因于应用模型。1 分钟空闲后窗口自动清空并回到 Start planning；设置已恢复并保存为 15 分钟。无系统确认弹框、无模型调用错误。

用户随后指出工具调用状态过长：将每轮连续工具记录聚合为折叠摘要，并按调用 ID 合并开始与结果，正文保持展开。已读资料数量与失败状态在摘要可见，展开时每次调用只占一行。Coach 相关 12 项测试通过，包含四次读取从八条记录压缩为一行摘要、展开四行再收起，以及到期自动清空。

第三阶段原生验证（Luna）：默认可见一行“已准备规划上下文”或“已读取 2 项资料”，展开后两次任务读取只占两行，收起正常。Week 的 `Wed density 5` 可直接完成并已还原，点击标题定位 2026-09-16 对应编辑字段，点击 `25m audit focus` 定位 Schedule 的同一专注块，未启动计时器。时长保留 15 分钟。

### Markdown 表格与 Halaska 聊天呈现

用户提供的表格显示为管道符段落，现有 `react-markdown` 没有 GFM 支持。改用 Streamdown 2.6.0，保留单一持久化消息模型与 `parseNextSteps`，单独封装 `ChatMarkdown`。GFM 表格、任务列表、删除线、引用、代码和强调由库解析，结构组件使用项目自身 token。表格/代码独立横向滚动；HTML 不执行，远程图片不加载，非法协议不进入链接展示。没有额外安装代码高亮、Mermaid、数学插件。

用户随后推荐 [UI by Halaska](https://github.com/Halaska-Studio/ui)。核对官方公开源后，它是 MIT 单文件组件和演示模式集合；其中 `AgentChatPattern` 模拟回复，`ThinkingTracePattern` 用计时器模拟步骤，`StreamingAnswerPattern` 模拟逐字显示，因此没有整份导入。选取 ThinkingIndicator 与消息入场、过程折叠、输入区的视觉模式，适配为现有会话状态驱动；授权文本放在 `public/licenses/halaska-ui.txt`，随 App 打包。没有注入演示站字体、外部请求或模拟规划内容。

输入区有 32px 独立发送按钮、Enter / Shift+Enter 提示；请求中禁用重复发送，失败保留草稿。阅读历史期间保持滚动位置，明确点击“回到最新消息”或再次发送后恢复跟随。等待指示与正文入场/折叠采用轻量动效，并遵循 reduced-motion。

自动化结果：38 文件 / 226 项前端测试通过；TypeScript、Vite 与原生 App 构建通过。新增验证表格语义和候选发送、部分强调/表格更新为完整消息、无重复内容、禁用任务列表、HTML/图片不执行或加载、发送按钮 pending 状态、历史阅读位置与回到最新。当前 IPC 仍在回合结束后返回完整消息，不能把渲染器的增量单测或消息动画报告为真实 token streaming。

原生最终包由 Luna 验证：通过粘贴发送完整中文只读请求，点击新 Send message 箭头成功，出现真实等待态，最终回复为三列表格（任务 / 下一步 / 备注）和三行数据，另有两条 disabled 任务清单。Coach 未被宽表格撑大，输入区为轻底圆角和独立发送箭头，没有重边框。初次坐标滚动未命中容器；补验点击 AX 表格 region 获取焦点后，三次 ArrowRight 使底部横向滚动条向右移动，左列移出、下一步与完整长备注露出，确认局部横滚有效。没有修改任务或启动计时器，没有发起额外规划写请求。App 保留打开。

Halaska 的 CUA 网页入口因浏览器 provider 不可用，未形成其演示站截图证据；另一站 AICSS 的观察不作为 Halaska 证据。本轮选型依据是 Halaska 官方 GitHub / 公开 JSX 源码及本 App 的真实交互验证。最终工具截图通过子 agent 工具展示，没有另存为本仓库图片。

最终执行 `cargo clean --manifest-path src-tauri/Cargo.toml`，移除 4830 个文件、2.1 GiB 构建产物；保留 51 MB 的 `/tmp/Planner UX Review.app` 独立验收包及运行进程。


### Coach 工具、persona 与真实确认闭环

模型目录收敛为 24 项，按技能按需提供，单次最多 19 项。任务内容走原始快照预览；周期、专注、归属、提醒、重复及 9 项常用设置走封闭枚举操作与 Coach 确认卡。确认命令不暴露给模型。参数约定与明确的未覆盖边界记录于 `docs/coach-tools.md`。全局简洁助理风格首次补齐到 `~/.goal/persona.md`，每次生成重读，保留用户修改。

修复真实多轮工具调用中丢失当前用户指令的问题；同一回合加载技能后保留当前请求，不再继续回答更早的表格请求。修复任务确认时会话清理重复开启 SQLite 事务导致的提交失败；任务修改与聊天回执同事务保存。

Luna 在上一版原生包使用已有 DeepSeek BYOK / deepseek-flash 验证：Coach 提议把空闲上下文从 15 调为 20 分钟，确认前设置仍为 15，确认后标题提示与设置页均为 20；随后恢复 15。任务 `Coach receipt audit` 删除确认成功，主计划恢复 Test 与 Workspace content verified 两项。成功回执留在聊天历史，不再固定于输入框上方。

### 主体即时预览、锁定与确认状态

移除主体任务行及底栏的重复确认按钮，只保留 Coach 确认卡。主体使用同一编辑树直接显示任务预览，不再将改动另追加到列表底部；沿用字体、复选框、色条与层级，浅底、细描边、小锁区分预览，删除线只作用于标题。Workspace、Calendar 与 Later 都展示预览并锁定手动操作。归属图保留预览节点身份，避免下级色条因上级暂未确认而消失。

任务服务拒绝受影响任务的手动写入；删除影响的关联后代也在编辑投影中标记并锁定，Coach 列出级联删除范围。页面删除、结束和跨日移动不能绕过锁；其他任务仍可编辑。确认清除快照并解锁，拒绝恢复原内容、位置和关系。日历将预览数量单列，正式统计不计预览。

回执携带目标类型、ID 与确认结果，前端关联原工具调用并把“待确认”改为“已确认”或“已放弃”。聊天直接记录已添加/更新/删除的具体任务；对同一任务后续提出的新改动仍保持待确认，不被旧回执误标。

自动化：前端 41 文件 / 235 项通过；最后的归属色与 Later 调整另跑 3 文件 / 20 项通过。Rust 库 367 项通过 / 7 项既有忽略，acceptance 3、calendar 3、invariants 14、services 39 项通过。reminders 29 项通过，定时线程检查首次超时，原样单独重跑 1 项通过（未改测试阈值或调度实现）。TypeScript / Vite / Tauri 原生构建与 diff whitespace 检查通过。新增检查覆盖原位只读、其他任务可编辑、拒绝恢复、后端锁、级联影响、确认摘要与同一任务后续提案区分。

原生验收包：`/tmp/Planner UX Review.app`，二进制 SHA-256 `f0254bfa4d91fba381c9aca1ed8f31b2d879d2873e5fcee8d2c49da4db84c4ab`。最新一轮 GUI 验收结果在下方补充。


最新原生 CUA 验收（Luna xhigh）完成：

- Coach 请求创建唯一测试任务 `Coach preview audit`，主体立即显示浅底、小锁、“预览”，复选框 disabled、标题只读；没有 Keep / Revert。关闭 Coach 后工具栏出现 1 项待确认，点击回到同一确认卡。
- 确认创建后主体变为普通可编辑行，工具摘要为“1 项修改已确认”，聊天显示“已添加任务「Coach preview audit」”。
- 请求改名为 `Coach preview audit revised` 后主体立即显示新名并锁定；拒绝后恢复原名且解锁，摘要为“1 项修改已放弃”。
- 请求删除后主体仅标题有删除线、浅底锁定并标注待删除；确认后任务消失，摘要为“1 项修改已确认”，聊天显示“已删除任务「Coach preview audit」”。
- Test 与 Workspace content verified 始终未修改；未启动计时器、未改变设置。测试任务已清理，App 保持打开。截图通过 CUA 内联检查，未落盘。Calendar 同步预览本轮由自动化覆盖，未额外记录原生验收。

收尾执行 cargo clean，移除 6240 文件、2.3 GiB 构建缓存；保留 54 MB 的独立原生审计 App。只读数据库复核测试任务和待确认任务数均为 0，原有两项任务保留。

工具过程文案补充：展开项复用持久化执行回执，显示实际操作及对象（例如已删除任务及其标题），收起汇总仍显示确认数量。复用同一条真实结果，不额外调用模型或维护前端文案状态。Coach 定向 18 项测试、TypeScript / Vite / Tauri 构建通过。

Luna 已启动文案修复包，旧会话按 15 分钟 TTL 清空，未再创建验收任务或发起模型请求。因此本次具体展开文案由组件测试验证，未重复记录原生实测。App 保持打开；再次 cargo clean 清理 3967 文件、1.5 GiB。


### Issues AI 检查专项复核

旧版真实窗口复现：日计划点击 AI 检查后没有可见进度或结果变化，仍显示「没有发现计划问题」；长期目标只有四项通用结构提示。代码定位确认，原语义候选条件依赖 needs_refinement 和缺失澄清字段，正常周/日任务未进入模型；请求与解析错误被吞为空数组，造成未检查与成功无问题无法区分。

本轮改为显式只读检查，报告区分规则和 AI，显示范围、状态、模型、时间；提供任务定位与 Coach 草稿交接。完整任务/周期快照和活跃模型用于结果新鲜度，返回前比对快照避免编辑竞态。规则阈值仅表达数量提醒，不把项数当作工时。

自动验证：前端 41 文件、244 项测试通过；Rust lib 366 通过、7 个既有测试忽略（含本轮 20 项 review 测试）。覆盖普通日任务真实进入 provider、合法空结果、未知 task ID/缺字段拒绝、provider 失败、模型与详情变更失效、检查中编辑冲突，以及 UI 进度、失败不误报成功、定位加载竞态、Coach 草稿不自动发送。TypeScript/Vite 与原生 debug app 构建通过，使用 /tmp/Planner UX Review.app 实测，未新增依赖或数据表。

本轮另发现但未混入改动的债务：旧 review_cached/结构缓存辅助入口已没有生产调用方，后续可单独移除并统一廉价结构查询入口；忽略记录当前不提供恢复入口，若增加需统一任务级/周期级恢复语义。当前 AI 报告只在内存保存，重启回到未检查，避免把未持久化结果当作历史审计记录。


原生 BYOK 实测（Luna xhigh / CUA）：当前选中的日计划为 2026-12-09。主动检查完成后显示「AI 已检查 1 项 · 18:27 · deepseek-flash」，对 Dec 9 audit plan 返回 1 条建议，指出缺少明确动作和预期产出。点击定位后切入 Week 50 / Dec 7–Dec 14 和 2026-12-09，对应任务标题保持不变；点击与 Coach 讨论预填该问题的中文草稿，未发送。

定位期间又出现 WKWebView 截图仅见 Days/Wed、但 AX 完整的绘制异常。普通切换 Workspace/Calendar 与横向滚动未恢复；切换窗口全屏后立即恢复，可见完整日任务、Connections 和右侧 Issues 卡。证据支持窗口重绘问题，不能把 AX 可读单独当作视觉通过；恢复后重新确认了任务和结果同时可见，保留该现象作为原生环境回归项。


第二次真实检查选中长期计划（2026-09-15）：显示「AI 已检查 2 项 · 18:32 · deepseek-flash」，共 4 项（2 条规则、2 条 AI 建议）。AI 分别指出 Launch a thoughtful product 的发布动作/下一步不明确、Design a calm weekly rhythm 的范围和成功标准缺失，任务归属准确；未改动或忽略任何既有任务，也未发送 Coach 草稿。结果卡按钮与长文本在侧栏内正常换行、无横向溢出，四张卡通过纵向滚动查看。退出全屏后常规窗口仍正常显示，最终 App 保留在可见的长期计划 Issues 页面。本轮截图由 Luna CUA 内联检查，未保存为新的文件，不能引用不存在的图片路径。

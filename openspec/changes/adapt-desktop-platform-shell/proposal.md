## Why

当前界面把 macOS 的窗口融合方式、红绿灯留白和 `⌘` 提示当作通用桌面行为，公共打包配置也只生成 macOS 产物。跨平台发布前，需要明确哪些是统一的产品 UI，哪些由操作系统负责，并使窗口配置、原生依赖和前端构建目标保持一致。

## What Changes

- 明确三平台外壳：macOS 保留原生红绿灯与融合式应用栏；Windows 采用自定义单行融合标题栏，右侧保留 Windows 习惯的最小化/最大化/关闭控件，原生适配负责系统窗口行为；Linux 保留桌面环境的窗口装饰及按钮位置，下面使用紧凑工具栏。
- 公共 UI 保留现有白底/灰底主题、字号、组件、Workspace/Calendar 页面切换和 Coach/Issues 辅助面板分组。仅调整平台安全区、工具栏高度、快捷键提示及相应交互，移除 Windows/Linux 的 macOS 专用空槽。
- 统一快捷键的显示与判断：macOS 使用 `Command`，Windows/Linux 使用 `Control`；处理输入法、重复按键和系统快捷键边界。
- Windows 的视觉、按钮命中与原生状态同步作为一个完整交付：保留缩放、双击最大化、标题栏系统菜单、Windows 11 Snap 布局及无障碍操作；不能仅关闭装饰再放置三个只处理点击的网页按钮。Snap 目标受当前窗口最小尺寸约束，不宣称支持所有窄分区。
- 隔离公共配置、平台配置、前端构建目标和 Rust 平台依赖；浏览器预览有独立的无系统窗口装饰模式，不把浏览器宿主误认成原生构建目标。
- 定义 macOS universal、Windows x64、Linux x64 的独立构建与验收矩阵，以及按目标区分的安装包、日志和测试证据。
- 本变更形成待实现规范，不执行发布，不更换应用标识、版本号或用户数据目录；不新增业务实体或数据迁移。

## Capabilities

### New Capabilities

- `desktop-platform`: 三平台窗口行为、目标平台的一致性、原生编译依赖隔离、安装产物隔离和验证边界。

### Modified Capabilities

- `planner-workspace`: 修订固定顶栏及涉及快捷键的要求，明确平台内边距、工具栏分组、侧栏对齐和 `Command` / `Control` 行为。对应 `openspec/specs/planner-workspace/spec.md`。

## Impact

- 前置依赖：`rebuild-baseline` 的桌面工作台与本地存储、`add-ai-planning-core` 的 Coach 候选回复，以及当前工作树已落地的 Workspace/Calendar 和双主题 UI。沿用现有实现作为基线，不要求归档仍在进行中的其他变更。
- 前端涉及 `src/App.tsx` 的 `WindowBar` 和快捷键、`src/features/agent/AgentPanel.tsx` 的候选快捷键、`src/main.tsx`、`src/index.css`、`vite.config.ts`；增加一个小型平台边界，不分叉三套业务页面。
- 桌面端涉及 `src-tauri/tauri.conf.json`、平台覆盖配置、`src-tauri/Cargo.toml` 的目标依赖与必要权限。Windows 窗口消息、命中区域和安全降级集中在仅 Windows 编译的外壳模块，不进入计划领域或 Coach 工具。新建三平台构建验证工作流，复用现有图标，Windows 使用已有 `icon.ico`。
- 实施完成后同步根 `design.md` 与 `docs/architecture.md`。本次规划仅写当前 change 中的 proposal、delta specs、design 和 tasks；不提前同步主规范或修改实现。
- 版本发布、签名/公证、更新服务、ARM Windows/Linux、移动端、Linux 自绘窗口装饰、为所有 Snap 分区重构窄屏业务布局不属于本变更。已发现的最低系统版本声明、窗口最小尺寸限制与 macOS 偶发窗口重绘问题列为独立复核项。


## v0.1.1 补充范围

发布前追加实际代码排查与必要修复：跨平台路径/文件保存/系统服务、应用内选择器定位和浮层键盘。共用组件取代原生 select，保持现有数据保存与确认规则。验收记录分层保留，发布本身仍由独立 release 工作流执行。

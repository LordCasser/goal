## Context

动机与范围见 [proposal.md](proposal.md)。本设计是供其他 agent 实施的目标方案，以下“采用”均指待实现决定，不表示当前代码已完成适配。

当前源码证据：

| 位置 | 当前事实 | 本次需要处理的边界 |
| --- | --- | --- |
| `src/App.tsx` / `WindowBar` | 总是保留 104px 红绿灯区域，显示 `⌘⇧L`；顶栏与业务根组件在同一文件 | 平台安全区和提示不能写死；无需复制业务根组件 |
| `src/index.css` | 应用栏 52px，计划面 512px，侧栏 424px；现有白底/灰底主题与本地 IBM Plex Sans | 工具栏变量可按平台覆盖，正文、计划面与侧栏 token 保持公共 |
| `src/features/agent/AgentPanel.tsx` | 接受 Meta 或 Control，但只显示 `⌘1…9` | 提示与事件判断共用同一平台规则 |
| `src-tauri/tauri.conf.json` | 公共窗口写入 `Overlay`、`trafficLightPosition`、`hiddenTitle`，只打包 app/dmg | macOS 属性移入平台覆盖；Windows 初始化融合外壳，Linux 使用原生装饰 |
| `src-tauri/Cargo.toml` | `keyring` 在公共依赖声明中同时启用三个平台 feature | 显式分入 target 依赖；不能误称当前库已把所有系统 SDK 一起编译，库自身也有 target 条件 |
| `src-tauri/capabilities/default.json` | 允许主窗口拖拽，未添加自绘关闭/最大化按钮权限 | 保持最小权限，不因平台适配引入一整组无用途的窗口写权限 |
| 构建目录 | 暂无三平台构建验证 workflow；已有 `icon.ico` 等图标 | 复用图标，补独立构建和证据，不重新设计品牌 |

Atlas 对 `src/App.tsx` 的限定查询没有返回 `WindowBar`，其缓存结果不能证明符号不存在；上表以实际源码读取为依据。

## Goals / Non-Goals

**Goals:**

- 一套业务 UI，三种适合目标系统的窗口外壳；平台判断在小型适配边界集中。
- 同一目标贯穿配置、前端资源、Rust 依赖和安装包；干净机器只需该目标的开发环境。
- 实施 agent 能依据明确尺寸、交互场景和构建步骤交付，不需要自行选择三套组件库或发明窗口管理框架。

**Non-Goals:**

- 不移植原版 Hyperfocus 的标题栏，不做三套任务/日历/设置页面。
- 不增加窗口风格设置、模拟 Windows Snap 菜单或 Linux 自绘窗口控制。Windows 专属命中适配属于本变更，范围只到窗口外壳。
- 不更换业务模型、存储路径、包标识、密钥服务名或版本号。
- 不在本变更完成代码签名、公证、更新服务、商店提交或对历史系统版本的兼容工程。

## Decisions

### D1：Windows 采用单行融合栏，原生适配负责窗口行为

macOS 延续融合方案与原生红绿灯。Windows 移除常规标题栏外观，由应用绘制 44px 单行工具栏和右侧窗口控制，窗口行为继续交给系统；Linux 保留桌面环境装饰，下面放置 44px 应用工具栏。两个浅色主题贯穿 Windows 整条栏，避免额外一行系统底色与应用分离。

Windows 的三个控件不是普通业务按钮：外观由前端负责，原生命中、系统菜单、边缘缩放与 Snap 由仅 Windows 编译的适配模块负责。允许为这个边界增加必要原生代码，不将其扩张为跨平台窗口管理框架。Linux 的桌面偏好与技术路径不同，不套用 Windows 无边框方案。

[Tauri 支持自定义标题栏与窗口操作](https://v2.tauri.app/learn/window-customization/)，但该示例不能证明 Snap 已完整支持。[微软要求自定义最大化区域参与原生命中测试](https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/ui/apply-snap-layout-menu)。系统不支持或用户关闭的功能不由应用补画；Windows 11 Snap 实测记录系统开关、显示器工作区及最小尺寸限制。

### D2：窗口栏的具体布局

示意图表达分区；macOS/Linux 的系统区不由前端绘制，Windows 控件按下述专属规范实现：

```text
macOS 常规窗口，52px 应用栏
[ 原生红绿灯安全区 104px ][Later] [可拖拽空白] [Workspace | Calendar] │ [Coach][待确认?][Issues] │ [设置]

Windows，44px 单行融合栏
[16px][Later] [可拖拽空白] [Workspace | Calendar] │ [Coach][待确认?][Issues] │ [设置][16px][ — ][ □ ][ × ]

Linux
[桌面环境自己的标题、按钮位置、窗口装饰] ← 不假设按钮一定在右侧
[16px][Later] [弹性空白] [Workspace | Calendar] │ [Coach][待确认?][Issues] │ [设置][16px] ← 44px
```

| 项目 | macOS | Windows | Linux | 独立 Web 预览 |
| --- | --- | --- | --- | --- |
| 系统装饰 | 原生红绿灯 + Overlay | 自定义单行栏 + Windows 原生行为 | 原生 GTK/桌面环境装饰 | 无 |
| 应用栏高度 | 52 CSS px | 44 CSS px | 44 CSS px | 44 CSS px |
| 左侧保留 | 常规窗口 104px；不再被系统控制占用的全屏状态收为 16px | 16px 内边距 | 16px 内边距 | 16px 内边距 |
| 右侧保留 | 16px | 设置后间隔 16px + 138px 窗口控制区 | 16px | 16px |
| 应用按钮 | 32px 高、16px 图标、沿用 13/20 字阶 | 相同；窗口按钮另见下文 | 相同 | 相同 |
| 窗口拖动 | 仅明确空白区域可拖动 | 融合栏空白区，原生命中适配 | 原生装饰承担 | 不调用原生拖拽 |
| 客户区圆角/外阴影 | 交给原生窗口裁剪 | 交给系统边框 | 交给桌面环境 | 不模拟系统窗口边框 |

补充约束：

- `--app-header-height` 表示应用工具栏自身高度。Windows 正常模式没有第二条标题栏；Linux 系统标题栏在 WebView 客户区之外，不另加 CSS 高度补偿。Later、Coach/Issues 与主视图从应用栏下方自然 flex 排布。
- 应用栏继续使用 `--surface-canvas` 与 1px `--border-light`。两种浅色主题共同生效；macOS/Linux 原生装饰由平台呈现，Windows 控件底色与应用连续，不增加一块永久深灰色按钮区。
- Later 位于左侧；两种视图是一个互斥组；Coach/待确认/Issues 是另一组；设置保持独立入口。待确认摘要限制宽度，必要时仅显示数量并提供完整 tooltip，不能把右侧入口挤出窗口。
- 应用栏不新增重复的 Planner 品牌标题、状态横条或平台名称标签；原生窗口的 title/图标仍设置，供任务切换、任务栏与辅助技术识别。
- 客户区默认尺寸仍为 1280×800、最小 960×600（逻辑单位）。512px 计划面与 424px 侧栏不随 DPI 人工乘倍数，也不为塞进一屏压窄正文。双侧栏共用时，主体允许缩小并内部横滚；不由子项的最小宽度撑大整窗。
- 设置弹窗、模型表单、浮层、Coach Markdown 表格沿用公共组件，最大高度/宽度按当前客户区约束，关闭入口和底部操作保持可达；不把某平台的系统标题栏计入弹窗布局。
- 保留本地 IBM Plex Sans 与当前字阶，补齐平台中文回退顺序（macOS PingFang SC、Windows Microsoft YaHei、Linux Noto Sans CJK SC/系统 sans-serif）。不使用远程字体，不通过改字号掩盖字体基线错位。

**Windows 控件细节：**

- 顺序为最小化、最大化/还原、关闭，每个命中格 46×44 CSS px，共 138px，贴合可操作客户区右上边缘；设置按钮到控制区间隔 16px。三个格子不额外套圆角胶囊，不带常驻描边，图标沿用 Windows 的横线、单框/叠框、叉形语义，采用清晰的 12px 图形并居中。
- 普通态透明底、正文级中性色；最小化/最大化 hover 使用现有轻灰交互色，pressed 更深一级；关闭 hover 为 `#C42B1C`、白图标，pressed 为 `#A92317`。颜色变化 100ms，不做缩放、位移动画；减少动态效果时直接切换。失焦仅降低栏内次要文案和图标强调度，不降低整页可读性。
- 键盘焦点轮廓清晰，提供最小化/最大化/还原/关闭的可访问名称；高对比度使用系统颜色与可见轮廓，不能只依赖红色。窗口状态从原生读取，不能点击后盲目翻转本地布尔值。
- 客户区不足 1100px 时，Coach/Issues 的文字收为图标加 tooltip，待确认摘要收为数量；页面标签保留文案，窗口控件与设置始终可见。至少保留 64px 明确空白拖动区，不把有功能的图标当作拖拽面。
- 最大化后的边缘与恢复态由原生边框/DWM 处理；不得用根元素的大圆角或透明外边距导致贴靠后漏出桌面、四周不能命中或任务栏被覆盖。
- `960×600` 最小客户区本次保持不变，不能宣称所有 Snap 分区都能容纳本应用。验收区分“菜单正确出现”与“尺寸足够的分区正确贴靠”；更窄窗口需要另做正文与双侧栏响应式设计，记录为独立后续项。[微软的最小尺寸说明](https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/ui/apply-snap-layout-menu)

### D3：窗口状态和键盘输入

macOS 的原生全屏状态从窗口 API 读取，并在原生尺寸/全屏状态变化时校正；浏览器的 `document.fullscreenElement` 不作为原生全屏真相。安全区收起的前提是系统控制已不再占用客户区，退出全屏须先恢复安全区再允许该区域点击。系统全屏控制临时显现也要实测命中，不能仅靠截图把它假定为安全。Windows 控件读取并订阅原生状态，Linux 交给原生装饰；不以点击次数推断是否最大化。

| 动作 | macOS | Windows/Linux |
| --- | --- | --- |
| Later | `⌘⇧L` | `Ctrl+Shift+L` |
| Coach 候选 1–9 | `⌘1`…`⌘9` | `Ctrl+1`…`Ctrl+9` |
| 消息发送/换行 | Enter / Shift+Enter | 相同 |
| 关闭当前浮层/面板 | Escape，先由最内层消费 | 相同 |

平台模块提供“主修饰键判断”和“快捷键文案”两个小函数供顶栏与 Coach 共用。拒绝额外的 Ctrl+Meta 组合、Alt、错误 Shift、IME composition 和自动 repeat；没有候选、回合 busy、面板 hidden/inert 时不发消息。系统 Alt+F4、窗口菜单、macOS 系统退出行为沿用原生入口。本变更不新增应用全局热键注册或托盘生命周期。

### D4：编译目标是唯一的原生平台来源

新增小型 `src/lib/platform.ts`，对公共 UI 暴露 `macos | windows | linux | web` 和所需的窗口栏/快捷键特征。业务页面不各自解析 UA，也不引入 PlatformProvider、平台设置表或三套 App 根组件。

Tauri hook 已提供 `TAURI_ENV_PLATFORM`；当前 CLI 2.x 的 Apple 值为 `darwin`，需显式归一为 `macos`，不能直接假定为 `macos`。前端从该受控构建输入获得常量，映射在首次绘制前可用；不先画 macOS 留白再等待异步 IPC 修正。来源见 [配置参考](https://v2.tauri.app/reference/config/#buildconfig) 和本地 CLI changelog 的目标命名说明。

构建链路约束：

```text
Tauri 目标 triple
  → 选中该平台的窗口/包配置
  → beforeBuildCommand：校验目标，构建当前目标前端
  → Vite：注入归一的平台常量，输出 desktop-build.json（仅平台与产品版本）
  → Rust：按目标 cfg 编译
  → beforeBundleCommand：核对前端标记、当前目标与版本
  → 生成当前平台的安装产物
```

- 原生构建目标缺失、未知，或 `desktop-build.json` 缺失/不匹配时阻止分发打包；不会退回 `web` 后继续打包。
- 单独 `npm run dev` / `npm run build` 可生成 `web` 预览；浏览器仅为快捷键提示读取宿主键盘习惯，不能由此打开原生拖拽或红绿灯占位。
- 前端平台不区分 arm64/x64，macOS universal 的两种机器码嵌入相同 `macos` 资源；原生架构由工具链与产物检查决定。
- 不添加可通过 URL/localStorage/设置切换的“模拟平台”到正式包。组件测试通过依赖注入或 mock 平台边界覆盖各分支；预览测试明确标注不是原生验收。
- 小型构建标记是构建产物，不是用户配置或业务实体；无密钥、用户名或机器路径。按目标重新构建，禁止从上一 job 直接拷贝 `dist`。
- Windows 外壳是否初始化成功是窗口能力状态，不是另一个平台来源。正常路径先建立原生适配，再启用自定义外观；失败时仍为 `windows`，退回原生装饰和 44px 普通工具栏，并隐藏自定义控件。降级不持久化成用户设置，不改变快捷键，也不能把降级结果记为融合栏验收通过。

### D5：Tauri 配置隔离与合并规则

采用官方平台覆盖文件，不生成三份独立项目或维护三份完整公共配置：

| 文件 | 负责内容 |
| --- | --- |
| `src-tauri/tauri.conf.json` | 产品名/版本/identifier、build hooks、CSP、主窗口 label/尺寸/最小尺寸、原生装饰基线与公共图标 |
| `src-tauri/tauri.macos.conf.json` | macOS 窗口 Overlay/隐藏标题/红绿灯位置，app/dmg 目标；原有最低系统声明单独复核 |
| `src-tauri/tauri.windows.conf.json` | Windows 外壳启动参数、NSIS 目标和必要安装参数，使用已有 icon.ico；在适配与前端控制就绪后启用自定义装饰 |
| `src-tauri/tauri.linux.conf.json` | AppImage/deb 目标和运行依赖；继承公共原生窗口，保留桌面环境装饰 |

公共配置移除 `titleBarStyle`、`trafficLightPosition`、`hiddenTitle` 和 macOS 专有打包参数。窗口 `decorations: true` 是公共安全基线。Windows 原生初始化完成并收到前端控制区域就绪确认后，才切换为自定义装饰；可用受控的先隐藏再显示避免启动闪现双层栏，但失败或等待超时必须恢复系统装饰并显示窗口，不能无限隐藏。其余原生窗口保持明亮配色以配合现有两个浅色主题，Linux 在框架不支持覆盖时遵从桌面环境，不追加平台私有调色 API。

**合并陷阱必须测试：** Tauri 使用 JSON Merge Patch，`app.windows` 等数组整体替换，不按 label 深合并。因此 macOS 或 Windows 覆盖若写 `app.windows`，必须保留 main 的 label、title、1280×800、960×600、decorations、theme 等完整公共窗口字段，再追加本平台属性。仅覆盖一个 `titleBarStyle` 或 `visible` 会导致其余值退回默认值。此处接受短小重复，并以配置契约测试校验共用几何和 label 一致，不引入配置生成器或动态窗口工厂。规则见 [Tauri 平台配置](https://v2.tauri.app/develop/configuration-files/#platform-specific-configuration)。

当前 `capabilities/default.json` 的主窗口 label 不变。拖拽权限移入对应平台 capability，公共能力保留既有 dialog/opener。Windows 单独允许实际使用的窗口状态读取、最小化、最大化/还原、正常关闭及受限外壳初始化/区域更新；Linux 不获得这组自绘控件权限。外壳 IPC 限定本地主窗口，不能传任意窗口句柄、原生消息号或操作码，也不暴露为 Coach toolcall。

### D6：Rust 依赖在编译边界隔离

Windows 增加窄范围原生外壳模块，例如 `src-tauri/src/platform/windows.rs`；业务 service/repository/domain 不依赖它。入口仅在 Windows 编译并初始化此模块，相关 imports、命令注册与依赖使用 `#[cfg(target_os = "windows")]` 和 Cargo target dependencies；`if cfg!(...)` 只返回一个布尔值，不能阻止另一分支被类型检查，不能用它隔离平台 SDK 引用。macOS/Linux 不编译 Windows 外壳代码；Linux 不新增 GTK 自绘层。

`keyring` 按目标声明持久化后端（以下为布局要求，不是额外升级依赖）：

| target 依赖范围 | 需要启用的后端 |
| --- | --- |
| `cfg(target_os = "macos")` | 现有版本 keyring 的 `apple-native` |
| `cfg(target_os = "windows")` | 同版本 keyring 的 `windows-native` |
| `cfg(target_os = "linux")` | 同版本 keyring 的 `sync-secret-service`，以及该版本实际需要的相关 feature |

删除公共 keyring 声明上的全平台 feature 列表；共同版本保持一致。Linux 按当前锁文件解析出的 Secret Service/DBus 依赖准备开发库和运行依赖，不假设换到 Linux 后安全存储自然可用。禁止发布构建退回 keyring mock。GUI 不新增密钥的明文回退方案，也不改变现有 AI 失败与本地计划解耦的约定。

使用每个 target 的 `cargo tree --target <triple> -e normal,build,features` 与真实编译核对被激活的依赖；不以 Cargo.lock 中出现其他系统 crate 认定泄漏，也不因 transitive `windows-sys` 在其他上下文可见就误判。验证的是目标专属链接/SDK需求与对应凭据后端是否正确激活。

### D6a：Windows 原生外壳的最小边界

先在当前锁定 Tauri/tao/wry/WebView2 组合上打通原生命中闭环，再做视觉细化。默认采用局部 Rust 适配，不因标题栏引入 WinUI 应用框架、替换 WebView 或升级整套 Tauri。Windows API crate 仅添加所需 feature 并限制在 Windows target。若现有框架提供等价能力可直接复用；社区插件的截图或 README 不能代替本项目原生验证，也不能未经核对引入第二套互相竞争的窗口消息处理。

职责约定：

1. **前端负责区域几何和可访问控件。** 控制区固定右端；空白拖拽区与应用按钮分离。布局就绪及尺寸变化时报告本窗口内的控制/拖动区域，仅当几何发生变化才更新。IPC 只接受有界矩形与布局修订号，拒绝非有限数、越界、重叠或陈旧区域；不允许网页提交任意 HWND。窗口仍不就绪时保持原生装饰。
2. **Rust 负责坐标和窗口消息。** 原生适配将 CSS 客户区坐标映射为当前窗口的物理/屏幕坐标，处理 DPI、窗口边框与 WebView 缩放，不能无条件再乘一次比例。命中查询使用已缓存的区域，不逐次同步等待 JavaScript；尺寸变化期间陈旧区域不得误触相邻按钮。
3. **Snap 由 Windows 提供。** 最大化/还原区域须能让系统得到 `WM_NCHITTEST` 的 `HTMAXBUTTON` 语义；空白区和边角各有正确窗口语义。须验证 WebView2 子窗口是否截获消息，不能只在顶层 HWND 返回结果就认定成功。保留原窗口过程链、DWM/默认处理与实际需要的 resize/maximize/system-menu 样式；只处理外壳所需消息。[微软自定义边框说明](https://learn.microsoft.com/en-us/windows/win32/dwm/customframe)
4. **一个动作只有一个执行者。** 系统命中和网页点击之间明确分工，避免鼠标点击产生两次 maximize 而立即还原；键盘/辅助技术路径使用同一动作语义。关闭走 Tauri 的正常关闭流程，不用强制退出代替。空白右击与 `Alt+Space` 调出原生菜单，触控/笔拖动在可用设备上补验证。
5. **生命周期有清理。** 主窗口线程安装适配、接收区域更新，销毁时卸载消息钩子并释放资源；React 监听器在卸载时清理，热重载不能累积重复事件。实际窗口状态驱动图标、名称和区域重算。
6. **失败保留可用窗口。** 建议初始窗口保持隐藏及原生装饰，收到控制区域确认后一次性显示融合栏；从原生初始化起最多等待 3 秒，失败/超时则恢复系统装饰并显示普通工具栏。布局就绪必须在隐藏窗口中可测量，不能依赖只在可见时执行的动画帧造成死锁。失败原因进入现有本地日志，不修改业务数据或增加持久设置。故障注入验证缺少前端确认、初始化错误、非法区域更新；恢复后不显示双套按钮。

能力状态是窗口级瞬态信息，不加入 SQLite、AI 上下文、用户设置或构建平台枚举。原生验收不通过时，对应任务保持未完成；不把安全降级当成满足 Windows 单行设计。

### D7：构建验证矩阵与发布边界

新增手动触发的构建验证 workflow，初始只保存 artifacts，不自动创建 Git tag 或 GitHub Release。使用干净 runner、固定依赖锁文件、独立输出与缓存；通用构建命令经过前述 hooks，不使用跳过前端构建的捷径。

| job | 原生环境/目标 | 标准命令 | 预期产物 |
| --- | --- | --- | --- |
| macOS universal | macOS，安装 `aarch64-apple-darwin` 和 `x86_64-apple-darwin` 两目标 | `npm run tauri build -- --target universal-apple-darwin` | `.app`（归档后上传）、`.dmg`；核对二进制含两种架构 |
| Windows x64 | Windows + MSVC/WebView2/NSIS 所需工具 | `npm run tauri build -- --target x86_64-pc-windows-msvc` | NSIS `*-setup.exe` |
| Linux x64 | Linux + WebKitGTK 4.1/GTK/DBus 等构建依赖 | `npm run tauri build -- --target x86_64-unknown-linux-gnu` | `.AppImage`、`.deb` |

- 不在 macOS 上假定用 `--target` 就能完成 Windows/Linux 安装包。首轮采用各目标原生 runner；跨架构 macOS universal 是单独支持的路径。
- 首轮 Linux 构建可用 Ubuntu 22.04 的更新仓库作为 glibc 基线，依赖以锁定 Tauri 版本的官方 prerequisite 为准；GUI 认证在 Ubuntu GNOME 和 KDE Plasma 桌面环境实际完成，至少覆盖 X11 和 Wayland。AppImage 不是“支持所有 Linux”的证明。[Tauri AppImage 限制](https://v2.tauri.app/distribute/appimage/#limitations)
- 安装脚本/工作流显式列出 Linux WebKitGTK 4.1、GTK3、DBus、SSL、pkg-config、librsvg、patchelf 等实际构建依赖；AppImage 打包依赖和桌面安全凭据服务分别核对。不要为窗口外壳加入无用途的 tray/appindicator 依赖。
- 前端测试复用 Vitest，原生构建与适当 Rust 测试在对应 runner 运行。固定支持的 Node 工具链，避免把本地 Node 26 的 Web Storage 测试差异误报为平台业务问题。
- 构建记录包含应用版本、源码版本/工作树状态、目标 triple、工具链、配置检查、前端平台标记、测试结果与校验和。缓存键含 OS、原生目标、profile 和 lockfile；不缓存用户目录、凭据或个人数据库。
- 签名、公证、自动更新和公开发布是后续发布任务。未签名开发包不等于可面向普通用户分发的成品；最低系统版本声明也必须另有对应环境证据。

### D8：实现验收矩阵

所有新增 tasks 初始保持未完成。以下是实施所需证据，不是本次规划已完成的测试：

| 范围 | 自动检查 | 原生人工检查 |
| --- | --- | --- |
| 配置与构建 | 三平台有效合并结果、macOS/Windows 数组字段完整、格式/icon 存在、目标常量与 manifest 一致、故意置入错误平台资源时打包失败 | 对应安装器安装、启动、卸载及重复启动 |
| 标题栏 | macOS 常规/全屏安全区、Windows/Linux/Web 无空槽；Windows 控件分区/实际状态/降级；功能按钮不在拖拽区 | 拖拽、缩放、最大化/还原、最小化、关闭；Windows 11 Snap/系统菜单/单次操作；Linux 桌面按钮偏好 |
| 布局 | 1280×800、960×600 客户区；两个主题；Later + Coach、设置模型表单、长文本 | Windows 100/125/150/200% DPI；Linux 至少普通及可用的高 DPI；macOS Retina/全屏 |
| 键盘 | 三平台显示/触发一致；IME、repeat、错误修饰键、隐藏 Coach 不发送 | 原生应用中的 Later、候选、Escape、系统关闭路径与输入法 |
| 状态与绘制 | 隐藏视图 inert、焦点不被平台初始化抢走、主题切换不改几何 | 全屏往返、跨显示器、失焦恢复；截图确实可见，不能仅 AX 可读 |
| 公共主流程 | 复用已有计划/预览/日历测试，避免复制三套业务用例 | 创建并完成临时日任务、Workspace/Calendar 联动、Coach 预览确认/拒绝、设置关闭；清理测试数据 |
| 系统集成 | 检查目标依赖与能力范围；Linux 凭据不可用不影响本地计划 | 安全存储重启后读回、真实系统文件保存/打开；沿用既有通知权限与提醒流程 |
| Windows 原生适配 | 区域验证/陈旧区域/DPI 换算；初始化失败与 3 秒超时恢复系统装饰 | 鼠标与键盘实际控制、跨屏后区域一致、高对比度与 Narrator；未出现无控件窗口或双套按钮 |

原生 GUI 验收仅使用专门测试数据。实际 BYOK 调用需要已有用户授权和测试凭据，不把密钥写进本变更、测试快照或 CI 日志。没有 Windows/Linux 环境的 agent 可以完成对应实现、静态检查和流水线准备，但必须留下这些原生验收任务未勾选，不能标记整项适配完成。

## Risks / Trade-offs

| 风险/取舍 | 处理 |
| --- | --- |
| Windows 外观融合需要维护原生适配 | 仅 Windows 编译；先原生命中验证再做样式，保留故障降级，禁止以三个点击按钮宣称完成 |
| Linux 多一条系统栏，与 macOS/Windows 视觉结构不同 | 接受桌面环境差异；44px 应用工具栏保持紧凑，保留原生窗口功能 |
| 960px 最小宽度限制部分 Windows Snap 分区 | 明确记录当前边界，测试可容纳的分区；窄屏正文与侧栏响应式作为独立后续项，不在本次仅降低 minWidth 掩盖溢出 |
| Linux 不同桌面/会话的装饰、字体和 WebKitGTK 表现不同 | GNOME/KDE 与 X11/Wayland 单列证据；不把 UA 模拟当作原生兼容验证 |
| macOS 全屏安全区切换与控制临时显现 | 基于原生状态且测试真实命中；若重叠或长期空槽存在，对应任务不能完成 |
| 旧 `dist` 或错误平台配置混入打包 | 每目标重建前端，beforeBundle 核对 manifest，故意错配作为负向测试 |
| 原有最低 macOS 11.0 声明与前端依赖能力未经认证 | 独立发布复核项；不在本次外壳适配顺带补旧系统 polyfill，也不将现有声明当作兼容证据 |
| 原有 macOS 偶发窗口重绘异常 | 保留 `43-planner-gui-verification-2026-09-15.md` 的证据；本次新增外壳回归必须记录是否复现，不能用 CSS 强制重绘掩盖未经定位的问题 |
| 当前主规范固定顶栏仍有早期引导表述 | 本次 delta 以当前已用的 Later/页面/工具分组整体替换该要求；其他旧规范债务不混入 |

## Migration Plan

1. 先落平台常量、配置隔离、目标依赖和构建校验；Windows 先验证原生命中、窗口控制和失败降级闭环，再让现有 WindowBar/Coach 消费平台边界；保持同一 app identifier、窗口 label 与数据契约。
2. 逐平台执行配置/组件/原生构建，之后补真实安装与窗口验收。证据写入独立跨平台验收记录，分清配置通过与原生通过。
3. 实施验证完成后同步根 `design.md`、`docs/architecture.md` 和 OpenSpec 主规范，再由独立发布工作处理签名与版本发布。
4. 无数据库迁移。若某平台回归，撤回本次外壳/构建改动并重建该目标即可；不要删除用户数据或用改 identifier 的方式回避问题。


## v0.1.1 复核补充

用户要求发布前再次检查路径、平台服务与控件一致性。应用选择器采用 Radix Select 的 headless primitive，由 `src/ui/Select.tsx` 统一封装样式及 popper 定位；使用触发框宽度、客户区碰撞避让、滚动和内层 Escape。业务页面只提供值、选项和保存回调，不分叉平台版本。排期浮层与菜单共享定位，但分别使用 dialog/menu 语义，表单键盘不由菜单导航截获。路径及平台机制审计仅修复可定位的问题；更大的未验证兼容范围单独列于审计记录，不以本机模拟代替 Windows/Linux 原生验收。

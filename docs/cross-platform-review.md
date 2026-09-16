# v0.1.1 跨平台代码复核

本轮检查覆盖应用运行路径、配置文件、系统凭据、通知、窗口外壳、快捷键、共享表单与发布脚本。代码检查、组件模拟和 CI 原生构建分别记录，不把它们当成 Windows/Linux 桌面实测。

## 隔离边界

| 边界 | 唯一入口与责任 | 验证重点 |
| --- | --- | --- |
| 应用数据 | `db::data_dir` 调用 Tauri `app_data_dir`；数据库、供应商元数据与日志通过 Path 拼接 | 不从工作目录推导路径，不硬编码用户名或盘符 |
| 用户 AI 指令 | `ai::skills::directory` 解析系统 home，persona/SKILL 保留在该用户的 `.goal` | 空格和中文目录；默认文件只安装缺失项；错误包含实际路径 |
| 凭据 | `providers::credentials` 接口与 Cargo target features | macOS Keychain、Windows Credential Manager、Linux Secret Service；无明文回退 |
| 通知 | `platform::notifications` 提交，reminder service 维护应用内到期状态 | 不伪报权限已授予；提交错误可观察；成功不代表用户已看到通知 |
| 窗口 | `src/lib/platform.ts`、`useDesktopWindow`、Rust `platform` 模块 | Windows 原生命中模块仅按目标编译；macOS 保留红绿灯；Linux 保留系统装饰 |
| 选择与浮层 | `src/ui/Select.tsx` 与 `Popover.tsx` | 应用内统一锚点、边界滚动、焦点与键盘；不按平台复制业务表单 |
| 日期与语言 | ISO 日期作为身份，Intl 仅负责显示，i18next 本地资源 | 切换语言或时区不改变任务与日期身份；系统控件语言不参与业务判断 |
| 构建 | Tauri 平台覆盖、`desktop-build.json` 与六个独立 runner | 前端与本机目标一致；Node 调用 CLI 时使用参数数组 |

## 已确认的修复

- 原生 select 在不同操作系统中的覆盖定位不一致：语言、周起始日、日志等级、供应商/模型、API 格式改用统一的应用内选择器。
- 排期浮层此前使用菜单语义并拦截输入框的方向键：现在表单浮层单独使用非模态 dialog 语义，菜单导航只作用于菜单项。
- 浮层增加客户区最大尺寸和内部滚动，避免小窗口或高缩放时必要操作被截断。
- Windows 初始化阶段 IPC 失败时撤掉等待中的自绘按钮，配合后端原生装饰恢复；已进入自绘模式后的瞬时失败保留窗口控制。
- 通知使用同步系统提交边界，设置页展示“由系统管理”和最近一次提交错误，避免异步插件吞掉错误后宣称送达。
- npm 脚本通过 Node 和 npm JavaScript 入口调用，避免 Windows `.cmd` 直接执行限制；persona 错误显示真实路径，配置连续保存测试包含空格和中文目录。

- 更新 macOS 最低版本为 13.3、Windows WebView2 为 111，Vite 目标对应 Safari 16.4 / Chrome 111 / Firefox 128。最低版本是能力合同，仍需对应旧设备原生认证。[Tailwind 浏览器要求](https://tailwindcss.com/docs/compatibility)

## 核对后排除的误报

`std::fs::rename` 并非在 Windows 必然拒绝覆盖文件。Rust 标准库明确提供替换已有文件的语义，Windows 实现使用 MoveFileExW 等系统 API。本轮保留标准库边界，通过跨平台的连续保存/重开测试验证配置持久化，不另造一套 Windows 文件替换实现。[Rust rename 契约](https://doc.rust-lang.org/std/fs/fn.rename.html)

## 仍须原生环境验证

- Windows 11 的 Snap、系统菜单、Narrator、高对比度、100–200% DPI 和跨屏命中。
- Linux GNOME/KDE 的 X11/Wayland 会话、Secret Service 锁定状态与实际系统通知。
- 支持的最低系统/WebView 版本在干净设备上的安装启动和中文字体回退。
- 当前 macOS 自签名更新的凭据身份连续性不等于 Developer ID、公证或系统信任。

这些环境项沿用 OpenSpec 平台变更中的未完成任务；测试发现的更大架构债务应单独拆分，不混入本次控件修复。

## 验证结果

2026-09-16：Rust 1.88 全量 496 项通过（8 项显式忽略）；前端 54 文件 / 365 项通过；OpenSpec strict 26 项通过。官方 npm registry 的 clean install 完成，npm audit 0 个已知漏洞。依赖许可清单覆盖 736 个包，六目标分发图缺失许可证文本为 0。

## 分离记录的后续事项

菜单当前在滚动、窗口缩放、菜单自身尺寸改变时重定位。仅由父布局改变造成的锚点移动没有持续追踪；若以后支持在菜单打开期间连续改变侧栏宽度，应在共享浮层层补锚点定位，不在业务表单中逐处修补。

最终 macOS ARM64 自签名 Goal.app（0.1.1）在约 1000×625 原生窗口完成中英切换、选择器定位、底部周起始日菜单向上翻转、两级 Escape、API 格式选择与通知系统管理状态检查。当前没有已保存的 provider/model，激活选择器保持空态；没有发送 LLM 请求。语言最终恢复简体中文。

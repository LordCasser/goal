# 桌面平台外壳实施与验证

变更：`adapt-desktop-platform-shell`。实施前 HEAD 为 `cf27605f05aba3b8f8975a0efd1baef19849f2ea`，工作树已有大量 UI、Coach 与领域修改，本次在其上实施外壳适配，保留既有修改。

## 数据与身份边界

- 产品 `Planner`、版本 `0.1.0`、identifier `dev.lordcasser.planner`、主窗口 label `main` 保持不变。
- 数据路径仍由 `db::data_dir → app.path().app_data_dir()` 解析，数据库仍为 `planner.db`。
- 凭据服务名保持 `providers::credentials::SERVICE = dev.lordcasser.planner`；仅调整各目标启用的 keyring 后端。
- 窗口状态与几何是瞬态 OS 集成数据，不进入 SQLite、providers.json、Coach 工具目录或用户设置。本变更无数据库迁移。
- 当前本机为 Apple Silicon macOS。Windows 原生命中模块可做交叉类型检查；不能以此代替 Windows GUI、Snap、DPI、Narrator 或安装测试。Linux 安装/桌面验证同样需相应环境。

## 实施前原生基线

见 [GUI 基线](../evidence/platform-shell-2026-09-15/native-gui-baseline.md)。现有审查应用为 `Planner UX` / `dev.lordcasser.planner.uxaudit`；这是已有测试应用身份，不是本次更改生产 identifier。Luna 实测 Coach、设置、最大化及原生全屏往返，未修改计划或调用 LLM。

## 验证结果

代码与当前 macOS 环境可执行的验证已完成；Windows/Linux 原生验收、macOS universal 分发及精确尺寸/临时全屏控制检查仍未完成。OpenSpec 保留这些任务，不归档为全平台完成。

## 磁盘

本项目初次检查约 231 MiB，主要是 node_modules（190 MiB）；无遗留 Cargo target。Rust 验证使用禁用增量缓存、关闭开发/测试调试符号的配置，构建结束保存必要产物后清理 target，不删除用户数据与分析证据。

## 已完成的自动验证

- 前端类型检查通过；前端全量 47 文件 / 295 测试通过，见 `frontend-tests.txt`。覆盖四平台控件边界、窗口状态、降级、异步 listener 清理、Later/Coach 输入守卫。
- 构建契约 24 测试通过，包含配置合并、目标归一、平台/版本/缺失 manifest 拒绝。三个目标 Cargo feature tree 分别留存。
- macOS Rust 全量测试退出码 0；几何修订后 3 个几何测试再次通过。新增 `persistence_baseline.rs` 使用 tempdir，验证已保存任务、Coach 待确认预览与快照在关闭/重开后保留，拒绝后恢复原值。
- Windows `geometry.rs + windows_frame.rs` 使用 Rust stable 1.88 的 `x86_64-pc-windows-msvc` 目标完成隔离类型检查。没有执行 Windows SDK 链接或 Tauri 整体 Windows 编译，不能视为原生验收。临时检查工程已 clean。
- 本机以低缓存配置生成 arm64 debug `.app`，beforeBuild/beforeBundle 均实际通过。它不是 universal 分发包。

## 首轮原生发现与修复

首次 macOS 新包通过窗口启动和全屏往返，但计划与设置 IPC 被拒绝。原因是新增 Tauri AppManifest 后启用了整个应用的命令授权检查，而不只是新增的三个窗口命令。GUI 实测捕获 `get_settings / set_theme / get_ai_settings not allowed`，详见 `native-gui-after.md`。修复须从既有注册清单生成有限业务授权，并将窗口权限继续限制在 Windows/main；后续复测记录另附，首轮失败证据保留。

Windows 几何复核发现正常 resize/minimize 会使已发送的 DOM 尺寸过期。现在陈旧修订直接忽略，尺寸不匹配时隐藏旧命中区等待新测量；只有非法范围或原生错误才永久恢复系统装饰，避免跨 DPI 操作无故退化。

## 未完成的验收与独立债务

- Windows：完整 MSVC 构建/安装、Windows 11 Snap、系统菜单/单次点击、100/125/150/200% DPI、跨屏、Narrator、高对比度和触控均仍需真机证据。
- Linux：AppImage/deb 原生构建和安装、GNOME/KDE、X11/Wayland、系统凭据服务有/无时的真实行为仍未验证。
- macOS：本轮是 arm64 debug 构建；universal 两架构、dmg 安装、签名/公证、最低 macOS 11.0 认证不标记完成。当前工具无法精确设置客户区大小或可靠召出全屏临时控制，因此相应验收仍保留。
- 960px 最小宽度会限制较窄的 Windows Snap 分区。更窄正文/双侧栏布局是独立响应式改造，不在本次仅调小 minWidth。
- 原有重新激活绘制问题、三平台签名发布与最低系统版本认证单独跟踪，不并入窗口外壳修补。
- 主规范全量校验曾发现既有 `agent-conversation` 与 `agent-proposals` 共 3 条需求缺 Scenario，本次不改其业务内容；受影响的 `planner-workspace` 工具栏条目已补可验证场景。新增 `desktop-platform`、更新的 `planner-workspace` 与本 change 的 strict 校验通过。全库不能宣称全部通过。


## 最终复测与保留产物

- [最终原生记录](../evidence/platform-shell-2026-09-15/native-gui-final.md)：重建后 Workspace 正常加载；General/AI 设置读取无权限错误，白底/灰底双向切换成功并恢复白底；Later 与模态守卫通过，原生红绿灯和全屏往返通过。生产测试数据没有周期，因此 Coach/Issues 空状态下禁用，不把它算作带数据的面板验收。
- ACL 修复最终从唯一的 `invoke_handler` 注册生成 OUT_DIR 内有限权限：一项业务组、三项窗口权限。删除重复的逐命令权限文件，原有命令和 Windows 隔离由 `desktop_acl.rs` 回归校验。
- 最终 Rust 复测：369 个库测试通过、7 个既有测试忽略；新增 ACL 和持久化集成测试各 1 项通过，见 `rust-final.log`。首轮完整集成套件也通过。前端最终再次执行为 47 文件 / 295 测试通过，见 `frontend-final.log`。
- 本地保留 `.artifacts/desktop/Planner.app`（macOS arm64 debug，约 54 MiB），未签名、未发布，不是 universal release。二进制 SHA-256：`b9d532b6cc14341bb90c0ada2736cdeb4b081650ddff7cbb36cfd58a31f7247d`。应用保持打开。
- `cargo clean --manifest-path src-tauri/Cargo.toml` 删除 9575 个构建文件，释放 4.0 GiB。清理后项目约 286 MiB（含 190 MiB node_modules、54 MiB 保留应用），日志和源码保留。由本次 DMG 布局脚本挂载的临时卷已卸载，失败中间产物随 target 清除。
- 本次未提交、推送或发布。原先工作树的其他 UI/AI 改动保留。

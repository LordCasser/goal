## 自动验证

- `openspec validate refine-task-interactions-and-continuity --type change --strict`、`openspec validate add-calendar-time-view --type change --strict`：通过。
- `openspec validate --specs`：20 份主规范通过。
- `cargo test --manifest-path src-tauri/Cargo.toml -q`：本轮完整测试通过；macOS release 原生包重新编译通过。执行 `cargo clean --profile dev` 清理测试构建残留，保留 release 应用供用户手动测试。
- `npm test`：66 个测试文件、545 项通过；输入行末尾显示、恢复项层级操作、单项列表把手显露，以及计划与 Later 标题双击后的文字选区清理均有前端回归测试。
- `cargo test --manifest-path src-tauri/Cargo.toml --test later --quiet`：14 项通过，覆盖 Later 恢复项位于输入行前，以及跨周期目标链接和多空行。
- `npm run build`、macOS Tauri bundle、`git diff --check`：通过。Vite 仍提示既有主包超过 500 kB。

## macOS 实机

此前使用独立 bundle ID 和隔离数据的 `Goal Smoke.app` 验收交互；本轮另对正式 `Goal.app` 作只读显示检查：

- 双击事项打开详情、备注保存后重开仍存在；同标题事项在周一至周三显示历时 3 天、记录 3 天，周三完成后结束该段。
- 双击标题后关闭详情，焦点返回原标题编辑框，原生整词选区不再残留；备注保存路径和 Later 行选区由前端回归测试覆盖。
- 开启自动导入后，显式创建次日日计划时带入上日未完成项，备注为空；稍后事项数量随内容变化并受设置开关控制。
- 页面右键显示应用菜单而非原生网页菜单；文本剪切、复制、粘贴与 Shift+F10、Escape 焦点恢复均已操作验证。
- 浅色和灰色主题下，完成态勾选框显示为细边框与勾号；右侧独立排序按钮已移除。行首小把手默认隐藏，悬停任务行或键盘聚焦时显示，包括只有一项的列表；方向键排序和鼠标拖动在同一日计划内均能改变顺序；跨计划列表的拖动不写入由前端测试覆盖。
- 重新构建并打开正式 `Goal.app` 后，原先存储顺序为“输入行 → 恢复项”的第 39 周计划显示为“恢复项 → 输入行”；无需修改已有用户数据。
- 标题栏保留 `data-tauri-drag-region`，已在应用窗口执行拖动手势；当前 UI 自动化接口不提供拖动前后的窗口坐标，不能据此断言窗口位移。

## 其他平台

Windows 和 Linux 的平台配置检查通过。当前没有这两种系统的图形桌面或测试机，用户也确认暂无测试环境，因此无法声称右键剪贴板、键盘焦点和标题栏拖动已经通过实机验收；在具备环境后仍需逐项复核。

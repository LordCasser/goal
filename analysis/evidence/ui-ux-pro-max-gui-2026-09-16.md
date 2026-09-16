# UI/UX Pro Max 原生 GUI 验收 — 2026-09-16

## 范围与包

- 先通过 CUA 关闭旧的 `planning-review-final/Goal.app`，再使用准确的完整路径打开签名包：
  `/Users/lordcasser/workspace/projects/goal/.artifacts/desktop/uiux-review/Goal.app`。
- 仅做短验收；未运行构建、未改生产代码、未调用真实 API、未保存新的长期周期或 Provider，也未写入新的密钥/计划数据。

## 结果

### 长期列日期旁的进度检查入口

- 中文工作台的长期列在日期旁显示紧凑入口：`进度检查安排: 9月29日 检查一次`。
- 点击后出现锚定的小 Popover，内容包含 `共 28 天`、`1 次进度检查` 与 `9月29日`；截图中未出现整条灰底 disclosure bar。
- Escape 关闭 Popover，AX 焦点明确回到原触发器 `进度检查安排: 9月29日 检查一次`。第一次工具调用因“应用状态已变化，请重新查询”被拒绝，未验证按键送达；刷新 AX 后的有效调用完成关闭，不能将工具重试解释为应用需要按两次。

### 长期周期 Custom / Repeat / Once

- `zh-CN` 自定义表单为紧凑布局，显示开始日期、结束日期、`定期检查 / 指定一次`、间隔数值、单位下拉与日期预览；单位下拉可见 `天 / 周`。
- 在中文表单中用方向键 `Right` 从 `定期检查` 切到 `指定一次`，`Left` 切回；切换后表单相应显示一次检查日期或重复间隔。
- 下拉中用 `Up` 聚焦 `天`，`Return` 仅改变草稿单位为 `天`，未保存周期；随后点击取消。
- 切换到 English 后复验：表单显示 `Custom`、`Repeat / Once`、`Every`、`Check interval unit`，下拉显示 `days / weeks`；方向键可在 `Repeat` 与 `Once` 之间切换，随后点击 `Cancel`。

### Provider 空态与 Header 行

- 中文 AI 模型页空态没有高灰空栏；`添加供应商` 与 `管理供应商与模型` 标题同行，空态说明正常显示。
- 打开 `添加供应商` 后未输入名称、地址、密钥或模型，`测试并保存` 保持 disabled。
- 点击 `添加请求 Header` 添加临时 Header 行，再点击 `移除 Header 1` 删除；AX 焦点回到 `添加请求 Header`，焦点落点合理。
- 未保存 Provider，未向真实服务发起请求。

## 最终状态与截图

- 通过设置短暂查看白底，再恢复灰底；语言最终恢复为 `简体中文`，灰底开、白底关，设置对话框已关闭。
- CUA 已输出两张局部干净截图用于现场查看（进度检查 Popover、Provider 表单）。当前 CUA 接口仅支持返回/展示截图字节，不支持将截图直接写入指定目录，因此未虚构 `.artifacts/audit/uiux-review` 下的截图路径。

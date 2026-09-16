# v0.1.1 发布就绪说明

自动化测试、构建矩阵和 OpenSpec 格式校验不能替代真实平台和用户流程验收。

v0.1.1 本机已通过 54 个前端测试文件（365 项）、496 项 Rust 测试，以及 17 份主规范和 9 份变更的 strict 校验。覆盖语言保存失败、语言切换、请求级 AI 输出语言、审批 freshness/拒绝、重复模板随日历移动不重复生成；两种语言的资源键和插值参数完整。macOS ARM64 原生 Goal 窗口已验证中英工作台、日历、Coach 和设置布局，任务正文保持原样；本次 GUI 环境没有已激活的 BYOK provider，AI 输出语言由请求捕获测试验证，未声称完成真实供应商回归。

v0.1.1 的以下验收项尚未全部完成，发布时保留这些限制，后续在相应环境逐项验证：

- macOS、Windows、Linux 安装启动，以及窗口控件、全屏、DPI、主题和跨平台数据链路；Windows 还要覆盖 Snap、缩放和辅助功能，Linux 要覆盖实际桌面会话。
- Linux 凭据验收需要运行中的 Secret Service（例如 `gnome-keyring`）；服务不可用时应明确报错，不能出现明文回退。
- 真实供应商连接、本地模型端点断网运行、供应商删除后的凭据清理，以及 AI 澄清和计划审查的完整用户路径。
- 日历拖动与合并、提醒和错过摘要、免打扰与通知权限、全新目录引导、退出调查、反馈重试和长周期复盘。

发布工作流覆盖 macOS、Windows、Linux 的 amd64 与 arm64 目标。版本 tag 会在全部目标构建与产物校验通过后创建 GitHub Release；手动运行只提供 Actions artifacts。这代表构建合同，不代表每个目标已经完成原生 GUI 验收。

当前 macOS 产物使用固定证书和 designated requirement 的自签名身份，未使用 Apple Developer ID，也未公证；Windows 产物未签名。发布准备只使用临时钥匙串，恢复原 search list 后删除，不安装系统 trust anchor，也不改变用户的 trust 设置。旧 ad hoc 包迁移时已有钥匙串项目可能需要一次授权；连续性检查已验证后续同签名更新无需系统弹窗。发布说明必须保留这些限制，不能把构建成功描述为平台信任已经建立。

语音输入不属于 v0.1.1 交付范围。它的规范保留为后续能力，是否实现及其实现载体另行决定。

最终 macOS ARM64 自签名 Goal.app（0.1.1）在约 1000×625 原生窗口完成中英切换、选择器定位、底部周起始日菜单向上翻转、两级 Escape、API 格式选择与通知系统管理状态检查。当前没有已保存的 provider/model，激活选择器保持空态；没有发送 LLM 请求。语言最终恢复简体中文。

## 已发布产物验收

[v0.1.1 Release](https://github.com/LordCasser/goal/releases/tag/v0.1.1) 已发布。[六目标流水线](https://github.com/LordCasser/goal/actions/runs/35001301041) 的原生测试、构建、架构检查与上传全部成功。发布代码为 `5cf5988`，正式 tag 保持不变。

从公开下载地址获取的八个安装包全部匹配 `SHA256SUMS`。两份 macOS DMG 额外通过固定签名身份、Goal 产品名、0.1.1 版本、13.3 最低系统版本和各自架构检查；Linux AppImage 的 ELF 架构以及 Debian 包架构/版本元数据一致。Windows 安装包完成下载校验，其应用二进制架构已由对应 CI runner 验证；此记录不代表 Windows/Linux 的人工桌面验收。

公开下载的 macOS ARM64 发布包已实际启动，简体中文偏好保留，工作台、设置及语言菜单正常，没有意外系统弹窗。此 smoke 未配置模型或调用 LLM，不将启动结果表述为真实供应商或钥匙串读取回归。

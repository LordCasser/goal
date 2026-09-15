# Goal

Goal 是一个本地优先的个人规划应用，把长期目标、周计划、日计划和专注块放在同一条链上。任务可以独立存在，也可以关联到周目标和长期目标；长期目标的颜色会沿关系显示在工作台和日历中。

当前公开版本是 `v0.1.0`。公开产品名为 **Goal**；当前安装的桌面应用显示为 **Planner**，bundle identifier 是 `dev.lordcasser.planner`。

## 能做什么

- 在长期、周、日三个层级建立计划，任务可以独立保留，也可以建立父目标关系。
- 在 Workspace 中编辑任务、排序、完成状态、Later 暂存项和长期目标颜色。
- 在 Calendar 的月 / 周视图查看日计划，在单日 Plan 和 Schedule 视图安排专注块。
- 配置自己的云端或本地模型（BYOK），支持 Anthropic Messages、OpenAI Chat Completions 和 OpenAI Responses 三种 API 格式。
- 使用 Coach 进行规划、目标澄清、优先级、复盘和时段分析。会改变本地数据的操作先进入预览和确认流程。
- 使用 Plan issues 查看规则提示；配置并测通模型后，可以运行 AI 检查，并逐项忽略或定位问题。
- 在设置中切换浅色主题、周起始日、日志级别和 Coach 上下文保留时间。

语音输入、云同步和自动更新不属于 `v0.1.0` 的可用能力。应用没有账户系统，也没有遥测上报；AI 请求只会在用户配置并调用供应商时发生。

## 下载

发布资产会放在 [Releases](https://github.com/LordCasser/goal/releases/latest)。当前发布约定如下，具体文件名和可下载资产以 Release 页面为准：

| 平台 | 目标架构 | 预期资产 |
| --- | --- | --- |
| Linux | `x86_64` | `.AppImage`、`.deb` |
| Linux | `aarch64` | `.AppImage`、`.deb` |
| Windows | `x86_64` | NSIS `.exe`（未签名） |
| Windows | `aarch64` | NSIS `.exe`（未签名） |
| macOS | `x86_64` | `.dmg`（固定自签名，未公证） |
| macOS | `aarch64` | `.dmg`（固定自签名，未公证） |

这些是发布 workflow 的六个目标，不代表本机已经完成所有原生 runner 构建。

### macOS 首次打开

当前 macOS 包没有 Developer ID，也没有经过 Apple notarization。构建使用固定的自签名身份，以便同一签名的后续更新尽量保持钥匙串身份；从旧的 ad hoc 包换到固定签名包时，macOS 可能会再次询问钥匙串访问，之后同签名更新通常可以继续使用原身份。

Gatekeeper 可能阻止首次打开。确认包来源可靠并检查校验值后，按 Apple 的说明在“系统设置 → 隐私与安全性”中使用 **Open Anyway**：<https://support.apple.com/en-gb/102445>。不要通过全局关闭 Gatekeeper 来安装应用。

## 开发

需要 Node.js `>=22.12.0` 和 Rust `>=1.88`。

```sh
npm ci
npm test

# 启动 Tauri 桌面开发环境
npm run tauri dev

# Rust 单元测试与集成测试
cargo test --manifest-path src-tauri/Cargo.toml --locked
```

桌面目标、平台配置、打包前检查和签名边界见 [`docs/desktop-build.md`](docs/desktop-build.md)。用户操作说明见 [`docs/user-guide.md`](docs/user-guide.md)，数据边界见 [`docs/privacy.md`](docs/privacy.md)。

## 仓库边界

公开仓库只放可维护的源代码、必要文档和构建所需资源。原始逆向材料、第三方打包源码和本机验收记录不作为公开发行内容。

## 许可

许可证文件由仓库维护者另行提供。

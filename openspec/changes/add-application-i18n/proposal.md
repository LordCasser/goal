## Why

Goal 当前界面混合中英文，日期、数量、错误和 Coach 状态各自拼接文案，无法可靠切换语言。`v0.1.1` 发布前建立统一国际化边界，首批完整支持简体中文和英语。

## What Changes

- 设置 → 通用增加语言选择，立即生效并持久化；首次按系统语言匹配，其他语言回退英语。
- 全部内置界面文案、辅助技术标签、动态数量及日期时间通过语言资源和格式化入口呈现。
- 后端保存语言偏好，通知与 AI 生成入口使用该偏好；错误按稳定 code 本地化，用户内容和技术标识保持原样。
- 为新增语言建立资源完整性与参数一致性校验，以及中英文组件、持久化和真实窗口验收。

## Capabilities

### New Capabilities

- `application-i18n`: 全应用语言资源、选择、格式化、动态状态、AI 和原生界面的语言契约。

### Modified Capabilities

无；新增横切语言契约适用于现有 `planner-workspace`、`agent-conversation`、`planning-issues`、`desktop-platform` 和提醒能力，业务规则保持不变。

## Impact

基于已实现的 `rebuild-baseline`、设置、Coach 和桌面平台能力。涉及 React 展示层、Rust 设置/通知/AI 入口、IPC ACL、测试和发布文档；引入 `i18next` 与 `react-i18next`，资源本地打包，不使用翻译网络服务。不新增数据库实体，不改计划身份、用户文本或凭据存储。

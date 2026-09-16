## Why

供应商表单无法配置网关或模型服务需要的请求 Header，既有 `extra_headers` 也未进入连接测试和真实采样。需要将填写、验证、存储与请求行为接通，并避免认证 Header 明文落盘。

## What Changes

- 在供应商表单中增加紧凑的可增删 Header 键值行，提供中英文标签、行内校验和已保存值的替换提示。
- 供应商导航改为轻量内容列表，空列表不占侧栏，添加入口移至管理标题旁。
- Header 属于供应商，全部模型的连接测试及真实 AI 请求共用；修改后必须重新测试再保存。
- 所有 Header 值存入系统凭据存储，配置及设置返回只包含名称；不增加逐行敏感开关。
- 三种协议共用 Header 合并规则：自定义项按不区分大小写的名称覆盖协议默认项，传输层管理的 Header 不允许覆盖。

## Capabilities

### New Capabilities

无。

### Modified Capabilities

- `ai-access`: 供应商自定义 Header 的编辑、凭据存储、测试和采样一致性。

## Impact

依赖已实现的 `rebuild-baseline`、`add-ai-access-and-voice` 和 `add-application-i18n`。涉及供应商表单、IPC、配置/凭据存储、采样客户端与 AI provider 解析。不增加依赖，不修改本地计划数据，不在本次变更发布新版本。

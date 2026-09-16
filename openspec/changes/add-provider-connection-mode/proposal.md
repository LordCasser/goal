## Why

企业 Windows 的系统代理例外规则可能无法被 HTTP 库完整解释。保留本机 LLM 回环直连，并允许用户按供应商选择连接方式，使连接测试与实际生成使用一致、可控的路由。

## What Changes

- 修改既有 AI 访问能力：供应商保存 `auto`、`direct`、`proxy` 三种连接方式，默认 `auto`。
- 本机回环地址始终直连；其他地址在自动模式下遵循现有代理环境，直接连接模式忽略代理，指定代理模式只使用填写的代理，失败不静默回退。
- 高级设置按需展开代理地址，支持 HTTP、HTTPS、SOCKS5、SOCKS5h；本次支持无需认证的代理，拒绝 URL 内嵌账号密码，不将凭据写入配置。
- 中英文文案、保存/验证状态和实际调用完整贯通。
- 不实现 Windows 原生代理解析、PAC/WPAD、代理认证或系统设置修改。

## Capabilities

### New Capabilities

无。

### Modified Capabilities

- `ai-access`：供应商级连接方式、回环直连和路由一致性。

## Impact

前置依赖为现有 `rebuild-baseline`、AI 访问和供应商设置实现。影响 `openspec/specs/ai-access/spec.md`、供应商配置与 IPC、共享 sampling HTTP 边界、供应商表单与 i18n；为既有 reqwest 开启 SOCKS 支持，不新增网络栈或平台专属依赖。

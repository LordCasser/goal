## Context

现有供应商配置和三种采样协议共享同一采样入口，连接测试也复用采样请求；当前已对回环地址显式直连，其他地址依赖 reqwest 默认代理行为。该变更需要把供应商级连接策略带入这条边界，并保证本机回环与显式代理规则在测试和真实采样中一致。行为约束见 `specs/ai-access/spec.md`，动机与范围见 `proposal.md`。

## Goals / Non-Goals

**Goals:**

- 让 `ProviderConfig.connection` 与 `SamplingRequest.connection` 共享同一 `ConnectionSettings` 契约，并以 `auto` 为默认值。
- 在一个网络边界集中校验代理 URL、识别回环目标并构造 reqwest 客户端。
- 让三种协议和连接测试使用同一条路由决策，且失败不改变路由。
- 让设置表单的高级连接选项、dirty 状态、测试与保存状态完整贯通，并覆盖中英文文案。

**Non-Goals:**

- 不实现 Windows 原生代理解析、PAC/WPAD、系统代理修改或自定义 bypass 列表。
- 不支持代理认证、URL 内嵌凭据、按模型连接设置或独立网络栈。

## Decisions

1. **共享连接值对象。** `ConnectionSettings` 使用封闭联合：`{ mode: "auto" }`、`{ mode: "direct" }`、`{ mode: "proxy", url: string }`。供应商配置和采样请求只携带这一对象，避免测试路径和实际调用分别解释模式。读取缺少字段的既有配置按 `auto` 解释；非 `proxy` 形态不保留代理 URL。

2. **集中网络决策。** 在 `src-tauri/src/network.rs` 放置无 IO 的 URL 校验、主机分类和路由决策，再由统一 reqwest 构造器应用结果。回环判定覆盖大小写不敏感的 `localhost`、末尾点、`127/8`、`::1` 与 IPv4-mapped loopback；回环目标始终禁用代理。`auto` 保留 reqwest 默认代理，`direct` 显式禁用代理，`proxy` 先禁用系统代理再只安装指定代理，不实现失败回退。

3. **严格代理 URL。** 只允许 `http`、`https`、`socks5`、`socks5h`，要求主机和显式 `1..65535` 端口，路径只能为空或 `/`，拒绝 query、fragment、userinfo 和空白。代理值作为普通配置字段保存但不含认证秘密；不增加钥匙串字段。

4. **配置与验证边界。** 供应商连接设置变更进入 dirty 草稿，不改写旧配置的验证标记；此时禁用已保存配置的独立测试。测试并保存从同一草稿构造 `SamplingRequest`；只有所有模型通过后才提交配置，失败保留旧配置并返回脱敏错误。三种协议只负责各自的 payload/SSE 转换，不自行决定代理。

5. **渐进式表单。** 高级设置默认折叠，已有非 `auto` 连接设置时展开；模式选择和代理 URL 条件输入由同一表单状态驱动。连接字段变化进入 dirty 计算，测试成功才清除 dirty；所有可见文案、错误和状态使用既有中英 i18n 资源。

## Risks / Trade-offs

- [回环地址与代理环境不一致] → 在统一网络决策中先做字面主机分类，并为每种回环写请求级测试。
- [系统代理环境使 `auto` 测试不稳定] → 测试中显式设置/清除代理环境，并用本地目标与本地代理断言实际到达路径。
- [SOCKS 支持扩大 reqwest 功能面] → 只启用既有 reqwest 的 SOCKS feature，不引入新的网络客户端。
- [代理 URL 误填造成不可达] → 在保存前执行严格结构校验；代理失败只报告失败，不尝试改变用户选择。

## Migration Plan

`providers.json` 不新增数据库迁移。缺少 `connection` 的现有供应商读取为 `auto`，下一次成功保存时写出显式连接值；连接设置变化按既有验证状态规则重新测试。实现失败或测试失败时保留原配置，不需要回滚网络环境或系统代理。

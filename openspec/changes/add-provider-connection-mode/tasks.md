## 1. 数据

- [x] 1.1 为供应商配置和采样请求加入共享 `ConnectionSettings` 契约，默认 `auto`，并确保代理 URL 只存在于 `proxy` 形态；验证序列化往返、旧配置缺省值和敏感信息脱敏。
- [x] 1.2 将连接设置纳入供应商保存与验证状态判定，失败保留旧配置、成功后持久化完整设置；验证草稿变更需重新测试、已保存配置重测失败清除验证状态且不会写入代理凭据。

## 2. 后端

- [x] 2.1 在统一网络边界实现代理 URL 校验、localhost/末尾点/IPv4 与 IPv6 回环识别，以及 `auto`/`direct`/`proxy` 的 reqwest 路由；验证 scheme、主机、显式 `1..65535` 端口、路径和 userinfo 的接受与拒绝矩阵。
- [x] 2.2 让连接测试和 Anthropic Messages、Chat Completions、Responses 共用该路由，并禁止代理失败回退；用本地目标、系统代理和四种代理 scheme 的集成测试断言实际请求路径与错误分类。

## 3. 前端

- [x] 3.1 在供应商高级设置中加入模式 Select、条件代理 URL、默认折叠/自定义时展开、dirty 状态和中英 i18n；验证三种模式的显示、校验和语言切换。
- [x] 3.2 打通草稿连接设置与连接测试/测试并保存的状态反馈，失败保留 dirty 草稿、成功清除 dirty 并显示验证状态；用组件测试覆盖保存、失败、重测和代理 URL 行内错误。

## 4. 验收

- [x] 4.1 运行 `cargo test`、前端相关测试与 OpenSpec 严格校验，并手工验证回环强制直连、direct 忽略系统代理、proxy 不回退以及三种协议路由一致。

验收记录：`analysis/evidence/provider-connection-mode-2026-09-16.md`。

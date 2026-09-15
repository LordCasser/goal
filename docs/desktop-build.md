# 桌面构建说明

桌面构建必须从 Tauri hook 进入。`TAURI_ENV_PLATFORM` 或 Tauri 提供的目标 triple 是原生平台的来源；脚本把 `darwin` 归一为 `macos`，前端只接收 `macos`、`windows`、`linux`、`web`。没有 Tauri 目标时，普通 `npm run build` 只生成 `web` 标记；原生 `before-dev`、`before-build` 和 `before-bundle` 会失败，避免旧的 `dist` 被静默打进错误平台。

本地构建使用 Rust 1.88 或更新版本、Node.js 22.12 或更新版本，依赖锁文件安装后执行：

```sh
npm ci
npm run desktop:config:check -- macos
npm test
CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test --manifest-path src-tauri/Cargo.toml --locked

# 由 Tauri 注入目标环境；不要手工把 DESKTOP_PLATFORM 当原生目标来源
npm run tauri -- build --target <rust-target>
```

Linux 的 secure store 测试需要真实的 Secret Service。CI 在独立的 `dbus-run-session` 中以一次性测试密码启动 `gnome-keyring-daemon --unlock --components=secrets`，再运行上述 `cargo test`；其他平台直接在本机 secure store 上测试。Linux 发布 runner 同时安装 `gnome-keyring` 和 `dbus-x11`。

构建 hook 会清除并重建当前目标的 `dist`，写入只含 `platform` 和产品 `version` 的 `dist/desktop-build.json`，打包前再次核对目标与版本。Tauri 的 JSON Merge Patch 按平台合并以下文件：

- `src-tauri/tauri.macos.conf.json`：`app`、`dmg`，最低 macOS 11.0。
- `src-tauri/tauri.windows.conf.json`：NSIS，启动时 `visible: false` 且 `decorations: true`。
- `src-tauri/tauri.linux.conf.json`：AppImage 与 deb，使用 WebKitGTK 4.1。

所有平台的完整 `app.windows` 数组都必须设置 `dragDropEnabled: false`。这是关闭 Tauri 原生文件拖放拦截，将任务排序、归属关联、日计划移动和时间轴排期交回 HTML5 拖放；不影响标题栏拖窗。只改基础配置无效，因为平台配置会替换整个窗口数组。配置检查脚本对此作强制校验。

应用授权由 build.rs 从现有命令注册生成：一项有限的业务命令组与三个 Windows 窗口权限，产物只存在于 Cargo OUT_DIR。公共 capability 仅授予本地主窗口业务组，Windows capability 额外授予窗口操作；不维护第二份业务命令名单，也不授予通配符。

依赖也按目标隔离：macOS 使用 keyring `apple-native`，Windows 使用 `windows-native` 和所需的 `windows-sys` features，Linux 使用 `sync-secret-service`。修改后分别以 `cargo tree --locked --target <rust-target> -e normal,build,features` 检查；当前锁文件中最高依赖 MSRV 为 Rust 1.88，因此验证 workflow 固定 Rust 1.88.0。工作流使用 Node 22.12.0、关闭 Cargo 增量调试缓存，并分别缓存 `src-tauri -> target`。

## 发布矩阵

`.github/workflows/release.yml` 是发布入口。它固定使用 Node 22.12.0、Rust 1.88.0，并以六个独立的原生 runner 构建；每个 job 都重新执行 `npm ci`、目标配置检查和 Tauri 构建。Linux runner 需要 WebKitGTK 4.1、GTK 3、GLib、librsvg、OpenSSL、DBus、libsecret、gnome-keyring、dbus-x11、pkg-config 和 patchelf；Windows runner 依赖 MSVC、WebView2 和 NSIS；macOS runner 依赖系统 SDK。

| 发布 ID | runner | Rust target | 产物 |
| --- | --- | --- | --- |
| `linux-amd64` | `ubuntu-22.04` | `x86_64-unknown-linux-gnu` | `.AppImage`、`.deb` |
| `linux-arm64` | `ubuntu-22.04-arm` | `aarch64-unknown-linux-gnu` | `.AppImage`、`.deb` |
| `windows-amd64` | `windows-2022` | `x86_64-pc-windows-msvc` | NSIS `-setup.exe`（未签名） |
| `windows-arm64` | `windows-11-arm` | `aarch64-pc-windows-msvc` | NSIS `-setup.exe`（未签名） |
| `macos-amd64` | `macos-15-intel` | `x86_64-apple-darwin` | `.dmg`（固定自签名，未公证） |
| `macos-arm64` | `macos-15` | `aarch64-apple-darwin` | `.dmg`（固定自签名，未公证） |

macOS 先执行 `prepare` 创建临时钥匙串并导入固定证书，再构建 `.app`；构建完成后执行 `sign → verify → package`，最后把 DMG 放入发布暂存目录。`release-artifacts.mjs` 只接受上述六个发布 ID，并在发布前验证每个目标的文件集合和 SHA-256。`workflow_dispatch` 只上传 Actions artifact；推送与版本一致的 `v<version>` tag 后，`publish` 才会校验全部六个目标并创建 GitHub Release。

这张表描述 workflow 的构建合同，不代表本机已经完成六个 runner 的原生验收。当前本机已验证配置、目标归一、前端标记、ACL 范围及静态 Rust/Cargo 依赖检查；真实 runner 构建、公证和 Windows 签名仍以发布运行结果为准。

## macOS 签名边界

当前公开发布固定使用自签名证书，不是 Apple Developer ID，也不做 notarization。没有无签名的 macOS fallback；发布 job 缺少证书、密码或签名身份会直接失败。发布说明会明确标注 `self-signed (not notarized)`，Windows 包也会标注未签名。

CI 只从 GitHub Secrets 读取以下敏感值，并且不应写入日志、仓库或发布说明：

- `APPLE_CERTIFICATE`：base64 编码的 P12 证书
- `APPLE_CERTIFICATE_PASSWORD`：P12 密码

本地发布工具的固定证书私钥放在 `~/.goal/release-signing`，永远不提交仓库；固定公开证书提交在 `.github/signing/macos-release.cer`。`prepare` 从该证书读取指纹，校验 P12 身份后将派生出的 `APPLE_SIGNING_IDENTITY` 写入 `GITHUB_ENV`，调用方不需要另配这个 secret。随后 `sign` 签 `.app`，`verify` 检查签名，`package` 生成 DMG，`cleanup` 在 job 结束时清理临时钥匙串。临时钥匙串和 P12 文件不得进入 artifact。

从旧 adhoc 包迁移到固定自签名身份时，macOS 可能首次再次要求用户允许钥匙串或应用访问；之后使用同一固定签名身份的更新保持身份连续。自签名不等于 Gatekeeper 已信任，也不替代公证；分发时应按发布说明引导用户在系统设置中选择“仍要打开”，不应要求用户全局关闭 Gatekeeper。

## 干净发布输入

发布应从干净 checkout 或等价的干净 snapshot 开始。snapshot 只包含源代码、锁文件、构建脚本、公开授权文件和必要的发布配置，不应把 `analysis/`、`.agents/`、根目录 `design.md` 或其他研究材料打进 release artifact。构建目录、`node_modules`、本地数据库、日志、P12 和临时钥匙串也不是发布输入。artifact 由 workflow 从 `src-tauri/target` 中按目标筛选后复制到 `release-artifacts/`；不要直接把整个 target 目录上传。

发布前至少执行：

```sh
npm ci
npm test
node scripts/release-artifacts.mjs verify-version --tag v<version>
npm run desktop:config:check -- macos
```

真正发布还必须让六个矩阵 job 都成功，并由 `release-artifacts.mjs verify` 验证六目标文件集合后再生成 `SHA256SUMS`。

磁盘紧张时，先复制或上传 `.app`、安装包、`build-evidence/` 和日志，再确认没有其他 agent 或构建进程使用 `src-tauri/target`，最后执行：

```sh
cargo clean --manifest-path src-tauri/Cargo.toml
```

只验证本机应用时使用以下命令，避免启动 DMG 的 Finder 布局脚本：

```sh
CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 npm run tauri -- build --debug --bundles app
```

不要在并行构建仍运行时清理共享 target。

# Third-party notices

本文件记录 Goal 发布包中由仓库或生产依赖带入的第三方运行时资产。`node_modules`、构建缓存和本地测试数据不是发布内容；依赖升级后应重新核对对应的许可文本。

## 已核对并随仓库提供

| 资产 | 许可 | 仓库中的完整文本 |
| --- | --- | --- |
| Halaska UI 相关资产 | MIT | [`public/licenses/halaska-ui.txt`](public/licenses/halaska-ui.txt) |
| Tauri decoration 相关资产 | MIT | [`public/licenses/tauri-decoration.txt`](public/licenses/tauri-decoration.txt) |
| IBM Plex Sans（`@fontsource/ibm-plex-sans` 5.3.0） | SIL Open Font License 1.1 | [`public/licenses/ibm-plex-sans.txt`](public/licenses/ibm-plex-sans.txt) |
| npm 生产依赖、Cargo.lock registry 依赖 | 以各包 metadata 为准 | [`public/licenses/dependencies.txt`](public/licenses/dependencies.txt) |

IBM Plex Sans 的字体文件由 `src/index.css` 通过 `@fontsource/ibm-plex-sans` 本地加载。许可来源已按 npm 包内的 `LICENSE` 原文复制到 `public/licenses/ibm-plex-sans.txt`，随应用静态资源发布。

`dependencies.txt` 是由 [`scripts/generate-third-party-notices.mjs`](scripts/generate-third-party-notices.mjs) 从本地 npm 生产依赖树和 `src-tauri/Cargo.lock` 生成的清单。它包含 127 个 npm 包和 Cargo.lock 中的 567 个 registry crate；其中 38 个 Cargo 包的缓存没有原文，但已按 `.cargo_vcs_info.json` 和 Cargo metadata 中的固定 revision 补入 `public/licenses/upstream/`，每项的仓库、revision、来源 URL 和摘要都记录在 [`public/licenses/upstream/sources.json`](public/licenses/upstream/sources.json) 中。`selectors` 的 Servo revision 没有自己的许可证文件，因此清单明确记录了这一事实，并逐字保留本地缓存中相同的 MPL-2.0 文本作为可核对副本，没有猜写许可文本。

完整 Cargo.lock 清单仍有 7 个包没有可证明的许可证原文：`libappindicator-sys@0.9.0`、`r-efi@5.3.0`、`r-efi@6.0.0`、`rsqlite-vfs@0.1.1`、`rustls-platform-verifier-android@0.1.1`、`winapi-i686-pc-windows-gnu@0.4.0` 和 `winapi-x86_64-pc-windows-gnu@0.4.0`。生成器会从 [`scripts/release-artifacts.mjs`](scripts/release-artifacts.mjs) 读取六个发布 triple，再用 Cargo 的 `normal,build` 图标记实际选中的包；这 7 项当前都不在六个目标图中，因此当前发布阻断缺口为 0。它们仍保留在完整清单中，未来若被任一发布目标选中，必须先补入可证明的原文。前两种 `r-efi` 版本的 metadata 继续保留 `MIT OR Apache-2.0 OR LGPL-2.1-or-later` 全部选项。

六个发布目标的 Cargo normal/build 依赖图都包含 `cssparser`、`cssparser-macros`、`dtoa-short`、`selectors` 和 `option-ext` 的 MPL-2.0 声明。`selectors` 的上游 revision 没有许可证文件，仓库现在随附由本地 `cssparser` 缓存逐字复制的 MPL-2.0 文本，并在清单中保留上游仓库和 revision 证明。完整 Cargo.lock 还记录了 `r-efi` 的 LGPL-2.1-or-later 选项、`ryu` 的 BSL-1.0 选项和 `webpki-root-certs` 的 CDLA-Permissive-2.0；它们未出现在六个目标的 normal/build 图中，但保留在清单中供依赖图变化时复核。

## 发布前核对

本清单覆盖当前仓库已确认的运行时资产，不代表 npm 依赖树中的每个开发依赖都应随应用发布。发布 snapshot 不应包含 `node_modules`；如果新增字体、图标或静态第三方资源，必须先取得其明确的版权/许可文本，再加入 `public/licenses/` 和本清单。无法确认的授权应停在发布前核对，不应根据文件名或包名猜测许可。

发布前应在完成目标构建后重新运行 `node scripts/generate-third-party-notices.mjs`，并把生成结果纳入发布资源；只有清单中的 `Release-blocking missing texts` 大于 0 时才阻断本次发布。`Non-selected Cargo.lock packages with missing texts` 仍需保留记录，并在它们进入任一发布目标前补齐。

根目录 `LICENSE` 由仓库发布流程另行维护；本文件不替代它，也不为项目自身代码声明新的许可。

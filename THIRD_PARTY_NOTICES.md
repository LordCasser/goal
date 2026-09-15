# Third-party notices

本文件记录 Goal 发布包中由仓库或生产依赖带入的第三方运行时资产。`node_modules`、构建缓存和本地测试数据不是发布内容；依赖升级后应重新核对对应的许可文本。

## 已核对并随仓库提供

| 资产 | 许可 | 仓库中的完整文本 |
| --- | --- | --- |
| Halaska UI 相关资产 | MIT | [`public/licenses/halaska-ui.txt`](public/licenses/halaska-ui.txt) |
| Tauri decoration 相关资产 | MIT | [`public/licenses/tauri-decoration.txt`](public/licenses/tauri-decoration.txt) |
| IBM Plex Sans（`@fontsource/ibm-plex-sans` 5.3.0） | SIL Open Font License 1.1 | [`public/licenses/ibm-plex-sans.txt`](public/licenses/ibm-plex-sans.txt) |

IBM Plex Sans 的字体文件由 `src/index.css` 通过 `@fontsource/ibm-plex-sans` 本地加载。许可来源已按 npm 包内的 `LICENSE` 原文复制到 `public/licenses/ibm-plex-sans.txt`，随应用静态资源发布。

## 发布前核对

本清单覆盖当前仓库已确认的运行时资产，不代表 npm 依赖树中的每个开发依赖都应随应用发布。发布 snapshot 不应包含 `node_modules`；如果新增字体、图标或静态第三方资源，必须先取得其明确的版权/许可文本，再加入 `public/licenses/` 和本清单。无法确认的授权应停在发布前核对，不应根据文件名或包名猜测许可。

根目录 `LICENSE` 由仓库发布流程另行维护；本文件不替代它，也不为项目自身代码声明新的许可。

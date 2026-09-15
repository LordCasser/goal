# Goal 使用说明

这份说明对应 Goal `v0.1.x`。

## 第一次启动

安装对应平台的包后启动应用。macOS 包目前没有 Developer ID 和 notarization，首次打开可能被 Gatekeeper 拦截。确认来源和校验值后，按 [Apple 的说明](https://support.apple.com/en-gb/102445) 到“系统设置 → 隐私与安全性”使用 **Open Anyway**。不需要也不建议关闭整个 Gatekeeper。

应用启动后会在本地创建数据库和 AI 配置。可以从长期目标开始，也可以直接创建本周或当天的独立事务。

## 界面语言

从 **设置 → 通用 → 语言** 选择 **简体中文** 或 **English**。切换即时生效，重启后保持，不会清空正在编辑的内容。首次启动按系统语言匹配，不支持的语言使用英语。任务标题、模型名、历史对话和自定义 persona/SKILL 保持原样；新 AI 回复默认使用界面语言，也可以在提问时明确要求其他语言。

In **Settings → General → Language**, choose **English** or **简体中文**. The change takes effect immediately and is saved for the next launch. Your plans, drafts and conversation history stay unchanged. AI uses the selected language by default; you can request another language in your message.

## 计划和任务

工作台（Workspace）按长期、周、日展示计划。任务有两种关系：

- **独立任务**：只属于当前周或当前日，不强行挂到长期目标。
- **关联任务**：通过父目标选择器关联到周目标或长期目标。长期目标颜色会沿关系显示，颜色只表达归属，不代表优先级分数。

任务可以直接编辑标题、完成状态、子任务、顺序和关联。需要先记下来的内容可以放进 **稍后（Later）**，之后再提升到某个长期周期。删除周期或任务时，应用会先显示影响范围和确认界面。

长期周期、周周期和日周期有自己的日期和生命周期。结束的周期仍保留本地记录；日历会根据日期显示当前日计划。

新建日计划默认没有专注块。只有明确启用“每天重复”的专注块才会自动出现在新日期；选择“停止重复”后，后续日期不再生成实例，已有记录保留。

## 日历和专注块

顶部切换到日历（Calendar） 后可以使用月视图或周视图。选中日期后，右侧有两个入口：

- **计划（Plan）**：查看当天任务、关联的周目标和长期目标。
- **排期（Schedule）**：查看当天时间轴。专注块可以先留在未安排区，再通过排期入口设置开始时间和时长；在浮层中调整开始时间与持续分钟数后，点击保存排期。

专注块的时长使用整数分钟。排期后，工作台的专注块区域会显示计划总时长。相邻或重叠的安排会在日历数据中保留，应用不会把它们悄悄改成另一个时间。

## 稍后（Later）

点击窗口顶部的 **稍后（Later）**，输入标题即可先保存一个待安排目标。稍后中的条目不占用长期、周或日计划；选择提升目标时，应用会要求选择一个长期周期。

## 配置 AI（BYOK）

Goal 不提供内置模型额度。AI 功能使用你自己配置的供应商和 API Key，费用和数据处理规则由你选择的供应商决定。

1. 打开右上角设置，进入 **AI 模型**。
2. 添加供应商，填写名称、Base URL、API 格式和至少一个模型 ID。支持 Anthropic Messages、OpenAI Chat Completions、OpenAI Responses，也可以填写本地模型服务的 HTTP 地址。
3. API Key 只在需要时填写。保存后先点击连接测试；连接测试成功的模型才可以设为当前使用。
4. 选择当前供应商 / 模型。需要时打开“计划区 AI 规划入口”，让 Workspace 显示 **用 AI 规划**。

模型的上下文窗口、最大输出 token、输入类型和是否支持工具是配置元数据。应用不会因为列表里有另一个模型就自动替换你选中的模型。

## 使用助理（Coach）

点击顶部 **助理（Coach）** 打开当前计划的对话。助理的会话按计划周期隔离，切换周期不会把另一周期的对话混进来。模型可能先读取周期、任务、日历或设置，再给出建议。

涉及本地数据写入时，助理先展示待确认的预览。检查变更对象、前后内容和所属周期后，再选择确认或放弃。确认后才调用本地写操作；对话中的工具结果会折叠成状态行，不直接把 JSON 铺满界面。

助理支持长期规划、周规划、日规划、目标澄清、优先级、复盘、时段分析和计划问题等技能。技能只决定当前工作流，不能绕过应用的确认边界。

### 上下文保留时间

在 AI 设置中可以修改 **助理上下文保留时间**，范围是 1–1440 分钟，默认 15 分钟。超过这段时间没有新对话后，应用会清理该周期的会话消息和活动技能；正在执行的回合不会被中途截断。重新打开助理会开始一个新的上下文。

## 计划问题（Issues）

打开计划问题可以查看当前计划的规则提示。它会显示任务数量、当日负荷、目标信息等问题，并支持定位任务、与助理讨论或选择忽略原因。

只有配置并测通模型后，才能点击 **AI 检查**。AI 检查结果会标明检查状态和模型；计划或模型发生变化后，旧结果会标记为需要重新检查。没有待办时，应用不会伪造一份“检查通过”的结果。

## 自定义 persona 和 skills

Goal 首次启动时会创建以下本地配置：

```text
~/.goal/persona.md
~/.goal/skills/<skill-name>/SKILL.md
```

`persona.md` 控制回答风格；`SKILL.md` 控制对应工作流的说明。已有文件不会被覆盖，应用每次生成前读取最新 UTF-8 内容。当前内置技能目录包括 `coach`、`goal-clarification`、`long-term-planning`、`weekly-planning`、`daily-planning`、`prioritization`、`cycle-review`、`period-analysis` 和 `planning-issues`。

这些文件会成为 AI 请求的一部分。不要在 persona 或 skill 文件里放 API Key、密码、Cookie 或其他不应发送给模型供应商的内容。

## 数据和隐私

任务、周期、日历、专注块、设置和会话默认留在本机。API Key 使用操作系统凭据存储，不写进普通配置文件。AI 请求的数据边界、供应商保存责任和本地文件说明见 [`docs/privacy.md`](privacy.md)。

`v0.1.x` 不包含语音输入、云同步或自动更新通道。需要升级时，请从 [Releases](https://github.com/LordCasser/goal/releases/latest) 获取新资产，并按平台说明安装。

## 开发和问题排查

开发者命令、平台目标和桌面打包限制见 [`docs/desktop-build.md`](desktop-build.md)。常用检查命令：

```sh
npm ci
npm test
npm run tauri dev
cargo test --manifest-path src-tauri/Cargo.toml --locked
```

如果 AI 页面显示“请选择已测通的模型”，先检查 Base URL、API 格式、模型 ID 和凭据，再重新运行连接测试。连接测试失败不会覆盖原来的已保存配置。

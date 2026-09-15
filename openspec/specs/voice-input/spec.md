## Purpose

定义语音输入通道。用户可以用说话代替打字回答 agent 的问题，音频在客户端采集并降采样后以固定帧长送往转写服务，转写文本回填到输入框。

## Requirements

### Requirement: 音频采集与帧格式

客户端 SHALL 通过 AudioWorklet 采集麦克风 PCM，降采样后按固定帧长传输。约定：采集 48 kHz、输出 16 kHz、`linear16` 小端、每帧 80 ms（1280 采样），缓冲区零拷贝转移。

#### Scenario: 开始听写
- **WHEN** 用户触发开始听写
- **THEN** 麦克风开始采集，worklet 按 80 ms 一帧产出 16 kHz linear16 数据

#### Scenario: 浏览器不支持所需能力
- **WHEN** 运行环境缺少 AudioWorklet 或音频录制能力
- **THEN** 系统提示 "Audio recording is not supported." / "AudioWorklet PCM capture is not supported."，不进入半可用状态

### Requirement: 转写凭据获取

系统 SHALL 在开始转写前取得短期转写凭据。凭据不可用时 MUST 明确失败，不得静默降级。

#### Scenario: 凭据获取成功
- **WHEN** 开始听写且授权有效
- **THEN** 客户端取得一次性 token 与有效期并发起转写连接

#### Scenario: 凭据获取失败
- **WHEN** 授权无效或网络失败
- **THEN** 提示 "Could not start transcription."

#### Scenario: 连接失败
- **WHEN** 转写连接建立失败
- **THEN** 提示 "Transcription connection failed."，采集停止

### Requirement: 麦克风权限与设备错误

系统 SHALL 区分权限拒绝与无可用设备两种失败，并给出不同提示。

#### Scenario: 权限被拒
- **WHEN** 用户拒绝麦克风权限
- **THEN** 提示 "Microphone permission was denied."

#### Scenario: 没有输入设备
- **WHEN** 系统找不到可用麦克风
- **THEN** 提示 "No microphone was found."

#### Scenario: 设备初始化失败
- **WHEN** 麦克风存在但打开失败
- **THEN** 提示 "Could not start microphone."

### Requirement: 听写交互

听写 SHALL 与文本输入共享同一个输入框，结束听写后转写文本 SHALL 落到该输入框供用户编辑后再发送。

#### Scenario: 听写进行中
- **WHEN** 听写已开始
- **THEN** 输入框提示 "Speak to type..." 并提供停止入口（快捷键 `⌘D`）

#### Scenario: 结束听写
- **WHEN** 用户停止听写
- **THEN** 已转写文本留在输入框，用户可修改后发送，不自动发送

#### Scenario: 没有选中目标任务
- **WHEN** 用户在没有选中任务的情况下要求澄清
- **THEN** 提示 "Choose a task before starting goal clarification."

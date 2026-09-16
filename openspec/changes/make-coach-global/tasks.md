## 1. 数据与迁移

- [x] 1.1 实现迁移 14 与全局单例数据约束：移除 `agent_conversations.cycle_id` 外键、固定 `coach` 会话身份，按 `turn_id` 和稳定消息顺序合并旧周期历史并连续重编号，保留 payload/回执/技能/错误、清空无法续用的 `active_turn_id`，验证 SQLite 完整性、单例并发初始化、历史不丢失与删除周期不级联会话的集成测试通过

## 2. 后端会话与执行

- [x] 2.1 将 repository、会话服务、IPC 和事件改为全局 Coach：引入 `pageContext`（`long_term_cycle_id`、`week_cycle_id`、`day_cycle_id`）的发送快照，生成 `<page_state>`，固定在途工具 scope，支持无计划 `active_cycle_id=null`、显式入口目标、`active_turn_id`、任务预览按 `task_id` 确认、`agent_actions` 按 `cycle_id`/`action_id`/approve 确认并保留原 `source_cycle_id` 校验及既有 TTL；确认用代码作用域 RAII guard 在同一事务 claim/写回执/finish，忙时先拒绝并由 owner token 释放失败状态，验证 Rust 回合/工具/确认/错误/无请求浏览测试通过

## 3. 前端工作台

- [x] 3.1 将 Coach 状态与侧栏提升到应用 shell，保持全局消息、草稿、滚动、在途忙态和待确认入口；页面切换仅更新下一次发送的页面状态，计划入口明确区分目标范围且无计划可直接打开，验证前端测试覆盖切页连续性、发送快照、无计划、跨页确认和不因浏览触发请求

## 4. 验收

- [x] 4.1 完成迁移数据库、并发入口、跨 Workspace/Calendar/长期/周/日切换、在途工具 scope、无计划聊天、删除计划保留历史、TTL 与确认原目标的自动化集成覆盖（含同一事务 claim/写回执/finish、忙时不改计划、失败释放 owner token），并用 GUI 验证全局历史/草稿/滚动/忙状态/待确认展示；以自动化 prompt/skill 覆盖验证 `coach` 默认英文技能

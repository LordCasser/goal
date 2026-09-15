## 1. 数据与迁移

- [x] 1.1 迁移：新增 `cycle_reviews(id, cycle_id, kind, is_final, facts_json, answers_json, snapshot_at, created_at, updated_at)`，`cycle_id` 唯一
- [x] 1.2 迁移：`cycle_reviews` 与 `cycles` 级联删除
- [x] 1.3 迁移：新增 `cycle_review_dispositions(review_id, task_id, disposition)`，`disposition IN ('carry','later','drop')`，唯一约束 `(review_id, task_id)`
- [x] 1.4 迁移：扩展 `agent_conversations.active_skill` 的允许值加入 `review`（SQLite 需重建表并回填数据）

## 2. 事实采集

- [x] 2.1 `review/facts.rs`：给定 `cycle_id` 计算完成率、累计专注时长、链接覆盖率、未完成项列表
- [x] 2.2 处理空周期分支：无任务时返回「无可复盘内容」而不是 0%
- [x] 2.3 处理混合层级：长周期的完成率需按链接的下层项聚合，且不重复计数
- [x] 2.4 单元测试：空周期、全完成、全未完成、存在未链接项、子项被删除后的统计

## 3. 评审结论持久化

- [x] 3.1 `repository/reviews.rs` + `service/reviews.rs`：创建/覆盖复盘，写入事实快照
- [x] 3.2 事实快照在提交时固定；后续读取一律返回快照值
- [x] 3.3 回答结构：每题 `{ id, status: answered|skipped, text }`；partially-saved 状态可保存
- [x] 3.4 测试：重复复盘为覆盖更新；快照不被事后数据变化改写；部分保存后可续

## 4. 未完成项去向

- [x] 4.1 `service/reviews.rs`：逐条写入去向；未决定的项不写入
- [x] 4.2 `carry` 走既有复制逻辑，写入 `copied_from_task_id`
- [x] 4.3 `later` 移动到 `later` 容器，保持树结构
- [x] 4.4 `drop` 归档原项，不产生副本
- [x] 4.5 测试：三种去向各一例；未决定项保持未决定；跨层级搬运不丢子项

## 5. 复盘技能

- [x] 5.1 `start_review` 工具：校验周期类型，session 返回 `unsupported_cycle_type`
- [x] 5.2 复盘系统提示：固定问题集、一次一问、候选回答、允许跳过、禁止改写事实数值
- [x] 5.3 上下文注入：最近一份复盘的结论 + 当前周期事实，不灌入全部历史
- [x] 5.4 测试：技能激活值正确；跳过全部问题后仍保存事实；上下文只含最近一份

## 6. 界面

- [x] 6.1 列头复盘入口（未复盘 / 已复盘两态）
- [x] 6.2 复盘面板：事实段（只读）与判断段（可编辑）两段式
- [x] 6.3 未完成项处理列表，每条三个去向按钮
- [x] 6.4 草稿保留：关闭面板不丢失已填内容
- [x] 6.5 汇总视图：专注时长与完成率趋势；只有一份复盘时退化为单点展示
- [x] 6.6 测试：空周期分支、部分保存后续填、单点趋势退化

## 7. 导出

- [x] 7.1 Markdown 导出：事实、回答、去向；空段不产生空标题
- [ ] 7.2 导出走系统保存对话框（复用基线的备份对话框路径）
- [x] 7.3 测试：只有事实的复盘导出结果为合法 Markdown

## 8. 验收

- [x] 8.1 `cargo test` 与 `npm run test` 通过
- [ ] 8.2 手工：完成一个长周期 → 复盘 → 三种去向各处理一条 → 开新周期确认上下文带上了复盘结论
- [ ] 8.3 手工：复盘后修改历史任务，确认快照数值未变

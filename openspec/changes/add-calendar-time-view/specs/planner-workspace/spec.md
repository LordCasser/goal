## ADDED Requirements

### Requirement: 视图切换入口

工作台 SHALL 在顶栏提供视图切换入口，且切换 MUST NOT 让用户丢失当前工作的位置。

#### Scenario: 切换视图
- **WHEN** 用户点击顶栏的视图切换控件
- **THEN** 在层级视图与日历视图之间切换，顶栏其余元素保持

#### Scenario: 打开的面板
- **WHEN** 用户在切换视图时 Do Later 抽屉或 agent 侧栏是打开状态
- **THEN** 该面板保持打开，不因切换而关闭

# Spec Delta

## MODIFIED Requirements

### Requirement: 动作映射与 on_conflict 相关的四格

四种冲突类状态中, "指向错误" MUST 固定导出"备份后替换", MUST NOT 读取 `on_conflict`: 该状态蕴含链接落在仓库根内, 属于本工具产物, 与 `undo` 的移除规则一致. 其余三种 ("别人的链接", "目标被普通文件占用", "目标被目录占用") MUST 按 `on_conflict` 二选一: `skip` 导出"拒绝冲突", `replace_and_backup` 导出"备份后替换".

#### Scenario: 指向错误在 skip 下仍被替换

- **WHEN** 状态为"指向错误", `on_conflict = skip`
- **THEN** 动作是"备份后替换", 该条目不被报告为拒绝冲突

#### Scenario: skip 策略保留占用物

- **WHEN** 状态为目标被普通文件占用, `on_conflict = skip`
- **THEN** 动作是"拒绝冲突", 该条目在本次运行中不触碰目标路径

#### Scenario: 允许覆盖

- **WHEN** 状态为目标被目录占用, `on_conflict = replace_and_backup`
- **THEN** 动作是"备份后替换"

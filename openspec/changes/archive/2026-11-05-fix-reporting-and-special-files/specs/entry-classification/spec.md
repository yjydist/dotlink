# Spec Delta

## MODIFIED Requirements

### Requirement: 当前状态互斥且穷尽

系统 MUST 为每个条目给出且仅给出一种当前状态: 源缺失, 未部署, 已是最新, 指向错误, 别人的链接, 目标被普通文件占用, 目标被目录占用, 目标被特殊文件占用, 无法判定. 该判定完全只读, MUST NOT 产生任何文件系统改动.

#### Scenario: 八种取值由不同实况取到

- **WHEN** 依次构造源缺失, 目标不存在, 目标是指向源的链接, 目标是指向仓库内其他路径的链接, 目标是指向仓库外的链接, 目标是普通文件, 目标是目录, 目标路径读不到
- **THEN** 每次都得到对应的那一种状态, 且八个结果互不相同

#### Scenario: 特殊文件是第九种取值

- **WHEN** 目标路径是 FIFO, socket 或设备文件
- **THEN** 状态为"目标被特殊文件占用", 与上述八种状态均不同

#### Scenario: 判定不写文件系统

- **WHEN** 只做判定 (预览与检查)
- **THEN** 目标路径, 父目录与备份目录均保持原样

### Requirement: 动作映射与 on_conflict 相关的四格

五种冲突类状态中, "指向错误" MUST 固定导出"备份后替换", MUST NOT 读取 `on_conflict`: 该状态蕴含链接落在仓库根内, 属于本工具产物, 与 `undo` 的移除规则一致. "别人的链接", "目标被普通文件占用" 与 "目标被目录占用" MUST 按 `on_conflict` 二选一: `skip` 导出"拒绝冲突", `replace_and_backup` 导出"备份后替换". "目标被特殊文件占用" MUST 固定导出"拒绝冲突", MUST NOT 读取 `on_conflict`: 特殊文件无法复制, 因此不存在"先完整备份再替换"的合法路径.

#### Scenario: 指向错误在 skip 下仍被替换

- **WHEN** 状态为"指向错误", `on_conflict = skip`
- **THEN** 动作是"备份后替换", 该条目不被报告为拒绝冲突

#### Scenario: skip 策略保留占用物

- **WHEN** 状态为目标被普通文件占用, `on_conflict = skip`
- **THEN** 动作是"拒绝冲突", 该条目在本次运行中不触碰目标路径

#### Scenario: 允许覆盖

- **WHEN** 状态为目标被目录占用, `on_conflict = replace_and_backup`
- **THEN** 动作是"备份后替换"

#### Scenario: 特殊文件不论策略都拒绝覆盖

- **WHEN** 状态为目标被特殊文件占用, `on_conflict = replace_and_backup`
- **THEN** 动作是"拒绝冲突", 目标路径不被备份, 删除或覆盖

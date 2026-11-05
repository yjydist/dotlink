# Spec Delta

## ADDED Requirements

### Requirement: 撤销只恢复用户原始备份

`undo` 的备份查找与恢复 MUST 只使用用户备份命名空间 `<原名>.dotlink-backup.` 中完整命名的备份. 自愈备份 (`<原名>.dotlink-heal.<时间戳>` 及其消歧形态) MUST NOT 被 `undo` 选中, MUST NOT 被恢复, MUST NOT 被删除或改写. 因此自愈后撤销回到自愈前的状态: 目标原本不存在时只移除链接; 目标原本是用户内容且其备份仍在时恢复该用户备份. 自愈备份继续留在磁盘上, 由用户自行清理. 本要求只约束可用于恢复的备份集合, token 与退出码映射仍由既有 requirement 决定.

#### Scenario: 自愈后撤销回到未部署态

- **WHEN** 目标路径是自愈产生的指向当前 `source` 的链接, 同目录只有该次自愈留下的 `.dotlink-heal.` 备份
- **THEN** `undo` 移除链接, 不恢复自愈备份, 自愈备份的路径与内容保持不变, 该行 token 为 `undo`

#### Scenario: 用户备份优先于自愈备份

- **WHEN** 同目录既有更早的用户备份 `<原名>.dotlink-backup.<时间戳>`, 又有自愈产生的 `.dotlink-heal.` 备份, 目标路径是自愈后的链接
- **THEN** `undo` 恢复用户备份, 自愈备份保持原样, 该行 token 为 `restore`

#### Scenario: 自愈备份不被消费

- **WHEN** 一次撤销在只有 `.dotlink-heal.` 备份的目标上移除了链接
- **THEN** 自愈备份仍在原路径且内容不变, 可再次检查而不会被第二次撤销删除

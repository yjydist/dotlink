# Spec Delta

## MODIFIED Requirements

### Requirement: 只有完整命名的备份参与查找

查找 MUST 只接受后缀为 15 位时间戳或"时间戳 + 数字序号"的备份. 复制中断留下的 `.partial` 残留 MUST NOT 参与"取最新", MUST NOT 被撤销消费, MUST NOT 被当作"没有备份"的理由删除. 查找 MUST 只接受用户备份命名空间 `<原名>.dotlink-backup.` 中的备份. 自愈备份使用独立命名空间 `<原名>.dotlink-heal.`, 其完整形态为 `<原名>.dotlink-heal.<15 位时间戳>` 或 `<原名>.dotlink-heal.<15 位时间戳>.<n>`, 该命名空间使用自己的标记, 整个名字不匹配用户备份的 `<原名>.dotlink-backup.<完整后缀>` 形态, MUST NOT 参与用户备份的"取最新", MUST NOT 被撤销消费, MUST NOT 被当作"有可用备份"的依据.

#### Scenario: 半成品残留

- **WHEN** 同目录既有完整备份, 又有文件名更晚的 `.partial` 残留
- **THEN** 取完整备份, 残留保持原样

#### Scenario: 只有残留

- **WHEN** 同目录只有 `.partial` 残留
- **THEN** 视为没有可用备份, 撤销只移除链接而不恢复

#### Scenario: 自愈备份不参与取最新

- **WHEN** 同目录既有用户备份 `<原名>.dotlink-backup.<时间戳>`, 又有文件名更晚的自愈备份 `<原名>.dotlink-heal.<时间戳>`
- **THEN** 用户备份查找只返回前者, 自愈备份被忽略

#### Scenario: 只有自愈备份

- **WHEN** 同目录只有 `<原名>.dotlink-heal.<时间戳>` 形态的自愈备份
- **THEN** 视为没有可用的用户备份, 撤销只移除链接, 自愈备份保持原样

#### Scenario: 自愈备份的命名形态

- **WHEN** 检查任一新生成的自愈备份名
- **THEN** 它以 `.dotlink-heal.` 为标记, 标记之后恰为 15 位时间戳, 或为该时间戳后追加 `.<n>` (n 从 2 起); 即使原文件名本身含 `.dotlink-backup.`, 整个名字也不构成用户备份的完整命名形态

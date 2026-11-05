# Spec Delta

## ADDED Requirements

### Requirement: 时间戳字段非法的备份不参与查找

查找 MUST 把 15 字符的时间戳后缀同时当作时间来校验: 年 MUST 是 4 位数字, 月 MUST 在 01 至 12 之间, 日 MUST 落在该月的真实天数内 (含闰年 2 月 29 日), 时 MUST 在 00 至 23 之间, 分与秒 MUST 在 00 至 59 之间. 字段值非法的名字 MUST NOT 参与 "取最新", MUST NOT 被撤销消费, MUST NOT 被当作 "有可用备份" 的依据. 校验 MUST NOT 删除, 重命名或移动该名字.

#### Scenario: 字段非法的名字被忽略

- **WHEN** 同目录有一个合法备份 `.rc.dotlink-backup.20260101T000000`, 以及一个字典序更大但月份非法的 `.rc.dotlink-backup.99999999T999999`
- **THEN** `undo` 恢复前者并报告前者的完整路径, 非法的名字保持原样不被读取或删除

#### Scenario: 只有字段非法的名字

- **WHEN** 同目录只有一个字段非法的 `<原名>.dotlink-backup.<15 字符>` 名字, 目标路径为空
- **THEN** 视为没有可用备份, `undo` 不改动文件系统, 该名字保持原样

#### Scenario: 闰日与月末

- **WHEN** 同目录并存 `.rc.dotlink-backup.20240229T000000` 与 `.rc.dotlink-backup.20230229T000000`
- **THEN** 前者 (合法闰日) 参与查找并可能被选中, 后者 (平年 2 月 29 日) 被忽略

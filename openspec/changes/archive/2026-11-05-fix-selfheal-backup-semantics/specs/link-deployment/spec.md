# Spec Delta

## MODIFIED Requirements

### Requirement: 本工具的错误链接在部署时自愈

`apply` MUST 把 "指向错误" 视为本工具创建的链接, 无论 `on_conflict` 取值都替换为指向当前 `source` 的正确链接. 替换前 MUST 完整备份旧链接本身 (不跟随其目标), 备份落定后才删除旧链接. 自愈备份 MUST 使用独立命名空间 `<原名>.dotlink-heal.<YYYYMMDDThhmmss>`, 同一秒内消歧追加 `.<n>` (n 从 2 起), MUST NOT 占用用户备份命名空间 `<原名>.dotlink-backup.<时间戳>`. 外部占用触发的替换仍使用用户备份命名空间. 该行 MUST 报告 `replace` 并给出自愈备份路径. `preview` MUST 报告同一动作, 并按自愈命名空间预测备份路径; 预测与随后 `apply` 实际使用的命名规则和消歧规则 MUST 一致, 跨秒时字面时间戳可以不同. `preview` MUST NOT 修改文件系统. 撤销只恢复用户原始备份, 自愈备份既不恢复也不删除, 详见 `link-teardown` 的 `撤销只恢复用户原始备份`.

#### Scenario: source 改名后 apply 自愈

- **WHEN** 目标路径是本工具创建的链接, 指向仓库内旧路径, 而清单的 `source` 已改为新路径, 且该条目使用默认 `on_conflict`
- **THEN** `apply` 成功, 旧链接被备份到自愈命名空间, 目标成为指向新 `source` 的链接, 该行 token 为 `replace`, 退出码按整体结果计算

#### Scenario: preview 预测自愈

- **WHEN** 同一状态执行 `preview`
- **THEN** 该行 token 为 `replace`, 说明给出按自愈命名空间算出的预测备份路径, 且 `preview` 不修改文件系统

#### Scenario: 自愈备份不占用用户备份命名空间

- **WHEN** 目标路径是本工具创建的链接且指向错误, 同目录此前没有该目标的备份, 执行 `apply`
- **THEN** 目录中新增的备份名以 `.dotlink-heal.` 为标记, 不新增 `.dotlink-backup.` 形态的备份

#### Scenario: 外部占用仍使用用户备份命名空间

- **WHEN** 目标路径被用户自己的普通文件占用, 条目声明 `on_conflict = "replace_and_backup"`, 执行 `apply`
- **THEN** 备份名仍为 `<原名>.dotlink-backup.<时间戳>` 形态, 不出现 `.dotlink-heal.` 标记

#### Scenario: 自愈备份失败时旧链接保留

- **WHEN** 自愈备份无法写入 (例如目标所在目录不可写)
- **THEN** 该条目报告失败, 旧链接仍在原路径且内容不变, 不建立指向新 `source` 的链接

#### Scenario: preview 预测与实际命名规则一致

- **WHEN** 同一状态先 `preview` 再 `apply`
- **THEN** 两处给出的备份路径都使用 `.dotlink-heal.` 命名空间, 时间戳与消歧序号按各自时刻的同一规则计算, 跨秒时字面值可以不同

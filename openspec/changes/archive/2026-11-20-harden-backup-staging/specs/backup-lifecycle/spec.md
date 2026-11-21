# Spec Delta

## ADDED Requirements

### Requirement: 备份查找按原始字节解析目录项

备份查找 MUST 按目录项的原始字节解析 `<原名>.dotlink-backup.` 与时间戳/序号后缀, MUST NOT 要求文件名可解码为 UTF-8. 目标末段包含非 UTF-8 字节时, 其备份 MUST 仍能被 `undo` 找到并恢复. 索引键 MUST 使用 `OsString` 或等价的原始字节类型, MUST NOT 使用有损字符串作为查找键.

#### Scenario: 非 UTF-8 目标名的备份仍可恢复

- **WHEN** 在支持非 UTF-8 文件名的文件系统上, 目标末段包含非 UTF-8 字节, `apply` 生成了该目标的备份, 随后执行 `undo`
- **THEN** 备份被找到并恢复, token 为 `restore`, 不因为文件名不可解码而当作"没有备份"

#### Scenario: 非 UTF-8 的无关目录项被忽略

- **WHEN** 备份目录中存在非 UTF-8 名字, 但既不是当前目标的备份, 也不符合备份命名形态
- **THEN** 查找跳过该项, 不影响其他目标的备份选择

### Requirement: 备份候选名的占用检测只在 NotFound 时视为空闲

计算备份名时的占用检测 MUST 只在元数据读取返回 `NotFound` 时把候选名视为空闲. 其他错误 (例如 `EACCES`, `ELOOP`, `ENAMETOOLONG`) MUST 使该次预测或写入失败, MUST NOT 跳过该错误继续选择后续候选名. `preview` 的预测失败 MUST 报告 `fail` 并给出底层原因; 写入路径 MUST 使该条目失败, 不发布最终名.

#### Scenario: 候选名父目录不可读

- **WHEN** 计算预测备份名时, 候选路径的父目录返回 `EACCES`
- **THEN** `preview` 报告 `fail` 并给出权限原因, 不报告一个实际无法发布的备份路径

#### Scenario: 候选名路径过长

- **WHEN** 候选路径返回 `ENAMETOOLONG`
- **THEN** 预测或写入失败并给出该错误, 不静默选择后续候选名

#### Scenario: 候选名不存在

- **WHEN** 候选路径返回 `NotFound`
- **THEN** 该候选被视为空闲, 按既有命名规则使用

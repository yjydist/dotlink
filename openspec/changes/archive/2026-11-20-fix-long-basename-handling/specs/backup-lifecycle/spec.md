# Spec Delta

## ADDED Requirements

### Requirement: 工具生成名的长度预算

备份最终名, 备份暂存名, 恢复暂存名与 apply/undo 暂存名 MUST 落在文件系统的单段名上限内. 系统 MUST 在清单校验阶段使用保守预算 200 字节拒绝超长 `target` basename, MUST NOT 在运行期截断, 哈希或改写原名. 预算 MUST 覆盖最长后缀 (包含 7 位 pid, 消歧序号与 `.partial` / `.dotlink-restore.partial`), 且 MUST 与 `manifest-schema` 的 V-12 共用同一个常量, 避免两处预算漂移.

#### Scenario: 预算内的名字可发布

- **WHEN** `target` 的末段为 200 字节, 执行 `apply` 的 `replace_and_backup`
- **THEN** 备份最终名与暂存名的字节数都不超过 `NAME_MAX`, 备份成功发布

#### Scenario: 超预算在改动前拒绝

- **WHEN** `target` 的末段为 201 字节, 执行任一命令
- **THEN** 清单校验阶段退出 2, 不创建目录, 不写备份, 不建立链接

#### Scenario: 预算覆盖最长后缀

- **WHEN** 审计预算常量与生成名的后缀拼接逻辑
- **THEN** 200 字节加上最长后缀 (7 位 pid, 消歧序号, `.partial` 或 `.dotlink-restore.partial`) 仍不超过 255 字节

# Spec Delta

## ADDED Requirements

### Requirement: 暂存目录的权限与清理不受 umask 影响

备份目录的暂存目录 MUST 以 0700 语义创建, 且创建后 MUST 立即恢复 owner 读写执行权限, 使后续子项创建与元数据应用不受进程 umask 影响. 备份失败清理时, 如果暂存树因已应用只读权限而无法删除, 系统 MUST 尽力恢复 owner 读写执行权限后再删除. 清理失败仍按既有的尽力而为语义处理, 但 MUST NOT 因为 umask 或只读目录留下本可避免的 `.partial` 残留.

#### Scenario: 受限 umask 下目录备份成功

- **WHEN** 进程 umask 为 `0200` 或 `0777`, 一个目录被 `replace_and_backup` 覆盖
- **THEN** 暂存目录仍可写入子项, 备份完整发布, 不因 owner 写权限被 umask 剥离而失败

#### Scenario: 只读暂存树在失败后被清理

- **WHEN** 目录备份在应用只读源权限后发生同步失败
- **THEN** 清理前恢复 owner 读写执行权限, 本次创建的 `.partial` 暂存树被移除, 既有备份与用户文件不变

#### Scenario: 受限 umask 下失败不遗留可避免的残留

- **WHEN** 进程 umask 为 `0777`, 目录备份在子项复制阶段失败
- **THEN** 该条目报告失败, 原目录保持原样, 本次创建的暂存树被清理而不是留在目录中

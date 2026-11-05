# Spec Delta

## ADDED Requirements

### Requirement: 特殊文件不可复制且目录备份整体失败

备份 MUST NOT 复制 FIFO, socket, 字符设备, 块设备或其他特殊文件. 复制过程遇到特殊文件时 MUST 使整个备份失败, MUST NOT 跳过该对象后继续, MUST NOT 发布最终备份名, MUST NOT 删除或修改原对象. 目录递归备份中任一子项是特殊文件时 MUST 同样整体失败, 失败说明 MUST 给出该特殊文件的完整路径, 并提示先手工移走该特殊文件再重试. 该失败 MUST 使 `apply` 的对应条目报告 `fail` 并保持原状.

#### Scenario: 直接复制特殊文件失败

- **WHEN** 备份一个 FIFO 或 socket
- **THEN** 备份失败, 不创建最终备份名, 原对象保持原样

#### Scenario: 目录内含特殊文件时整体失败

- **WHEN** 一个目录含有普通文件与 FIFO, 该目录被 `replace_and_backup` 覆盖
- **THEN** 整个备份失败, 不跳过 FIFO 后发布部分备份, 最终备份名不出现, 原目录与 FIFO 保持原样, 说明给出 FIFO 的完整路径

#### Scenario: 失败说明提示手工处理

- **WHEN** 目录备份因 socket 而失败, 执行 `apply`
- **THEN** 该条目报告 `fail`, 说明给出 socket 的完整路径并提示先手工移走再重试, 不建立链接

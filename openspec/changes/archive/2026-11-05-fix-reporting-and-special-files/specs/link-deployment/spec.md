# Spec Delta

## ADDED Requirements

### Requirement: 不可备份的占用物不建立链接

目标路径是 FIFO, socket, 字符设备, 块设备或其他既非普通文件, 非目录, 非符号链接的对象时, `preview` 与 `apply` MUST 报告 `conflict`, MUST NOT 报告 `replace`, MUST NOT 预测或执行备份, MUST NOT 删除, 截断或覆盖该对象, 且该结论 MUST NOT 依赖 `on_conflict`. 说明 MUST 给出精确的特殊文件类型, 并写明特殊文件不支持备份且不会覆盖; `undo` 对该路径 MUST 保持既有 `skip` 语义, 不触碰它. 该要求 MUST NOT 改变十二个 token 的词汇.

#### Scenario: 特殊文件目标在预览中得到 conflict

- **WHEN** 目标路径是 FIFO 且条目声明 `replace_and_backup`, 执行 `preview`
- **THEN** token 为 `conflict`, 说明指出 FIFO 且不给出备份路径, 目标未被触碰

#### Scenario: 两种策略都拒绝

- **WHEN** 同一 FIFO 目标分别以 `skip` 与 `replace_and_backup` 执行 `apply`
- **THEN** 两次都报 `conflict`, FIFO 保持不变, 不建立链接

#### Scenario: 撤销不触碰特殊文件

- **WHEN** 目标路径是 socket, 执行 `undo`
- **THEN** token 为 `skip`, socket 保持不变

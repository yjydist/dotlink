# Spec Delta

## ADDED Requirements

### Requirement: 报告写出失败不改变命令结局

报告写入标准输出失败时, 系统 MUST NOT panic, MUST NOT 以 101 等契约外退出码结束. 报告写出失败 MUST NOT 参与现有的 0/1 两条件判据: 退出码仍只由命令的目标谓词与逐条结果决定. 标准输出返回 `BrokenPipe` 时 MUST 静默结束, MUST NOT 向标准错误输出诊断; 其他写错误 MUST 尽力向标准错误输出一行诊断. 标准错误不可写时 MUST 静默忽略, 退出码仍按该情形原本的结局计算.

#### Scenario: 标准输出被提前关闭

- **WHEN** 通过管道运行任一子命令, 消费方在报告写完之前关闭标准输出
- **THEN** 进程不 panic, 标准错误不出现诊断, 退出码仍按该命令的目标谓词与逐条结果计算

#### Scenario: 标准输出出现其他写错误

- **WHEN** 报告写入标准输出时返回 `BrokenPipe` 之外的错误
- **THEN** 进程不 panic, 标准错误输出一行说明写失败, 退出码不变

#### Scenario: 标准错误不可写

- **WHEN** 需要写标准错误时标准错误已关闭
- **THEN** 进程不 panic, 退出码仍按该情形原本的 0/1/2/3 结局计算

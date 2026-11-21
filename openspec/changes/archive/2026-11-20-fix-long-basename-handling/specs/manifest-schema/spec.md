# Spec Delta

## ADDED Requirements

### Requirement: 清单校验 V-12: 目标 basename 长度预算

`target` 解析后的末段 basename MUST 不超过 200 个字节; 超过时整体拒绝, 退出码 2, 且 MUST NOT 做任何文件系统改动. 预算为备份最终名, 备份暂存名, 恢复暂存名与 apply/undo 暂存名的固定后缀预留空间, 确保清单接受的每个目标都能被四条命令处理. 计数 MUST 使用路径的原始字节, MUST NOT 使用字符数或显示宽度. 说明 MUST 给出条目下标, 书写形态, 实际字节数与上限.

#### Scenario: 边界内的长 basename

- **WHEN** 某条目的 `target` 末段恰好 200 个 ASCII 字节, 其余字段合法
- **THEN** 清单被接受, 四命令可以正常处理该条目

#### Scenario: 超过预算的 ASCII basename

- **WHEN** 某条目的 `target` 末段为 201 个 ASCII 字节
- **THEN** 整体拒绝, 退出码 2, 零改动, 说明包含 201 与 200

#### Scenario: 多字节文件名按字节计数

- **WHEN** 某条目的 `target` 末段是 67 个 3 字节 UTF-8 字符 (共 201 字节), 显示宽度只有 134 列
- **THEN** 整体拒绝, 退出码 2, 说明按字节给出 201 与 200, 而不是按字符数放行

#### Scenario: 只创建链接的条目也受同一预算约束

- **WHEN** 某条目的目标当前不存在, `apply` 只会创建链接, 但 basename 为 201 字节
- **THEN** 仍在清单校验阶段拒绝, 退出码 2, 不因为本次动作不需要备份而放行

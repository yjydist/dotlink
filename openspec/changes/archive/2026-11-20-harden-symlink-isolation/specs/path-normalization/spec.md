# Spec Delta

## ADDED Requirements

### Requirement: 清单校验的比较形态

系统 MUST 为清单校验提供一种只读的比较形态: 在现有词法规范化结果之外, 额外解析末段的符号链接链, 使同一对象的别名形态在比较中重合. 该形态 MUST 只用于清单校验的路径比较, MUST NOT 用作运行时 `target_path` 或 `source_path`; 运行时仍遵守"末段永不解析". 比较形态 MUST 复用现有的符号链接深度上限与词法回退, MUST NOT 因链接环而无界递归.

#### Scenario: 符号链接末段在比较形态中被解析

- **WHEN** 某目标的末段是符号链接 `~/.config`, 指向 `/data/config`
- **THEN** 该目标的比较形态是 `/data/config`, 而运行时目标路径仍是 `~/.config` 本身

#### Scenario: 运行时末段仍不解析

- **WHEN** 目标路径末段是已存在的符号链接, 执行 `status` 或 `undo`
- **THEN** 判定仍看到链接自身, 不会把状态塌缩成"目标被普通文件占用"

#### Scenario: 比较形态对链接环有界

- **WHEN** 末段符号链接自指或两节点互指, 参与清单校验
- **THEN** 比较形态在有限时间内返回, 校验不崩溃或无响应

# Spec Delta

## ADDED Requirements

### Requirement: 报告转义覆盖 Default_Ignorable 码点

除了既有"报告中的控制字符必须转义"列出的集合之外, 报告层 MUST 把 Unicode 17.0 中具有 Default_Ignorable_Code_Point 属性的码点同样转为 `\u{...}` 形式. 覆盖范围至少包含 U+00AD, U+034F, U+061C, U+115F-1160, U+17B4-17B5, U+180B-180D, U+180F, U+200B-200F, U+202A-202E, U+2060-2064, U+2066-206F, U+3164, U+FE00-FE0F, U+FEFF, U+FFA0, U+FFF0-FFF8, U+1BCA0-1BCA3, U+1D173-1D17A 与 U+E0000-E0FFF. 转义 MUST 只影响显示, MUST NOT 改变清单解析, 路径解析, 文件系统操作或退出码.

#### Scenario: 软连字符与阿拉伯字母标记

- **WHEN** 清单的 `target` 包含 U+00AD 或 U+061C, 执行 `preview`
- **THEN** 输出分别包含 `\u{ad}` 与 `\u{61c}`, 不包含这两个原始字符, 每个条目仍只占一行

#### Scenario: 标签字符

- **WHEN** 清单的 `target` 包含 U+E0041, 执行 `preview`
- **THEN** 输出包含 `\u{e0041}`, 不包含原始标签字符

#### Scenario: 未分配但属于默认忽略的码点

- **WHEN** 清单的 `target` 包含 U+FFF0, 执行 `preview`
- **THEN** 输出包含 `\u{fff0}`, 不包含原始字符

#### Scenario: 文件系统操作使用原始路径

- **WHEN** 目标路径包含 U+00AD, 执行 `status` 或 `apply`
- **THEN** 文件系统操作仍使用包含 U+00AD 的原始路径, 转义只出现在报告文本中

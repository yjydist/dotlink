# Spec Delta

## ADDED Requirements

### Requirement: 报告列宽按终端显示宽度计算

报告行 MUST 按终端显示宽度而不是字符个数对齐: 序号与 token 列定宽, target 列按显示宽度补齐, 使各行说明列的起点不因 target 内容而漂移. 宽度计算 MUST 把 East Asian Wide 与 Fullwidth 码点计 2 列, 组合记号与变体选择符计 0 列, 其余计 1 列. 宽度只影响补齐的空格数量, MUST NOT 改变行内容, 转义形式, token 或退出码.

#### Scenario: CJK 目标与等宽 ASCII 目标对齐

- **WHEN** 清单同时声明 `target = "~/.配置"` 与 `target = "~/.abcd"`, 执行 `preview`
- **THEN** 两行的说明列起点相同 (7 列宽 target 得到相同的补齐), 且行内容仍是清单原文

#### Scenario: 宽字符符号与 emoji

- **WHEN** 清单同时声明 `target = "~/ab"`, `target = "~/⌚"` 与 `target = "~/🚀"`, 执行 `preview`
- **THEN** 三行的说明列起点相同 (U+231A 与 U+1F680 各计 2 列)

#### Scenario: 组合记号不占列

- **WHEN** target 含组合记号 (例如 `~/.e` 后接 U+0301), 执行 `preview`
- **THEN** 该行按 4 列宽 target 补齐, 与 `target = "~/ab"` 的说明列起点相同, 组合记号仍原样保留在行内

#### Scenario: 超宽 target 不截断

- **WHEN** target 的显示宽度超过 target 列的补齐宽度
- **THEN** 该行不截断也不折行, 说明列整体右移, 其余条目各行仍保持一致的说明列起点

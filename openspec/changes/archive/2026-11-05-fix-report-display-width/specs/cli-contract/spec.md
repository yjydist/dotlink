# Spec Delta

## ADDED Requirements

### Requirement: 报告列宽按终端显示宽度计算

报告行 MUST 按终端显示宽度而不是字符个数对齐: 序号与 token 列定宽, target 列按显示宽度补齐, 使各行说明列的起点不因 target 内容而漂移. 宽度计算 MUST 把 East Asian Wide 与 Fullwidth 码点计 2 列; MUST 把整块保留的区间 (CJK 相关区块 `U+2E80-U+303E`, `U+3041-U+33FF`, `U+3400-U+4DBF`, `U+4DC0-U+4DFF`, `U+4E00-U+9FFF`, `U+A000-U+A4CF`, `U+A960-U+A97C`, `U+AC00-U+D7A3`, `U+F900-U+FAFF`, `U+FE10-U+FE19`, `U+FE30-U+FE6F`, `U+FF00-U+FF60`, `U+FFE0-U+FFE6`, `U+20000-U+3FFFD`, 以及 emoji 整块 `U+1F300-U+1F64F`, `U+1F900-U+1F9FF`) 内的码点计 2 列, 包括块内未分配码点与文本呈现型 emoji; MUST 把组合记号区段 (`U+0300-U+036F`, `U+1AB0-U+1AFF`, `U+1DC0-U+1DFF`, `U+20D0-U+20FF`, `U+FE20-U+FE2F`) 与变体选择符 (`U+FE00-U+FE0F`) 计 0 列; 其余码点按其 East Asian 宽度计列 (多数 1 列, 区段之外的 Wide 组合记号仍计 2 列). 宽度只影响补齐的空格数量, MUST NOT 改变行内容, 转义形式, token 或退出码.

#### Scenario: CJK 目标与等宽 ASCII 目标对齐

- **WHEN** 清单同时声明 `target = "~/.配置"` 与 `target = "~/.abcd"`, 执行 `preview`
- **THEN** 两行的说明列起点相同 (7 列宽 target 得到相同的补齐), 且行内容仍是清单原文

#### Scenario: 宽字符符号与 emoji

- **WHEN** 清单同时声明 `target = "~/ab"`, `target = "~/⌚"` 与 `target = "~/🚀"`, 执行 `preview`
- **THEN** 三行的说明列起点相同 (U+231A 与 U+1F680 各计 2 列)

#### Scenario: 组合记号不占列

- **WHEN** target 含组合记号 (例如 `~/.e` 后接 U+0301), 执行 `preview`
- **THEN** 该行按 4 列宽 target 补齐, 与 `target = "~/ab"` 的说明列起点相同, 组合记号仍原样保留在行内

#### Scenario: 整块保留的码点仍计 2 列

- **WHEN** target 含文本呈现型 emoji (`U+1F321`) 或 CJK 区块内 East Asian Ambiguous 的码点 (`U+3248`), 执行 `preview`
- **THEN** 这两个码点各按 2 列计算, 该行的说明列起点与等显示宽度的 ASCII target 相同

#### Scenario: 区段之外的组合记号按宽度判定

- **WHEN** target 含区段之外的组合记号 (`U+3099`), 执行 `preview`
- **THEN** 该码点按 East Asian Wide 计 2 列, 说明列起点与等显示宽度的 ASCII target 相同

#### Scenario: 超宽 target 不截断

- **WHEN** target 的显示宽度超过 target 列的补齐宽度
- **THEN** 该行不截断也不折行, 说明列整体右移, 其余条目各行仍保持一致的说明列起点

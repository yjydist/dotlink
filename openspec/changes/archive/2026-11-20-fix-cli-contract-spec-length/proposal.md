# Proposal

## Why

`openspec validate --all --strict` 当前失败, 原因是 `spec/cli-contract` 的 requirements[15] "报告列宽按终端显示宽度计算" 正文超过 500 字符. 当前输出为:

```
✗ spec/cli-contract
  ⚠ [WARNING] requirements[15]: Requirement text is very long (>500 characters). Move examples and edge cases into scenarios, or split it into separate requirements that each state one behavior.
```

非严格模式可以通过, 所以问题只在严格模式的正文长度上限. 该 requirement 同时描述补齐行为与宽度表, 单条正文过长, 审查与逐项对照都不方便.

## What Changes

- 把 "报告列宽按终端显示宽度计算" 拆成两条 requirement: 一条描述按显示宽度补齐的行为, 一条描述宽度表的整块保留与近似规则.
- 原 requirement 的全部 MUST 片段与 7 个 Scenario 逐条保留到拆分后的两条 requirement 中, 只做归属重组, 不改变任何行为措辞.
- 用 `REMOVED` 下线原 requirement, 用 `ADDED` 写入两条新 requirement. 不使用 `MODIFIED`, 因为它会拒绝丢弃现有 Scenario.
- 新增的两条 requirement 正文均不超过 500 字符, 每条至少包含一个 Scenario.
- 不改代码, 测试, CLI, README 或任何运行时行为.

## Capabilities

### New Capabilities

无.

### Modified Capabilities

- `cli-contract`: 拆分 "报告列宽按终端显示宽度计算" 的正文与 Scenario 归属, 行为不变. 归档或同步后写入主规格, 使严格校验恢复通过.

## Impact

- `openspec/specs/cli-contract/spec.md`: 归档或同步时用两条新 requirement 替换原 "报告列宽按终端显示宽度计算".
- 本变更自身只写入 `openspec/changes/fix-cli-contract-spec-length/` 下的制品, 不直接修改主规格.
- 无代码, 测试, 依赖, 运行时, 输出格式或退出码变化.
- 完成后 `openspec validate --all --strict` 不再报告 requirements[15] 的正文长度 warning.

# Proposal

## Why

`openspec validate --all --strict` 报告 `manifest-schema` 的 V-9 requirement 正文超过 500 字符. 该 requirement 同时描述受保护落点, 主目录的两种形态与文件系统身份判等, 单条正文过长, 审查与逐项验证都难以对照.

## What Changes

- 把 V-9 拆成三条 requirement: 受保护落点, 主目录书写形态与物理形态, 落点命中的身份判等.
- 原 V-9 的全部 MUST 与 8 个 Scenario 分别保留在拆分后的三条 requirement 中.
- 不改任何可观察行为, 不改代码, 测试, CLI 或 README.

## Capabilities

### New Capabilities

无.

### Modified Capabilities

- `manifest-schema`: 拆分 V-9 requirement 的正文与 Scenario 归属, 行为不变.

## Impact

- `openspec/specs/manifest-schema/spec.md`: 归档时用三条 requirement 替换原 V-9.
- 无代码, 测试, 依赖, 运行时或退出码变化.

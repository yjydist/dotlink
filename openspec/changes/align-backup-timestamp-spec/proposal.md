# Proposal

## Why

`backup-lifecycle` 的这条需求标题写 "备份名携带秒级本地时间戳", 正文却要求 UTC, README 也写 UTC.
标题是引用与匹配的入口: 归档时 `MODIFIED` 与 `RENAMED` 都按标题定位需求, 后来者读标题会以为时间戳与
本地时区有关, 从而按错误前提改这条需求, 或在评审中引入"本地时间"的第二种说法.

这条债是已知的: `2026-11-05-harden-backup-ordering` 的 design D6 记录 "归档后如需让标题与正文一致,
由独立变更以 RENAMED 处理", tasks 的 workflow follow-up 也留了同一条. 本变更就是那次偿还, 不改任何
行为.

## What Changes

- 用 `## RENAMED Requirements` 把需求 "备份名携带秒级本地时间戳" 改名为 "备份名携带秒级 UTC 时间戳".
- 正文与全部 Scenario 逐字保留: 不改要求, 不改可验证性, 只纠正名称.
- 不改代码, 不改 README, 不触碰 `openspec/changes/archive/` 下的历史制品.

## Capabilities

### New Capabilities

无.

### Modified Capabilities

- `backup-lifecycle`: 只改 "备份名携带秒级本地时间戳" 的需求名称, 正文与 Scenario 不变.

## Impact

- `openspec/specs/backup-lifecycle/spec.md`: 归档时由 RENAMED 增量重写该需求的标题.
- 不受影响: `src/`, `tests/`, 报告与退出码, 备份命名与排序行为, 依赖树.

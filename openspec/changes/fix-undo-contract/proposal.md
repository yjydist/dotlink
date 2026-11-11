# Proposal

## Why

`undo` 在"目标为空且同目录有完整备份"时会恢复备份并退出 1. README 与测试都这样写, 但 `cli-contract` 的两条件判据与 `entry-classification` 的目标谓词文本推出的是退出 0: 目标为空时没有本工具创建的链接, `restore` 又在成功列. 这是一个面向脚本的公开契约冲突, 必须明确裁决. 同时 README 对"源缺失但目标为空且有备份"的恢复承诺过宽, 实际分类先判 `SourceMissing`, 不会恢复.

## What Changes

- 把 `undo` 的目标谓词改写为: "目标路径上不存在本工具创建的链接, 且本次没有从备份恢复任何内容". 这样 `Absent + 备份` 与链接态 `restore` 都正确地使目标谓词为假, 两条件判据仍然成立.
- `entry-classification` 的计数基准同步改为: `Absent` 且无备份可恢复时对 undo 谓词为真; 有备份可恢复时为假.
- `link-teardown` 增加"空目标恢复的退出码"与"源缺失优先于撤销恢复"的明确要求.
- README 开头与"命令与退出码"表补充: `restore` 行计入成功列但退出码为 1; 源缺失时 `undo` 不触碰目标, 即使目标为空且有备份.
- 增加表驱动测试覆盖: `Absent + 备份` -> 1; `Absent` 无备份 -> 0; `SourceMissing + 空目标 + 备份` -> `missing`, 不恢复; `SourceMissing + 本工具链接` -> `missing`, 链接保留.
- 本次不改变代码行为; 只是把规格, README 与测试统一到现有行为. 如果实现与新增规格不一致, 以规格为准修正实现.

## Capabilities

### New Capabilities

- 无.

### Modified Capabilities

- `cli-contract`: `undo` 的目标谓词与两条件判据的衔接.
- `entry-classification`: `Absent` 在 undo 谓词下的计数基准.
- `link-teardown`: 空目标恢复的退出码与源缺失边界.

## Impact

- `openspec/specs/cli-contract/spec.md`, `openspec/specs/entry-classification/spec.md`, `openspec/specs/link-teardown/spec.md`: 规格文本.
- `README.md`: 开头幂等段与命令表.
- `tests/exit_codes.rs`, `tests/undo.rs`: 退出码与源缺失恢复用例.
- `src/error.rs`: 仅在注释或实现与规格不一致时调整; 预期无行为变化.

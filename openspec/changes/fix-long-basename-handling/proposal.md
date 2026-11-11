# Proposal

## Why

备份, 恢复与撤销暂存名都会在原 basename 后追加固定后缀. 在 `NAME_MAX = 255` 的卷上, basename 超过约 207 字节时备份暂存名会先失败; 超过约 223 字节时撤销暂存名失败. 已复现: `apply` 可以在 229 字节的 basename 上成功建立链接, 但 `undo` 无法暂存并移除它, 工具自己的链接变成永久残留. 清单当前接受这些目标, 没有在改动前拒绝.

## What Changes

- 新增清单校验 V-12: `target` 的**末段 basename** (按字节, 不是显示宽度) MUST 不超过工具预算 200 字节; 超过时整体拒绝, 退出码 2, 零改动, 说明给出条目下标, 实际字节数与上限.
- 该预算为备份暂存名, 恢复暂存名与 apply/undo 暂存名的后缀预留空间; 具体常量与最长后缀写在 `backup-lifecycle`.
- **BREAKING**: 旧版接受的长 basename 清单现在会被拒绝. 这是有意的 fail-fast: 工具无法为这些名字生成合法备份/暂存名, 让 `apply` 成功而 `undo` 失败更糟.
- README 的"已知限制"补充 NAME_MAX 预算与多字节文件名的字节计数规则.
- 增加边界测试: 200 字节接受, 201 字节拒绝, 多字节文件名按 UTF-8 字节数拒绝, 既有短名清单行为不变.

## Capabilities

### New Capabilities

- 无.

### Modified Capabilities

- `manifest-schema`: 新增 V-12 目标 basename 长度预算.
- `backup-lifecycle`: 明确生成名的长度预算与"超限在清单层拒绝"的分工.

## Impact

- `src/manifest.rs`: 新增 V-12 校验与错误说明.
- `src/os/mod.rs` 或 `src/backup.rs`: 暴露一个共享的预算常量, 避免清单与 naming 逻辑各写一份.
- `tests/manifest_errors.rs`: 边界与多字节用例.
- `README.md`: 已知限制与清单校验说明.
- 不改变备份命名, 查找, 恢复或暂存语义; 不实现截断或哈希命名.

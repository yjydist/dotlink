# Proposal

## Why

`LinkedWrong` (目标路径上的符号链接落在仓库内, 但不指向本条目的 `source`) 目前和 `Foreign*` 一样受 `on_conflict` 控制. 默认 `skip` 下, `apply` 会把它报成 `conflict` 并拒绝修复, 尽管 `undo` 已经按"本工具产物"移除它. source 改名后更新清单就会触发: 旧链接仍指向仓库内旧路径, `status` 报 `wrong`, `apply` 报 `conflict`, 声明式部署无法收敛. 这也与 `link-deployment` 的 FR-007 和 README 对 `on_conflict` 的描述冲突.

## What Changes

- `LinkedWrong` 不再读取 `on_conflict`, 一律导出 `ReplaceAndBackup`: 先把旧链接备份到同目录 `<原名>.dotlink-backup.<时间戳>`, 再建立指向当前 source 的链接.
- `on_conflict` 只约束 `ForeignLink` / `ForeignFile` / `ForeignDir`; 默认 `skip` 不再阻止修复本工具自己的错误链接.
- `preview` / `apply` 对该状态报 `replace`, 说明给出旧链接的备份路径; `undo` 的分支与 token 不变.
- README 的 `on_conflict` 字段说明改为"目标被外部内容占用时的策略", 并补充 source 改名后 `apply` 自愈的说明.
- 新增 source 改名与默认 `skip` 下修复 `wrong` 链接的端到端测试; 更新分类表单测.

**BREAKING**: 同一份清单在默认策略下遇到 `LinkedWrong` 时, 行为从 `conflict` 变为 `replace`, 退出码可能从 1 变为 0. 这是刻意的行为修正.

## Capabilities

### New Capabilities

无.

### Modified Capabilities

- `entry-classification`: 动作映射中 `LinkedWrong` 不再依赖 `on_conflict`, 固定为 `ReplaceAndBackup`.
- `link-deployment`: 未获许可的冲突只覆盖外部内容; 补充本工具错误链接自愈的 Scenario.

## Impact

- 代码: `src/classify.rs` 的动作映射; `src/render.rs` 的 `replace` 说明措辞可能需要区分"外部占用"与"本工具错误链接".
- 测试: `src/classify.rs` 单测, `tests/apply.rs`, `tests/conflict.rs`, `tests/consistency.rs` 与 `tests/exit_codes.rs` 的相关断言.
- 文档: `README.md` 的 `on_conflict` 字段表与说明.
- 依赖与平台: 无新增依赖; macOS 与 Linux 行为一致.

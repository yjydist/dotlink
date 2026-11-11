# Proposal

## Why

`apply` 在 `LinkedWrong` 自愈路径上按判定时的状态选择 `.dotlink-heal.` 命名空间, 但备份前不再复核目标. 如果目标在 classify 之后被外部替换成普通文件, 目录或仓库外链接, 新内容会被复制到 `.dotlink-heal.` 并被删除/替换; `undo` 从不扫描该命名空间, 因此用户内容只能手工找回. 同一窗口在 `undo` 中已经由 `stage_verified_link` 复核, apply 侧缺失.

## What Changes

- 在 `apply` 的 `LinkedWrong` 自愈路径上, 备份前重新 lstat 目标并复核它仍是符号链接, 且链接目标仍落在仓库根内; 不满足时该条目失败, 不备份, 不删除, 不建立链接.
- 复核使用与 `classify` / `undo` 相同的 `os::link_destination` 与仓库根基准; `apply_entry` 增加已规范化的 `repo_root` 参数.
- 错误说明指出"判定之后目标已被替换为非本工具链接或内容", 提示重新运行.
- 增加测试接缝: 在 classify 与备份之间把目标替换成普通文件或仓库外链接, 断言条目失败且目标原样保留.
- `preview` 行为不变: 它不执行动作, 仍按判定结果预测自愈备份.

## Capabilities

### New Capabilities

- 无.

### Modified Capabilities

- `link-deployment`: 自愈备份前必须复核目标仍是本工具的错误链接.

## Impact

- `src/apply.rs`: `replace_and_backup` 前的复核, `apply_entry` 参数与竞态单元测试 (直接调用 `apply_entry`).
- `src/lib.rs`: 把已规范化的仓库根传给 apply.
- `README.md`: 已知限制补充该窗口已收窄但仍非事务.

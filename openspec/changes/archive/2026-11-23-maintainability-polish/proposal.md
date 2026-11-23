# Proposal

## Why

探索阶段列出了 6 个可独立排期的维护项: 并发调用没有串行化, 递归复制 / 删除仍是路径式操作, `--help` 暴露 markdown 强调符, `manifest::parse` 过长, `time` 依赖只为 UTC 时间戳, 以及运行期不探测文件系统的 NAME_MAX / PATH_MAX. 它们属于同一类长期成本, 但其中两项 (并发锁与 fd 相对遍历) 会改变失败窗口, 需要显式设计与测试.

## What Changes

- 并发串行化: 对同一清单的 `apply` / `undo` 取排他 `flock`, `preview` / `status` 取共享 `flock`; 锁加在 `dotlink.toml` 的文件描述符上, 不新建状态文件. 平台不支持 `flock` 时按既有降级策略继续, README 记录.
- 递归操作 fd 相对化: `copy_object`, `find_mount_boundary`, `remove_tree` 与 `make_tree_writable` 改用 `openat` / `fstatat` / `unlinkat` 等 fd 相对原语, 不跟随符号链接; 保持现有错误语义与测试.
- `--help` 去掉字面 `**`.
- 把 `manifest::parse` 拆成若干私有校验函数, 不改变错误文本与优先级.
- 移除 `time` 依赖, 用项目内 UTC 时间戳格式化与日历校验替代; 保持 UTC 秒级, 闰年, 月末与非法字段拒绝行为不变.
- 运行期探测 NAME_MAX: 在清单校验时, 对目标最近的已存在祖先读取 `f_namemax`, 若生成名预算超过该值则整体拒绝; PATH_MAX 仍使用平台常量, 因为没有可移植的只读运行期接口.
- 更新 README 与相关 spec delta; 不改变 token 词汇, 退出码取值或备份命名规则.
- **BREAKING**: 在 `f_namemax` 小于静态预算 250 的文件系统上, 过去被接受的清单现在会以退出码 2 整体拒绝.

## Capabilities

### New Capabilities

- 无.

### Modified Capabilities

- `cli-contract`: 新增同一清单上的并发调用串行化与帮助文本纯文本要求.
- `manifest-schema`: 新增运行期 NAME_MAX 预算 (V-14).
- `backup-lifecycle`: 新增并发调用不产生重复备份与运行期 NAME_MAX 约束.
- `backup-fidelity`: 递归复制与失败清理必须使用 fd 相对遍历, 不跟随被替换的目录项.
- `link-deployment`: 目录替换的递归删除必须使用 fd 相对遍历, 不跟随被替换的目录项.

## Impact

- `src/lib.rs`, `src/manifest.rs`, `src/cli.rs`, `src/os/{fs_ops,mount,metadata}.rs`, `src/backup/copy.rs`, `src/backup/name.rs`, `Cargo.toml`, `Cargo.lock`.
- 测试覆盖并发锁, fd 相对遍历的替换竞态, 运行期 NAME_MAX, UTC 时间戳与帮助文本.
- 不新增依赖, 不引入异步, 不写状态文件.

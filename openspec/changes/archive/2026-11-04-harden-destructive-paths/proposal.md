# Proposal

## Why

`manifest-schema` 的 V-9 拒绝文件系统根, 主目录, 仓库根及其祖先, 但没有拒绝清单文件自身. `target = "dotlink.toml"` 配合 `replace_and_backup` 会把唯一配置源备份后替换成链接, 之后所有命令都因清单不可读而退 2. 另外, `apply` 在备份后删除原对象, `undo` 在 `lstat` 后删除链接, 两步之间目标被替换时可能删掉未备份的新内容; 备份数据也没有在删除原对象前同步到稳定存储, 掉电可能同时失去原对象与备份.

## What Changes

- V-9 扩展: 归一化后的 target 等于仓库根下的 `dotlink.toml` 时整体拒绝, 说明指出条目与落点. **BREAKING**: 之前能通过校验的清单现在退 2.
- `apply` 的替换路径改为"备份 -> 原子暂存目标 -> 复核身份 -> 删除暂存对象": 备份完成后用不覆盖的 rename 把目标移到同目录暂存名, 用设备号, inode 与对象类型复核它确实是刚备份的对象, 匹配才删除; 不匹配则移回并报 `fail`, 不删除未备份内容.
- `undo` 的无备份移除同样改为"原子暂存 -> 复核仍是符号链接 (并按 design 的取舍复核链接目标归属) -> 删除"; 复核失败时移回原路径并报错, 不删除用户内容.
- 备份发布前对复制产物做 `fsync` (文件 `sync_all`, 目录在子项完成后同步), 再 rename 到最终名; 目录树按 design 的取舍决定是否同步每个子项.
- 新增清单自指的校验测试, 身份不匹配的单元测试 (通过测试接缝替换目标), 以及备份发布后可读的断言; README 的已知限制补充持久性说明.

## Capabilities

### New Capabilities

无.

### Modified Capabilities

- `manifest-schema`: V-9 扩展, 拒绝以清单文件自身为落点.
- `link-deployment`: 替换路径的原子暂存与身份复核; 备份发布前同步数据.
- `link-teardown`: 无备份移除的原子暂存与身份复核.
- `backup-fidelity`: 发布最终名前将复制产物同步到稳定存储.

## Impact

- 代码: `src/manifest.rs`, `src/apply.rs`, `src/undo.rs`, `src/backup.rs`, `src/os/mod.rs` (同步原语).
- 测试: `tests/manifest_errors.rs`, `tests/conflict.rs`, `tests/apply.rs`, `tests/undo.rs`, `tests/metadata.rs`; 可能新增测试接缝.
- 文档: `README.md` 的已知限制与 `on_conflict` 安全说明.
- 依赖与平台: 不新增依赖; `sync_all` 在 macOS 与 Linux 可用, 其他平台的既有失败分支保持不变.

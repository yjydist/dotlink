# Proposal

## Why

备份与恢复目前只复制内容与权限位. 实测: 一个 mtime 为 2020 年, 带有 `user.test` 扩展属性的文件, 在被 `replace_and_backup` 覆盖后, 备份的 mtime 变成备份时刻, `user.test` 完全丢失. 对 dotfile 管理而言, "内容恢复了但元数据回不去"仍是不完整的回滚, 会影响依赖时间戳的工具与带用户扩展属性的文件.

## What Changes

- 把备份与恢复共用的复制路径收敛为一个保真原语: 在复制内容, 类型与权限位之外, 同时复制 mtime 与全部扩展属性.
- 扩展属性采用**尽力而为**策略: 文件系统不支持, 内核附加属性不可写或权限不足时跳过该属性; 其他 I/O 错误 (如磁盘满, 复制中途读失败) 仍使该条目失败, 原对象保持原样.
- 目录的元数据在全部子项复制完成后应用, 且 mtime 最后设置 -- 子项写入会改变目录 mtime.
- `restore_backup` 应用备份中记录的元数据, 使 `apply` -> `undo` 能回到原有 mtime 与扩展属性; 旧版本产生的备份没有保真元数据, 恢复时只能得到该备份自身的时间戳, 内容与权限不受影响.
- 明确非目标: 所有权 (需要特权), ACL (macOS 的 ACL 不通过 `listxattr` 暴露), 硬链接与稀疏结构, 符号链接自身的元数据, atime, 断电持久性 (`fsync`), 以及特殊文件 (保持现有拒绝语义). 若平台把 ACL 暴露为扩展属性 (例如 Linux 的 `system.posix_acl_*`), 它会随通用扩展属性复制路径一并处理, 但不单独承诺.
- 使用已依赖的 `rustix` 提供的 `xattr` 与 `utimensat`, 不新增依赖; 不改变任何 CLI, 输出 token 或退出码.
- 更新 README 与既有设计文档中关于备份内容的描述, 并补充单元与端到端测试.
- 非破坏性变更 (行为增强): **无 BREAKING 变更**.

## Capabilities

### New Capabilities

- `backup-fidelity`: 备份对象与恢复目标在内容, 权限, mtime 与扩展属性上的保真规则, 以及元数据不可设置时的降级与失败语义.

### Modified Capabilities

无. 本项目尚无 OpenSpec 主 specs; 现有需求文档位于 `specs/001-dotfile-link-management/`, 本变更只新增上述能力.

## Impact

- 代码: `src/backup.rs` 的复制/恢复路径; `src/apply.rs` 与 `src/undo.rs` 仅通过既有 `write_backup` / `restore_backup` 接口受影响, 判定层与渲染层不变.
- 依赖: 无新增; `rustix` 的 `fs` 特性已包含所需 API.
- 文档: `README.md`, `specs/001-dotfile-link-management/data-model.md` 第 4 节.
- 测试: `src/backup.rs` 单元测试与 `tests/` 端到端用例.

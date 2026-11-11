# Proposal

## Why

替换与撤销的暂存名消歧仍是无界循环, 受限 umask 会让目录备份在暂存目录创建后失败并留下 `.partial`, 备份查找把文件名转成 UTF-8 因而可能漏掉非 UTF-8 名字, 而 `backup_path_for` 把任何 `symlink_metadata` 错误都当作"名字空闲". 这些都属于同一类问题: 暂存/备份路径的边界条件没有像正常路径那样被收紧.

## What Changes

- `os::stage_aside` 的候选名重试加入 16 次上限, 超限返回条目级错误并给出最后候选名; 与备份/恢复暂存根的既有上限一致.
- 替换与撤销暂存名的消歧序号放在 `.partial` 之前, 形如 `<原名>.<pid>.<序号>.dotlink-replace.partial` / `<原名>.<pid>.<序号>.dotlink-undo.partial`, 使所有暂存残留都以 `.partial` 结尾.
- 目录备份的暂存目录在 `mkdir` 后立即 chmod 0700, 不受进程 umask 影响; 失败清理前尽力恢复 owner rwx, 避免只读暂存树无法删除.
- 备份查找按 `OsStr` 原始字节解析目录项与后缀, 不要求文件名是 UTF-8; 索引键改为 `OsString`.
- `backup_path_for` 只有在 `NotFound` 时把候选名视为空闲; 其他错误使预测/写入失败, 不再静默选一个无法发布的名字.
- 增加测试: 占满 16 个暂存候选, umask 0200/0777 的目录备份, 非 UTF-8 备份名查找, EACCES 候选名预测, 以及替换/撤销暂存名形状.

## Capabilities

### New Capabilities

- 无.

### Modified Capabilities

- `backup-lifecycle`: 备份查找的原始字节解析与候选名占用检测错误分类.
- `backup-fidelity`: 暂存目录权限不受 umask 影响, 清理只读暂存树.
- `link-deployment`: 替换暂存名的有界重试与形状.
- `link-teardown`: 撤销暂存名的有界重试与形状.

## Impact

- `src/os/mod.rs`: `stage_aside` 上限与序号位置.
- `src/backup.rs`: 目录暂存 chmod, 清理前 chmod, `BackupFinder` 的 `OsString` 键与字节解析, `backup_path_for` 错误分类.
- `tests/apply.rs`, `tests/undo.rs`, `tests/conflict.rs`, `tests/metadata.rs`: 新用例.
- `README.md`: 已知限制与暂存名形态.
- 不改变备份内容保真, 不改变 16 次上限的既有备份/恢复语义.

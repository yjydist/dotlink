# Tasks

## 1. 替换与撤销暂存名

- [x] 1.1 把 `os::stage_aside` 改为最多 16 个候选, 超限返回包含尝试次数与最后候选名的错误; 验证: 新增单测占满 16 个候选, 断言失败说明含 16 且目录项不增加.
- [x] 1.2 把替换/撤销暂存名的消歧序号放到 `.partial` 之前, 首次候选不带序号; 验证: 单测断言 `<原名>.<pid>.dotlink-replace.partial` 与 `.<pid>.2.dotlink-replace.partial` 两种形态.
- [x] 1.3 确认暂存名不参与备份查找, 也不会被 `undo` 当作可恢复备份; 验证: 既有 `BackupFinder` 用例通过, 暂存名文件在目录中不影响 `restore`.

## 2. umask 与清理

- [x] 2.1 目录暂存根创建后立即 chmod 0700, 不受进程 umask 影响; 验证: 子进程 `umask 0200` 与 `0777` 下目录 replace 成功.
- [x] 2.2 失败清理前尽力恢复 owner rwx, 再删除暂存树; 验证: 只读源权限 + 同步失败的用例后目录中没有 `.partial` 残留.
- [x] 2.3 确认普通文件与符号链接暂存不受影响; 验证: `cargo test --locked backup` 通过.

## 3. 原始字节查找

- [x] 3.1 `BackupFinder` 使用 `OsString` 与原始字节解析目录项, 不再调用 `to_str()`; 验证: `cargo test --locked backup` 通过.
- [x] 3.2 在支持非 UTF-8 文件名的文件系统上增加"非 UTF-8 目标名备份可恢复"用例; 验证: 查找器按原始字节命中并恢复; macOS APFS 跳过该用例并说明原因. (实现注: 清单必须是 UTF-8, 无法经 CLI 构造非 UTF-8 目标, 故用例落在 `backup.rs` 单测, 直接调用 `restore_backup`.)
- [x] 3.3 确认非 UTF-8 的无关目录项不会影响其他目标; 验证: 同目录混入无关项时 `find_latest` 仍选中正确备份.

## 4. 预测错误分类

- [x] 4.1 `backup_path_for` 改为返回 `Result<PathBuf>`, 只在 `NotFound` 时视为空闲; 验证: EACCES 或 ENAMETOOLONG 的候选路径返回错误而不是继续选后续候选.
- [x] 4.2 修改 `src/lib.rs::preview` 处理 `backup_path_for` 的 `Result<PathBuf>`: 仅在 `Action::ReplaceAndBackup` 时调用, `Err` 时生成 token 为 `fail` 的报告行 (`render::fail_row`) 并附底层原因, 不展示预测路径; 验证: `cargo check --locked` 通过, 且 `rg -n "fail_row|backup_path_for" src/lib.rs` 能定位新增分支.
- [x] 4.3 确认 `apply` 的写入路径仍按既有错误收尾, 不发布最终名; 验证: `cargo test --locked apply` 通过.
- [x] 4.4 在 `tests/preview.rs` 增加用例: 候选路径的父目录不可读 (`EACCES`) 或路径过长 (`ENAMETOOLONG`) 时, `preview` 对该条目报告 `fail`, 说明含底层权限或路径错误原因, 且不报告预测备份路径; 验证: `cargo test --locked preview` 通过, 且用例在未处理 `Err` 时失败. (实现注: V-12/V-13 后 CLI 无法构造这两个错误, 改由 `backup_path_for` 单测覆盖错误分类, 并用线程局部注入单测覆盖 `preview` 的 `Err -> fail_row` 传播.)

## 5. 文档与全量验证

- [x] 5.1 更新 `README.md` 的"已知限制": 替换/撤销暂存名上限与形态, umask 下的目录备份, 非 UTF-8 备份查找, 预测失败的含义; 验证: `rg -n "暂存|umask|UTF-8" README.md` 能定位.
- [x] 5.2 运行 `openspec validate harden-backup-staging --strict`; 验证: 无 error 与 warning.
- [x] 5.3 运行 `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings` 与 `./scripts/check.sh`; 验证: 全部通过.

## Workflow follow-up

- 归档前确认 `stage_aside` 与备份暂存根共用 16 次常量, 且 README 的两种暂存名形态都写到.

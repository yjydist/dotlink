# Tasks

## 1. 清单自指

- [x] 1.1 在 `src/manifest.rs` 的 `reject_degenerate_target` 增加清单文件自身比较; 在模块单测中覆盖 `dotlink.toml`, `./dotlink.toml` 与绝对路径; 运行 `cargo test --locked manifest` 确认通过.
- [x] 1.2 在 `tests/manifest_errors.rs` 增加端到端用例, 断言退出码 2, 说明指出清单文件自身, 且文件系统无改动; 运行 `cargo test --locked --test manifest_errors` 确认通过.

## 2. 替换路径原子化

- [x] 2.1 在 `src/apply.rs` 增加对象身份快照与暂存复核助手, 覆盖匹配与不匹配两种单测; 运行 `cargo test --locked apply` 确认通过.
- [x] 2.2 把 `replace_and_backup` 的 `remove_object` 替换为"备份 -> 原子暂存 -> 身份复核 -> 删除暂存对象"; 增加正常替换与外部替换的用例, 运行 `cargo test --locked --test apply` 与 `cargo test --locked --test conflict` 确认通过.
- [x] 2.3 为身份不一致路径增加测试接缝, 断言新对象被移回并保留, 条目报 `fail`, 备份路径出现在说明中; 运行对应单测确认通过.

## 3. 撤销路径原子化与归属复核

- [x] 3.1 把 `src/classify.rs` 的 `link_destination` 提升为 `src/os/mod.rs` 的共享函数, 更新 `classify` 调用点; 运行 `cargo test --locked` 确认既有行为不变.
- [x] 3.2 给 `src/undo.rs` 的 `undo_entry` 增加已规范化的 `repo_root` 参数, `src/lib.rs::undo` 在循环外计算一次; 运行 `cargo test --locked undo` 确认签名迁移完成.
- [x] 3.3 在 `remove_link` 与 `replace_link_with_backup` 中复用 `stage_link`, 复核暂存对象仍是符号链接且目标仍属于本工具; 增加"暂存后换成仓库外链接"的用例, 断言移回, 不删除, 报失败; 运行 `cargo test --locked --test undo` 确认通过.

## 4. 备份持久性

- [x] 4.1 在 `src/os/mod.rs` 增加文件与目录同步原语, 平台不支持时返回 `Unsupported`; 增加临时文件/目录单测, 运行 `cargo test --locked os` 确认通过.
- [x] 4.2 在 `src/backup.rs` 的 `copy_object` 中对文件与目录调用同步, 在 `write_backup_at` 与 `restore_backup` 的 rename 后同步父目录; 增加同步失败接缝, 断言不发布最终名且原对象保留; 运行 `cargo test --locked backup` 确认通过.
- [x] 4.3 在 `README.md` 的已知限制补充备份持久性与额外同步开销; 人工核对与 `backup-fidelity` delta 一致.

## 5. 集成验证

- [x] 5.1 运行 `./scripts/check.sh`, 确认 fmt, test, clippy, rustdoc 与 release build 全部通过.
- [x] 5.2 运行 `cargo check --locked --target x86_64-unknown-linux-gnu` 与 `cargo check --locked --target x86_64-pc-windows-gnu`, 确认同步原语与暂存逻辑不破坏交叉编译.

## 6. 验证复核修复

- [x] 6.1 明确 rename 前/后同步失败的差异: 更新 `backup-fidelity` delta 与 design D4; `write_backup_at` 在父目录同步失败时给出已发布的备份路径; 增加仅目录注入的同步失败接缝与对应单测; 运行 `cargo test --locked backup` 确认通过.
- [x] 6.2 让 `restore_backup` 区分"未落位"与"已落位但父目录同步失败"; `undo` 在后者不回滚并报告内容已恢复; 增加注入目录同步失败的 undo 用例; 运行 `cargo test --locked undo` 确认通过.

## Workflow follow-up

- 归档前复核 `manifest-schema`, `link-deployment`, `link-teardown` 与 `backup-fidelity` 的 delta 合并结果.
- 归档后确认 README 的清单自指与持久性说明仍与主规格一致.

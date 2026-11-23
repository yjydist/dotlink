# Tasks

## 1. 并发调用串行化

- [x] 1.1 在 `src/manifest.rs` 把 `load` 改为"打开 `dotlink.toml` -> 取得 flock -> 从同一文件描述符读取"; `apply` / `undo` 取排他锁, `preview` / `status` 取共享锁, 锁 guard 由 `lib::run_on` 持有到结束; 验证: `cargo test --locked` 通过, 现有并发 apply 用例仍通过.
- [x] 1.2 处理 `flock` 的 `ENOTSUP`, `EOPNOTSUPP`, `ENOLCK`, `ENOSYS` 降级与其他错误映射, 并增加锁模式与降级的单元测试; 验证: 新测试通过, 不支持锁时命令仍按清单结果退出.
- [x] 1.3 更新 `README.md` 的并发已知限制, 说明同一清单的写入命令串行化与不支持 flock 时的降级; 验证: `grep -n "flock" README.md` 命中.

## 2. fd 相对删除与挂载扫描

- [x] 2.1 在 `src/os/mount.rs` 把 `find_mount_boundary` 与 `remove_tree` 改为从目录 fd 出发的递归: 使用 `openat` + `O_DIRECTORY | O_NOFOLLOW`, `fstatat` + `SYMLINK_NOFOLLOW`, `unlinkat` 与 `REMOVEDIR`; 验证: `cargo test --locked os::mount::` 通过, 现有 `remove_tree_*` 与挂载边界测试不回归.
- [x] 2.2 在遍历期间目录项被替换为符号链接时, 不进入链接目标; 增加测试接缝或并发测试构造替换, 断言链接目标内容保持原样; 验证: 新测试通过.
- [x] 2.3 把 `make_tree_writable` 改为 fd 相对递归, 保持挂载边界与"只作用于本次暂存对象"的语义; 验证: 现有 umask 与只读清理测试通过.

## 3. fd 相对复制与元数据

- [x] 3.1 在 `src/backup/copy.rs` 把 `copy_object` 的目录遍历改为从源目录 fd 出发: `fstatat` 判定子项, 目录用 `openat` + `NOFOLLOW` 打开, 文件用 `openat` + `O_EXCL | O_NOFOLLOW` 创建, 符号链接用 `readlinkat` + `symlinkat`; 验证: `cargo test --locked backup` 通过.
- [x] 3.2 把权限, mtime 与扩展属性应用到 fd (`fchmod`, `futimens`, `flistxattr` / `fgetxattr` / `fsetxattr`), 保持既有"扩展属性 -> 权限 -> mtime"顺序与尽力而为语义; 验证: 现有元数据保真测试通过.
- [x] 3.3 增加"复制期间目录项被替换为符号链接"的测试, 断言不跟随目标, 原对象保持原样; 验证: 新测试通过.
- [x] 3.4 运行 `cargo test --locked --test conflict --test mount_boundaries`; 验证: 通过.

## 4. 帮助文本与 parse 拆分

- [x] 4.1 去掉 `src/cli.rs` 中 `long_about` 的字面 `**`, 保持退出码段落与脚本示例不变; 验证: `dotlink preview --help` 输出不含 `**`, `cargo test --locked --test usage_errors` 通过.
- [x] 4.2 把 `manifest::parse` 拆成按校验顺序排列的私有函数, 保持错误文本, 报错优先级与 V-5 到 V-14 顺序; 验证: `cargo test --locked manifest` 通过, `git diff` 中无错误文本变化.

## 5. 移除 time 依赖

- [x] 5.1 在 `src/backup/name.rs` 用 `SystemTime` 与 civil-from-days 算法实现 UTC 秒到 `YYYYMMDDThhmmss` 的格式化, 以及 `is_timestamp` 的闰年与月天数校验; 验证: 现有合法 / 非法时间戳测试通过.
- [x] 5.2 把测试中的 `time` 构造改成固定 Unix 秒值, 移除 `time` 依赖; 验证: `cargo tree` 不再包含 `time`, `cargo test --locked` 通过.
- [x] 5.3 更新 `Cargo.lock` 并运行 `cargo test --locked`; 验证: 锁文件无 `time` 及其传递依赖, 测试通过.

## 6. 运行期 NAME_MAX

- [x] 6.1 在 `src/manifest.rs` 增加 V-14: 从 `target_path` 向上找最近已存在祖先, 用 `rustix::fs::statvfs` 读取 `f_namemax`; 探测成功且预算超过上限时整体拒绝, 失败或找不到祖先时跳过; 验证: 新增单元测试覆盖 255 接受, 143 拒绝, 探测失败跳过.
- [x] 6.2 增加测试接缝注入 `f_namemax`, 避免依赖真实文件系统的 255 上限; 验证: 注入测试通过且不并行互相影响.
- [x] 6.3 更新 `README.md` 的长度预算章节与已知限制, 写明运行期 NAME_MAX 与 PATH_MAX 的边界; 验证: `grep -n "f_namemax" README.md` 命中.

## 7. 文档与规格一致性

- [x] 7.1 确认五个 delta spec 与实现一致, 并更新 README 的并发, 遍历, 帮助与长度预算说明; 验证: `openspec validate maintainability-polish --strict` 通过.
- [x] 7.2 运行 `python3 scripts/check-punctuation.py`; 验证: 退出 0.

## 8. 全量验证

- [x] 8.1 运行 `cargo test --locked`; 验证: 全部通过.
- [x] 8.2 运行 `cargo clippy --locked --all-targets -- -D warnings`; 验证: 0 warning.
- [x] 8.3 运行 `./scripts/check.sh`; 验证: 脚本退出 0.
- [x] 8.4 运行 `openspec validate maintainability-polish --strict`; 验证: 0 error, 0 warning.

## Workflow follow-up

- Archive the change after review.

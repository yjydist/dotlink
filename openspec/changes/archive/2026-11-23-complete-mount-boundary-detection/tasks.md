# Tasks

## 1. Linux mountinfo fallback

- [x] 1.1 在 `src/os/mount.rs` 增加只读的 mountinfo 解析辅助: 解析 `/proc/self/mountinfo` 的 mount id 与 mount point, 反转义 `\040`, `\011`, `\012`, `\134`, 按路径组件找最长前缀; 验证: 新增单元测试覆盖普通路径, 转义路径, 嵌套挂载与无匹配, `cargo test --locked os::mount::` 通过.
- [x] 1.2 用 `OnceLock` 缓存一次解析结果, 在 Linux 的 `mount_id` 中当 `statx` 没有返回 `STATX_MNT_ID` 时使用 fallback; 读不到 mountinfo 时返回 `None`; 验证: 新增测试用例或测试接缝断言 fallback 被选中且解析失败退回 `None`, `cargo test --locked os::mount::` 通过.

## 2. macOS f_fsid

- [x] 2.1 在 `#[cfg(target_os = "macos")]` 的 `mount_id` 分支使用 `statvfs` 或 `fstatvfs` 的公开 `f_fsid: u64`; 失败或 `f_fsid` 为 0 时返回 `None`; 验证: `cargo check --locked` 在当前 Linux 环境通过, macOS 分支以代码审阅与条件编译为证据.
- [x] 2.2 保持 `same_mount` 的既有语义: 两侧都有标识比标识, 否则退回设备号; 验证: 现有 `mount_ids_win_only_when_both_sides_have_them` 等单元测试通过.

## 3. 集成与文档

- [x] 3.1 确认 `copy_object`, `find_mount_boundary`, `remove_tree` 都通过 `mount_identity` 获得新来源, 无需改动失败语义; 验证: 现有挂载边界单元测试与 `tests/mount_boundaries.rs` 通过.
- [x] 3.2 更新 `README.md` 的 `on_conflict` 挂载说明与已知限制, 写明 Linux mountinfo fallback 与 macOS `f_fsid`, 以及仍存在的平台边界; 验证: `grep -n "mountinfo\|f_fsid" README.md` 命中.
- [x] 3.3 确认两个 delta spec 与实现一致; 验证: `openspec validate complete-mount-boundary-detection --strict` 通过.

## 4. 全量验证

- [x] 4.1 运行 `cargo test --locked`; 验证: 全部通过.
- [x] 4.2 运行 `python3 scripts/check-punctuation.py`; 验证: 退出 0.
- [x] 4.3 运行 `./scripts/check.sh`; 验证: 脚本退出 0.
- [x] 4.4 运行 `openspec validate complete-mount-boundary-detection --strict`; 验证: 0 error, 0 warning.

## Workflow follow-up

- Archive the change after review.

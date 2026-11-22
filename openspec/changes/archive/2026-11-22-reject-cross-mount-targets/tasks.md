# Tasks

## 1. 挂载身份原语

- [x] 1.1 在 `src/os` 增加 `MountIdentity` 与 `mount_identity`: 保存设备号, Linux 使用 `rustix::fs::statx` 的 `STATX_MNT_ID`, 不可用或失败时回退设备号, macOS 只使用设备号, 非 Unix 提供可编译 stub; 验证: 新增单元测试覆盖同一挂载, 不同设备, 挂载标识缺失三种比较, `cargo test --locked os::` 通过.
- [x] 1.2 增加线程局部测试接缝, 让下一次挂载身份比较返回模拟的挂载边界; 验证: 接缝单元测试通过, 生产构建中不包含该分支 (以 `#[cfg(test)]` 收口).
- [x] 1.3 为 Linux `statx` 的 `stx_mask` 判定与 `NOSYS`/`EINVAL` 回退补测试或代码审阅记录; 验证: 单元测试通过, 且在支持与不支持 `STATX_MNT_ID` 的路径上都不产生 panic.

## 2. 复制路径

- [x] 2.1 让 `copy_to_staging` 计算顶层源的父目录挂载身份, `copy_object` 在创建暂存对象或读取内容之前比较文件/目录与父目录的挂载身份, `copy_child` 传递当前目录的挂载身份; 挂载边界使用 `CopyFailure::Fatal`, 不重试; 验证: 新增单元测试通过, `cargo test --locked backup` 通过.
- [x] 2.2 让复制失败清理 (`remove_any`) 与 `make_tree_writable` 使用不跨挂载边界的删除/权限遍历; 边界出现时停止清理并保留残渣; 验证: 接缝测试覆盖清理停止分支, `cargo test --locked backup` 通过.
- [x] 2.3 新增 `tests/mount_boundaries.rs` 的 Linux 端到端用例: 在 `unshare -Ur -m sh -c` 中构造 tmpfs, 源仓库 bind mount, 目标自身挂载点与文件挂载点, 运行真实 `dotlink apply`; 断言退出码 1, 行 token 为 `fail`, 挂载内容与源仓库保持原样, 不出现最终备份名; `unshare` 或 `mount` 不可用时打印原因并跳过; 验证: `cargo test --locked --test mount_boundaries` 通过或按原因跳过.
- [x] 2.4 补充恢复路径端到端用例: 备份目录中含挂载点时 `undo` 报告 `fail`, 按既有撤销规则回滚或保留目标, 备份与挂载内容不变; 验证: 同一测试文件中的用例通过, `cargo test --locked --test mount_boundaries` 通过.
- [x] 2.5 更新 README 的 `on_conflict` 说明与"已知限制": 目录含挂载点时备份与替换失败, 不跨挂载边界复制或删除, Linux 设备号加挂载标识, macOS 同设备挂载的漏检边界; 验证: `rg -n "挂载" README.md` 能定位说明, 内容与规格一致.
- [x] 2.6 运行 50 条目性能用例, 观察新增挂载身份检查的开销; 验证: `DOTLINK_PERF=1 cargo test --locked --test apply fifty_entries_preview_and_apply_finish_under_two_seconds_with_fifty_rows` 通过, 默认 (不设 `DOTLINK_PERF`) 测试也通过.

## 3. 删除路径

- [x] 3.1 在 `src/os` 或 `src/backup` 增加 `find_mount_boundary` 与不跨挂载边界的递归删除原语: 进入每个子项前比较挂载身份, 发现边界返回携带路径的错误, 不进入该挂载点; 保持不跟随符号链接与既有只读目录清理语义; 验证: 单元测试与接缝测试通过, `cargo test --locked apply undo` 通过.
- [x] 3.2 修改 `remove_staged`: 删除目录前先扫描挂载边界; 发现边界时不删除任何对象, 尝试 `rename_noreplace` 把暂存目录移回目标, 移回失败时报告暂存路径; 未发现边界时使用新的递归删除; 删除过程中出现边界时停止并保留暂存目录; 验证: 新增单元测试覆盖移回成功, 移回失败, 删除中止三个分支, `cargo test --locked apply` 通过.
- [x] 3.3 把 `remove_staged` 的挂载边界失败接入现有 `fail` 行, 说明包含挂载点完整路径与"先卸载或移走"的提示; 验证: 单元测试断言 token 为 `fail` 且说明包含路径, 现有 `tests/apply.rs` 与 `tests/exit_codes.rs` 通过.
- [x] 3.4 更新 README 已知限制: `preview` 不递归预检目录内的挂载点; 静态挂载点在备份阶段失败, 备份后出现的挂载点在删除前被拦截; 验证: `rg -n "preview.*挂载|挂载.*preview" README.md` 能定位说明.
- [x] 3.5 为删除阶段补充端到端或接缝集成用例, 断言挂载内容不被删除, 目标路径或暂存路径可恢复, 不建立链接; 验证: `cargo test --locked --test mount_boundaries` 与 `cargo test --locked apply` 通过.

## 4. 全量验证

- [x] 4.1 运行本地完整门禁: `cargo fmt --check`, `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps --document-private-items`, `./scripts/check.sh`; 验证: 全部通过.
- [x] 4.2 运行 `openspec validate reject-cross-mount-targets --strict` 与 `openspec validate --all --strict`; 验证: 无 error, 无 warning.
- [x] 4.3 运行 `./scripts/check-cross.sh`; 验证: 已安装目标通过, 未安装目标与缺失 `cargo-audit` 按原因跳过.
- [x] 4.4 检查 `git diff` 只包含本变更声明的生产代码, 测试, README 与 OpenSpec 制品; 验证: `git status --short` 与 `git diff --stat` 人工核对, 没有无关改动.

## Workflow follow-up

- 归档前确认本变更的规格 delta 已同步到主规格, 且 README 与规格行为一致.
- 归档后在支持 Linux user namespace 的环境运行一次 `tests/mount_boundaries.rs`, 记录跳过原因或结果.
- 若未来要覆盖 Linux 旧内核的 bind mount 或 macOS 同设备挂载, 另立变更评估 `/proc/self/mountinfo` 或平台挂载列表, 不修改本次已归档的契约.

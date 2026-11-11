# Tasks

## 1. 拆分 backup 模块

- [ ] 1.1 建立 `src/backup/` 目录, 把命名/时间戳/查找移到 `name.rs`, 复制/发布/恢复/同步移到 `copy.rs`, API 与 `BackupKind` 留在 `mod.rs`; 验证: `cargo test --locked backup` 通过.
- [ ] 1.2 把内联测试移到 `backup/tests.rs`, 测试接缝移到 `backup/test_support.rs`; 验证: `cargo test --locked backup` 通过且测试数量不变.
- [ ] 1.3 收窄 `staging_path` 为私有, 检查 `mod.rs` re-export 只暴露既有调用点需要的项; 验证: `cargo clippy --locked --all-targets -- -D warnings` 通过.

## 2. 拆分 os 模块

- [ ] 2.1 把 `Resolver`, `normalize`, `link_destination`, `is_inside_repo` 移到 `os/normalize.rs`; 验证: `cargo test --locked os::` 通过.
- [ ] 2.2 把 `rename_noreplace`, `stage_aside`, `sync_file`, `sync_dir` 移到 `os/fs_ops.rs`, `os/mod.rs` 保留平台 stub, `FileIdentity` 与 re-export; 验证: `cargo test --locked os::` 通过.
- [ ] 2.3 确认 `normalize` 的末段不解析语义与深度上限未变; 验证: 既有 `normalize_*` 测试全部通过.

## 3. 宽度表与退出码

- [ ] 3.1 把 `character_width` 的宽字符范围改为排序静态表 + 有序查找, 保留组合记号/零宽分支; 验证: 现有渲染测试通过, 并用 Python 3.15 unicodedata 17.0 对已分配码点抽查无差异.
- [ ] 3.2 让 `goal_reached` 与 `no_pending_work` 使用穷尽 `match` 或共享 `EntryOutcome::column()`; 验证: 新增遍历全部 token 的测试通过, 手动新增一个假变体时编译失败.
- [ ] 3.3 把 `deploy_token` 的表外 fallback 改为穷尽或 `debug_assert`; 验证: 现有 token 表测试通过.

## 4. 渲染与错误说明

- [ ] 4.1 `render` 预分配并直接写入转义字段与补齐空格, `summary_line`/`exit_code_of` 不物化 token `Vec`; 验证: 输出逐字节比较测试通过, 5000 条目 fixture 不回归.
- [ ] 4.2 `restore_backup` 增加同目录 `debug_assert` 与文档; 验证: 既有恢复测试通过, debug 构建断言不触发.
- [ ] 4.3 `RestoreError::LandedButUnsynced` 在 apply 的说明中区分"内容已恢复但同步失败"; 验证: 注入父目录同步失败的用例通过, 说明不同时出现"恢复失败"与"已恢复"的矛盾.
- [ ] 4.4 合并 `row_from_state` 的参数为上下文结构或接受 `&EntryState` (以实现可读性为准); 验证: 现有报告测试与 `cargo doc` 通过.

## 5. 全量验证

- [ ] 5.1 运行 `openspec validate refactor-internal-structure --strict`; 验证: 无 error 与 warning.
- [ ] 5.2 运行 `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps --document-private-items` 与 `./scripts/check.sh`; 验证: 全部通过.
- [ ] 5.3 确认 `git diff --stat` 只有模块移动与局部重写, 没有行为相关改动; 验证: 人工核对 diff 与既有规格.

## Workflow follow-up

- 归档前确认 `skip_specs: true` 仍成立: 本变更没有用户可见行为变化.

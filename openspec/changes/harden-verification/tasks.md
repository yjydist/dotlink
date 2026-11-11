# Tasks

## 1. 元数据快照与保真

- [ ] 1.1 扩展 `tests/support/mod.rs` 的 `snapshot`/`collect`: 记录 lstat mode, file type, device/inode 与 mtime, 不记录 atime; 验证: `cargo test --locked status preview manifest_errors` 通过.
- [ ] 1.2 在 `tests/metadata.rs` 增加权限位断言: 0640 文件, 0750 目录与嵌套子项, 断言备份与恢复两侧的 `mode & 0o777`; 验证: `cargo test --locked metadata` 通过.
- [ ] 1.3 确认零改动测试在只读命令上仍通过, 并新增一个故意 chmod 的负向断言验证快照能发现变化; 验证: 负向用例按预期失败后再改回.

## 2. 每条命令独立零改动

- [ ] 2.1 把 `assert_all_commands_reject` 的 before/after 比较移入 `for command` 循环, 每条命令单独快照; 验证: `cargo test --locked manifest_errors` 通过.
- [ ] 2.2 检查 `tests/apply.rs` 与 `tests/manifest_errors.rs` 中其他"四条命令共用一个前后快照"的写法, 改为逐命令比较; 验证: `rg -n "snapshot_all" tests` 人工核对没有遗留的 net-zero 模式.

## 3. CLI 与标点契约

- [ ] 3.1 根帮助测试解析 Commands 段并断言恰好四个子命令; 验证: 新增第五个假命令时测试失败, 移除后通过.
- [ ] 3.2 扩展 `scripts/test_check_punctuation.py`: 为 `src`, `tests`, `scripts`, `openspec/specs`, `openspec/changes`, `AGENTS.md`, `README.md` 各加一条命中; 增加 symlink 跳过与非法 UTF-8; 验证: `python3 scripts/test_check_punctuation.py` 通过.
- [ ] 3.3 把 `.github` 相关测试保留为"未来模板"用例, 并在 README 中说明该范围是刻意保留; 验证: 文档与测试一致.

## 4. 稳定性

- [ ] 4.1 删除或把 `tests/apply.rs` 的 2 秒墙钟断言改为显式 opt-in, 保留行数/token/汇总断言; 验证: 默认测试在负载下不因时间失败.
- [ ] 4.2 `running_as_root` 改用 `tempfile::tempdir()` 或线程唯一后缀; 验证: 并行测试重复运行 10 次无偶发失败.
- [ ] 4.3 增加 `TZ=Pacific/Nowhere` 与 unset TZ 的备份命名用例; 验证: 时间戳仍是 15 字符 UTC.
- [ ] 4.4 增加 symlink manifest 指向目标的反例; 验证: V-9 退出 2.
- [ ] 4.5 合并重复的 socket 目录用例, 并给并发 apply 用例增加启动屏障或重复竞态; 验证: 删除重复后覆盖不变, 并发用例至少观察一次真实重叠.

## 5. 可选跨目标脚本

- [ ] 5.1 新增 `scripts/check-cross.sh` (或 `check.sh --full`): 在目标已安装时运行 `cargo check --locked --target x86_64-unknown-linux-gnu` 与 `x86_64-pc-windows-gnu`, 以及 `cargo audit`; 目标/工具缺失时跳过并说明; 验证: 在本机运行输出与跳过原因.
- [ ] 5.2 在 README 的开发段说明该脚本是可选本地门禁, 不引入 CI; 验证: `rg -n "check-cross|audit" README.md` 能定位.

## 6. 全量验证

- [ ] 6.1 运行 `openspec validate harden-verification --strict`; 验证: 无 error 与 warning.
- [ ] 6.2 运行 `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings` 与 `./scripts/check.sh`; 验证: 全部通过.

## Workflow follow-up

- 归档前确认 `skip_specs: true` 仍成立: 本变更没有产品行为变化.

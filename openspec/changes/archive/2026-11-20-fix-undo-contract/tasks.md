# Tasks

## 1. 规格与文档

- [x] 1.1 更新 `openspec/specs/cli-contract/spec.md` 的"各命令的目标谓词": undo 条件一加入"本次没有从备份恢复任何内容", 并补空目标有/无备份的 Scenario; 验证: `openspec validate fix-undo-contract --strict` 通过.
- [x] 1.2 更新 `openspec/specs/entry-classification/spec.md` 的"目标谓词对特殊状态的计数基准": `Absent` 且无备份为真, 有备份为假, 并补源缺失 + 空目标 + 备份的 Scenario; 验证: 同上.
- [x] 1.3 更新 `openspec/specs/link-teardown/spec.md`: 新增"空目标恢复的退出码与源缺失边界"要求, 明确 restore 退 1 与源缺失不恢复; 验证: 同上.
- [x] 1.4 更新 `README.md` 开头幂等段与"命令与退出码"表: restore 计入成功列但退出码 1, 源缺失时 undo 不触碰目标; 验证: `rg -n "restore|源缺失" README.md` 能定位新增限定.
- [x] 1.5 更新 `src/cli.rs` 的 `undo` long_about, 在"目标路径上不存在本工具创建的链接"之后写入"且本次没有从备份恢复任何内容", 使 `Absent + 有备份` 不再暗示退出 0; 只改帮助文本, 不改判定或退出码; 验证: `rg -n "没有从备份恢复" src/cli.rs` 能定位, 且 `cargo run --locked -- undo --help` 输出含完整谓词.

## 2. 测试矩阵

- [x] 2.1 在 `tests/exit_codes.rs` 增加表驱动用例: `Absent + no backup -> ok/0`, `Absent + backup -> restore/1`, `LinkedCorrect + no backup -> undo/1`, `LinkedCorrect + backup -> restore/1`; 验证: `cargo test --locked exit_codes` 全部通过.
- [x] 2.2 在 `tests/undo.rs` 增加"源缺失 + 空目标 + 备份"用例: token 为 `missing`, 目标仍为空, 备份未被恢复或删除, 退出码 1; 验证: 该用例通过.
- [x] 2.3 在 `tests/undo.rs` 增加"源缺失 + 悬空链接 + 备份"用例: 链接保持原样, 备份不变, 退出码 1; 验证: 该用例通过.
- [x] 2.4 确认既有 `undo_restores_a_backup_when_the_target_is_absent` 仍断言退出 1, 且说明包含备份路径; 验证: 该用例通过.
- [x] 2.5 在 `tests/usage_errors.rs` 增加断言: `undo --help` 同时包含"不存在本工具创建的链接"与"没有从备份恢复"语义, 防止帮助文本再次退化为只写前半句; 验证: `cargo test --locked usage_errors` 通过, 且删除 long_about 后半句时该用例失败.

## 3. 全量验证

- [x] 3.1 运行 `cargo test --locked` 与 `cargo clippy --locked --all-targets -- -D warnings`; 验证: 全部通过.
- [x] 3.2 运行 `./scripts/check.sh`; 验证: 标点检查, fmt, 测试, clippy, rustdoc 与 release 构建全部通过.

## Workflow follow-up

- 归档前确认 README, cli-contract, entry-classification 与 link-teardown 对 "restore 退出码 1" 的措辞一致.

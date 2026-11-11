# Tasks

## 1. 规格与文档

- [ ] 1.1 更新 `openspec/specs/cli-contract/spec.md` 的"各命令的目标谓词": undo 条件一加入"本次没有从备份恢复任何内容", 并补空目标有/无备份的 Scenario; 验证: `openspec validate fix-undo-contract --strict` 通过.
- [ ] 1.2 更新 `openspec/specs/entry-classification/spec.md` 的"目标谓词对特殊状态的计数基准": `Absent` 且无备份为真, 有备份为假, 并补源缺失 + 空目标 + 备份的 Scenario; 验证: 同上.
- [ ] 1.3 更新 `openspec/specs/link-teardown/spec.md`: 新增"空目标恢复的退出码与源缺失边界"要求, 明确 restore 退 1 与源缺失不恢复; 验证: 同上.
- [ ] 1.4 更新 `README.md` 开头幂等段与"命令与退出码"表: restore 计入成功列但退出码 1, 源缺失时 undo 不触碰目标; 验证: `rg -n "restore|源缺失" README.md` 能定位新增限定.

## 2. 测试矩阵

- [ ] 2.1 在 `tests/exit_codes.rs` 增加表驱动用例: `Absent + no backup -> ok/0`, `Absent + backup -> restore/1`, `LinkedCorrect + no backup -> undo/1`, `LinkedCorrect + backup -> restore/1`; 验证: `cargo test --locked exit_codes` 全部通过.
- [ ] 2.2 在 `tests/undo.rs` 增加"源缺失 + 空目标 + 备份"用例: token 为 `missing`, 目标仍为空, 备份未被恢复或删除, 退出码 1; 验证: 该用例通过.
- [ ] 2.3 在 `tests/undo.rs` 增加"源缺失 + 悬空链接 + 备份"用例: 链接保持原样, 备份不变, 退出码 1; 验证: 该用例通过.
- [ ] 2.4 确认既有 `undo_restores_a_backup_when_the_target_is_absent` 仍断言退出 1, 且说明包含备份路径; 验证: 该用例通过.

## 3. 全量验证

- [ ] 3.1 运行 `cargo test --locked` 与 `cargo clippy --locked --all-targets -- -D warnings`; 验证: 全部通过.
- [ ] 3.2 运行 `./scripts/check.sh`; 验证: 标点检查, fmt, 测试, clippy, rustdoc 与 release 构建全部通过.

## Workflow follow-up

- 归档前确认 README, cli-contract, entry-classification 与 link-teardown 对 "restore 退出码 1" 的措辞一致.

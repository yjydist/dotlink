# Design

## Context

`undo` 的退出码由 `src/error.rs` 的 `goal_reached(Command::Undo)` 导出: 只要出现 `Undo` 或 `Restore`, 条件一即为假. README 与 `tests/undo.rs` 已经把 `Absent + 备份 -> restore + 退出 1` 作为用户可见行为. 但 `cli-contract` 的两条件判据与 `entry-classification` 的"目标谓词对特殊状态的计数基准"按字面推出 0. 这不是实现分歧, 而是契约文本没有把"恢复动作"纳入 undo 的目标谓词.

## Goals / Non-Goals

**Goals:**

- 用一条可判定的目标谓词同时解释: 链接态 `undo`, 链接态 `restore`, 空目标 `restore`, 空目标 `ok`, 源缺失 `missing`.
- 保持现有代码行为与 README/测试的退出码 1, 避免对脚本造成新的 breaking change.
- 明确源缺失时不恢复空目标上的备份, 并为此补规格与测试.

**Non-Goals:**

- 不改变 `restore` 在汇总"成功"列的归属; 成功列与退出码条件一是两个维度.
- 不改变备份查找, 恢复落位或 token 表.
- 不引入 `undo --dry-run` 或新的命令面.

## Decisions

### D1. undo 的目标谓词纳入"本次没有恢复"

把 `undo` 的条件一定义为: "目标路径上不存在本工具创建的链接, 且本次没有从备份恢复任何内容". 这与 `goal_reached(Undo) = !(Undo | Restore)` 完全同构, 也使两条件判据保持原样.

备选方案: 把代码改成 `Absent + Restore` 退出 0. 否决理由: 恢复是一次可观察的写操作, 退出 0 会让脚本误以为"什么都没发生"; README 与既有测试也会变成 breaking change. 备选方案: 把 `Restore` 从成功列移到未处理列. 否决理由: 恢复成功应当计入成功, 分列语义不应为了退出码而扭曲.

### D2. 源缺失优先于空目标恢复

`classify` 已经在目标侧判定之前返回 `SourceMissing`. `undo` 的快照也只对 `LinkedCorrect | LinkedWrong | Absent` 查备份. 因此"源缺失 + 空目标 + 备份"不会恢复. 这是有意的: 没有源就无法判断条目是否仍然有效, 恢复旧内容可能把用户带回一个已被删除的配置. 本次只把该边界写入规格与 README.

备选方案: 源缺失时仍按目标是否为空决定是否恢复. 否决理由: 会让 `undo` 在源已被删除的情况下写回旧内容, 且与"源缺失是终态"的求值顺序冲突.

### D3. 用表驱动测试锁定退出码

在 `tests/exit_codes.rs` / `tests/undo.rs` 增加覆盖矩阵: `(Current, has_backup, token, exit)` 四列, 至少包含 `Absent + no backup -> ok/0`, `Absent + backup -> restore/1`, `LinkedCorrect + no backup -> undo/1`, `LinkedCorrect + backup -> restore/1`, `SourceMissing + backup -> missing/1`. 这样以后改 token 或目标谓词时, 表格会先失败.

## Risks / Trade-offs

- [规格文本与 README 的措辞可能再次漂移] -> 在 `link-teardown` 的新增要求中直接写出退出码 1, 并在 README 的命令表与开头同时限定; 两者用同一组测试锁定.
- [用户可能认为"恢复成功却退 1"反直觉] -> README 解释: 退出码表达"是否还有未达成的目标", 恢复动作本身就是 undo 要报告的事件; 汇总仍把该行计入成功.
- [源缺失时不恢复可能让用户觉得数据不可达] -> 备份仍在原地, 说明与 README 给出找回路径; 用户可先恢复源再运行 undo.

## Migration Plan

无数据迁移. 行为不变, 只统一规格与文档. 回滚方式是还原规格, README 与测试的提交; 已部署文件系统不受影响.

## Open Questions

- 无.

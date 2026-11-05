# Tasks

## 1. 备份命名空间基础

- [ ] 1.1 在 `src/backup.rs` 引入 `BackupKind` 与用户/自愈两个命名空间标记, 让 `backup_path_for`, `write_backup`, `backup_candidates_at` 与暂存名构造按命名空间生成, 共享既有的排他暂存, 完整复制, 原子发布与父目录同步路径; 验证: 运行 `cargo test --locked backup`, 新增用例覆盖两种命名空间的裸名与 `.2` 消歧名.
- [ ] 1.2 在 `src/backup.rs` 增加单测: 自愈名的完整形态, 同一秒内的消歧, 自愈暂存名, `BackupFinder` 忽略只有自愈备份的目录, 以及原文件名本身包含 `.dotlink-backup.` 时自愈名仍不被查找接受; 验证: 运行 `cargo test --locked backup`, 断言用户备份的查找结果与既有测试完全一致.
- [ ] 1.3 更新 `README.md` 的备份命名说明, 写清 `.dotlink-backup.` 与 `.dotlink-heal.` 两种命名空间的来源与用途; 验证: 人工核对 README 与 `backup-lifecycle` delta 的"只有完整命名的备份参与查找"逐条一致.

## 2. apply 自愈写入与外部替换分流

- [ ] 2.1 在 `src/apply.rs` 增加从 `Current` 到 `BackupKind` 的单一映射, 让 `LinkedWrong` 使用自愈命名空间, `ForeignLink` / `ForeignFile` / `ForeignDir` 使用用户命名空间, 并把命名空间传入 `replace_and_backup`; 验证: 运行 `cargo test --locked apply`, 断言自愈路径写出的名字含 `.dotlink-heal.` 且外部替换路径写出的名字含 `.dotlink-backup.`.
- [ ] 2.2 在 `src/apply.rs` 与 `tests/apply.rs`, `tests/conflict.rs` 增加用例: 自愈后目录不新增该目标的用户备份, 外部占用仍只产生用户备份, 自愈备份无法写入时旧链接保留且条目报失败; 验证: 运行 `cargo test --locked --test apply --test conflict`, 权限不足用例在 root 下按既有约定跳过并打印原因.
- [ ] 2.3 更新 `README.md` 的自愈段落, 写清自愈备份使用 `.dotlink-heal.` 而不是 `.dotlink-backup.`, 并说明外部占用的命名不变; 验证: 人工核对 README 与 `link-deployment` delta 的"本工具的错误链接在部署时自愈"一致.

## 3. preview 预测一致性

- [ ] 3.1 在 `src/lib.rs` 的 `preview` 中按 `state.current` 选择 `BackupKind`, 复用第 2 组的映射, 使 `LinkedWrong` 预测自愈命名空间, 外部占用预测用户命名空间; 验证: 运行 `cargo test --locked lib`, 断言 `preview` 仍不执行任何写操作.
- [ ] 3.2 在 `tests/preview.rs` 增加用例: 错误链接的 `replace` 行给出 `.dotlink-heal.` 路径, 外部占用的 `replace` 行给出 `.dotlink-backup.` 路径, 且两次 `preview` 对文件系统无副作用; 验证: 运行 `cargo test --locked --test preview`, 断言路径的命名空间与随后 `apply` 实际写入的命名空间相同.
- [ ] 3.3 更新 `README.md` 的 `preview` 说明, 写清预测备份路径按现状区分两种命名空间, 跨秒时字面时间戳可以不同; 验证: 人工核对 README 与 `link-deployment` delta 的两个 preview Scenario 一致.

## 4. undo 只恢复用户备份

- [ ] 4.1 在 `src/lib.rs` 的 `undo` 与 `src/backup.rs` 的 `BackupFinder` 旁补充注释, 说明撤销只查用户命名空间, 自愈备份不参与快照与 token 的"有无备份"判断; 不改变查找与恢复代码路径; 验证: 运行 `cargo test --locked backup undo`, 既有 token 与恢复测试全部通过.
- [ ] 4.2 在 `tests/undo.rs` 增加情形 A 的端到端用例: 目标原本不存在, 部署后将 `source` 改名并 `apply` 自愈, 再 `undo`; 断言目标回到不存在, token 为 `undo`, 自愈备份仍在, 没有用户备份, 第二次 `undo` 全部为 `ok` 且退出码 0; 验证: 运行 `cargo test --locked --test undo`, 并断言重复 `apply` 不再新增该目标的用户备份.
- [ ] 4.3 在 `tests/undo.rs` 增加情形 B 的端到端用例: 用户文件经 `replace_and_backup` 产生用户备份 B1, 之后 `source` 改名触发自愈产生 B2, 执行 `undo`; 断言恢复的是 B1, token 为 `restore`, B2 的路径与内容不变; 验证: 运行 `cargo test --locked --test undo`, 断言恢复后的目标内容等于用户原始内容而不是符号链接.
- [ ] 4.4 更新 `README.md` 的 `undo` 段落, 说明撤销只恢复用户原始备份, 自愈备份永远不会被选中; 验证: 人工核对 README 与 `link-teardown` delta 的"撤销只恢复用户原始备份"及其三个 Scenario 一致.

## 5. 旧残留与已知限制

- [ ] 5.1 增加一个特征化测试, 在临时目录构造旧版本形态的 `.dotlink-backup.` 链接备份, 断言新代码不会把它改名或迁移成 `.dotlink-heal.`, 且它仍按既有查找规则被选中, 从而把已知限制固定为可观察行为; 验证: 运行 `cargo test --locked backup undo`, 断言测试同时记录"旧备份不被自动删除".
- [ ] 5.2 在 `README.md` 的"已知限制"补充旧版自愈备份残留: 说明其与合法用户备份同名形, 给出 `ls -l` 与 `readlink` 的检查步骤, 人工确认后改名移开而不是直接删除, 以及 `undo` 已恢复旧链接时如何用较早备份人工恢复用户内容, 符号链接备份使用 `cp -P` 或 `mv`; 验证: 人工核对步骤在临时目录可执行且不包含未经确认的删除命令.
- [ ] 5.3 在 `README.md` 的自愈段落补充"升级后旧自愈备份可能仍污染 undo"的交叉引用, 指向 5.2 的已知限制; 验证: 人工核对交叉引用锚点与文字, 确认没有承诺自动迁移或自动清理.

## 6. 集成验证

- [ ] 6.1 运行 `./scripts/check.sh`, 确认格式化, 全量测试, clippy, rustdoc 与 release 构建全部通过; 验证: 命令退出码 0, 失败项定位到具体任务后修复并重跑.
- [ ] 6.2 运行 `cargo test --locked --test apply --test undo --test preview --test conflict`, 确认自愈, 撤销, 预览与冲突四条端到端路径在同一构建上通过; 验证: 测试摘要显示 0 failed, 并抽查情形 A 与情形 B 的断言输出.
- [ ] 6.3 运行 `openspec validate "fix-selfheal-backup-semantics" --strict`, 并人工复核 README 与三份 delta 没有矛盾; 验证: 校验输出为通过, README 的命名空间, 旧残留与 undo 语义三处说明与规格逐条对应.

## Workflow follow-up

- 归档前复核 `backup-lifecycle`, `link-deployment` 与 `link-teardown` 的 delta 合并到主规格后的文本与 Scenario 完整性.
- 归档后确认 README 的自愈, undo 与已知限制三处说明仍与主规格一致, 且没有引入自动回收或状态文件.

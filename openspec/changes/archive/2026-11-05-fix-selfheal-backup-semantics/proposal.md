# Proposal

## Why

`apply` 的"错误链接自愈"会把旧链接本身按用户备份的命名空间写成 `<目标>.dotlink-backup.<时间戳>`, 该符号链接备份随即成为同目录"最新备份", 于是 `undo` 恢复它而不是用户的原始内容: 目标原本不存在时撤销后回到悬空链接, 目标原本是用户文件时用户内容对应的更早备份永远不会被使用. 每次再次 `apply` 又新增一份自愈备份, 状态不收敛到未部署态; README 分别记录了"自愈会备份后重建"与"undo 只恢复最新备份", 但没有记录二者交互.

## What Changes

- 自愈备份改用独立命名空间: `<原名>.dotlink-heal.<YYYYMMDDThhmmss>`, 同一秒内消歧为 `.dotlink-heal.<时间戳>.<n>`. 该命名空间使用独立标记, 整个名字不构成用户备份的 `<原名>.dotlink-backup.<完整后缀>` 形态, 因此不进入"用户备份"查找.
- `apply` 的自愈路径仍保持"先完整备份旧链接, 备份落定后才删除旧链接", 但备份写入使用自愈命名空间; 外部占用的 `replace_and_backup` 备份命名保持 `<原名>.dotlink-backup.<时间戳>` 不变.
- `preview` 对 `LinkedWrong` 的 `replace` 行预测自愈命名空间的备份路径, 与 `apply` 实际使用的命名规则和消歧规则同源; 外部占用的预测仍使用用户备份命名空间.
- `undo` 只查找并恢复用户原始备份. 自愈备份留在磁盘上 (备份不自动回收), 但永远不会被 `undo` 选中; 自愈后撤销因此能回到未部署态, 或在用户内容被覆盖前已有用户备份时恢复该内容.
- 旧版本已经产生的自愈型链接备份仍以旧命名空间留在同目录, 可能继续污染 `undo`; 这是已知限制, 写入 README 并给出人工识别, 清理与恢复建议, 不做自动迁移.
- 保持不变: token 词汇与退出码语义, 外部占用备份的命名, 备份不自动回收, 不引入状态文件, 自愈"先备份后删除"的顺序, 以及 `LinkedWrong` 不受 `on_conflict` 约束的分类结论.

## Capabilities

### New Capabilities

无.

### Modified Capabilities

- `backup-lifecycle`: "只有完整命名的备份参与查找" 增加自愈备份命名空间不参与用户备份查找的要求与 Scenario.
- `link-deployment`: "本工具的错误链接在部署时自愈" 增加自愈备份命名空间, 备份路径与 preview 预测一致的要求与 Scenario.
- `link-teardown`: 新增一条 requirement "撤销只恢复用户原始备份" (ADDED), 不修改该能力已有的 requirement.

## Impact

- 代码: `src/backup.rs` (命名空间常量, 候选名构造, 写入与查找), `src/apply.rs` (自愈调用点传入命名空间), `src/lib.rs` (preview 按现状选择命名空间); `src/classify.rs` 的动作映射不变.
- 测试: `src/backup.rs`, `src/apply.rs` 与 `src/lib.rs` 的模块单测; `tests/apply.rs`, `tests/undo.rs`, `tests/conflict.rs` 与 `tests/preview.rs` 的端到端用例, 覆盖自愈后撤销与 preview/apply 备份路径一致.
- 文档: `README.md` 的自愈段, `undo` 段与"已知限制", 补充自愈备份命名空间, 旧备份残留的人工处理建议.
- 兼容性: 只新增备份命名空间并停止写入旧的自愈备份形态; 旧版产生的自愈备份不迁移, 不自动删除; 无状态文件, 无新依赖, 平台无关.

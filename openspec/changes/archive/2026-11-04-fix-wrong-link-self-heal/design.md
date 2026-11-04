# Design

## Context

`classify` 是唯一的状态判定入口, 输出 `Current` 与 `Action`. 当前 `action_for` 把 `LinkedWrong` 和三种 `Foreign*` 放在同一分支, 由 `on_conflict` 决定 `RefuseConflict` 或 `ReplaceAndBackup`; `apply` 已经实现 `ReplaceAndBackup` 的备份与替换, `undo` 也已经把 `LinkedWrong` 视为本工具产物. 因此本变更只需要改动作映射与报告措辞, 不需要新增执行路径.

## Goals / Non-Goals

**Goals:**

- `LinkedWrong` 在默认 `skip` 下也导出 `ReplaceAndBackup`, 使 source 改名后 `apply` 能收敛.
- `on_conflict` 只约束真正的 `Foreign*`.
- `preview` / `apply` 仍用 `replace` token, 说明能区分"外部占用"与"本工具链接指向错误".
- 用端到端测试锁定 source 改名与默认策略的组合.

**Non-Goals:**

- 不改变 `undo` 的归属规则与分支表.
- 不新增 token, `Action` 变体或状态文件.
- 不改变备份命名, 不自动回收旧备份.
- 不处理 `LinkedWrong` 与外部并发替换的竞态; 该问题由 `harden-destructive-paths` 承接.

## Decisions

### D1. 复用 `ReplaceAndBackup`, 不新增动作

`LinkedWrong` 固定映射到现有 `Action::ReplaceAndBackup`. 这样可以复用"先备份, 再删除, 再建链"的既有顺序与错误路径, 也不需要改 `token_for` 的 token 表. 备选方案是新增 `Action::ReplaceOwnLink`, 直接删除旧链接而不备份; 否决理由是需要新增 enum 分支, 报告分支与测试矩阵, 而旧链接本身是符号链接, 备份成本极低, 保留备份更符合现有"替换前先备份"的不变量.

### D2. 旧链接也备份

即使 `LinkedWrong` 是"本工具创建"的, 替换前仍走标准备份命名 `<原名>.dotlink-backup.<时间戳>`. 这样 `apply` 的删除步骤只面对"已备份对象", 不需要在 `replace_and_backup` 内再区分对象来源. 备份内容是链接本身, 不会复制其目标; 每条目多一个备份名是刻意的审计痕迹.

### D3. 报告措辞区分两种 replace

token 仍是 `replace`, 但说明按 `Current` 分两种:

- `Foreign*` -> 保持"冲突占用 (...)"的措辞.
- `LinkedWrong` -> "本工具的链接指向错误 (实际指向: ...)" + "已备份到 ... 并重建" (preview 用将来时).

`note_for_token` 已经同时拿到 `current` 与 `backup_path`, 因此不需要新增参数; 只增加一个按 `current` 分支的措辞. 这样 `preview` 与 `apply` 的 token 序列保持一致, 只有措辞区分.

### D4. README 与 on_conflict 语义

README 的字段表改为"目标被外部内容占用时的策略", 并在说明中补一句: 指向错误的本工具链接在 `apply` 时自动备份并重建, 不受 `on_conflict` 影响. 这样文档与 `link-deployment` 的 FR-007 重新一致.

## Risks / Trade-offs

- **用户手工建在仓库内的链接会被替换** -> 该链接在现有归属规则下已经被 `undo` 视为本工具产物; 现在 `apply` 与 `undo` 一致, 且旧链接先备份. 在 README 中说明.
- **source 改名会产生额外备份** -> 备份是符号链接, 体积可忽略; 用户可自行清理, 系统保持"不自动回收"的边界.
- **行为变更影响脚本** -> 默认 `skip` 下同一状态的退出码可能从 1 变为 0; 在 proposal 标记 **BREAKING**, README 说明.
- **报告措辞变更影响文本断言** -> 更新 `tests/conflict.rs` 与 `tests/apply.rs` 中针对 `replace` 说明的断言, 但 token 不变.

## Migration Plan

无数据迁移. 已有清单无需修改; 行为变化只影响 `LinkedWrong` 且默认 `skip` 的组合. 回滚时恢复 `action_for` 与报告措辞即可, 备份文件不受影响.

## Open Questions

无.

# Design

## Context

`apply` 在 classify 之后才执行动作. `LinkedWrong` 自愈路径无条件选择 `.dotlink-heal.` 并备份目标. `BackupFinder` 只扫描用户命名空间, 因此 heal 备份中的用户内容无法由 `undo` 恢复. `undo` 已经在暂存后复核链接归属, apply 缺少对应复核.

## Goals / Non-Goals

**Goals:**

- 在写入自愈备份前确认目标仍是"符号链接且目标在仓库内"的形态.
- 不满足时 fail-fast, 保留目标, 不写备份, 不建立链接.
- 复用 `classify` / `undo` 的链接归属口径, 不引入第二套路径语义.

**Non-Goals:**

- 不消除 classify 与备份之间的全部 TOCTOU; 复核后到复制开始仍有窄窗口.
- 不改变 `Foreign*` 分类的 `on_conflict` 语义.
- 不改变自愈命名空间或 `undo` 的恢复集合.

## Decisions

### D1. 复核失败即失败, 不按实际对象改选命名空间

如果目标已变成普通文件, 目录或仓库外链接, 说明判定结果已失效. 此时若把内容备份到用户命名空间并继续替换, 会绕过 `on_conflict = skip` 的许可, 因为 `LinkedWrong` 的动作本来不读取该策略. 因此必须停下, 让用户基于新状态重新运行.

备选方案: 按备份时的实际对象选择 `BackupKind` (文件/目录 -> User, 仓库外链接 -> User) 并继续. 否决理由: 这会在 `skip` 条目上删除用户内容; 备份虽可恢复, 但不是用户授权的动作. 备选方案: 只在目标不是符号链接时失败, 不检查链接去向. 否决理由: 仓库外链接仍是符号链接, 会把用户链接塞进 heal 命名空间.

### D2. `apply_entry` 接收已规范化的仓库根

`replace_and_backup` 需要 `os::link_destination` 与 `os::is_inside_repo` 复核归属. 仓库根由 `lib::apply` 在循环外规范化一次, 作为参数传入, 避免每条目重复规范化.

### D3. 测试接缝

沿用 `apply.rs` 现有的线程局部 hook 风格, 增加"备份前"一次性 hook: 在 classify 之后, `write_backup` 之前替换目标. 用例覆盖替换成普通文件与仓库外链接, 断言失败, 目标原样, 目录中没有 `.dotlink-heal.` 备份.

## Risks / Trade-offs

- [复核与复制之间仍有窄窗口] -> 与 README 既有并发限制一致; 复核把"分类后任意时刻"的窗口收窄为一次 lstat 到复制开始.
- [合法竞态下条目失败而不是自动收敛] -> 用户重新运行 apply 会得到新状态; 这比在 `skip` 条目上删除用户内容安全.
- [新增 `repo_root` 参数扩散到测试调用点] -> 测试已有 `repo_root` 上下文; 调整是机械的, 并由编译器强制.

## Migration Plan

无数据迁移. 自愈备份命名与恢复不变. 回滚时移除复核与参数即可; 已部署链接不受影响.

## Open Questions

- 无.

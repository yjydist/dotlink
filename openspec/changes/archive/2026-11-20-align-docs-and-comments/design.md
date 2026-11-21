# Design

## Context

本变更只改文档与注释, 动机与已发现的漂移见 `proposal.md` 的 Why. `.openspec.yaml` 设置 `skip_specs: true`, 因为没有用户可见行为变化; 但当前 CLI 2.1.1 不支持 `skip_design`, `openspec status` 会把 design 列为 `ready`, `isPlanningComplete` 为 false. 归档要求所有制品为 done 或 skipped, 因此需要一份 design.md 使规划完整.

文档校准依赖以下实现事实:

- `src/entry.rs` 的 `Entry.source_path` 在 `resolve_with` 中由 `expand_source` 经 `os::normalize` 得到绝对路径, `Entry` 的文档也声明 `source_path` 与 `target_path` 都是绝对路径.
- `src/apply.rs` 的 `create_link` 直接把 `entry.source_path` 交给 `create_symlink`, 因此链接文本是绝对 source 路径.
- `openspec/specs/backup-fidelity/spec.md` 只承诺复制可读取的扩展属性, 不单独承诺 ACL; macOS 的 ACL 不经 `listxattr` 暴露, 平台若把 ACL 暴露为扩展属性则随属性复制.
- 现行 `README.md` 在元数据说明与"元数据保真为尽力而为"两处使用"不保留所有权与 ACL"的笼统措辞, 与上述规格不一致.

## Goals / Non-Goals

**Goals:**

- 用 design.md 记录文档与实现的对应关系, 使注释与 README 的修改有可核对基准.
- 补齐 README 的两处文档缺口: 绝对链接文本在仓库移动后的限制, 以及 ACL 措辞与 `backup-fidelity` 规格对齐.
- 让 OpenSpec 规划状态完整, 变更可进入归档流程.

**Non-Goals:**

- 不改变任何生产逻辑, token, 退出码, 测试断言或清单格式.
- 不把链接文本改为相对路径; 该改动会改变链接归属判定, `undo` 语义与既有链接的字节内容, 超出本变更范围.
- 不实现 ACL 保真或所有权保留.
- 不修改主规格或新增能力规格.

## Decisions

### D1. 用 design.md 记录纯文档变更

备选是留空 design 或依赖 `skip_design`; CLI 2.1.1 没有该能力, 会让 design 一直处于 `ready`, 规划无法完成. 这里补一份简短 design.md, 内容限定为文档与实现的对应关系及验证方式, 不描述代码实现. 这样 `openspec status` 与 `openspec validate --strict` 能通过, 归档不会因规划不完整被阻塞.

### D2. README 明确绝对链接文本与仓库移动限制

`apply` 写入的是规范化后的绝对 source 路径. 仓库改名或整体移动后, 已存在的旧链接仍指向旧绝对路径; 该路径落在新仓库根之外, 按归属规则会被判为 `foreign`. `undo` 只移除本工具创建的链接, 因而跳过这些旧链接; 默认 `on_conflict = "skip"` 的 `apply` 报 `conflict`, 不做替换, 工具无法自愈. 因此 README 的已知限制应写明: 移动仓库前先执行 `undo`, 或移动后手工清理旧链接. 同时明确当前不使用相对链接文本, 避免读者误以为已有相对链接支持.

备选是改用相对链接文本; 否决理由: 链接文本会随目标与源的相对位置变化, 归属复核与 `undo` 的仓库内判定都需要额外语义, 且会改变现有链接的字节内容, 超出纯文档变更范围.

### D3. README 的 ACL 措辞与 backup-fidelity 规格对齐

现行 README 写"不保留所有权与 ACL", 而 `backup-fidelity` 规格的表述是: 不单独承诺 ACL, macOS 的 ACL 不经 `listxattr` 暴露, 若平台把 ACL 暴露为扩展属性则随该属性一并复制. README 应改为同一语义, 所有权仍明确不保留. 可以举例说明 Linux 的 `system.posix_acl_access` 属于扩展属性, 但只作为例子, 不构成跨平台承诺.

备选是保留笼统的"不保留 ACL"; 否决理由: 与规格冲突, 在 Linux 等平台上会低估扩展属性复制带来的实际效果, 也可能让维护者误判保真边界.

### D4. 文档与最终代码一致

注释和 README 的表述以最终实现为准. 每项修改在 tasks 中附带可执行验证: `rg` 定位关键字, `cargo doc` 验证私有项文档可构建, 以及全量 `./scripts/check.sh`. 若实现与文档再次漂移, 以代码和规格为准修正文档, 不在本变更中反向修改代码.

## Risks / Trade-offs

- **文档再次漂移** -> tasks 为每项修改给出定位命令, 并用 `./scripts/check.sh` 做全量验证; 后续行为变更应同步更新 README 与注释.
- **相对链接看起来更利于仓库移动** -> 当前实现固定使用绝对路径; 本变更只记录限制并对移动场景给出操作建议, 不改变链接语义.
- **ACL 措辞可能被读成跨平台承诺** -> 使用条件句并明确指出"不单独承诺", 只把扩展属性复制作为唯一机制.
- **仓库移动后的旧链接无法自动识别** -> README 建议移动前 `undo`, 或移动后手工清理被判为 `foreign` 的旧链接; 不把静默跳过描述成成功.

## Migration Plan

无产品迁移. 只修改 README, 源码注释与脚本文档; 随后运行 `openspec validate align-docs-and-comments --strict` 与 `./scripts/check.sh`. 回滚即还原这些文档与注释改动, 不涉及文件系统状态或数据格式.

## Open Questions

无.

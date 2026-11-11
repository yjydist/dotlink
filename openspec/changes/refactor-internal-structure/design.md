# Design

## Context

当前三个大文件的生产代码与测试交错, 读者必须在同一文件内跳转数千行. `backup.rs` 的线程局部测试接缝与生产函数相邻; `os/mod.rs` 同时承担路径解析, 身份, 暂存, 同步与平台 stub. 这不是行为问题, 但每次修改都要重新建立上下文. 另有两处结构性风险: 退出码谓词用 `matches!` 非穷尽, Unicode 宽度表在生产与测试侧各一份.

## Goals / Non-Goals

**Goals:**

- 按职责拆分大模块, 测试与测试接缝有独立位置, 生产 API 面收窄.
- 退出码谓词对新增 token 编译期失败; 汇总与退出码共用同一 token 分桶.
- 渲染减少每行分配; 宽度表只有一份生产数据, 测试保留独立 oracle.
- 行为, token, 退出码, 备份命名与恢复语义完全不变.

**Non-Goals:**

- 不改变任何规格或用户可见输出.
- 不引入新依赖, 不改变 MSRV, 不引入异步.
- 不重写 `normalize`, `classify` 或 `backup` 的算法; 只移动与收窄.
- 不为性能做超出"去掉明显重复分配"的优化.

## Decisions

### D1. `backup` 与 `os` 按职责拆分

- `backup/name.rs`: 命名空间常量, 时间戳格式化/校验, 候选名生成, `BackupFinder`.
- `backup/copy.rs`: `copy_object`, `copy_to_staging`, 发布, `restore_backup`, 同步与清理.
- `backup/mod.rs`: `BackupKind`, `PublishedBackup`, `write_backup`, `find_latest_backup`, re-export.
- `backup/tests.rs`: 现有内联测试, 使用 `use super::*`.
- `backup/test_support.rs`: `#[cfg(test)]` 线程局部注入与守卫.
- `os/normalize.rs`: `Resolver`, `normalize`, `link_destination`, `is_inside_repo`, 相关测试.
- `os/fs_ops.rs`: `rename_noreplace`, `staging_path` (私有), `stage_aside`, `sync_file`, `sync_dir`.
- `os/mod.rs`: 平台 stub, `FileIdentity`, `pub(crate) use` 与模块声明.

备选方案: 仅移动测试, 不拆生产代码. 否决理由: 生产文件仍然混杂命名与复制, 审查成本只下降一半. 备选方案: 新建 crate 或 feature gate. 否决理由: 单个 bin crate 没有复用需求, 增加构建复杂度.

### D2. 宽度表使用排序静态表

把生产 `character_width` 的匹配臂改为 `const WIDE_RANGES: &[(u32, u32)]` 与有序查找; 组合记号/零宽区间仍单独判断. 测试侧保留 `UNICODE_17_WIDE_RANGES` 作为独立 oracle, 不复用生产表, 以免同源错误互相掩盖.

备选方案: 保留 match 并只改注释. 否决理由: 生产表与 oracle 双份 300 行数据, 升级 Unicode 时要改两处; 排序表至少让生产侧只有一份. 备选方案: 生成代码模块. 否决理由: 当前没有生成脚本, 引入 build.rs 或 checked-in generated file 的成本高于收益.

### D3. 退出码谓词穷尽

`goal_reached` 与 `no_pending_work` 改为穷尽 `match`; 或者暴露 `EntryOutcome::column()` 并让 `tally`, `goal_reached`, `no_pending_work` 共用. 新增测试遍历所有 `EntryOutcome` 变体, 断言每个变体都有分桶与退出码处理.

备选方案: 只加测试, 保留 `matches!`. 否决理由: 测试可能漏掉新增变体; 编译期穷尽更可靠.

### D4. 渲染减少分配

`render` 预分配 `String` (按行数 * 估计宽度), 用 `write!`/`push_str` 写入转义后的字段与补齐空格; `summary_line` 从 `rows` 迭代计数; `exit_code_of` 改为迭代器输入, 不先收集 `Vec<EntryOutcome>`. 这保持"报告一次 write_all"的契约.

### D5. 收窄公开面与文档

`staging_path` 改为私有; `restore_backup` 增加同目录 `debug_assert`; `RestoreError::LandedButUnsynced` 在 apply 的说明中区分"内容已恢复但同步失败"; `deploy_token` fallback 改为穷尽. 这些都是内部质量修正, 不改变输出契约.

## Risks / Trade-offs

- [大规模移动产生巨大 diff, 可能丢失注释或可见性] -> 按模块分批提交, 每批运行 `cargo test` 与 `cargo clippy`; 只移动, 不重写算法.
- [拆分后 `pub` 面意外扩大] -> 用 `pub(crate)`/私有; 只在 `mod.rs` re-export 既有调用点需要的项.
- [排序表改变宽度结果] -> 先保留同一组区间, 用现有渲染测试与 Python oracle 对照; 任何差异都视为 bug.
- [渲染重写引入格式差异] -> 现有输出/对齐测试覆盖; 逐字节比较 `render` 结果.
- [测试文件移动导致路径/模块名冲突] -> 保持 `mod tests;` 与 `#[cfg(test)] mod test_support;`, 运行全量测试.

## Migration Plan

纯内部重构, 无数据迁移. 回滚方式是还原对应提交; 用户文件系统与备份不受影响. 建议按 D1 -> D2 -> D3 -> D4 -> D5 分批, 每批独立通过门禁.

## Open Questions

- 无.

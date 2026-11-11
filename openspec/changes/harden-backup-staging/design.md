# Design

## Context

备份/恢复暂存根已经有 16 次上限与"序号在 `.partial` 之前"的形态 (`backup-lifecycle`). 替换与撤销暂存名由 `os::stage_aside` 另起一套: 无界循环, 序号追加在完整名之后, 目录创建受 umask 影响, 查找要求 UTF-8, 预测把任意元数据错误当作空闲. 这些差异使同一类暂存名在不同路径上行为不一致.

## Goals / Non-Goals

**Goals:**

- 替换/撤销暂存名与备份暂存名一样有界, 可诊断, 形态一致.
- 目录备份不受进程 umask 影响, 失败清理能删除只读暂存树.
- 备份查找按原始字节处理, 不因非 UTF-8 名字漏掉备份.
- 预测/写入只在 `NotFound` 时把候选名视为空闲.

**Non-Goals:**

- 不改变备份内容保真, 恢复语义或 token 表.
- 不新增清理命令; 不改变"残留不自动回收"的边界.
- 不把备份/恢复暂存根的 16 次上限扩大到最终名发布循环 (最终名循环仍按既有规格无界重试).

## Decisions

### D1. `stage_aside` 复用 16 次上限

把 `os::stage_aside` 的 `for serial in 1_u64..` 改为 `1..=MAX_STAGING_ATTEMPTS`, 超限返回 `io::Error` 或条目级错误, 说明包含尝试次数与最后候选名. 常量与 `backup.rs` 共用, 避免两套数字.

备选方案: 保持无界但每次记录日志. 否决理由: 日志不解决挂死; 上限的目标是收敛. 备选方案: 只限制 apply, 不限制 undo. 否决理由: 两条路径使用同一 helper, 分开会制造第二套逻辑.

### D2. 序号放在 `.partial` 之前

`staging_path` 的 base 仍为 `<原名>.<pid>.<label>`, 但 label 拆成 `dotlink-replace` / `dotlink-undo` 与 `.partial` 后缀; 消歧序号插在 label 与 `.partial` 之间. 这与备份暂存名的 `<pid>.<serial>.partial` 形态一致, 所有残留都以 `.partial` 结尾.

备选方案: 保留 `<...>.partial.<serial>` 并在清理文档中说明两种形态. 否决理由: 文档与未来清理逻辑要处理两种后缀, 且与备份族不一致.

### D3. 暂存目录创建后 chmod 0700

目录备份的暂存根由 `DirBuilder::mode(0o700)` 创建, 但 `mkdir` 仍会应用 umask. 创建成功后立即 `set_permissions(0o700)`, 使后续子项创建不受 umask 影响. 失败清理前, 如果目标仍存在, 尽力 chmod owner rwx 再 `remove_dir_all`, 避免只读源权限导致的清理失败.

备选方案: 在创建前后临时清空 umask. 否决理由: 进程级 umask 是多线程共享状态, 修改它会影响其他线程; Rust 标准库也不提供安全的 umask 读写. 备选方案: 创建时使用 0777 再依赖 umask. 否决理由: 受限 umask 仍会剥离 owner 位, 且 0777 更宽松.

### D4. 备份查找使用 `OsString` 原始字节

`scan_backups` 不再对 `DirEntry::file_name()` 调用 `to_str()`. 在 Unix 上用 `OsStrExt::as_bytes()` 按 `<原名>.dotlink-backup.` 的字节切分, 时间戳与序号要求 ASCII; 索引键改为 `OsString`. 非 UTF-8 的无关项跳过, 非 UTF-8 的目标名仍能命中.

备选方案: 继续用 `to_str()` 并在文档声明"不支持非 UTF-8 目标名". 否决理由: README 已承诺文件系统操作保留原始字节, 只有报告层有损; 备份查找不应成为例外.

### D5. 预测占用检测只在 NotFound 时视为空闲

`backup_path_for` 改为返回 `Result<PathBuf>`, 只把 `ErrorKind::NotFound` 视为空闲; 其他错误原样上抛. `preview` 在预测失败时把该行报告为 `fail` 并给出原因; `apply` 的写入路径本来就会在创建/改名时失败, 现在提前得到一致错误.

备选方案: 预测失败时退回第一个候选并继续. 否决理由: 这会让 preview 报告一个实际无法发布的名字, 与"预览结论与 apply 一致"冲突.

### D6. 测试

- `stage_aside`: 占满 16 个候选, 断言失败说明与不创建第 17 个; 首次与序号形态.
- umask: 在子进程中设置 `umask 0200` / `0777`, 目录 replace 成功; 同步失败后无 `.partial`.
- 非 UTF-8: 在支持该名字的文件系统上创建目标与备份, 断言 finder 能命中; macOS APFS 跳过.
- 预测: 用不可读父目录或 `ENAMETOOLONG` 构造, 断言 `preview` 报告 `fail`.

## Risks / Trade-offs

- [替换/撤销暂存名上限可能导致极端目录下多失败一次] -> 说明给出尝试次数与最后候选名, 用户可清理; 16 与备份暂存根一致, 便于解释.
- [序号位置变化影响外部脚本匹配暂存名] -> 暂存名是工具保留命名空间, README 明确禁止用户依赖; 变化写进已知限制.
- [chmod 0700 可能绕过用户对 umask 的预期] -> 暂存目录是工具私有临时对象, 发布后会应用源权限; 不受 umask 影响是正确性要求.
- [非 UTF-8 查找增加 `cfg(unix)` 分支] -> 与现有 `os` 平台层一致; 非 Unix 平台由平台门禁拒绝, 不需要运行时支持.
- [预测失败改为 fail 可能让 preview 更严格] -> 这是诚实报告: 无法确认候选名空闲时不应承诺一个路径.

## Migration Plan

无数据迁移. 已有备份名仍按原形态可查找; 替换/撤销暂存名是临时产物, 形态变化不影响已部署状态. 回滚时恢复 `stage_aside` 的无界循环与 `to_str()` 查找即可; 备份内容不变.

## Open Questions

- 16 次上限是否需要在后续按用户反馈调整; 当前与备份暂存根共用常量.

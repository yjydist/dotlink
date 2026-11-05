# Tasks

## 1. 暂存根排他取得与有界重试

- [x] 1.1 在 `src/backup.rs` 顶部新增 `const MAX_STAGING_ATTEMPTS: u64 = 16;`, 并新增模块私有的 `CopyFailure { StagingNameTaken, Fatal(io::Error) }`; 把 `copy_object` 的返回值改为 `Result<(), CopyFailure>`, 只有创建本次调用的目标根名时收到 `AlreadyExists` 才给出 `StagingNameTaken`, 其余错误一律 `Fatal`; 运行 `cargo test --locked backup` 确认既有测试 (含 `a_copy_failure_cleans_only_its_own_partial_directory`) 仍通过.
- [x] 1.2 把 `copy_to_staging` 重写为 `serial in 1..=MAX_STAGING_ATTEMPTS` 的有界循环: 仅 `StagingNameTaken` 继续下一个候选, `Fatal` 立即转成 `Error::EntryFailed`; 超限时返回包含已尝试次数与最后一个候选名的说明; 运行 `cargo test --locked backup`, 并新增单测占满前 16 个候选, 断言返回 `Err` 且说明含 `16`, 目录项数量不增加, 16 个占用者的 inode 与内容不变.
- [x] 1.3 收紧清理边界: 根名冲突时不调用任何清理 (占用者属于别人), 根取得后填充失败时只对本次暂存根调用 `finish_copy`; 用目录内含 socket 的方式触发填充失败, 断言既有备份与用户文件原样, 原对象可读; 运行 `cargo test --locked backup` 确认通过.
- [x] 1.4 在 `README.md` 的"已知限制"补充: 暂存根名的尝试上限为 16 个候选名, 超限时条目失败且说明给出已尝试次数与最后一个候选名; 人工核对措辞与 `specs/backup-lifecycle/spec.md` 的"暂存根名冲突重试有界" Scenario 一致.

## 2. 嵌套复制冲突快速收敛

- [x] 2.1 按 `src/os/mod.rs:151-226` 的形态, 在 `src/backup.rs` 的 `#[cfg(test)]` 区域增加线程局部注入: `Cell<u32>` 记录注入次数, 守卫在 `Drop` 时恢复前值, 并在子项递归入口返回 `AlreadyExists`; 注入点不得覆盖暂存根的创建; 运行 `cargo test --locked backup` 确认编译且既有测试不受影响.
- [x] 2.2 让子项递归把 `StagingNameTaken` 包装成带子项路径的 `Fatal`, 使说明指出失败子项而不是"暂存名被占用"; 新增单测在目录含子项时启用注入并调用 `write_backup_at`, 断言返回 `Err`, 错误文本含子项名且不含"暂存名被占用", 注入计数恰为 1; 运行 `cargo test --locked backup` 确认通过.
- [x] 2.3 在同一用例中断言失败后没有最终名的备份, 没有本次的 `.partial` 残留, 原对象内容与权限不变, 同目录既有备份不变; 运行 `cargo test --locked backup` 确认通过.
- [x] 2.4 在 `README.md` 的"已知限制"补充: 递归复制中的 `AlreadyExists` 使该条目直接失败, 不再换暂存名重试; 人工核对与 `specs/backup-fidelity/spec.md` 的"嵌套复制冲突收敛为条目失败" Scenario 一致.

## 3. 恢复路径与正常路径回归

- [x] 3.1 确认 `restore_backup` 仍复用 `copy_to_staging`; 新增恢复路径单测: 用 `RESTORE_SUFFIX` 占满前 16 个候选时返回 `RestoreError::NotLanded`, 目标路径未被创建, 备份内容不变; 再增加一个嵌套注入用例, 断言同样返回 `NotLanded`; 运行 `cargo test --locked backup` 确认通过.
- [x] 3.2 回归正常路径: 备份与恢复成功时不残留 `.partial`, 首个候选被占用时仍能换到 `.2.partial` 并成功发布, 目录备份内容与元数据不变; 运行 `cargo test --locked` 确认全部既有测试通过.
- [x] 3.3 用 `git diff --stat` 复核本变更只改 `src/backup.rs` 与 `README.md`; 确认没有改动其他 capability 的主规格, 没有改动 `openspec/changes/` 下其他目录; 核对 `copy_to_staging` 的循环里不再有一揽子重试判据 `Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue`, 只有最外层的暂存根冲突会换名.
- [x] 3.4 在 `README.md` 的同一条"已知限制"中明确恢复路径复用相同的暂存上限与相同的失败语义; 人工核对措辞与 `specs/backup-lifecycle/spec.md` 的"暂存根名冲突重试有界" Scenario 一致.

## 4. 集成验证

- [x] 4.1 运行 `./scripts/check.sh`, 确认 fmt, test, clippy, rustdoc 与 release build 全部通过.
- [x] 4.2 运行 `cargo check --locked --target x86_64-unknown-linux-gnu` 与 `cargo check --locked --target x86_64-pc-windows-gnu`, 确认 `CopyFailure` 与测试接缝不破坏交叉编译.
- [x] 4.3 运行 `openspec validate "bound-staging-retries" --strict`, 确认规格 delta 与主规格要求块一致且无警告.
- [x] 4.4 人工复核 `specs/backup-lifecycle/spec.md` 与 `specs/backup-fidelity/spec.md` 的 MODIFIED 块逐字包含主规格原有标题与 Scenario, 且新增 Scenario 均使用 4 个 `#`.
- [x] 4.5 人工核对 `design.md` 的 D2 上限常量 16, README 的已知限制与两份 delta 的 Scenario 三方一致, 确认没有遗留"无界重试"或"嵌套冲突换名"的表述.

## 5. 验证补强

- [x] 5.1 在 `src/backup.rs` 的 `#[cfg(test)]` 区域增加 `apply_metadata` 的线程局部失败接缝, 新增 `a_metadata_failure_publishes_no_backup_and_keeps_the_original` 覆盖 `backup-fidelity` 的"元数据阶段失败" Scenario, 运行 `cargo test --locked --lib backup` 确认通过.
- [x] 5.2 将 `design.md` D4 的接缝描述改为启用标志与注入计数分开, 并注明仅测试用的元数据失败接缝; 人工核对与 `src/backup.rs` 的 `FAIL_METADATA` 与 `METADATA_FAILURES` 一致.

## Workflow follow-up

- 归档前复核 `backup-lifecycle` 与 `backup-fidelity` 的 delta 合并结果, 确认并行的其他变更对 `backup-lifecycle` 的修改未被本次覆盖.
- 归档后确认 README 的已知限制与主规格的"暂存名排他创建且只清理本次产物"及"备份的发布仍以完整复制为前提"一致.
- 若后续调整 `MAX_STAGING_ATTEMPTS`, 同步更新 `specs/backup-lifecycle/spec.md` 的 Scenario, `design.md` 的 D2 与 README.

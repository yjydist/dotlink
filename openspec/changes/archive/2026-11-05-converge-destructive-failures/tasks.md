# Tasks

## 1. 身份基准改为备份实际复制的对象

- [x] 1.1 让 `backup::copy_object` 返回复制起点的顶层对象元数据, 由 `copy_to_staging` 与 `write_backup_at` 一路带回; `write_backup` 返回 `PublishedBackup { path, source_metadata }`, 由 `apply` 转为对象身份, 并同步调整 `src/backup.rs` 中所有解构该返回值的单测; 运行 `cargo test --locked backup` 确认通过.
- [x] 1.2 删除 `apply::replace_and_backup` 在写备份之前的目标身份快照, 改用 1.1 返回的元数据构造 `ObjectIdentity`; 保留 `after_backup` 接缝并新增 `before_backup` 接缝, 单测覆盖"备份前目标被替换 -> `replace`"与"备份后目标被替换 -> `fail`"两种情形; 运行 `cargo test --locked apply` 确认通过.
- [x] 1.3 把暂存复核失败的说明改为"备份时的对象与删除时的对象不一致", 并更新 `src/apply.rs` 中依赖旧措辞的单测, 补充替换物为符号链接与目录的断言; 运行 `cargo test --locked apply` 确认通过.
- [x] 1.4 在 `README.md` 的"已知限制"中把身份复核的边界写准确: 基准是备份复制起点, 备份前/后被替换的失败方向与残余窄窗口; 人工核对与 `link-deployment` 的"替换删除前必须原子复核身份" delta 一致.

## 2. 替换末段建链失败时恢复原对象

- [x] 2.1 让 `replace_with_backup` 接收本次备份路径; `remove_staged` 成功后 `create_link` 失败时调用 `backup::restore_backup` 恢复原内容, 备份保留; 恢复成功与恢复失败分别组装说明, 两者都给出备份路径; 运行 `cargo test --locked apply` 确认通过.
- [x] 2.2 增加 `#[cfg(test)]` 的"下一次建链失败"注入点, 单测覆盖: 恢复成功时目标内容与备份一致且备份仍在; 恢复失败时目标占用物保留, 说明同时含备份路径与恢复失败原因; 原对象为目录时完整恢复; 运行 `cargo test --locked apply` 确认通过.
- [x] 2.3 保留并复核 `tests/apply.rs` 既有的"建链失败不阻塞其余条目"端到端用例; 恢复分支由 2.2 的 `#[cfg(test)]` 接缝覆盖, 不在端到端伪造无法与恢复共享写权限的建链失败; 运行 `cargo test --locked --test apply` 确认通过.
- [x] 2.4 在 `README.md` 的"已知限制"中说明建链失败会从备份恢复, 但恢复本身也可能失败, 此时原内容不在目标路径上, 只能从说明给出的备份路径找回; 人工核对与 `link-deployment` 的"替换末段建链失败时恢复原对象"一致.

## 3. `undo` 移除失败回滚与清理残留报告

- [x] 3.1 把 `remove_link` 的删除段抽成 `remove_staged_link`, `remove_file` 失败时先尝试 `os::rename_noreplace` 移回原路径; 移回成功与移回失败分别组装说明; 运行 `cargo test --locked undo` 确认既有移除路径不变.
- [x] 3.2 增加 `#[cfg(test)]` 的一次性移除失败注入点, 单测覆盖: 移回成功时目标仍是可重试链接; 移回失败时目标占用物保留, 链接留在暂存名且说明给出完整路径; 运行 `cargo test --locked undo` 确认通过.
- [x] 3.3 修改 `replace_link_with_backup` 恢复成功后的暂存清理失败分支: 不再 `let _ =`, 改为返回含残留暂存路径的条目失败, 恢复出的内容不回滚; 复用 3.2 的一次性移除失败注入点, 单测断言 token 走 `fail` 通道且残留路径存在; 运行 `cargo test --locked undo` 确认通过.
- [x] 3.4 在 `replace_link_with_backup` 的恢复失败分支 (`NotLanded` 回滚前后与 `LandedButUnsynced`) 把所尝试的备份路径加入说明; 单测断言两种失败说明都含备份路径与底层原因, 回滚成功与失败两种收尾都保留该路径; 运行 `cargo test --locked undo` 确认通过.
- [x] 3.5 在 `README.md` 的"已知限制"中说明移除失败会先移回, 恢复成功后的暂存清理失败会报告残留路径, 恢复失败说明给出所尝试的备份路径; 人工核对与 `link-teardown` 的"恢复失败时链接原样保留以便重试" delta 一致.

## 4. 目标为空且有完整备份时由 `undo` 恢复

- [x] 4.1 在 `src/lib.rs` 的撤销计划中把备份快照扩展到 `Current::Absent`; 目录读取失败仍记为 `fail` 并在动手前短路; 运行 `cargo test --locked undo` 确认既有链接态路径不变.
- [x] 4.2 在 `src/undo.rs` 为 `Absent` + 备份实现恢复, 无备份保持 `Ok`; 恢复失败说明给出备份路径; 在 `src/render.rs` 把 `Absent` + 有备份映射为 `restore`, 并区分"已移除链接后恢复"与"目标为空时恢复"的说明; 运行 `cargo test --locked undo` 与 `cargo test --locked render` 确认通过.
- [x] 4.3 在 `tests/undo.rs` 增加端到端用例: 目标为空且同目录有完整备份时, `undo` 报 `restore`, 退 1, 内容恢复, 备份仍在; 第二次 `undo` 报 `skip`, 目标与备份不变; 另断言无备份时仍报 `ok` 退 0; 运行 `cargo test --locked --test undo` 确认通过.
- [x] 4.4 同步 `README.md`: 开头幂等段的"撤销后目标为空即退 0"限定为"无完整备份可恢复"; 命令表补充目标为空但有备份时也恢复; "已知限制"补充恢复后的 `skip` / restore-again 分支; 人工核对与 `link-teardown` 的"撤销的 token 由状态与备份有无共同决定" delta 一致.

## 5. 集成验证

- [x] 5.1 运行 `./scripts/check.sh`, 确认格式检查, 全量测试, Clippy, rustdoc 与 release 构建全部通过.
- [x] 5.2 运行 `cargo check --locked --target x86_64-unknown-linux-gnu` 与 `cargo check --locked --target x86_64-pc-windows-gnu`, 确认新增的测试接缝与恢复路径不破坏交叉编译.
- [x] 5.3 用 `openspec validate "converge-destructive-failures" --strict` 复核变更仍通过, 并逐条核对 `link-deployment` 与 `link-teardown` 的 MODIFIED requirement 保留原 Scenario 标题.

## Workflow follow-up

- 归档前复核 `link-deployment` 的两个 MODIFIED/ADDED requirement 与 `link-teardown` 的两个 MODIFIED requirement 合并到主规格后的完整性, 尤其确认原 Scenario 未被删减.
- 归档后人工复核 README 中"目标为空且无备份才退 0"的措辞与主规格一致.
- 与 `fix-selfheal-backup-semantics` 合并后复核: `undo` 在"目标为空且有完整备份"分支中使用的"完整备份"必须与 `link-teardown` 的"撤销只恢复用户原始备份"同口径, 仅指用户备份命名空间; 自愈备份 (`<原名>.dotlink-heal.`) 不得触发该分支, 两个变更归档前统一措辞.
- 后续评估是否把身份基准推进为打开句柄后的 fstat, 以关闭 lstat 与 open/read_dir 之间的残余窗口; 本变更不包含该加固.

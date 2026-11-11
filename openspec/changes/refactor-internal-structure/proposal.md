# Proposal

## Why

生产代码的行为契约已经稳定, 但 `backup.rs` (2565 行), `render.rs` (1945 行) 与 `os/mod.rs` (827 行) 把命名, 复制, 恢复, 测试接缝, 路径解析, 同步与测试混在同一文件; 退出码谓词使用非穷尽 `matches!`; 报告渲染每行多次分配; Unicode 宽度表在生产与测试侧各存一份. 这些会提高后续修改的审查成本, 并留下"新增 token 不触发编译错误"的漂移风险. 本变更只做内部重构, 不改变用户可见行为.

## What Changes

- 拆分 `src/backup.rs`: `backup/name.rs` (命名, 查找, 时间戳), `backup/copy.rs` (复制, 发布, 恢复, 同步), `backup/mod.rs` (API 与 `BackupKind`); 内联测试移到 `backup/tests.rs`, 测试接缝移到 `#[cfg(test)] mod test_support`.
- 拆分 `src/os/mod.rs`: `os/normalize.rs` (`Resolver`, `normalize`, `link_destination`, `is_inside_repo`), `os/fs_ops.rs` (staging, rename, sync); `os/mod.rs` 只做 re-export.
- 退出码谓词改为穷尽 `match`, 或复用 `EntryOutcome::column()`; 新增遍历全部 token 的测试.
- 报告渲染预分配 `String`, 直接写入转义字段与补齐空格; 汇总与退出码从 `rows` 迭代计算, 不物化 token `Vec`.
- Unicode 宽度范围改为一份排序静态表 + 二分; 测试侧保留独立 oracle. 不改变现有范围数据.
- `staging_path` 收窄为私有; `restore_backup` 增加同目录 `debug_assert` 与文档.
- `RestoreError::LandedButUnsynced` 在 apply 的恢复说明中区分"内容已恢复但同步失败".
- `deploy_token` 的 fallback 改为穷尽或 `debug_assert`; `row_from_state` 的参数合并为一个上下文结构 (可选, 以可读性为准).

## Capabilities

### New Capabilities

- 无.

### Modified Capabilities

- 无. 本变更只改内部结构, `.openspec.yaml` 设置 `skip_specs: true`.

## Impact

- `src/backup.rs` / `src/os/mod.rs` 拆分; `src/render.rs`, `src/error.rs`, `src/apply.rs` 局部调整.
- 测试文件路径与 `mod` 声明变化; 测试数量与断言保持不变.
- 不改变清单格式, token, 退出码, 备份命名, 恢复语义或平台门禁.

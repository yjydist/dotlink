# Proposal

## Why

README 的"两侧都不存在的别名路径"只写到"目标与源都尚不存在"; 同样的退化也发生在两个尚不存在的 target 上. 在大小写不敏感的文件系统 (例如 macOS 默认 APFS) 或会做 Unicode 规范化的文件系统上, `~/.Foo` 与 `~/.foo` 都不存在时, 清单校验取不到文件系统身份, 字符串比较又不同, 因此会接受两个实际指向同一对象的 target. `preview` 报两个 `create`, `apply` 第一条建链后第二条因 EEXIST 报 `fail`. 这是已知限制, 不是本次要改变的行为.

## What Changes

- 扩展 README 的"两侧都不存在的别名路径"已知限制: 明确它同时覆盖两个 target 之间, 并写出 preview 与 apply 的具体后果.
- 增加一个仅在大小写不敏感卷上运行的回归测试, 锁定当前行为: 清单被接受, `preview` 报两个 `create`, `apply` 第二条报 `fail` 且说明包含 `File exists` 或底层 `EEXIST` 原因.
- 不改变清单接受面, token, 退出码或 apply 顺序.

## Capabilities

### New Capabilities

- 无.

### Modified Capabilities

- 无. 本变更只记录已知限制并增加回归测试, `.openspec.yaml` 设置 `skip_specs: true`.

## Impact

- `README.md` 与 `tests/manifest_errors.rs`; 不改变 `src/` 的生产行为.

# Proposal

## Why

清单校验目前把"规范化后的词法路径"当作唯一的比较形态. `os::normalize` 刻意不解析末段符号链接, 但会解析祖先段; 这使 V-9/V-10/V-11 在"某目标是符号链接, 另一目标位于其下"或"源本身是指向目标的符号链接"时漏判. 已复现两种后果: `apply` 成功之后清单反而失效, 以及 `status` 把自指环报告为 `ok`.

## What Changes

- 为清单校验引入"比较形态": 在现有词法规范化路径之外, 额外解析末段符号链接. 该形态只用于 V-5/V-9b/V-10/V-11 的比较, 不改变运行时 `target_path`, 也不改变 `classify` 的"末段永不解析"语义.
- V-5/V-10/V-11 同时比较词法形态与解析形态; 目标祖先/后代判定覆盖"父目标是符号链接"的嵌套.
- V-10/V-11 的 source 判定同时使用 lstat 身份与 follow 身份; 拒绝 source 符号链接指向目标或目标祖先的清单.
- V-9b 的基准身份同时保存 lstat 与 follow 形态, 避免主目录或清单文件自身是符号链接时的别名漏判.
- 增加对应集成测试, 并同步 README 的"源与目标必须相互隔离"与"落点本身也不得退化"说明.
- **BREAKING**: 旧版会接受的"符号链接父目标 + 嵌套子目标"与"source 符号链接自指"清单现在会被拒绝, 退出码 2, 零改动.

## Capabilities

### New Capabilities

- 无.

### Modified Capabilities

- `manifest-schema`: V-5/V-9b/V-10/V-11 的路径比较与身份判定扩展为双形态.
- `path-normalization`: 新增"比较形态"语义, 同时保持运行时末段不解析.

## Impact

- `src/os/mod.rs`: 新增只读的比较形态 helper, 复用现有 `Resolver`, 深度上限与词法回退保持不变.
- `src/manifest.rs`: V-5/V-9b/V-10/V-11 使用双形态与双身份; 错误说明指出实际命中的形态.
- `src/entry.rs`: 可能复用比较 helper, 但 `Entry` 的运行时路径不变.
- `tests/manifest_errors.rs`: 增加符号链接父目标嵌套, source 符号链接自指, 主目录/清单符号链接别名.
- `README.md`: 说明比较形态与"运行时末段不解析"的区别, 以及新的拒绝面.

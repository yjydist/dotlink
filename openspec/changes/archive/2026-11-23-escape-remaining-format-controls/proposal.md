# Proposal

## Why

报告层已经转义 C0/C1/DEL, U+2028/U+2029 与常见零宽 / bidi 控制符, 但 README 与 `render` 仍列出 U+00AD, U+061C, 标签字符等未覆盖的格式字符. 这些码点属于 Unicode `Cf` 与 Default_Ignorable, 可能在终端中隐藏文本或改变阅读顺序, 与"报告不改变视觉顺序"的承诺不一致.

## What Changes

- 扩展报告转义集合: 在现有 C0/C1/DEL 与布局控制符之外, 覆盖 Unicode 17.0 的 Default_Ignorable_Code_Point 码点, 例如 U+00AD, U+034F, U+061C, U+115F-1160, U+17B4-17B5, U+180B-180D, U+180F, U+200B-200F, U+202A-202E, U+2060-2064, U+2066-206F, U+3164, U+FE00-FE0F, U+FEFF, U+FFA0, U+FFF0-FFF8, U+1BCA0-1BCA3, U+1D173-1D17A 与 U+E0000-E0FFF.
- 被覆盖的字符统一以 `\u{...}` 形式输出, 与现有控制字符转义一致.
- 更新 `cli-contract` 的转义要求, README 的已知限制与转义测试.
- 不改变文件系统操作, token, 退出码或报告列宽规则.

## Capabilities

### New Capabilities

- 无.

### Modified Capabilities

- `cli-contract`: 报告转义新增"覆盖全部 Default_Ignorable 码点"的要求.

## Impact

- `src/render.rs` 的转义表与测试; `README.md`; `openspec/specs/cli-contract/spec.md` 的 delta.
- 不新增依赖, 不改变 manifest 解析, 路径解析, 链接建立或备份行为.

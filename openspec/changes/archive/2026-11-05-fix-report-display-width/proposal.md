# Proposal

## Why

README 承诺 "target 按终端显示宽度补齐, CJK 按 2 列", 但 `src/render.rs` 的宽度表只枚举了少量码点区间.
落在区间外的宽字符会被算作 1 列, 于是说明列的起点比其它行偏 1 列. 已复现两种常见字符:

```
target = "~/⌚"   (U+231A, East Asian Wide)  -> 说明列 35
target = "~/🚀"   (U+1F680, East Asian Wide) -> 说明列 35
target = "~/ab"   (2 列内容)                 -> 说明列 34
```

列对齐是报告可读性的一部分 (逐行对照清单时说明列必须稳定), 现在的实现与文档不一致, 且不一致只出现在
特定字符上, 用户难以预判.

## What Changes

- 补全 `character_width` 的宽字符区间: 按 EastAsianWidth 数据取全部 Wide 与 Fullwidth 码点, 保留现有
  CJK 区块级区间 (含未分配码点的前向兼容), 只补当前缺失的部分.
- 在实现处记录数据来源与 Unicode 版本, 在 README 写明仍然保留的近似边界 (不按字素簇分段, 变体选择符
  与 ZWJ 序列按码点近似).
- 新增单元测试锁定宽字符与邻接窄字符的判定, 新增端到端用例锁定 "等显示宽度的 target 说明列对齐".
- 不改变行内容, 转义规则, token, 汇总或退出码; 不引入新依赖.

## Capabilities

### New Capabilities

无.

### Modified Capabilities

- `cli-contract`: 新增一条需求 "报告列宽按终端显示宽度计算", 把 README 已有的对齐承诺写成可验证的
  输出契约 (现有需求只约束字段与转义, 不约束宽度).

## Impact

- `src/render.rs`: `character_width` 的区间表与注释, 少量单元测试.
- `tests/output.rs`: 对齐回归用例.
- `README.md`: 报告对齐段落的近似边界说明.
- 不受影响: 判定, 动作, 文件系统操作, 依赖树 (`Cargo.toml` / `Cargo.lock` 不变).

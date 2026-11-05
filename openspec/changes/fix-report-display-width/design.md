# Design

## Context

见 `proposal.md` 的 Why. 现状在 `src/render.rs`:

- `character_width` 用一张手写区间表: 组合记号 / 变体选择符计 0 列, 少量 Wide 与 Fullwidth 区间计 2 列,
  其余计 1 列; 注释明确"只依赖码点区间, 不引入依赖", 并把"不按字素簇分段"写成已知近似.
- README 的 "报告对齐" 段落向用户承诺了 CJK 计 2 列与按显示宽度补齐.
- 报告行的动态字段在计算宽度之前已经过 `escape_controls`, 因此宽度计算面对的是纯文本 (控制字符已变成
  `\n` / `\u{1b}` 之类 ASCII 形式).

## Goals / Non-Goals

- Goal: 全部 East Asian Wide 与 Fullwidth 码点计 2 列, 不再出现"文档说 2 列, 实现算 1 列".
- Goal: 对齐契约可被单元测试与端到端用例锁定, 防止下一次表格维护再漏块.
- Non-Goal: 不实现字素簇分段, 不处理 emoji ZWJ 序列与变体选择符的呈现形态差异 (仍按码点近似).
- Non-Goal: 不改行内容, 转义规则, token 与退出码, 不引入新依赖.

## Decisions

### D1. 补齐区间表, 不引入 `unicode-width`

备选: 依赖 `unicode-width` 并用 `UnicodeWidthStr::width`. 否决理由: 为一个纯对齐问题新增生产依赖
(当前只有 5 个直接依赖, 且 `render` 的模块注释已明确"不引入依赖"的选择); 该 crate 还会改写
Ambiguous 与变体选择符的既有约定 (README 声明 Ambiguous 按 1 列), 等于在修表的同时换了语义.
代价是 emoji ZWJ 序列仍按码点累加, 但这本来就是已声明的近似, 不影响本次要修的可观察症状.

### D2. 保留现有 CJK 区块级区间, 只补缺失的 Wide/Fullwidth 区间

现有 `0x3400..=0x4DBF`, `0x4E00..=0x9FFF`, `0x20000..=0x3FFFD` 等区块把未分配码点也算作 2 列, 这是
有意的前向兼容 (将来分配的 CJK 字符不会让旧二进制突然错位). 整体替换成"只覆盖已分配码点"的精确表会
让未分配码点退回 1 列, 属于无收益的行为回归.

因此补的只是当前完全缺失的 Wide/Fullwidth 区间: 杂项符号与箭头 (`0x231A` 起), 补充标点后的符号块,
Hangul Jamo Extended, Tangut / Khitan / Nushu 等历史文字, 以及 `0x1F000` 至 `0x1FAFF` 的各 emoji 块.

### D3. 数据来源与版本写进注释

区间取自 Unicode 17.0 的 `EastAsianWidth.txt` (`W` 与 `F` 两类), 在注释里记录版本与"如何重新生成"
(用 `unicodedata.east_asian_width` 逐码点判定并压缩成区间). 没有生成脚本参与构建, 也没有 `build.rs`:
表是源码的一部分, 升级 Unicode 版本时人工重跑一次并更新版本号.

### D4. 契约写进 `cli-contract`, 近似边界留在 README

规格只写可观察结果: 等显示宽度的 target 说明列起点相同, 宽字符计 2 列, 组合记号计 0 列, 超宽不截断.
"不按字素簇分段, ZWJ 与变体选择符按码点近似"属于已知近似, 写在 README 与该函数注释里, 不写进需求,
避免把实现层的限制升级成对外契约.

## Risks / Trade-offs

- **区间表再次漏块** -> 单元测试覆盖每个新增块的边界码点与其邻接的窄码点; README 段落写明数据来源,
  下次升级 Unicode 时按同一方法重跑.
- **把宽字符算成 2 列但终端实际按 1 列显示** (文本呈现型 emoji, 例如不带 U+FE0F 的 U+1F321) -> 现状
  同样按 2 列, 本次不改变这批码点的判定; 已在 README 的近似边界里说明.
- **对齐回归用例在不同终端字体下不稳定** -> 用例只断言"说明列起点的显示宽度相等", 不依赖字体度量, 也
  不断言具体空格个数以外的渲染结果.

## Migration Plan

无. 报告的行数, token, 说明文本与退出码不变, 只有补齐空格的数量在受影响字符上变正确.

## Open Questions

无.

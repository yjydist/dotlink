# Design

## Context

`openspec/specs/cli-contract/spec.md` 的 "报告列宽按终端显示宽度计算" 是一条 requirement, 正文 716 字符, 超过 `openspec validate --all --strict` 的 500 字符上限. 正文前半描述列补齐行为, 后半描述宽度表的整块保留与逐码点判定, 两个关注点适合分别审查. 该 requirement 目前带 6 个 Scenario: CJK 对齐, 宽字符符号与 emoji, 组合记号不占列, 整块保留的码点, 区段之外的组合记号, 超宽 target 不截断. 本变更只改规格文本, 不改实现.

## Goals / Non-Goals

**Goals:**

- 把原 requirement 拆成两条 requirement, 每条正文不超过 500 字符.
- 原正文的全部 MUST 片段与 6 个 Scenario 逐条原样保留, 仅重组归属.
- 变更级 `openspec validate fix-cli-contract-spec-length --strict` 通过.

**Non-Goals:**

- 不修改代码, 测试, CLI, README, 依赖或运行时行为.
- 不改变任何输出内容, 补齐列数, 转义形式, token 或退出码.
- 不在本变更中直接修改主规格.

## Decisions

### D1. 用 REMOVED + ADDED 表达拆分

`MODIFIED` 要求保留原有 Scenario 集合, 丢弃任一 Scenario 都会被拒绝; 即使把被丢弃的 Scenario 放进 `ADDED` 块, 校验仍按 `MODIFIED` 块的 Scenario 集比较而失败. 因此用 `REMOVED` 下线原 requirement, 再用 `ADDED` 写入两条新 requirement. 归档或同步时旧正文与旧 Scenario 集合被整体替换, 不留下重复或缺失. 备选方案是 `MODIFIED` 加 `ADDED`, 因上述限制被否决.

### D2. 两条新 requirement 使用不同名称

新 requirement 分别命名为 "报告列宽按显示宽度补齐" 与 "宽度表的整块保留与近似规则", 均不同于 `REMOVED` 块的 "报告列宽按终端显示宽度计算". 若沿用原名, 会与 `REMOVED` 块重名并被校验拒绝. 先例 `split-manifest-v9-requirement` 的核心块改名也是同一原因.

### D3. Scenario 按关注点归属

- "报告列宽按显示宽度补齐": CJK 目标与等宽 ASCII 目标对齐, 宽字符符号与 emoji, 组合记号不占列, 超宽 target 不截断.
- "宽度表的整块保留与近似规则": 整块保留的码点仍计 2 列, 区段之外的组合记号按宽度判定.

归属只决定 Scenario 随哪条 MUST 正文相邻, 不改变 WHEN/THEN 内容. 每条新 requirement 至少保留一个 Scenario.

### D4. 不在本变更直接修改主规格

主规格由 archive 或 sync 流程根据 delta 写入. 直接编辑 `openspec/specs/cli-contract/spec.md` 会绕过 delta 校验, 并让变更目录与主规格同时成为真源, 归档时产生冲突. 本变更只在 change 目录维护 delta, 由工作流后续步骤统一落盘.

## Risks / Trade-offs

- **拆分成两条后引用位置变化** -> README 或代码注释若按 requirement 名称引用 "报告列宽按终端显示宽度计算", 将不再匹配. 当前引用按行为描述而非 requirement 名称, 无需修改; 归档后按新名称检索即可.
- **REMOVED 与 ADDED 的名称差异** -> 审查时易误以为行为变化. 迁移说明与 tasks 的集合比较覆盖此风险: MUST 片段与 Scenario 名称差集为空即证明行为不变.
- **近似规则句子为说明性文字** -> "该规则是近似" 一句来自 `src/render.rs` 的既有实现注释, 不新增 MUST; 若审查认为它超出拆分范围, 可删除该句而保持正文仍不超过 500 字符.

## Migration Plan

1. 在 change 目录写入 delta: `REMOVED` 原 requirement, `ADDED` 两条新 requirement.
2. 运行 `openspec validate fix-cli-contract-spec-length --strict`.
3. 归档或同步时, 主规格中的原 requirement 被两条新 requirement 替换, 内容与 Scenario 集合不变.
4. 重新运行 `openspec validate --all --strict`, 确认 requirements[15] 的长度 warning 消失.
5. 回滚只需还原 `openspec/specs/cli-contract/spec.md` 的对应文本; 本变更不触碰代码或数据, 无运行时回滚步骤.

## Open Questions

无.

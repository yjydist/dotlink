# Design

## Context

见 `proposal.md` 的 Why 与 `2026-11-05-harden-backup-ordering` 的 design D6. 主规格里这条需求的正文
已经完整按 UTC 书写 (定长 15 字符, 字典序即时间序, 生成时不读本地时区), 只有 `### Requirement:`
那一行的名称还留着 "本地". 代码侧 `src/backup.rs` 的 `timestamp()` 使用 `OffsetDateTime::now_utc()`,
README 与已知限制也都按 UTC 描述, 因此不一致只存在于抬头.

## Goals / Non-Goals

- Goal: 需求的名称与正文, README, 实现说的是同一件事.
- Goal: 只改名称, 归档结果里这条需求的正文与 Scenario 与改名前逐字相同.
- Non-Goal: 不改命名规则, 不改排序规则, 不改任何代码或测试.
- Non-Goal: 不改写归档目录里的历史制品.

## Decisions

### D1. 用 `## RENAMED Requirements`, 不用 `MODIFIED`

归档按标题匹配需求: `MODIFIED` 块里改名会让定位失败 (归档报 not found), 并可能用 delta 正文覆盖主规格.
`RENAMED` 的语义就是"只改名称", 与本次意图一一对应.

备选: 直接在 `MODIFIED` 块里写新标题并附全文. 否决: 既要满足"标题必须逐字匹配"就改不了名, 又要
重贴一遍正文, 平白引入正文被改动或被截断的风险, 而这正是 `harden-backup-ordering` 当初放弃改名的原因.

### D2. 只写 FROM/TO 两行, 不重贴正文

delta 里不出现 `### Requirement:` 块, 正文因此完全不参与归档合并. 这也让交付物的正确性可以一眼核对:
配对成立则改名成功, 除此之外没有别的可漂移内容.

### D3. 归档目录保持原样

`openspec/changes/archive/2026-11-05-harden-backup-ordering` 与 `2026-11-04-migrate-spec-kit-contracts`
里的旧标题是历史事实, 改它等于伪造当时的制品. 本变更只影响主规格.

### D4. 与 `validate-backup-timestamp-fields` 归档顺序无关

同期另一个变更修改的是 `backup-lifecycle` 的另一条需求 ("只有完整命名的备份参与查找"). 两条需求的
标题不同, 互不参与对方的匹配, 因此先归档哪一个结果都一样.

## Risks / Trade-offs

- **已有或将来在其他在飞变更里以旧标题写 `MODIFIED`** -> 那些块的定位会失败. 当前在飞变更只有本次
  四个, 其中没有一个引用旧标题; 归档前重跑 `openspec validate --all` 与
  `openspec change show <id> --json --deltas-only` 复核.
- **仓库内其他文档按旧标题引用** -> 已搜索: 除主规格自身与归档目录外没有引用; 归档后两处都会是历史
  或新名称, 无需再改 README.
- **改名让读者以为行为也变了** -> proposal 与 design 都写明"只改名称"; 归档后的主规格正文可作为证据.

## Migration Plan

归档时 `openspec archive align-backup-timestamp-spec` 把主规格里该需求标题替换为新名称. 无回滚需求:
标题不含任何运行语义.

## Open Questions

无.

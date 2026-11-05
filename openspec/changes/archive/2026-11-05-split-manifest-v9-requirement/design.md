# Design

## Context

`manifest-schema` 的 V-9 是一个单条 requirement, 正文同时覆盖受保护落点, 主目录的两种形态与文件系统身份判等. 该正文 1569 字符, 超过 `openspec validate --all --strict` 的 500 字符上限. 本变更只改规格文本, 不改任何实现.

## Goals / Non-Goals

**Goals:**

- 把 V-9 拆成三条 requirement, 每条正文不超过 500 字符.
- 原 V-9 的全部 MUST 与 8 个 Scenario 原样保留.

**Non-Goals:**

- 不修改代码, 测试, CLI, README, 依赖或运行时行为.
- 不改变任何退出码, 错误消息或文件系统语义.

## Decisions

### D1. 用 REMOVED + ADDED 表达拆分

`MODIFIED` 会拒绝丢弃现有 Scenario, 即使被丢弃的 Scenario 出现在 `ADDED` 块中也一样. 拆分因此使用 `REMOVED` 表达旧 V-9 的下线, 再用 `ADDED` 写入三条新 requirement. 这样归档时旧正文与旧 Scenario 集合被完整替换, 不会留下重复或缺失.

### D2. 核心 requirement 改名为 "受保护落点"

新核心 requirement 若继续使用原名称, 会与 `REMOVED` 块重名并被校验拒绝. 核心块改名为 `清单校验 V-9: 受保护落点`; V-9a 与 V-9b 继续使用原编号前缀, 保持与代码注释和 README 的 V-9 引用可对应.

### D3. Scenario 按关注点归属

- V-9: 裸 `~`, 仓库根, 清单文件自身.
- V-9a: 主目录自身是符号链接.
- V-9b: 大小写别名仓库根, 别名清单文件, firmlink 别名主目录, 不存在的落点只做字符串比较.

## Risks / Trade-offs

- **拆分后编号增加** -> V-9, V-9a 与 V-9b 共享 V-9 前缀; 代码注释继续以 V-9 指代整组, 不要求改代码.
- **REMOVED 与 ADDED 的名称差异** -> 迁移说明明确核心块改名, 行为不变; 审查时按 Scenario 集合与 MUST 片段对照.

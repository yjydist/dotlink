# Design

## Context

报告的组装集中在 `src/render.rs`: `row_from_state` 生成 token 与说明, `fail_row` 生成失败行, `render` 把 target 与说明按固定列宽拼成文本. `src/lib.rs` 的 `write_report` 一次写出整份报告, 清单错误与平台门禁各自写一行标准错误. 当前所有动态字段都直接拼进 `format!`, 控制字符不做处理.

## Goals / Non-Goals

**Goals:**

- 报告与诊断中的动态文本不含未转义的 C0/C1/DEL 控制字符, 特别是换行, 回车, ESC 与 BEL.
- 每条目仍恰占一行, 汇总行仍是最后一行.
- 转义只影响显示, 不改变路径解析, 链接操作或退出码.
- 覆盖 target, source, 实际指向, 备份路径, 错误原因五类动态字段.

**Non-Goals:**

- 不拒绝含控制字符的清单; 路径能力保持不变.
- 不改变 token, 列宽, 汇总分列或退出码.
- 不引入机器可读输出或终端能力探测.
- 不处理 bidi 控制字符与零宽字符; 它们不在 C0/C1/DEL 范围内, 作为残余风险记录.

## Decisions

### D1. 在报告边界统一转义

转义放在 `render` 的组装点, 而不是 `classify` 或 `apply`: 前者是所有报告行的唯一出口, 一次处理 target 与 note 即可覆盖 target, source, 实际指向, 备份路径与错误原因. `fail_row` 与 `note_for_token` 都经过 `row_for`, 因此只需在 `row_for` 或 `render` 内调用一个助手. 标准错误的清单诊断在 `src/lib.rs` 写出前复用同一助手.

备选方案是在每个 producer 处转义; 否决理由是 producer 多 (classify, apply, undo, manifest error), 容易漏掉新增字段, 且会重复转义.

### D2. 只转义控制字符

使用自定义 `escape_controls`:

- 对 `char::is_control()` 的字符, 用 `char::escape_default()` 生成可读形式 (换行 -> `\n`, 回车 -> `\r`, 制表 -> `\t`, ESC -> `\u{1b}`, BEL -> `\u{7}`).
- 其他字符原样保留, 避免把路径中的反斜杠或引号也转义, 改变正常目标的可读性.

这样 `target` 列在正常情况下仍与清单原文一致, 只有控制字符变为转义序列, 满足 `cli-contract` 的"可辨认原始写法".

### D3. 覆盖范围

- `ReportRow.target_display` 与 `note` 在 `render` 前转义.
- 清单错误 (`dotlink: {error}`) 与报告写失败诊断 (`dotlink: 写出报告失败: {error}`) 在写出前转义.
- 平台门禁的固定文本无需转义.
- `summary_line` 只含数字与固定中文, 无需转义; 但 `render` 可以统一走同一助手, 使未来新增字段默认安全.

### D4. 测试策略

- 单元测试直接断言 `escape_controls` 对换行, 回车, 制表, ESC, BEL 的输出.
- `render` 单元测试用含换行的 target 断言行数仍为 `N + 1`, 汇总行在最后, 且输出不含原始换行以外的控制字符.
- 端到端测试用 ESC/BEL 的 target 与 source 执行 `preview` / `status` / `undo`, 断言标准输出字节不含 `0x1b` 与 `0x07`, 且每条目一行.
- 退出码测试确认转义不改变 0/1/2/3 的既有结论.

## Risks / Trade-offs

- **转义后的 target 与清单原文不完全一致** -> 只对控制字符生效, 正常路径不受影响; 在 README 中说明.
- **用户依赖原始控制字符做脚本解析** -> 报告本来就是人类可读输出, 且原始控制字符会破坏行协议; 转义是必要的契约修正.
- **bidi 与零宽字符仍可造成视觉欺骗** -> 不在 C0/C1/DEL 范围, 记录为残余风险; 如未来需要, 可扩展助手覆盖 Unicode `Cf` 类别.
- **标准错误诊断也转义** -> 错误信息仍可读, 且避免终端注入; 不影响退出码.

## Migration Plan

无数据迁移. 现有输出在正常路径上逐字节不变; 只有含控制字符的清单或文件系统文本会出现转义序列. 回滚时移除助手调用即可.

## Open Questions

无.

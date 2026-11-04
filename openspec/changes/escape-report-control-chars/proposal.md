# Proposal

## Why

清单的 `source` / `target` 与文件系统返回的说明目前被原样写入报告. TOML 字符串可以包含换行, 回车, ESC, BEL 等控制字符; 一个合法清单就能让 `preview` 输出多行, 或向终端发送 OSC 序列 (例如 OSC 52 写剪贴板, OSC 0 改标题). 这违反 `cli-contract` 的"每条目一行", 也让只读的 `preview` 对终端产生副作用.

## What Changes

- 在报告渲染层对 target 展示, 说明文本, source, 实际指向, 备份路径与错误原因中的控制字符做转义 (例如换行显示为 `\n`, ESC 显示为 `\u{1b}`), 保证每条目恰一行, 且不包含未转义的 ESC/BEL.
- 转义只发生在显示层, 不改变清单解析, 路径解析, 链接建立或退出码; 路径比较与文件系统操作仍使用未转义的原始值.
- 新增含换行, 回车, ESC/BEL 的 target/source 端到端与单测, 断言行数, 汇总行与输出中无原始控制字符.
- README 的报告说明或"已知限制"补一句: 控制字符在输出中转义显示.

## Capabilities

### New Capabilities

无.

### Modified Capabilities

- `cli-contract`: 报告行中的控制字符必须转义; 每条目仍恰一行; 输出不得包含未转义的 ESC/BEL.

## Impact

- 代码: `src/render.rs` 的动态字段转义; `src/lib.rs` 的诊断行与 `src/error.rs` 的错误文本可复用同一助手.
- 测试: `src/render.rs` 单测, `tests/output.rs`, `tests/preview.rs`, `tests/status.rs`, `tests/undo.rs`.
- 文档: `README.md`.
- 依赖: 无新增依赖; 使用标准库字符转义.

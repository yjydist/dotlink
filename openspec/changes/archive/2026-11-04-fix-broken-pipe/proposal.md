# Proposal

## Why

`preview` / `apply` / `status` / `undo` 用 `print!` 一次性写出报告. 标准输出被提前关闭时 (例如 `dotlink status | head -1`), 进程 panic 并以 101 退出, 标准错误出现 `failed printing to stdout: Broken pipe`. 同一条路径上, 清单读取失败时用 `eprintln!` 写标准错误; 标准错误关闭时同样 panic 并退 101. 两者都不在公开的 0/1/2/3 退出码表内, 会让正常的分页与截断在脚本里表现为崩溃.

## What Changes

- 报告写入改为 `io::Write` 接缝: 写出失败不再 panic.
- `BrokenPipe` 视为消费方主动结束: 不写额外的标准错误, 退出码仍按命令的目标谓词与逐条结果计算.
- 其他标准输出写错误: 尽力向标准错误写一行说明, 退出码不变.
- 清单读取失败时写标准错误改为忽略写失败, 与已有的平台门禁一致.
- `cli-contract` 新增 requirement: 报告写出失败不改变命令结局; 覆盖 `BrokenPipe`, 其他标准输出错误, 标准错误不可写三个 Scenario.
- README 与 `src/lib.rs` 模块文档补充输出失败的行为说明.
- 新增单元测试与端到端测试: 关闭标准输出/标准错误时无 panic, 退出码正确.

**BREAKING**: 无. 退出码集合与既有语义不变.

## Capabilities

### New Capabilities

无.

### Modified Capabilities

- `cli-contract`: 新增 "报告写出失败不改变命令结局" 的 requirement, 明确写失败 (含 `BrokenPipe`, 其他标准输出错误, 标准错误不可写) 不参与现有的 0/1 两条件判据, 也不产生 101.

## Impact

- 代码: `src/lib.rs` 的唯一报告写入点与清单错误写入点; 不涉及 `render.rs`, `classify.rs`, `backup.rs`, `undo.rs`.
- 测试: `src/lib.rs` 的单元测试; 新的端到端用例 (关闭标准输出/标准错误).
- 文档: `README.md` 的 "命令与退出码"; `src/lib.rs` 模块文档.
- 契约: `openspec/specs/cli-contract/spec.md` 由本变更的 delta 更新.
- 依赖与平台: 无新增依赖; 行为在 macOS 与 Linux 一致.

# Design

## Context

动机见 `proposal.md` 的 Why. 当前生产代码只有两处输出写入:

- `src/lib.rs` 的 `run_on` 用 `print!` 写报告, 用 `eprintln!` 写清单读取失败的说明;
- `src/lib.rs` 的 `platform_gate` 已经用 `let _ = writeln!(stderr, ...)`, 写失败被忽略;
- `clap` 的帮助与用法错误由 `clap` 自己写, 关闭标准输出或标准错误时仍按 0/2 退出.

`render::render` 返回完整报告字符串, 末尾恒有换行. 退出码由 `exit_code_for` 从报告行导出, 与写出是否成功无关. 现有测试没有覆盖输出写入失败.

## Goals / Non-Goals

**Goals:**

- 四条子命令的报告写入与清单错误说明写入都不因 I/O 失败 panic.
- 写出失败不改变 `cli-contract` 定义的 0/1/2/3 退出码.
- 用可注入的 writer 单元测试与真实管道端到端测试锁定行为.

**Non-Goals:**

- 不改报告格式, token, 措辞, 汇总行或 `render.rs`.
- 不引入流式输出, 不新增退出码, 不改 `clap` 路径.
- 不改任何文件系统操作, 备份, 判定或撤销逻辑.
- 不处理文件系统层面的持久性错误 (例如 `fsync`), 只处理标准输出与标准错误的写入.

## Decisions

### D1. 在 `src/lib.rs` 增加 `write_report` 接缝

新增 `fn write_report(rows: &[ReportRow], stdout: &mut impl Write, stderr: &mut impl Write)`. 它调用 `stdout.write_all(render::render(rows).as_bytes())` 并随后 `flush`, 把写与刷新合并成一个 `io::Result<()>`; 错误分类只在这一处. `run_on` 先算 `code`, 再调用 `write_report`, 最后返回 `code`, 使 "写失败不改退出码" 是结构性的, 不依赖调用顺序的约定.

备选之一是把错误处理直接内联在 `run_on`: 代码更少, 但错误分支无法用自定义 writer 做确定性单测. 备选之二是让 `render` 直接写入 `Write`: 会重写一个已被大量断言锁定的格式化路径, 并让失败时可能只写出半份报告; 不值得.

### D2. 写失败不改变退出码

`BrokenPipe` 视为消费方主动结束: 不写标准错误, 直接返回命令正常算出的 0/1. 其他标准输出错误: 尽力向标准错误写一行 `dotlink: 写出报告失败: ...`, 然后仍返回命令算出的 0/1. 标准错误不可写时忽略该行.

选择保持退出码, 是因为 `cli-contract` 的 0/1 判据只由 "目标谓词 + 逐条结果" 决定. 若改成 1, 会与 "0 当且仅当" 冲突; 若新增 4, 会扩大公开的 0/1/2/3 面. 写出失败时用户仍能从标准错误看到诊断, 但脚本看到的退出码仍描述命令的文件系统结局. 这是一次有意的权衡, 不是遗漏.

### D3. 清单错误路径改为忽略标准错误写失败

`src/lib.rs` 的 `eprintln!("dotlink: {error}")` 改为 `let _ = writeln!(std::io::stderr().lock(), "dotlink: {error}")`, 与 `platform_gate` 同构. `eprintln!` 在标准错误关闭时会 panic 并退 101; 这条路径发生在派发之前, 应保持原有的退出码 2. 忽略写失败不会改变清单错误的判断, 只放弃不可送达的诊断.

### D4. 使用 `write_all` 与 `flush`, 不做流式输出

报告在写出前已经完整生成, `write_all` 一次写出并 `flush`. 不逐行写: 逐行写会让失败时的输出截断点依赖缓冲与调度, 测试更难稳定; 当前实现也已经是 "先收集全部报告行再输出". `flush` 是显式的, 因为未来若报告末尾的换行被改动, 仅靠 `LineWriter` 的换行刷新会让错误留在缓冲区里.

### D5. 测试分两层

单元测试用自定义 writer 覆盖分类:

- 写入成功: 输出等于 `render::render(rows)`.
- 写或刷新返回 `BrokenPipe`: 不写标准错误.
- 写或刷新返回其他错误: 标准错误出现一行诊断.
- 标准错误 writer 失败: 不 panic.

端到端测试用真实进程与管道:

- 一份 5000 条目的清单, 共享一个已存在的源文件, target 全部不存在; 启动 `status` 后立即 drop 标准输出读端. 输出超过管道缓冲, 必然触发 `BrokenPipe`; 断言无 panic, 标准错误为空, 退出码 1.
- 没有清单的临时目录, 启动 `status` 后 drop 标准错误读端; 断言无 panic, 退出码 2.

端到端用例放在 `tests/output.rs` 或现有 `tests/exit_codes.rs`; 前者按行为分文件, 后者复用既有退出码夹具. 选择其一即可, 不新增测试依赖.

## Risks / Trade-offs

- **端到端测试的竞态** -> 子进程可能在读端关闭前写完小报告. 用超过管道缓冲的大报告并立即关闭读端; 即使偶发跳过, 单元测试仍锁定分类逻辑.
- **非 `BrokenPipe` 写失败时退出码仍为 0** -> 标准错误会给出诊断, 但脚本可能只看退出码. 这是保持 "0 当且仅当" 契约的代价; 若将来需要 "写失败一定非零", 应作为独立契约变更, 而不是在本修复里偷偷改.
- **`flush` 增加一次系统调用** -> 每条命令一次, 可忽略; 换来错误不被留在缓冲区.
- **忽略标准错误写失败会丢失诊断** -> 标准错误不可写时本来也无法送达; 退出码仍正确.

## Migration Plan

1. 在 `src/lib.rs` 实现 `write_report`, 替换 `print!`; 替换清单错误路径的 `eprintln!`.
2. 增加单元测试与端到端测试.
3. 更新 `README.md` 的 "命令与退出码" 与 `src/lib.rs` 模块文档.
4. 运行 `./scripts/check.sh` 与 `openspec validate --strict`.
5. 回滚策略: 还原 `src/lib.rs`, 新增测试, README 与模块文档; 不涉及数据格式或文件系统状态, 回滚无迁移成本.

## Open Questions

无. 非 `BrokenPipe` 写错误的退出码已按 D2 决定; 若后续需要改为非零, 另起契约变更.

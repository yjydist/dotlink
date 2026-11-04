# Tasks

## 1. 报告写入接缝

- [x] 1.1 在 `src/lib.rs` 新增 `write_report(rows, stdout: &mut impl Write, stderr: &mut impl Write)`: 用 `write_all` 写 `render::render(rows)` 的字节并 `flush`, 把 `BrokenPipe` 与其他写错误分开处理; 把 `run_on` 的 `print!` 换成该函数, 并在写入前算出 `code`, 写入后返回 `code`. 验证: `cargo test --locked --lib` 现有测试全部通过.
- [x] 1.2 为 `write_report` 增加单元测试: `Vec` 写入完整报告; `BrokenPipe` writer 不写 stderr; 其他错误 writer 写一行诊断; stderr writer 失败不 panic. 验证: `cargo test --locked --lib write_report` 全部通过, 断言报告内容与 stderr 内容.
- [x] 1.3 更新 `src/lib.rs` 模块文档与 `run_on` 的文档, 写明唯一的 stdout 写入点, `BrokenPipe` 静默, 其他写错误尽力诊断, 退出码不变. 验证: `cargo doc --locked --no-deps --document-private-items` 通过, 且文档审阅覆盖上述三点.

## 2. 标准错误与端到端覆盖

- [x] 2.1 把 `src/lib.rs` 清单读取失败路径的 `eprintln!` 改为 `let _ = writeln!(std::io::stderr().lock(), "dotlink: {error}")`, 与 `platform_gate` 一致. 验证: `cargo test --locked --lib` 通过; `2.3` 的端到端用例覆盖 stderr 关闭.
- [x] 2.2 新增 `tests/output.rs` 的关闭 stdout 用例: 5000 条目的清单共享一个已存在的源文件, target 全部不存在; spawn `status` 后立即 drop stdout 读端; 断言无 panic, stderr 为空, 退出码 1. 验证: `cargo test --locked --test output` 通过.
- [x] 2.3 在 `tests/output.rs` 增加关闭 stderr 的用例: 没有清单的临时目录, spawn `status` 后 drop stderr 读端; 断言无 panic, 退出码 2. 验证: `cargo test --locked --test output` 通过.
- [x] 2.4 更新 `README.md` 的 "命令与退出码": 说明管道提前关闭不 panic, 退出码仍按命令结果; 其他标准输出写错误给一行标准错误; 标准错误不可写时静默. 验证: 审阅 README 段落, 与 `2.2` / `2.3` 的断言逐条对应.

## 3. 集成检查

- [x] 3.1 运行 `./scripts/check.sh`, 确认 fmt, 测试, clippy, rustdoc 与 release 构建全部通过.
- [x] 3.2 运行 `openspec validate "fix-broken-pipe" --strict`, 确认 delta 规格通过严格校验.
- [x] 3.3 用大清单手工冒烟一次: `dotlink status | head -1`; 验证没有 panic 文本, `PIPESTATUS[0]` 为 1, 而不是 101.

## Workflow follow-up

- 评审通过后归档本变更.
- 归档后复核 `openspec/specs/cli-contract/spec.md` 中的新 requirement 与 Scenario 已落地.

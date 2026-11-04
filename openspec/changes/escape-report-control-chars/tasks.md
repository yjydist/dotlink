# Tasks

## 1. 转义助手

- [ ] 1.1 在 `src/render.rs` 增加只处理控制字符的转义助手, 覆盖换行, 回车, 制表, ESC, BEL 与 DEL; 增加单元测试并运行 `cargo test --locked render` 确认通过.
- [ ] 1.2 在报告组装点对 target 与说明调用该助手; 增加含换行 target 的渲染测试, 断言行数为 `N + 1` 且汇总行在最后; 运行 `cargo test --locked render` 确认通过.

## 2. 诊断与端到端

- [ ] 2.1 在 `src/lib.rs` 的清单错误与报告写失败诊断中复用同一转义助手; 增加单元测试并运行 `cargo test --locked --lib` 确认通过.
- [ ] 2.2 在 `tests/output.rs` 或 `tests/preview.rs` 增加含 ESC/BEL 与换行的 target/source 端到端用例, 断言标准输出字节不含 `0x1b` 与 `0x07`, 且每条目一行; 运行对应测试确认通过.
- [ ] 2.3 检查 `tests/status.rs` 与 `tests/undo.rs` 的说明断言, 更新受转义影响的预期; 运行 `cargo test --locked` 确认全套通过.

## 3. 文档与质量门

- [ ] 3.1 在 `README.md` 的报告说明或"已知限制"补充控制字符转义的行为; 人工核对示例与实现一致.
- [ ] 3.2 运行 `./scripts/check.sh`, 确认 fmt, test, clippy, rustdoc 与 release build 全部通过.

## Workflow follow-up

- 归档前复核 `cli-contract` delta 与主规格的合并结果.
- 归档后确认正常路径的输出与变更前逐字节一致.

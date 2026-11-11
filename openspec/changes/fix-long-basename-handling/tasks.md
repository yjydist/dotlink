# Tasks

## 1. 共享预算常量

- [ ] 1.1 在 `src/entry.rs` 增加 `MAX_TARGET_BASENAME_BYTES: usize = 200` 与按字节计算的辅助函数, 供 `manifest` 校验与 `backup`/`os` 的命名测试共同引用; 验证: 新增单测覆盖 ASCII, 多字节, 无末段 (根路径) 三种输入.
- [ ] 1.2 审计最长后缀: 7 位 pid, 消歧序号, `.partial` 与 `.dotlink-restore.partial`; 验证: 单测断言 200 + 最长后缀不超过 255.

## 2. V-12 清单校验

- [ ] 2.1 在 `src/manifest.rs` 的条目循环中, 在目标解析与 V-9/V-10/V-11 之后增加 V-12 校验: 末段 basename 字节数超过 200 时整体拒绝, 退出码 2, 说明给出条目下标, 书写形态, 实际字节数与上限; 验证: `cargo test --locked manifest_errors` 通过.
- [ ] 2.2 增加边界用例: 200 字节接受, 201 字节拒绝, 67 个 3 字节 UTF-8 字符拒绝, 只创建链接的条目同样拒绝; 验证: 四个用例的退出码与说明断言通过.
- [ ] 2.3 确认 V-9/V-10/V-11 的错误优先级不被 V-12 打乱: 更长且同时命中受保护落点的目标仍报原规则; 验证: 组合用例断言说明命中的是原规则.

## 3. 运行期一致性与文档

- [ ] 3.1 增加 200 字节 basename 的端到端用例: `apply` 的 replace 成功, 备份名与暂存名都在 255 字节内, 随后 `undo` 能恢复; 验证: `cargo test --locked apply undo` 通过.
- [ ] 3.2 在 `README.md` 的清单校验与"已知限制"补充 200 字节预算, UTF-8 字节计数与多字节文件名更早触发的说明; 验证: `rg -n "200|字节" README.md` 能定位.
- [ ] 3.3 运行 `openspec validate fix-long-basename-handling --strict`; 验证: 无 error 与 warning.

## 4. 全量验证

- [ ] 4.1 运行 `cargo test --locked` 与 `cargo clippy --locked --all-targets -- -D warnings`; 验证: 全部通过.
- [ ] 4.2 运行 `./scripts/check.sh`; 验证: 标点检查, fmt, 测试, clippy, rustdoc 与 release 构建全部通过.

## Workflow follow-up

- 归档前确认 `manifest-schema` V-12 与 `backup-lifecycle` 的长度预算使用同一个 200 字节常量.

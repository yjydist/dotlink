# Tasks

## 1. 共享常量与最长后缀

- [x] 1.1 在 `fix-long-basename-handling` 引入的共享模块中新增平台路径上限常量 (Linux 4096, macOS 1024), 并在已审计的 `MAX_GENERATED_SUFFIX_BYTES` (50) 之上加 5 字节安全余量, 合计预留 55 字节; 提供"目标接受上限 = 平台上限 - 55"的辅助函数; 验证: 单测断言两平台常量, 预留 55, 接受上限的数值, 且辅助函数在边界输入上返回预期布尔值.
- [x] 1.2 审计最长生成后缀: 备份最终名, 备份暂存名, 恢复暂存名, 替换/撤销暂存名, 覆盖 7 位 pid 与 2 位序号, `.partial` 与 `.dotlink-restore.partial`; 复用 `fix-long-basename-handling` 已锁定的 `MAX_GENERATED_SUFFIX_BYTES = 50`; 验证: 单测断言实际最长后缀 50 不超过预留 55, 且 200 字节 basename 加上 50 不超过 255 字节.

## 2. 清单校验 V-13

- [x] 2.1 在 `src/manifest.rs` 增加 V-13, 位置在 V-9/V-10/V-11/V-12 之后 (V-11 与 V-12 是循环后的全量检查): 规范化绝对路径字节数加 55 超过平台上限时整体拒绝, 退出码 2, 零改动, 说明给出条目下标, `target` 书写形态, 解析后字节数与预算; 验证: `cargo test --locked manifest_errors` 通过.
- [x] 2.2 确认 V-13 不打乱 V-9/V-10/V-11/V-12 的优先级: 同时命中受保护落点, 路径重叠或 basename 预算的条目仍报原规则; 验证: 组合用例断言说明命中的是原规则而不是 V-13.

## 3. 测试

- [x] 3.1 增加单测边界: 预算减 1 字节接受, 恰好预算接受, 预算加 1 字节拒绝, 多字节 (3 字节 UTF-8 字符) 路径按字节数拒绝; 验证: 四个用例分别断言退出码, 说明中的字节数与预算.
- [x] 3.2 增加接近预算的端到端用例: 例如 Linux 上 4041 字节的 `target` 经 `apply` 建链, 再由 `undo` 成功移除; 验证: `apply` 退 0, `undo` 按既有契约退 1 (`undo` token, 链接已移除), 备份名与暂存名都不触发 `ENAMETOOLONG`.
- [x] 3.3 增加超预算端到端用例: `apply`, `undo`, `preview`, `status` 四条命令都退 2; 验证: 链接, 父目录与备份目录均保持原样, 标准错误给出解析后字节数与预算.
- [x] 3.4 增加相对 `target` 的接受面用例: 同一清单在较浅仓库根下被接受, 在更深的仓库根下因解析后超预算被拒; 验证: 两次运行的退出码与说明符合规格.

## 4. 文档

- [x] 4.1 在 `README.md` 的"已知限制"与清单校验说明中补充完整路径预算, Linux 与 macOS 的差异, 相对 `target` 的接受面依赖仓库位置, 以及 V-12 与 V-13 需同时满足; 验证: `rg -n "4096|1024|完整路径" README.md` 能定位这些说明.
- [x] 4.2 运行 `openspec validate bound-target-path-length --strict`; 验证: 没有 error 与 warning, 且规格中的预算数字与实现常量一致.

## 5. 全量验证

- [x] 5.1 运行 `cargo test --locked`; 验证: 全部测试通过, 包含新增的边界与端到端用例.
- [x] 5.2 运行 `cargo clippy --locked --all-targets -- -D warnings`; 验证: 没有 warning.
- [x] 5.3 运行 `openspec validate bound-target-path-length --strict`; 验证: 没有 error 与 warning.
- [x] 5.4 运行 `./scripts/check.sh`; 验证: 标点检查, fmt, 测试, clippy, rustdoc 与 release 构建全部通过.

## Workflow follow-up

- 归档前确认 V-13 的平台常量, 最长后缀预留与 V-12 的 200 字节预算位于同一共享模块, 且 README 同时说明两个预算.
- 归档前确认 `fix-long-basename-handling` 已落地共享预算模块; 若尚未实现, 先完成其常量与测试, 再实现本变更.

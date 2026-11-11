# Tasks

## 1. 源码注释

- [ ] 1.1 修正 `src/render.rs` 的 `TOKEN_WIDTH` 注释: token 字段 9 列加 1 列分隔, `create` 后 4 个空格; 验证: 对照 `render` 的格式串与既有对齐测试.
- [ ] 1.2 修正 `src/backup.rs` 的 `RESTORE_SUFFIX` 注释与模块文档, 说明文件/目录/符号链接都使用该后缀, 并写全暂存名形态; 验证: `rg -n "RESTORE_SUFFIX|dotlink-restore.partial" src/backup.rs`.
- [ ] 1.3 更新 `src/manifest.rs` 的 `parse` 与 `invalid_entry` 文档: 元数据读取, V-9a/V-9b, V-11; 验证: `cargo doc --locked --no-deps --document-private-items` 通过.
- [ ] 1.4 更新 `src/entry.rs` 的 `expand_source` 文档指向 `resolve_with`; 验证: `rg -n "resolve_with|resolve" src/entry.rs`.
- [ ] 1.5 更新 `src/classify.rs` 测试注释的 `8 x 2` 为当前状态数; 验证: `rg -n "8 x 2|9 x 2" src/classify.rs`.

## 2. README 与脚本文档

- [ ] 2.1 `README.md` 的标点检查说明把 `mktemp` 改为 `tempfile.TemporaryDirectory`, 并说明 `.github/` 范围是刻意保留; 验证: `rg -n "tempfile|.github" README.md`.
- [ ] 2.2 `README.md` 的已知限制补充硬链接与稀疏结构不保留, 并指向 `backup-fidelity` 的边界; 验证: `rg -n "硬链接|稀疏" README.md`.
- [ ] 2.3 `scripts/check-punctuation.py` 的模块文档注明 `.github` 不存在时零成本; 验证: `python3 scripts/check-punctuation.py` 仍通过.

## 3. 全量验证

- [ ] 3.1 运行 `openspec validate align-docs-and-comments --strict`; 验证: 无 error 与 warning.
- [ ] 3.2 运行 `./scripts/check.sh`; 验证: 标点检查, fmt, 测试, clippy, rustdoc 与 release 构建全部通过.

## Workflow follow-up

- 归档前确认 `skip_specs: true` 仍成立: 本变更没有用户可见行为变化.

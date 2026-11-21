# Tasks

## 1. 源码注释

- [x] 1.1 修正 `src/render.rs` 的 `TOKEN_WIDTH` 注释: token 字段 9 列加 1 列分隔, `create` 后 4 个空格; 验证: 对照 `render` 的格式串与既有对齐测试.
- [x] 1.2 修正 `src/backup.rs` 的 `RESTORE_SUFFIX` 注释与模块文档, 说明文件/目录/符号链接都使用该后缀, 并写全暂存名形态; 验证: `rg -n "RESTORE_SUFFIX|dotlink-restore.partial" src/backup.rs`.
- [x] 1.3 更新 `src/manifest.rs` 的 `parse` 与 `invalid_entry` 文档: 元数据读取, V-9a/V-9b, V-11; 验证: `cargo doc --locked --no-deps --document-private-items` 通过.
- [x] 1.4 更新 `src/entry.rs` 的 `expand_source` 文档指向 `resolve_with`; 验证: `rg -n "resolve_with|resolve" src/entry.rs`.
- [x] 1.5 更新 `src/classify.rs` 测试注释的 `8 x 2` 为当前状态数; 验证: `rg -n "8 x 2|9 x 2" src/classify.rs`.

## 2. README 与脚本文档

- [x] 2.1 `README.md` 的标点检查说明把 `mktemp` 改为 `tempfile.TemporaryDirectory`, 并说明 `.github/` 范围是刻意保留; 验证: `rg -n "tempfile|.github" README.md`.
- [x] 2.2 `README.md` 的已知限制补充硬链接与稀疏结构不保留, 并指向 `backup-fidelity` 的边界; 验证: `rg -n "硬链接|稀疏" README.md`.
- [x] 2.3 `scripts/check-punctuation.py` 的模块文档注明 `.github` 不存在时零成本; 验证: `python3 scripts/check-punctuation.py` 仍通过.
- [x] 2.4 `README.md` 的已知限制说明: `apply` 写入的链接文本是绝对 source 路径; 仓库改名或移动后旧链接落在新仓库根之外, 会被判为 `foreign`; `undo` 跳过它们, 默认 `apply` 报 `conflict`, 工具无法自愈. 写明移动前先 `undo` 或移动后手工清理, 并明确当前不使用相对链接文本; 验证: `rg -n "绝对 source|移动|相对链接" README.md` 能定位该段, 人工审阅与 `src/apply.rs` 的 `create_link` 及 `src/entry.rs` 的绝对 `source_path` 一致.
- [x] 2.5 `README.md` 的 ACL 措辞与 `backup-fidelity` 规格一致: 不单独承诺 ACL, 若平台把 ACL 暴露为扩展属性则随该属性一并复制; 可举例 Linux 的 `system.posix_acl_access`, 但不作跨平台承诺, 所有权仍明确不保留; 验证: `rg -n "ACL|system.posix_acl_access" README.md` 能定位, 人工对照 `openspec/specs/backup-fidelity/spec.md` 的"备份保留扩展属性"要求.

## 3. 全量验证

- [x] 3.1 运行 `openspec validate align-docs-and-comments --strict`; 验证: 无 error 与 warning.
- [x] 3.2 运行 `./scripts/check.sh`; 验证: 标点检查, fmt, 测试, clippy, rustdoc 与 release 构建全部通过.

## Workflow follow-up

- 归档前确认 `skip_specs: true` 仍成立: 本变更没有用户可见行为变化.

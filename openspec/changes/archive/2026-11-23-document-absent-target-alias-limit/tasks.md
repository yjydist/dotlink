# Tasks

## 1. 记录已知限制

- [x] 1.1 修改 `README.md` 的"两侧都不存在的别名路径"条目, 说明该边界同时覆盖两个 target 之间, 并写出 `preview` 报两个 `create`, `apply` 第二条 `fail` 的后果; 验证: `grep -n "两个 target" README.md` 命中, 且同条出现 `EEXIST` 或 `File exists`.
- [x] 1.2 在 `tests/manifest_errors.rs` 增加 `case_insensitive_volume` 门控测试: 两个仅大小写不同的不存在 target 被清单接受, `preview` 两行均为 `create`, `apply` 第一行 `create`, 第二行 `fail` 且原因包含 `File exists`; 验证: `cargo test --locked --test manifest_errors absent_case_aliases` 在大小写不敏感卷上通过, 在敏感卷上打印跳过原因.

## 2. 全量验证

- [x] 2.1 运行 `cargo test --locked --test manifest_errors`; 验证: 通过.
- [x] 2.2 运行 `python3 scripts/check-punctuation.py`; 验证: 退出 0.
- [x] 2.3 运行 `./scripts/check.sh`; 验证: 脚本退出 0.
- [x] 2.4 运行 `openspec validate document-absent-target-alias-limit --strict`; 验证: 0 error, 0 warning.

## Workflow follow-up

- Archive the change after review.

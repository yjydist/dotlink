# Proposal

## Why

代码行为已由测试锁定, 但若干净路径上的注释, 文档与测试注释与实现不符, 后续维护者会按错误的前提修改. 当前已知的具体漂移包括: `TOKEN_WIDTH` 注释的列宽算错, `RESTORE_SUFFIX` 注释说只用于目录, `parse` 文档说只读 `text`/`repo_root`/`HOME` 却还做元数据读取, `invalid_entry` 文档停在 V-10, `expand_source` 指向测试专用 `resolve`, README 说标点回归用 `mktemp` 而实际用 `tempfile`, 以及 README 未说明硬链接/稀疏结构不在备份保真范围. 本变更只改文档与注释, 不改变行为.

## What Changes

- `src/render.rs`: 修正 `TOKEN_WIDTH` 注释; 说明 9 列 token 字段加 1 列分隔, `create` 后是 4 个空格.
- `src/backup.rs`: `RESTORE_SUFFIX` 注释改为文件/目录/符号链接通用; 模块文档写全暂存名形态.
- `src/manifest.rs`: `parse` 文档补充元数据读取与 V-9a/V-9b; `invalid_entry` 文档补 V-11.
- `src/entry.rs`: `expand_source` 指向 `resolve_with`, 不再指向 cfg(test) 的 `resolve`.
- `src/classify.rs`: 测试注释从 `8 x 2` 改为当前 9 个状态与动作矩阵.
- `README.md`: `mktemp` 改为 `tempfile`; 说明 `.github/` 扫描范围是刻意保留的未来模板防护; 在已知限制补充硬链接与稀疏结构不保留.
- `scripts/check-punctuation.py`: 模块文档注明 `.github` 不存在时零成本, 保留范围是为了未来 issue/PR 模板.

## Capabilities

### New Capabilities

- 无.

### Modified Capabilities

- 无. 本变更只改文档与注释, `.openspec.yaml` 设置 `skip_specs: true`.

## Impact

- `README.md`, `src/render.rs`, `src/backup.rs`, `src/manifest.rs`, `src/entry.rs`, `src/classify.rs`, `scripts/check-punctuation.py`.
- 不改变生产逻辑, token, 退出码或测试断言.

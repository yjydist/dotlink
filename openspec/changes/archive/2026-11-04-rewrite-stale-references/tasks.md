# Tasks

## 1. README

- [x] 1.1 把 `README.md` 第 270 行的 `specs/001-dotfile-link-management/` 引用改为 `openspec/specs/` 的能力清单; 运行 `rg 'specs/001|quickstart\.md' README.md` 确认零命中.
- [x] 1.2 把第 279 行的 `quickstart.md` 坏链接改为 `openspec/specs/cli-contract/spec.md` 的 Scenario 与 README 示例; 人工核对链接目标存在.

## 2. 源码注释

- [x] 2.1 改写 `src/**/*.rs` 中的 `research.md`, `contracts/cli.md`, `data-model.md`, `contracts/manifest.md`, `quickstart.md` 引用为对应 `openspec/specs/<capability>/spec.md` 路径加编号; 运行 `rg 'research\.md|contracts/cli\.md|data-model\.md|contracts/manifest\.md|quickstart\.md' src` 确认零命中.
- [x] 2.2 删除或改写 `src/**/*.rs` 中的 `T-nnn`, `SC-n` 与非 R2 的 `R-n` 引用; 运行 `rg '\bT[0-9]{3}\b|SC-[0-9]+' src` 与 `rg '\bR(?!2\b)[0-9]+\b' src --pcre2` 确认零命中.
- [x] 2.3 抽查 `src/backup.rs` 的时间戳注释, 确认 `T000000` 形态未被 `T-nnn` 规则误删; 人工核对备份命名说明仍完整.

## 3. 测试注释

- [x] 3.1 改写 `tests/**/*.rs` 中的旧文件名与旧编号引用, 规则与源码注释一致; 运行同样的 `rg` 命令并限定 `tests` 目录, 确认零命中.
- [x] 3.2 抽查 `tests/undo.rs` 与 `tests/conflict.rs` 的归属说明, 确认引用改写后语义未变化; 人工核对.

## 4. 验证

- [x] 4.1 运行 `cargo fmt --check`, `cargo test --locked` 与 `cargo clippy --all-targets -- -D warnings`, 确认全部通过.
- [x] 4.2 运行 `openspec validate --all --strict`, 确认主规格与四个变更仍然有效.

## Workflow follow-up

- 归档前确认 `rg` 的四条验收命令在 `src`, `tests` 与 `README.md` 上零命中.
- 归档后确认 README 的设计文档入口指向 `openspec/specs/`.

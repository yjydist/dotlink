# Proposal

## Why

spec-kit 的 `specs/001-dotfile-link-management/` 已删除并迁移到 OpenSpec, 但 `src/`, `tests/` 与 `README.md` 仍按旧文件名引用 `research.md` (24 处), `contracts/cli.md` (19 处), `data-model.md` (13 处), `contracts/manifest.md` (4 处), `quickstart.md` (1 处), 并有 84 处 `T-nnn`, 19 处 `SC-n` 与 15 处非 R2 的 `R-n`. 迁移设计明确把引用改写留给后续变更, 但该变更尚未创建. 这些引用现在无法跳转, README 还有两个坏链接, 评审和后续修改都会失去契约落点.

## What Changes

- 把文件名引用改为 `openspec/specs/<capability>/spec.md` 加保留的 `V-n` / `FR-n` 编号; 无法对应到 requirement 的 `R-n` 删除或改写成 requirement 名称.
- 删除 `T-nnn` (已完成任务编号) 与 `SC-n` (未迁移的验收指标) 引用, 保留必要的行为描述.
- README 第 270 行改为指向 `openspec/specs/` 并按能力列出; 第 279 行的 `quickstart.md` 坏链接改为 `cli-contract` 的 Scenario 与 README 示例.
- 纯注释与文档变更, 不改变任何可执行行为; `.openspec.yaml` 设置 `skip_specs: true`.
- 验收: `rg 'research\.md|contracts/cli\.md|data-model\.md|contracts/manifest\.md|quickstart\.md|T0[0-9]{2}|SC-0' src tests README.md` 零命中; `cargo fmt --check`, `cargo test --locked` 与 `cargo clippy --all-targets -- -D warnings` 保持通过.

## Capabilities

### New Capabilities

无.

### Modified Capabilities

无. 本变更只改文档与注释, 不改变 spec 级行为; `.openspec.yaml` 使用 `skip_specs: true`.

## Impact

- `README.md`, `src/**/*.rs`, `tests/**/*.rs` 的注释; 无运行时代码, 无依赖, 无平台差异.

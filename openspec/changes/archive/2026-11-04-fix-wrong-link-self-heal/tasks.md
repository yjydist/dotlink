# Tasks

## 1. 动作映射

- [x] 1.1 修改 `src/classify.rs` 的 `action_for`, 使 `LinkedWrong` 无论 `on_conflict` 取值都导出 `ReplaceAndBackup`; 更新该模块的动作映射单测并运行 `cargo test --locked classify` 确认通过.
- [x] 1.2 更新 `src/render.rs` 的 `replace` 说明, 让 `LinkedWrong` 使用"本工具的链接指向错误"措辞, `Foreign*` 保持"冲突占用"; 增加渲染单测并运行 `cargo test --locked render` 确认通过.

## 2. 部署行为

- [x] 2.1 在 `tests/apply.rs` 增加 source 改名 + 默认 `skip` 的端到端用例, 断言 `apply` 成功, 旧链接被备份, 目标指向新 source; 运行 `cargo test --locked --test apply` 确认通过.
- [x] 2.2 在 `tests/preview.rs` 增加同一状态的预览用例, 断言 token 为 `replace` 且给出预测备份路径; 运行 `cargo test --locked --test preview` 确认通过.
- [x] 2.3 更新 `tests/conflict.rs`, `tests/consistency.rs` 与 `tests/exit_codes.rs` 中依赖旧行为的断言, 运行 `cargo test --locked` 确认全套通过.

## 3. 文档与质量门

- [x] 3.1 更新 `README.md` 的 `on_conflict` 字段表与说明, 明确它只约束外部内容, 并补充 source 改名后自愈; 人工核对字段表与 `apply` 行为一致.
- [x] 3.2 运行 `./scripts/check.sh`, 确认 fmt, test, clippy, rustdoc 与 release build 全部通过.

## Workflow follow-up

- 归档前按项目评审要求复核 proposal, design, specs 与 tasks 的一致性.
- 归档后确认主规格 `entry-classification` 与 `link-deployment` 已包含本变更的 delta.

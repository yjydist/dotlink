# Tasks

## 1. 扩展转义表

- [x] 1.1 在 `src/render.rs` 增加 `DEFAULT_IGNORABLE_RANGES` (Unicode 17.0 Default_Ignorable_Code_Point) 与 `is_default_ignorable`, 用二分查找; 验证: 新增单元测试覆盖 U+00AD, U+061C, U+FFF0, U+E0041 与一个普通可见字符, `cargo test --locked render::` 通过.
- [x] 1.2 把 `push_escaped` 的判断改为 `character.is_control() || is_default_ignorable(character)`, 保留 `is_layout_control` 如仍被其他调用点使用; 验证: `cargo test --locked render::` 通过, 现有转义与列宽测试不回归.
- [x] 1.3 在 `src/render.rs` 增加报告级测试: target 含 U+00AD, U+061C, U+E0041 时, `render` 输出包含对应 `\u{...}` 且仍为每条目一行; 验证: 该测试通过.

## 2. 文档与规格

- [x] 2.1 更新 `README.md` 的"部分格式字符仍按原样显示"条目, 改为说明 Default_Ignorable 已被覆盖, 并保留仍不覆盖的边界 (例如未列入的普通 `Cf` 字符如有); 验证: `grep -n "Default_Ignorable" README.md` 命中.
- [x] 2.2 确认 `openspec/changes/escape-remaining-format-controls/specs/cli-contract/spec.md` 的 delta 与实现一致; 验证: `openspec validate escape-remaining-format-controls --strict` 通过.

## 3. 全量验证

- [x] 3.1 运行 `cargo test --locked`; 验证: 全部通过.
- [x] 3.2 运行 `python3 scripts/check-punctuation.py`; 验证: 退出 0.
- [x] 3.3 运行 `./scripts/check.sh`; 验证: 脚本退出 0.
- [x] 3.4 运行 `openspec validate escape-remaining-format-controls --strict`; 验证: 0 error, 0 warning.

## Workflow follow-up

- Archive the change after review.

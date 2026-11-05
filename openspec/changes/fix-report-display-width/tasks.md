# Tasks

## 1. 宽度区间

- [ ] 1.1 按 EastAsianWidth 17.0 的 `W` / `F` 数据补齐 `character_width` 的宽字符区间, 保留现有 CJK
      区块级区间, 并在注释记录数据来源, 版本与重新生成的方法; 验证: 新增单测断言 `0x231A`, `0x2329`,
      `0x2B50`, `0x1F004`, `0x1F680`, `0x1F7E0`, `0x1FAF8`, `0x16FE0`, `0xA960` 各计 2 列.
- [ ] 1.2 同一组单测断言邻接的窄码点仍计 1 列 (`0x2319`, `0x2B1A`, `0x1F6C6`, `0x1F6CA`), 且组合记号
      (U+0301) 与变体选择符 (U+FE0F) 仍计 0 列; 验证: `cargo test --locked render` 通过.
- [ ] 1.3 确认没有引入依赖或改动构建配置; 验证: `git diff --stat` 中不出现 `Cargo.toml` 与
      `Cargo.lock`, `cargo build --locked` 成功.

## 2. 端到端对齐回归

- [ ] 2.1 在 `tests/output.rs` 增加用例: 清单含 `target = "~/ab"`, `"~/⌚"`, `"~/🚀"` (均 4 列) 与
      `"~/.配置"`, `"~/.abcd"` (均 7 列); 验证: `preview` 输出中各行的说明列起始显示宽度相同.
- [ ] 2.2 同文件增加组合记号与超宽 target 的断言: `~/.e` 加 U+0301 与 `~/ab` 对齐; 超过补齐宽度的
      target 不截断, 说明列右移且行数不变; 验证: `cargo test --locked --test output` 通过.
- [ ] 2.3 更新 README 报告对齐段落里的近似边界 (字素簇, 变体选择符, ZWJ 序列按码点近似) 并写明宽字符
      数据来源; 验证: 该段与 `character_width` 的注释在版本与近似范围上逐项一致.

## 3. 集成

- [ ] 3.1 运行 `cargo fmt --check` 与 `cargo clippy --locked --all-targets -- -D warnings`; 验证: 两者
      均为空输出.
- [ ] 3.2 运行 `./scripts/check.sh`; 验证: 退出 0, 标点检查, 格式, 全套测试, Clippy, rustdoc 与 release
      构建全部通过.

## Workflow follow-up

- 归档前复核 README 的对齐说明与新增需求 "报告列宽按终端显示宽度计算" 的措辞无冲突.
- 归档后重跑 `./scripts/check.sh` 与 `openspec validate --all`.

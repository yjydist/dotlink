# Tasks

## 1. 复核实现

- [ ] 1.1 给 `apply::apply_entry` 与 `replace_and_backup` 增加已规范化的 `repo_root` 参数, 由 `lib::apply` 在循环外传入; 验证: `cargo test --locked apply` 编译并通过.
- [ ] 1.2 在 `LinkedWrong` 自愈备份前重新 lstat 目标, 复核它是符号链接且 `link_destination` 落在仓库根内; 不满足时返回条目级错误; 验证: 新增单测直接调用 `apply_entry`, 断言不写备份且目标原样.
- [ ] 1.3 错误说明包含"判定之后目标已被替换为非本工具链接或内容", 并给出目标路径; 验证: 断言说明子串.

## 2. 竞态测试

- [ ] 2.1 增加"备份前替换成普通文件"的 hook 用例: 条目失败, 文件内容与 inode 不变, 目录中没有 `.dotlink-heal.`; 验证: `cargo test --locked apply` 通过.
- [ ] 2.2 增加"备份前替换成仓库外链接"的 hook 用例: 条目失败, 链接文本不变, 不出现 heal 备份; 验证: 同上.
- [ ] 2.3 确认正常自愈路径 (仍是仓库内错误链接) 仍成功并产生 heal 备份; 验证: 既有自愈测试通过.

## 3. 文档与验证

- [ ] 3.1 更新 `README.md` 已知限制: classify 与自愈备份之间的复核, 以及复核后到复制开始的窄窗口; 验证: `rg -n "自愈|复核" README.md` 能定位.
- [ ] 3.2 运行 `openspec validate fix-selfheal-namespace-race --strict`; 验证: 无 error 与 warning.
- [ ] 3.3 运行 `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings` 与 `./scripts/check.sh`; 验证: 全部通过.

## Workflow follow-up

- 归档前确认 README 的并发限制与 `link-deployment` 的新 Scenario 一致.

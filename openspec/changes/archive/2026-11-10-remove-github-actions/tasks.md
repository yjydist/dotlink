# Tasks

## 1. 删除 workflow 与对齐 README

- [x] 1.1 删除 `.github/workflows/ci.yml` 与 `.github/workflows/audit.yml`. 验证: `test ! -e .github/workflows/ci.yml && test ! -e .github/workflows/audit.yml` 退 0; 提交后 `git ls-tree -r --name-only HEAD .github` 无输出.
- [x] 1.2 删除 `README.md` 顶部的 CI badge 行. 验证: `grep -c "badge.svg" README.md` 输出 0; `sed -n '1,6p' README.md` 中 H1 之后直接是项目描述.
- [x] 1.3 用本地质量门段落替换 `README.md` 开发段中的 CI job 列表, 段落覆盖三点: `./scripts/check.sh` 是唯一必需门禁; 最低 Rust 版本与跨平台编译按紧随其后的既有命令块手工执行; 依赖审计不进入 `check.sh`, 安装 `cargo-audit` 后按需运行 `cargo audit`. 验证: `grep -n -E "ubuntu-latest|macos-latest|actions/workflows" README.md` 零命中; 人工阅读该段, 确认三点齐全, 且门禁描述与 `scripts/check.sh` 的检查项一致.

## 2. 一致性与回归验证

- [x] 2.1 确认标点检查的范围说明未失真: `scripts/check-punctuation.py` 的 `.github/**` 条目与 docstring 保持原样, README 中覆盖范围的句子与该脚本的 `TEXT_TREES` 一致, 不修改回归测试. 验证: `python3 scripts/check-punctuation.py` 退 0; 人工比对 README 该句与脚本 docstring 的目录列表.
- [x] 2.2 确认仓库内无残留引用: 排除归档与本次变更目录后, 源码, README 与脚本中不再出现 `workflows/ci.yml` 或 `workflows/audit.yml`. 验证: `grep -rn --exclude-dir=target --exclude-dir=.git --exclude-dir=.agents --exclude-dir=archive --exclude-dir=remove-github-actions -E "workflows/(ci|audit)\\.yml" .` 无输出.
- [x] 2.3 运行完整的本地质量门. 验证: `./scripts/check.sh` 退 0.
- [x] 2.4 校验变更本身. 验证: `openspec validate remove-github-actions --strict` 退 0, 且 `openspec status --change remove-github-actions --json` 中 `specs` 为 `skipped`, 其余制品为 `done`.

## Workflow follow-up

- 推送后确认 GitHub Actions 不再产生新的 CI 或 Audit 运行: `gh run list --limit 5` 只显示历史记录.
- 在项目评审要求满足后归档本变更, 并验证归档结果.

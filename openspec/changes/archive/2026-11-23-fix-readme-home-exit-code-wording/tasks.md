# Tasks

## 1. 修正 README 措辞

- [x] 1.1 修改 `README.md` 的"退出码 2 的两重含义"一节, 把"主目录无法确定"限定为需要展开 `~` 时的情况, 并补充"不含 `~` 的清单不因 `HOME` 缺失被拒"; 验证: `grep -n "主目录无法确定" README.md` 只命中限定后的表述, 且同段出现"需要展开 `~`".
- [x] 1.2 运行 `python3 scripts/check-punctuation.py`; 验证: 退出 0 且无禁止标点输出.

## 2. 全量验证

- [x] 2.1 运行 `./scripts/check.sh`; 验证: 脚本退出 0.
- [x] 2.2 运行 `openspec validate fix-readme-home-exit-code-wording --strict`; 验证: 0 error, 0 warning.

## Workflow follow-up

- Archive the change after review.

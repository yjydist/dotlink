# Tasks

## 1. 扩展扫描范围

- [x] 1.1 在 `TEXT_TREES` 增加 `(".github", (".yml", ".yaml", ".md"))`, 并把 `openspec/config.yaml` 与
      `openspec/config.yml` 加入 `TEXT_FILES`; 验证: 用 `--root` 指向仓库根调用 `iter_files`, 输出包含
      `.github/workflows/ci.yml`, `.github/workflows/audit.yml` 与 `openspec/config.yaml`, 且文件总数从
      115 变为 118.
- [x] 1.2 负向验证: 在 `mktemp -d` 的临时根目录下建 `.github/workflows/probe.yml`, 内容含一个全角逗号;
      验证: `python3 scripts/check-punctuation.py --root <临时根>` 退出 1, 且命中行给出该文件与行号.
- [x] 1.3 同步脚本 docstring 与 `--help` 的覆盖范围描述; 验证: `python3 scripts/check-punctuation.py --help`
      的输出提到 CI 配置与 OpenSpec 项目配置, 且与白名单逐项一致.
- [x] 1.4 把 1.2 的负向验证固化为 `scripts/test_check_punctuation.py`, 让 `scripts/check.sh` 紧随标点检查运行;
      验证: `python3 scripts/test_check_punctuation.py` 全绿 (临时目录里 `.github/` 与
      `openspec/config.{yaml,yml}` 的命中被报告, 白名单外的后缀被忽略, `--root` 缺失退 2).

## 2. 文档与集成

- [x] 2.1 更新 README "开发" 节里对检查覆盖范围的枚举, 加入 `.github/` 与 `openspec/config.yaml`; 验证:
      该段枚举与脚本白名单逐项一致 (人工核对五个位置: AGENTS.md, README.md, openspec, src/tests, scripts/.github).
- [x] 2.2 回归: 运行 `python3 scripts/check-punctuation.py`; 验证: 退出 0, 无命中.
- [x] 2.3 集成: 运行 `./scripts/check.sh`; 验证: 退出 0, 格式, 测试, Clippy, rustdoc 与 release 构建全部通过.

## Workflow follow-up

- 归档前运行 `openspec validate --all` 与 `./scripts/check.sh`.
- 归档后运行 `python3 scripts/check-punctuation.py` 确认新增的归档目录不引入命中.

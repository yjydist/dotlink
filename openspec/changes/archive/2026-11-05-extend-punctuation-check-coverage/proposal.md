# Proposal

## Why

AGENTS.md 要求"标点只能使用 ASCII"适用于"文档, 代码注释等一切自然语言输出", 而唯一的自动执行点是
`scripts/check-punctuation.py`. 该脚本的白名单不含 `.github/` 与 `openspec/config.yaml`, 但这两个位置
已经在写中文 (CI 工作流里的说明性注释, OpenSpec 项目上下文与 per-artifact 规则). 于是在这些文件里写入
全角标点不会被 `./scripts/check.sh` 发现: 规则在它们真实的写作面上没有执行点, 只能靠人工评审兜住.

## What Changes

- 扫描范围新增 `.github/` 目录下的 `.yml` / `.yaml` / `.md` 文件 (现有工作流注释与将来可能出现的模板).
- 扫描范围新增 `openspec/config.yaml` 与 `openspec/config.yml` 两个显式文件, 存在哪个就检查哪个.
- 同步更新脚本 docstring, `--help` 描述与 README "开发" 节对覆盖范围的枚举, 使文档与白名单一致.
- 不改检查算法, 不改退出码语义 (无命中 0, 有命中或读取失败 1, 参数错误 2), 不新增依赖, 不改 Rust 代码.

## Capabilities

### New Capabilities

无. 本变更只改检查脚本的扫描范围, dotlink 的可观察行为 (命令, 报告, 退出码, 文件系统操作) 一律不变.

### Modified Capabilities

无.

这是纯工具链改动, 因此变更的 `.openspec.yaml` 设置 `skip_specs: true`, 不写规格增量.

## Impact

- `scripts/check-punctuation.py`: 白名单与文档字符串.
- `README.md`: "开发" 节里对检查覆盖范围的枚举.
- 不受影响: `src/`, `tests/`, `Cargo.lock`, 依赖树, 平台矩阵, 命令契约.

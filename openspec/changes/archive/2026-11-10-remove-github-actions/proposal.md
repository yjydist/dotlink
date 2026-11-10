# Proposal

## Why

当前环境下 GitHub Actions 无法启动 workflow, 仓库不再有可用的托管 CI. 保留 `.github/workflows/` 只会让定时任务持续产生失败状态, 同时 README 的 badge 与 CI 描述仍在声明一个不会运行的流程.

## What Changes

- 删除 `.github/workflows/ci.yml` 与 `.github/workflows/audit.yml`; `.github/` 目录随之消失.
- 删除 `README.md` 顶部的 CI badge.
- 用本地质量门说明替换 `README.md` 开发段中的 CI job 列表: `./scripts/check.sh` 是唯一必需门禁; 最低 Rust 版本与跨平台编译按该段已有的手工命令执行; 依赖审计不进入 `check.sh`, 由维护者在安装 `cargo-audit` 后按需运行 `cargo audit`.
- 不修改 `scripts/check.sh`, `scripts/check-punctuation.py` 及其回归测试, `rust-toolchain.toml`, `Cargo.toml` 与 `openspec/config.yaml`.
- 不修改已归档的 OpenSpec 变更, 包括 `add-project-hygiene` 中的 CI 选型记录.
- 记录已知覆盖缺口: 删除后 Linux 上的自动执行与依赖公告监控都不再存在, 二者改为维护者按需手工验证.

对 CLI 行为与使用者无影响.

## Capabilities

### New Capabilities

无. 本变更不引入新的系统行为.

### Modified Capabilities

无. 现有 capability 描述的都是 CLI 运行时行为, 没有任何规格要求 CI 或托管服务. 因此本变更在 `.openspec.yaml` 中设置 `skip_specs: true`, 不创建 delta spec.

## Impact

- 删除文件: `.github/workflows/ci.yml`, `.github/workflows/audit.yml`.
- 修改文件: `README.md` (badge 与开发段).
- 恢复路径: 两个 workflow 可从 git 历史 `cbb7c01` 取回, 例如 `git show cbb7c01:.github/workflows/ci.yml`.
- 不影响: 源码, 测试, 本地检查脚本, 依赖与工具链配置, 已归档的 OpenSpec 变更.

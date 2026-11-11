# Proposal

## Why

测试套件的覆盖面很广, 但有几处结构性缺口会让真实回归静默通过: 备份保真没有断言权限位, 零改动快照不记录 mode/type/mtime, 四条命令共用一个前后快照, 标点检查的回归测试只覆盖 `.github` 与 config, 根帮助没有锁定"恰好四个子命令", 还有 2 秒墙钟预算与固定 root 探测路径等不稳定因素. 本变更只改测试与本地脚本, 不改变产品行为.

## What Changes

- 备份保真测试增加权限位断言: 文件 0640, 目录 0750 与嵌套子项, 覆盖备份与恢复两侧.
- `snapshot`/`collect` 记录 lstat mode, file type, device/inode 与 mtime (不记录 atime), 使只读命令的"零改动"能发现 chmod/touch.
- 四条命令的"清单校验零改动"改为每条命令各自比较前后快照, 避免后一条命令抵消前一条的写入.
- 标点检查回归测试覆盖 `src`, `tests`, `scripts`, `openspec/specs`, `openspec/changes`, `AGENTS.md`, `README.md`, 以及 symlink 跳过与非法 UTF-8.
- 根帮助测试断言子命令集合恰好是 `preview/apply/status/undo`, 而不是"四个名字存在且五个禁用词不存在".
- 2 秒墙钟预算改为 opt-in 或删除; root 探测用 `tempfile`; 增加非法 TZ, symlink manifest, 并发启动屏障, 以及重复 socket/exit-code 测试的合并.
- 增加可选 `scripts/check-cross.sh` (或 `check.sh --full`), 在本机目标可用时跑 Linux/Windows check 与 `cargo audit`; 不引入 CI.

## Capabilities

### New Capabilities

- 无.

### Modified Capabilities

- 无. 本变更只改测试与本地验证脚本, `.openspec.yaml` 设置 `skip_specs: true`.

## Impact

- `tests/metadata.rs`, `tests/support/mod.rs`, `tests/manifest_errors.rs`, `tests/usage_errors.rs`, `tests/apply.rs`, `tests/backup_ordering.rs`, `tests/exit_codes.rs`.
- `scripts/test_check_punctuation.py`, `scripts/check.sh`, 新增可选 `scripts/check-cross.sh`.
- 不改变 `src/` 的生产行为.

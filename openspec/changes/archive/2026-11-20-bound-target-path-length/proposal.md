# Proposal

## Why

`fix-long-basename-handling` 的 V-12 只约束单段 basename 不超过 200 字节, 明确把整条路径的长度 (PATH_MAX) 交给操作系统报告. 但整条路径也会出现同一类坏状态: 工具在目标路径后追加固定后缀生成备份名与暂存名, 当目标已接近 PATH_MAX 时, `apply` 的 create 能成功, 而 `undo` 的暂存名再多约 28 字节便超出限制, 返回 `ENAMETOOLONG`, 工具自己建立的链接永久残留. 已在临时目录复现: 4071 字节的 target 在 Linux 上 `apply` 退出 0 且链接存在, `undo` 退出 1 并报告 `File name too long (os error 36)`. macOS 的 PATH_MAX 为 1024, 触发门槛更低.

清单接受的目标必须能被四条命令 (`apply`, `undo`, `preview`, `status`) 处理, 这与其 basename 是否超长属于同一类问题, 只是维度从单段换成了完整路径. `fix-long-basename-handling` 只覆盖了前者, 本变更补上后者.

## What Changes

- 新增清单校验 V-13 "目标完整路径长度预算": `target` 的规范化绝对路径字节数, 加上工具可能追加的最长生成后缀 (备份最终名, 备份暂存名, 恢复暂存名, 替换/撤销暂存名) 后, MUST 不超过平台路径预算 (Linux 4096 字节, macOS 1024 字节). 超过时整体拒绝, 退出码 2, 零改动, 说明给出条目下标, 书写形态, 解析后字节数与预算.
- 预算常量与 V-12 的 200 字节 basename 预算放在同一处共享模块, 清单校验与命名逻辑共同引用; 依赖 `fix-long-basename-handling` 引入的共享预算常量, 建议实现顺序在其之后.
- **BREAKING**: 旧版接受的超长完整路径清单现在会被整体拒绝, 退出码 2, 零改动. 这是有意的 fail-fast: 工具无法为这些路径生成合法的备份/暂存名, 让 `apply` 成功而 `undo` 失败更糟.
- README 的"已知限制"补充完整路径预算, 平台差异与相对 target 的接受面随仓库位置变化的说明.
- 增加边界测试: 预算内接受, 超出 1 字节拒绝, 多字节路径按 UTF-8 字节数拒绝, 接近预算的 create 与 undo 端到端成功.

## Capabilities

### New Capabilities

- 无.

### Modified Capabilities

- `manifest-schema`: 新增 V-13 目标完整路径长度预算.
- `backup-lifecycle`: 明确备份最终名, 备份暂存名与恢复暂存名的长后缀属于 V-13 预算, 超限在清单层拒绝而不是运行期报 `ENAMETOOLONG`.

## Impact

- `src/manifest.rs`: 新增 V-13 校验与错误说明.
- 共享预算常量所在模块 (与 `fix-long-basename-handling` 的 200 字节常量同处): 新增平台 PATH_MAX 常量与最长生成后缀预算.
- `tests/manifest_errors.rs`: 边界, 多字节与超预算用例.
- `README.md`: 已知限制与清单校验说明.
- 不改变备份命名, 查找, 恢复或暂存语义; 不实现截断或哈希命名.

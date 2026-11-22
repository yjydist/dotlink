# Proposal

## Why

`apply` 对目录目标执行 `replace_and_backup` 时, 先递归复制目标 (`copy_object`), 再对暂存目录调用 `fs::remove_dir_all` (`remove_staged`). 复制与删除都不识别挂载边界, 因此目标目录内只要有一个挂载点, 工具就会先把该挂载点的内容复制进备份, 再在删除阶段跨越挂载点递归删除.

已用临时目录与 user namespace 复现两类后果:

- 目标目录内挂载 tmpfs: `apply` 报告 `fail`, 退出 1, 但 tmpfs 中的文件已被删除; 挂载点仍停留在 `<target>.<pid>.dotlink-replace.partial` 上, 原目标路径消失.
- 目标目录内 bind mount 源仓库: `apply` 报告 `fail`, 退出 1, 但源仓库中的源文件, README 与清单都被删除; 只有备份目录保留了一份副本.

这类失败发生在工具承诺"失败时保持原状"的路径上, 并且可能删除源仓库或其他文件系统的数据. 必须在复制与删除之前识别挂载边界, 让该条目失败并停止对挂载内容的任何操作.

## What Changes

- 新增挂载身份判定: 至少比较设备号; Linux 在可用时同时比较 `statx` 返回的挂载标识 (`STATX_MNT_ID`), 以识别共享设备号的 bind mount.
- `backup-fidelity`: 备份与恢复的递归复制遇到挂载边界时整体失败, 不发布最终备份名, 不删除或修改原对象; 复制失败后的清理同样不得跨越挂载边界删除内容.
- `link-deployment`: 目录替换在删除暂存目录之前复核挂载边界; 发现边界时不得递归删除该挂载点之后的内容, 应尽力把暂存目录移回原路径并报告失败; 删除使用不跨挂载边界的递归实现.
- 失败说明给出挂载点完整路径, 并提示先卸载或移走该挂载点再重试.
- README 与已知限制更新: 目录目标含挂载点会被拒绝; macOS 同设备 bind/nullfs 的识别边界; `preview` 不递归预检目录内的挂载点.
- **BREAKING**: 过去会被接受并可能删除挂载内容的目录目标, 现在以条目失败结束; token 词汇与退出码取值不变.

## Capabilities

### New Capabilities

- 无.

### Modified Capabilities

- `backup-fidelity`: 新增"备份与恢复不得跨越挂载边界"的要求, 覆盖复制, 失败清理, 最终名发布与原对象保持.
- `link-deployment`: 新增"目录替换不得跨越挂载边界删除内容"的要求, 覆盖删除前复核, 挂载点保护, 暂存目录回滚与失败报告.

## Impact

- 新增 `src/os` 的挂载身份原语; 生产路径修改 `src/backup/copy.rs` 的复制与清理, 以及 `src/apply.rs` 的暂存目录删除.
- 复用现有 `rustix` 依赖的 `statx` 能力, 不新增依赖, 不引入 `unsafe`; macOS 使用设备号, 非 Unix 平台保留可编译 stub 并由平台门禁拒绝.
- 测试新增 Linux user namespace + tmpfs/bind mount 的端到端用例, 环境不支持时跳过; 单元测试使用现有线程局部测试接缝覆盖删除阶段.
- README 的 `on_conflict` 说明与"已知限制"更新; 不新增命令, 不修改 token, 退出码, 备份命名或清单格式.
- 不写状态文件, 不支持预览阶段的递归预检; 挂载点在运行时出现时仍按既有并发边界处理.

# Proposal

## Why

`reject-cross-mount-targets` 加入了挂载边界检查, 但平台能力仍有缺口: Linux 在 `statx` 不提供 `STATX_MNT_ID` 时退回设备号, 同设备 bind mount 可能漏检; macOS 只比较设备号, 同设备 bind mount 与 nullfs 属于已知漏检. 在这些环境下, 备份或删除可能跨越挂载边界.

## What Changes

- Linux: 当 `statx` 取不到 `STATX_MNT_ID` 时, 读取 `/proc/self/mountinfo`, 按最长的挂载点前缀解析 mount id; mountinfo 不可用时仍退回设备号.
- macOS: 在设备号之外使用 `statvfs` / `fstatvfs` 的 `f_fsid` 作为挂载标识; 取不到时仍退回设备号.
- 保持现有失败语义: 发现边界时备份整体失败, 不发布最终名, 不删除或修改原对象; 删除阶段遇到边界时停止并保留暂存目录.
- 更新 `backup-fidelity` 与 `link-deployment` 的挂载边界要求, README 的已知限制与平台说明.
- 增加 Linux mountinfo 解析与 fallback 的单元 / 集成测试; macOS 分支在没有 macOS 环境时只做编译与代码审阅证据.

## Capabilities

### New Capabilities

- 无.

### Modified Capabilities

- `backup-fidelity`: 挂载身份新增 Linux mountinfo fallback 与 macOS `f_fsid` 判定.
- `link-deployment`: 目录替换的递归删除使用同一套挂载身份来源.

## Impact

- `src/os/mount.rs` 的 `MountIdentity` 与 `mount_id`; 可能新增 mountinfo 解析辅助模块; `README.md`; 两个 spec 的 delta.
- 不新增依赖, 不改变 mount 边界发现后的用户可见失败语义, token 或退出码.

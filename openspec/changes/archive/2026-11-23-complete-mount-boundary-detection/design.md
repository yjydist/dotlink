# Design

## Context

`src/os/mount.rs` 的 `MountIdentity` 保存设备号与可选的挂载标识. Linux 使用 `statx` 的 `STATX_MNT_ID`; 取不到时只比较设备号. macOS 没有 `statx`, 只比较设备号. 现有 `same_mount` 与递归复制 / 删除的失败语义已经由测试锁定, 本变更只补充挂载标识来源, 不改变语义.

## Goals / Non-Goals

**Goals:**

- 在 Linux 旧内核或受限环境没有 `STATX_MNT_ID` 时, 用 `/proc/self/mountinfo` 提供同设备 bind mount 的挂载标识.
- 在 macOS 上用 `statvfs` / `fstatvfs` 的 `f_fsid` 提供同设备挂载的附加标识.
- 平台能力仍不可用时退回设备号, 不把"取不到标识"变成新的拒绝条件.

**Non-Goals:**

- 不识别 btrfs subvolume 等非挂载边界的文件系统子结构.
- 不改变复制 / 删除的失败语义, token, 退出码或报告措辞.
- 不在 `preview` 阶段递归预检目录内容.

## Decisions

### D1. Linux mountinfo fallback

新增一个只读解析器: 读取 `/proc/self/mountinfo`, 解析每个条目的 mount id 与 mount point, 反转义 `\040` / `\011` / `\012` / `\134`, 然后对被查询路径按路径组件找最长前缀. 结果用 `OnceLock` 缓存一次; 取不到时返回 `None`, 继续退回设备号. 备选方案是每次调用都重新读取 mountinfo; 否决理由是递归复制会对每个子项触发读取, 成本过高, 且本变更只承诺静态挂载的识别. 缓存带来的"运行中新增挂载"竞态与现有文档中的并发边界一致.

### D2. macOS `f_fsid`

在 `#[cfg(target_os = "macos")]` 分支调用 `statvfs` (或对目录 fd 调用 `fstatvfs`), 使用 rustix `StatVfs` 的公开 `f_fsid: u64` 作为 `mount` 字段. 调用失败或 `f_fsid` 为 0 时返回 `None`. 备选方案是使用 `statfs` / `fstatfs`; 否决理由是 macOS `libc::statfs.f_fsid` 的字段在 rustix 与 libc 中不可安全读取, 会迫使项目引入 `unsafe` 或 `libc` 依赖. 另一备选方案是解析 macOS 挂载列表; 否决理由是 `statvfs` 已经由 rustix 安全暴露, 成本更低.

### D3. 同一套 `same_mount` 语义

`MountIdentity::same_mount` 保持不变: 两侧都有 mount id 时比较 id, 任一侧缺失时退回设备号. 新来源只填充 `mount` 字段, 不增加新的比较分支.

## Risks / Trade-offs

- [mountinfo 缓存过期] -> 静态挂载是主要目标, 运行中新增挂载的竞态与现有并发限制一致; 缓存避免递归路径上的重复读取.
- [mountinfo 反转义不完整] -> 单元测试覆盖空格, 制表符, 换行与反斜杠转义; 解析失败只退回设备号.
- [macOS `f_fsid` 语义] -> 只在 macOS 编译分支使用; 没有 macOS 环境时以代码审阅与 `cargo check` 为证据, 并在 README 记录.
- [性能] -> fallback 只在 `statx` 取不到标识时触发; 缓存后每个进程只解析一次.

# Spec Delta

## ADDED Requirements

### Requirement: 挂载标识的平台补全

挂载身份 MUST 在设备号之外尽量取得平台挂载标识. Linux 上 `statx` 未声明 `STATX_MNT_ID` 或调用失败时, 系统 MUST 读取 `/proc/self/mountinfo` 并按最长挂载点前缀解析 mount id; mountinfo 不可用时 MUST 退回设备号. macOS 上系统 MUST 使用 `statvfs` 或 `fstatvfs` 的 `f_fsid` 作为挂载标识; 取不到时 MUST 退回设备号. 平台能力不足 MUST NOT 变成新的拒绝条件或失败原因.

#### Scenario: Linux 没有 statx 挂载标识时使用 mountinfo

- **WHEN** 一个目录内存在与父目录设备号相同的 bind mount, 且 `statx` 没有返回 `STATX_MNT_ID`
- **THEN** 备份该目录时该 bind mount 仍被识别为挂载边界, 条目报告 `fail`, 不复制挂载内容, 不发布最终备份名

#### Scenario: Linux mountinfo 不可用

- **WHEN** `statx` 没有返回挂载标识, 且 `/proc/self/mountinfo` 无法读取或解析
- **THEN** 挂载身份退回设备号, 备份行为与既有设备号比较一致, 不因读取失败而拒绝清单或提前失败

#### Scenario: macOS 使用 f_fsid

- **WHEN** 一个目录内的同设备挂载与父目录设备号相同, 且 `statvfs` 返回不同的 `f_fsid`
- **THEN** 该挂载被识别为边界, 备份报告 `fail`, 原对象与挂载内容保持原样

#### Scenario: macOS f_fsid 不可用

- **WHEN** `statvfs` 或 `fstatvfs` 失败, 或 `f_fsid` 为 0
- **THEN** 挂载身份退回设备号, 不因取不到标识而新增失败

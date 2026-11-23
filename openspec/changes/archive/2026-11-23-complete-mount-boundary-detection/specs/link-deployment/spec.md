# Spec Delta

## ADDED Requirements

### Requirement: 目录删除使用同一挂载标识来源

目录替换的删除阶段 MUST 使用与备份复制相同的挂载身份来源, 包括 Linux 的 `statx` / `mountinfo` fallback 与 macOS 的 `f_fsid`. 发现挂载边界时 MUST 停止删除该边界之后的内容, MUST 尽力把暂存目录原子移回原路径, MUST NOT 建立链接. 移回失败时 MUST 保留暂存目录并给出其完整路径. 现有 token, 退出码与失败措辞 MUST NOT 改变.

#### Scenario: Linux fallback 在删除阶段识别边界

- **WHEN** 备份完成后删除暂存目录, `statx` 没有返回挂载标识, 而 mountinfo 显示暂存目录内有同设备 bind mount
- **THEN** 删除停止在该挂载点之前, 暂存目录尽量移回原路径, 该条目报告 `fail`, 挂载内容不变

#### Scenario: macOS 在删除阶段使用 f_fsid

- **WHEN** 删除暂存目录时 `statvfs` 返回与父目录不同的 `f_fsid`
- **THEN** 删除停止在该挂载点之前, 暂存目录尽量移回原路径, 该条目报告 `fail`, 挂载内容不变

#### Scenario: 平台标识不可用时不新增失败

- **WHEN** 删除阶段取不到 `statx` 挂载标识, mountinfo 或 `f_fsid`
- **THEN** 删除按设备号比较继续, 既有已知限制不变, 不因取不到标识而提前失败

# Design

## Context

生成的单段文件名由 target basename 加上固定后缀构成:

- 备份暂存名: `<basename>.dotlink-backup.<15>.<pid>.partial`
- 备份最终名: `<basename>.dotlink-backup.<15>[.<n>]`
- 恢复暂存名: `<basename>.<pid>.<n>.dotlink-restore.partial`
- apply/undo 暂存名: `<basename>.<pid>.<label>[.<n>]`

在 `NAME_MAX = 255` 的卷上, 最长后缀 (7 位 pid, 消歧序号, `.partial`) 约为 48 字节, 因此 basename 的安全上限约为 207 字节. 当前清单不限制, 导致 `apply` 成功而 `undo` 失败. 已复现 229 字节 basename 的 `create` 成功与 `undo` 失败.

## Goals / Non-Goals

**Goals:**

- 让所有被清单接受的目标都能被四条命令处理, 消除"apply 成功但 undo 无法移除"的状态.
- 在清单校验阶段 fail-fast, 退出 2, 零改动, 说明给出字节数与上限.
- 预算常量只有一份, 清单校验与命名逻辑共用.

**Non-Goals:**

- 不支持任意长度 basename; 不引入截断, 哈希或映射文件.
- 不改变 200 字节以内的现有命名, 查找与恢复语义.
- 不改变 `NAME_MAX` 之外的路径总长度 (PATH_MAX) 行为; 后者仍由操作系统错误报告.

## Decisions

### D1. 在清单层拒绝, 而不是截断或哈希

选择 200 字节预算并在 `manifest` 校验阶段拒绝.

备选方案: 截断原名并追加哈希. 否决理由: 备份名是查找索引, 截断后 `BackupFinder` 需要第二套映射; 用户也无法从名字直接识别原对象; 迁移旧备份与 README 的命名契约都会变复杂. 备选方案: 只在备份/暂存失败时报告 ENAMETOOLONG. 否决理由: 会让 `apply` 的 create 路径成功, 而后续 `undo` 失败, 正是本变更要消除的状态.

### D2. 预算取 200 字节, 为 7 位 pid 与消歧序号留余量

`NAME_MAX = 255`; 最长后缀约为 48 字节 (7 位 pid + 消歧序号 + `.partial` 或 `.dotlink-restore.partial`). 200 字节预算留出 55 字节余量, 覆盖实现细节变化. 常量放在 `entry` 或共享模块中, 由 `manifest` 校验与 `backup`/`os` 的命名测试共同引用.

备选方案: 取 207 字节的精确上限. 否决理由: 依赖 pid 位数与序号长度, 边界过于贴近; 200 更易解释与测试. 备选方案: 取 224 字节 (只看备份最终名). 否决理由: 会漏掉暂存名的更长后缀.

### D3. 统一拒绝, 不区分动作

即使当前动作只是 create, 也按同一预算拒绝. 理由: target 状态会变化, 今天 create 的路径明天可能进入 replace; 清单契约不应依赖运行时状态.

### D4. 文档与测试

README 的"已知限制"补充: 200 字节上限, 按 UTF-8 字节计数, 多字节文件名更早触发; 超过时整个清单拒绝而不是跳过该条目. 测试覆盖 200/201 ASCII, 67 个 3 字节字符, 以及 200 字节 basename 的 replace/undo 端到端成功.

## Risks / Trade-offs

- [兼容性收紧: 201-255 字节的合法文件名被拒绝] -> 这是有意的 fail-fast; 说明给出上限, 用户可缩短名字或改用目录链接. README 明确标记.
- [200 字节对某些文件系统仍偏大或偏小] -> 255 是 macOS APFS 与 Linux ext4 的常见上限; 更小的文件系统 (如某些 FAT) 会在运行期报错, 不在本变更承诺范围. 预算常量集中, 后续可按平台调整.
- [多字节用户误以为字符数够] -> 错误说明与 README 都按字节解释, 测试用 67 个 3 字节字符锁定.
- [只创建链接的条目被拒绝] -> 清单契约统一, 避免运行时状态决定是否接受; 这是与 V-9 一致的"落点退化"思路.

## Migration Plan

无数据迁移. 超长 target 的清单需要缩短 basename, 或改为部署其父目录. 已部署的链接不受影响; 回滚时移除 V-12 校验即可恢复旧接受面.

## Open Questions

- 200 字节是否需要在后续按平台或用户反馈调整; 当前作为共享常量写入规格, 调整时同步 `manifest-schema` 与 `backup-lifecycle`.

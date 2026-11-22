# Spec Delta

## ADDED Requirements

### Requirement: 备份与恢复不得跨越挂载边界

备份与恢复的递归复制 MUST NOT 跨越挂载边界. 被复制的对象自身或其后代若是挂载点, 即其挂载身份与父目录不同, MUST 使该条目失败; MUST NOT 复制该挂载点之后的内容, MUST NOT 跳过它继续, MUST NOT 发布最终备份名, MUST NOT 删除或修改原对象. 挂载身份 MUST 至少包含设备号; 平台提供挂载标识时 MUST 同时比较该标识. 平台无法提供挂载标识时, 同设备挂载的漏检属于已知限制. 失败说明 MUST 给出挂载点完整路径, 并提示先卸载或移走该挂载点再重试.

#### Scenario: 目录内含 tmpfs 挂载

- **WHEN** 一个目录被 `replace_and_backup` 覆盖, 其子目录是 tmpfs 挂载点, 执行 `apply`
- **THEN** 该条目报告 `fail`, 退出码 1, 不出现最终备份名, 原目录与 tmpfs 中的内容都保持原样, 不建立链接

#### Scenario: 目录内含源仓库的 bind mount

- **WHEN** 一个目录被 `replace_and_backup` 覆盖, 其子目录是源仓库的 bind mount, 执行 `apply`
- **THEN** 该条目报告 `fail`, 源仓库中的文件, 清单与父目录内容都保持原样, 不出现最终备份名, 不建立链接

#### Scenario: 目标自身是挂载点

- **WHEN** 目标路径本身是挂载点, 条目声明 `replace_and_backup`, 执行 `apply`
- **THEN** 该条目报告 `fail`, 不复制该挂载内容, 不出现最终备份名, 目标挂载点与其内容保持原样

#### Scenario: 恢复备份不得跨越挂载边界

- **WHEN** `undo` 要恢复的备份目录中含挂载点
- **THEN** 恢复报告 `fail`, 按既有撤销规则回滚或保留目标路径, 备份与挂载点内容保持不变

#### Scenario: 不含挂载点的目录正常备份

- **WHEN** 一个目录不含任何挂载点, 条目声明 `replace_and_backup`, 执行 `apply`
- **THEN** 备份成功发布, 目标被替换为链接, 行为与既有目录替换一致

### Requirement: 复制失败清理不得跨越挂载边界

复制失败后的清理 MUST 使用与复制相同的挂载边界判定, MUST NOT 跨越挂载边界删除内容. 无法安全清理时 MUST 保留残渣, MUST NOT 为清理而删除挂载点之后的内容; 原始失败原因 MUST 仍被报告.

#### Scenario: 失败清理遇到挂载点

- **WHEN** 目录备份在子项复制阶段失败, 本次创建的暂存树中出现挂载点
- **THEN** 清理在挂载点之前停止, 挂载点及其内容保持原样, 残留的暂存对象不被当作完整备份, 原始失败原因仍出现在该条目的说明中

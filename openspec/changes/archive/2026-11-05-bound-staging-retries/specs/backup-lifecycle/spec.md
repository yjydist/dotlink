# Spec Delta

## MODIFIED Requirements

### Requirement: 暂存名排他创建且只清理本次产物

复制 MUST 先落到同目录的暂存名 `<原名>.dotlink-backup.<YYYYMMDDThhmmss>.<pid>.partial`, 且 MUST 以排他方式取得该名字: 已被占用时 (含被悬空链接占用) MUST 在 `.partial` 之前追加递增序号重试, MUST NOT 覆盖或合并既有文件, 目录或符号链接. 取得暂存根名之后 MUST 再递归填充其内容: 填充阶段出现的一切错误 (包括子项的 `AlreadyExists`) MUST 使该条目失败, MUST NOT 被当作暂存根名冲突而更换名字重试. 暂存根名冲突的尝试 MUST 有明确上限 (最多 16 个候选名): 达到上限时该条目 MUST 失败, 说明 MUST 给出已尝试次数与最后一个暂存名, MUST NOT 继续尝试. 因此失败清理只会作用于本次创建的暂存对象, 既有备份与用户文件 MUST 保持原样; 根名被占用时 MUST NOT 清理占用者. 暂存名只用于写入, MUST NOT 参与查找.

#### Scenario: 中断留下的暂存名

- **WHEN** 备份写入被中断, 目录里留下本次的中间名
- **THEN** 该名字形如 `<原名>.dotlink-backup.<时间戳>.<pid>.partial`, 且不参与"取最新"

#### Scenario: 暂存名被占用

- **WHEN** 首次算出的暂存名上已有用户文件
- **THEN** 使用下一个序号, 用户文件保持不变

#### Scenario: 失败只清理本次产物

- **WHEN** 复制中途失败
- **THEN** 只移除本次创建的暂存对象, 既有备份与用户文件保持原样

#### Scenario: 嵌套复制冲突不换名重试

- **WHEN** 暂存根已排他取得后, 递归复制某个子项时该子项的目标名创建返回 `AlreadyExists`
- **THEN** 该条目立即失败且不再生成新的暂存名, 说明指出失败的子项, 原对象与既有备份保持原样

#### Scenario: 暂存根名冲突重试有界

- **WHEN** 从第一个候选起的 16 个暂存根名全部被占用
- **THEN** 该条目失败, 说明给出已尝试次数 16, 不创建第 17 个候选, 占用物保持原样

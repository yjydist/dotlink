# Spec Delta

## ADDED Requirements

### Requirement: 备份发布前数据必须落盘

在把备份从暂存名发布为最终名之前, 系统 MUST 将复制产物同步到稳定存储: 普通文件 MUST 在 rename 前完成数据与元数据同步; 目录 MUST 在全部子项复制与元数据设置完成后同步目录自身; rename 后 MUST 同步父目录. 平台不支持同步时 MUST 按既有的 `Unsupported` 降级处理; 其余同步错误 MUST 使该条目失败, MUST NOT 发布最终名, MUST NOT 删除原对象.

#### Scenario: 普通文件备份先同步

- **WHEN** 一个普通文件被备份
- **THEN** 文件数据与元数据在 rename 到最终名前已同步, 最终名只在同步成功后出现

#### Scenario: 目录备份先同步

- **WHEN** 一个目录被递归备份
- **THEN** 全部子项与目录自身的同步在 rename 到最终名前完成

#### Scenario: 同步失败保留原对象

- **WHEN** 复制产物的同步失败
- **THEN** 该条目报告失败, 不发布最终名, 原对象保持原样

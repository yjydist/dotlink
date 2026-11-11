# Spec Delta

## MODIFIED Requirements

### Requirement: 各命令的目标谓词

条件一 MUST 按命令取下列唯一解释: `apply` 要求每个条目的目标路径都已是指向其源条目的正确链接; `undo` 要求每个条目的目标路径上都不存在本工具创建的链接, 且本次没有从备份恢复任何内容; `preview` 要求没有任何条目需要执行动作; `status` 要求清单与实际状态完全一致. `missing` 与 `fail` 不计入条件一 (见 `entry-classification` 的"目标谓词对特殊状态的计数基准"), 其残留待办只由条件二表达; `preview` 的"需要动作"按可消除的待办计, `conflict` 虽没有文件系统动作, 也代表本次未能达成期望, 因而计入条件一为假.

#### Scenario: 未部署清单上的预览

- **WHEN** 清单中的条目全部未部署, 执行 `preview`
- **THEN** 目标谓词为假, 退出码 1

#### Scenario: 空目标且有备份的撤销

- **WHEN** 目标路径本就不存在, 同目录有一份完整备份, 执行 `undo`
- **THEN** 备份被恢复, token 为 `restore`, 计入成功列, 但目标谓词为假, 退出码 1

#### Scenario: 空目标且无备份的撤销

- **WHEN** 目标路径本就不存在, 同目录没有备份, 执行 `undo`
- **THEN** token 为 `ok`, 目标谓词为真, 退出码 0

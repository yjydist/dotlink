# Spec Delta

## ADDED Requirements

### Requirement: 本工具的错误链接在部署时自愈

`apply` MUST 把 "指向错误" 视为本工具创建的链接, 无论 `on_conflict` 取值都替换为指向当前 `source` 的正确链接. 替换前 MUST 备份旧链接本身 (不跟随其目标), 备份命名沿用既有规则. 该行 MUST 报告 `replace` 并给出备份路径. `preview` MUST 报告同一动作与预测备份路径, 且不修改文件系统. `undo` 的语义不变.

#### Scenario: source 改名后 apply 自愈

- **WHEN** 目标路径是本工具创建的链接, 指向仓库内旧路径, 而清单的 `source` 已改为新路径, 且该条目使用默认 `on_conflict`
- **THEN** `apply` 成功, 旧链接被备份, 目标成为指向新 `source` 的链接, 该行 token 为 `replace`, 退出码按整体结果计算

#### Scenario: preview 预测自愈

- **WHEN** 同一状态执行 `preview`
- **THEN** 该行 token 为 `replace`, 说明给出预测备份路径, 且 `preview` 不修改文件系统

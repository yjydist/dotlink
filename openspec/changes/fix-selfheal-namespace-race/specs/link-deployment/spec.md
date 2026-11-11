# Spec Delta

## ADDED Requirements

### Requirement: 自愈备份前复核目标仍是本工具链接

在 `LinkedWrong` 自愈路径上, 写入自愈备份之前 MUST 重新 lstat 目标并复核: 目标仍是符号链接, 且其链接文本解析并规范化后仍落在仓库根内. 不满足时 MUST 使该条目失败, MUST NOT 写自愈备份, MUST NOT 删除或替换目标, MUST NOT 建立链接. 说明 MUST 指出"判定之后目标已被替换为非本工具链接或内容", 并提示重新运行. 复核 MUST 使用与 `classify` 和 `undo` 相同的 `link_destination` 与已规范化的仓库根.

#### Scenario: 目标被替换成普通文件

- **WHEN** 判定时目标是本工具的错误链接, 备份前被外部替换成普通文件
- **THEN** 该条目报告 `fail`, 普通文件保持原样, 不出现自愈备份, 不建立链接

#### Scenario: 目标被替换成仓库外链接

- **WHEN** 判定时目标是本工具的错误链接, 备份前被替换成指向仓库外的链接
- **THEN** 该条目报告 `fail`, 该链接保持原样, 不出现自愈备份, 不建立链接

#### Scenario: 目标仍是仓库内的错误链接

- **WHEN** 判定时目标是本工具的错误链接, 备份前仍是符号链接且目标仍在仓库根内
- **THEN** 自愈继续, 旧链接被备份到 `.dotlink-heal.`, 目标成为指向当前 `source` 的链接, token 为 `replace`

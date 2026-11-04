# Spec Delta

## MODIFIED Requirements

### Requirement: 报告行格式 [FR-016]

每条目 MUST 输出一行, 依次包含: `<序号>/<总数>`, token, 用户书写的原始目标字符串, 人类可读说明. 除这四项外 MUST NOT 输出其他列: 尤其不得增加"源条目"列 (它会让行需要折行, 而源条目在 `missing` 一类说明中已以文字给出) 或"动作措辞"列 (`status` 与 `undo` 只渲染现状, 不存在四命令共有的动作列). 目标列 MUST 展示清单中的原始写法而不是解析后的绝对路径, 以便用户逐行对照自己的清单. 控制字符 MUST 按"报告中的控制字符必须转义"规定的转义形式显示.

#### Scenario: 不增加额外列

- **WHEN** 检查任一条目行的字段
- **THEN** 只出现上述四项, 不出现源条目或动作措辞的独立列

#### Scenario: 展示原始写法

- **WHEN** 清单中某条目写 `target = "~/.bashrc"`
- **THEN** 该行展示 `~/.bashrc`, 而不是主目录展开后的绝对路径

## ADDED Requirements

### Requirement: 报告中的控制字符必须转义

报告行与末尾汇总 MUST NOT 包含未转义的控制字符 (C0, C1 与 DEL), 特别是换行, 回车, ESC 与 BEL. 动态字段 (target 展示, 说明文本, source, 实际指向, 备份路径与错误原因) 中的控制字符 MUST 以可读转义形式输出, 例如换行显示为 `\n`, ESC 显示为 `\u{1b}`. 转义 MUST 只影响显示, MUST NOT 改变清单解析, 路径解析, 文件系统操作或退出码. 每条目仍 MUST 恰占一行, 且转义后的 target 仍 MUST 可辨认出原始字符串.

#### Scenario: target 含换行

- **WHEN** 清单的 target 字符串包含换行, 执行 `preview`
- **THEN** 每个条目仍只占一行, 换行在 target 列显示为 `\n`, 汇总行仍是最后一行

#### Scenario: target 或 source 含 ESC/BEL

- **WHEN** 清单的 target 或 source 包含 ESC 或 BEL, 执行 `preview`
- **THEN** 输出不包含未转义的 ESC/BEL, 说明仍给出可读的原因或指向

#### Scenario: 文件系统返回的控制字符

- **WHEN** 实际指向或错误原因包含控制字符 (例如用户自建链接的目标名)
- **THEN** 说明中的该字段同样被转义, 不破坏单行输出

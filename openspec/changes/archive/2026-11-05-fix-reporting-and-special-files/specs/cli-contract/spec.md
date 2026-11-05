# Spec Delta

## MODIFIED Requirements

### Requirement: 退出码 2 的两重含义靠标准错误区分

清单校验失败与调用方式错误 (子命令缺失, 拼错或未知选项) MUST 共用码值 2. 二者的可区分性 MUST 由标准错误的内容承担: 前者是单行的 `dotlink: ...` 说明; 后者是同时含 `error:` 与 `Usage:` 的 clap 用法说明, 其具体错误短语可以是 `unrecognized subcommand` 或 `unexpected argument`, 判据 MUST NOT 绑定其中某一个短语. 调用方式错误 MUST 在读取清单之前被拒绝.

#### Scenario: 拼错的子命令

- **WHEN** 用户执行 `dotlink statu`
- **THEN** 标准错误是含 `error:` 与 `Usage:` 的用法说明, 退出码 2, 不读取清单

#### Scenario: 未知选项

- **WHEN** 用户执行 `dotlink status --json`
- **THEN** 标准错误是含 `error: unexpected argument` 与 `Usage:` 的用法说明, 退出码 2, 不读取清单

#### Scenario: 清单不存在

- **WHEN** 当前目录没有 `dotlink.toml`
- **THEN** 标准错误只有一行"无法读取清单"说明, 退出码 2

### Requirement: 检查类 token 只由状态决定 [FR-011]

`status` MUST 只渲染状态, MUST NOT 渲染动作: 未部署 -> `absent`; 已是最新 -> `ok`; 指向错误 -> `wrong`; 目标被普通文件, 目录, 特殊文件或别人的链接占用 -> `foreign`; 源缺失 -> `missing`; 无法判定 -> `fail`.

#### Scenario: 检查不提动作策略

- **WHEN** 一条允许覆盖的条目在 `status` 下目标被普通文件占用
- **THEN** token 为 `foreign`, 与不允许覆盖的条目一致

#### Scenario: 指向错误与占用物的分工

- **WHEN** 同一条目在 `status` 下分别是"本工具的链接指错"与"别人的链接"
- **THEN** 前者为 `wrong`, 后者为 `foreign`

#### Scenario: 特殊文件也归入 foreign

- **WHEN** 目标路径是 FIFO, socket 或设备文件, 执行 `status`
- **THEN** token 为 `foreign`, 说明给出精确的特殊文件类型, 且不给出任何动作承诺

### Requirement: 报告中的控制字符必须转义

报告行与末尾汇总 MUST NOT 包含未转义的控制字符 (C0, C1 与 DEL), 特别是换行, 回车, ESC 与 BEL. 动态字段 (target 展示, 说明文本, source, 实际指向, 备份路径与错误原因) 中的控制字符 MUST 以可读转义形式输出, 例如换行显示为 `\n`, ESC 显示为 `\u{1b}`. 除控制字符外, U+2028, U+2029, U+200B-200F, U+202A-202E, U+2060-2064, U+2066-2069 与 U+FEFF MUST 同样以 `\u{...}` 形式输出, 使 bidi 与零宽字符不能改变报告行的视觉顺序或折行. 转义 MUST 只影响显示, MUST NOT 改变清单解析, 路径解析, 文件系统操作或退出码. 每条目仍 MUST 恰占一行, 且转义后的 target 仍 MUST 可辨认出原始字符串.

#### Scenario: target 含换行

- **WHEN** 清单的 target 字符串包含换行, 执行 `preview`
- **THEN** 每个条目仍只占一行, 换行在 target 列显示为 `\n`, 汇总行仍是最后一行

#### Scenario: target 或 source 含 ESC/BEL

- **WHEN** 清单的 target 或 source 包含 ESC 或 BEL, 执行 `preview`
- **THEN** 输出不包含未转义的 ESC/BEL, 说明仍给出可读的原因或指向

#### Scenario: 文件系统返回的控制字符

- **WHEN** 实际指向或错误原因包含控制字符 (例如用户自建链接的目标名)
- **THEN** 说明中的该字段同样被转义, 不破坏单行输出

#### Scenario: target 含 U+2028 或 bidi 控制符

- **WHEN** 清单的 target 包含 U+2028, U+2029, U+202E 或 U+200B, 执行 `preview`
- **THEN** 这些字符分别显示为 `\u{2028}`, `\u{2029}`, `\u{202e}` 与 `\u{200b}`, 输出不含这些原始字符, 每个条目仍只占一行

### Requirement: 必现信息

输出 MUST 包含以下信息: 发生备份时给出备份文件的完整路径; 失败, 跳过, 冲突与源缺失给出原因; status 的 `wrong` 与 `foreign` 行在占用物是符号链接时给出实际指向, `foreign` 行同时给出占用物类型: 符号链接, 普通文件, 目录或特殊文件, 特殊文件 MUST 进一步给出 FIFO, socket, 字符设备, 块设备或其他类型; 撤销对上述四类占用物一律不移除, 该信息使用户能判断该路径是否需要自行处理. 失败原因 MUST 给出失败侧的解析路径: 源侧失败给源路径, 目标侧失败给目标路径, MUST NOT 以另一侧的路径代替. 实际指向 MUST 按解析结果原样给出, 首尾空白 MUST NOT 被裁剪. `preview` 与 `apply` 的 `replace` 行只要求给出"冲突占用"这一状态粒度, 是否细分占用类型不影响本要求.

#### Scenario: 备份位置

- **WHEN** 某条目备份后替换成功
- **THEN** 该行给出备份文件的完整路径

#### Scenario: 占用物类型

- **WHEN** 检查命令发现目标被占用
- **THEN** 说明指出是符号链接, 普通文件, 目录还是特殊文件; 特殊文件进一步指出 FIFO, socket, 字符设备, 块设备或其他类型

#### Scenario: 实际指向保留首尾空白

- **WHEN** 目标路径是指向某路径的符号链接, 而该路径的首部或尾部含有空白, 执行 `status`
- **THEN** 说明给出的实际指向保留解析结果的首尾空白, 其余字符按转义规则显示

#### Scenario: 源侧失败给出源路径

- **WHEN** 源路径的祖先目录权限不足, 而目标路径一切正常, 执行 `preview`
- **THEN** 失败说明给出源路径, 不只给出目标路径, 也不把目标路径当作失败侧

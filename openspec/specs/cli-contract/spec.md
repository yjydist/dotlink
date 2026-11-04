# cli-contract Specification

## Purpose

定义面向用户与脚本的公开命令行契约: 命令面, 平台门禁, 退出码判据, 报告行的 token 词汇与汇总分列, 以及必须出现在输出中的信息.

## Requirements

### Requirement: 命令面固定为四个子命令且不接受操作性参数

系统 MUST 只提供 `preview` / `apply` / `status` / `undo` 四个子命令, 且它们 MUST NOT 接受任何操作性参数; 仅 `-h` / `--help` 与 `-V` / `--version` 例外. 四条命令 MUST NOT 读取标准输入, MUST NOT 发出任何交互提示或询问. 系统 MUST NOT 提供指定仓库根或清单位置的选项与环境变量, 也 MUST NOT 提供全局覆盖开关, 机器可读输出开关或主目录覆盖选项: 配置面与覆盖许可都写在清单里, 避免出现与清单矛盾的第二个来源.

#### Scenario: 传入多余参数

- **WHEN** 用户执行 `dotlink status --json`
- **THEN** 该次调用被拒绝, 退出码 2, 不读取清单, 不修改文件系统

#### Scenario: 帮助与版本

- **WHEN** 用户执行 `dotlink -h` 或 `dotlink -V`
- **THEN** 输出打印到标准输出并退出 0, 属于正常路径

#### Scenario: 不读取标准输入

- **WHEN** 在标准输入保持打开且不提供任何内容的脚本中执行任一子命令
- **THEN** 命令不等待输入, 也不输出任何交互提示

### Requirement: 不维护持久化状态

系统 MUST NOT 创建或读取数据库, 状态文件或持久化索引; 每个条目的判定 MUST 只来自清单与文件系统实况. 该约束使用户可以删除仓库中的任何非源文件而不影响下一次运行, 也避免出现与文件系统实况矛盾的第二个来源.

#### Scenario: 命令不留下状态文件

- **WHEN** 依次执行四条命令
- **THEN** 仓库与主目录中只出现源条目, 链接与备份, 不出现状态文件或索引文件

### Requirement: 平台门禁先于一切其他步骤 [FR-019]

平台检查 MUST 是进程启动后的第一步, 早于参数解析与清单读取. 在不支持的平台上 MUST 立即以退出码 3 结束, 标准输出 MUST 为空, 标准错误 MUST 输出一行说明, 且 MUST NOT 触碰文件系统. 该门禁 MUST 是运行时拒绝, 使二进制在不受支持的平台上仍可构建并给出可读提示.

#### Scenario: 在不支持的平台上运行

- **WHEN** 在不支持的操作系统上执行任一子命令, 即使参数也非法
- **THEN** 标准输出为空, 标准错误只有一行平台说明, 退出码 3

#### Scenario: 平台检查与清单无关

- **WHEN** 在不支持的平台上运行于一个没有 `dotlink.toml` 的目录
- **THEN** 退出码仍是 3, 而不是清单错误 2

### Requirement: 退出码的两条件判据 [FR-020]

退出码 0 MUST 当且仅当两个条件同时成立: 该命令自身的期望状态已达成, 且全部条目都落在汇总的"成功"或"已是最新"两列 (即没有任何条目落在"未处理"或"失败"列). 判据 MUST 由"逐条结果 + 该命令的目标谓词"导出, 且四个命令 MUST 共用同一个导出函数; 不得实现成"执行了几个动作"或"有没有条目失败".

#### Scenario: 二次部署全部已是最新

- **WHEN** 一份清单已经部署完成, 再次执行 `apply`
- **THEN** 所有条目报告"已是最新", 退出码 0, 且不产生任何文件系统改动

#### Scenario: 跳过使退出码非零

- **WHEN** 撤销时某条目被真实目录占用, 因而不触碰
- **THEN** 即使目标谓词已成立, 退出码仍为 1

### Requirement: 各命令的目标谓词

条件一 MUST 按命令取下列唯一解释: `apply` 要求每个条目的目标路径都已是指向其源条目的正确链接; `undo` 要求每个条目的目标路径上都不存在本工具创建的链接; `preview` 要求没有任何条目需要执行动作; `status` 要求清单与实际状态完全一致. `missing` 与 `fail` 不计入条件一 (见 `entry-classification` 的"目标谓词对特殊状态的计数基准"), 其残留待办只由条件二表达; `preview` 的"需要动作"按可消除的待办计, `conflict` 虽没有文件系统动作, 也代表本次未能达成期望, 因而计入条件一为假.

#### Scenario: 未部署清单上的预览

- **WHEN** 清单中的条目全部未部署, 执行 `preview`
- **THEN** 目标谓词为假, 退出码 1

### Requirement: 只读与写入命令的求值时刻

`preview` 与 `apply` MUST 使用同一个目标谓词, 差别只在求值时刻: `preview` 在动手之前求值, `apply` 在动手之后求值. `undo` 的目标谓词与它们刻意相反, 且 MUST 在动手之前求值一次 -- 若在动手之后求值, 撤销后链接已不存在, 该谓词将恒为真, "刚部署好则退 1"与"撤销后为空则退 0"无法同时成立.

#### Scenario: 同一份未部署清单

- **WHEN** 依次执行 `preview` 与 `apply`
- **THEN** `preview` 退 1, `apply` 退 0

#### Scenario: 刚部署好就撤销

- **WHEN** 一份清单刚刚部署成功, 立即执行 `undo`
- **THEN** 退出码 1, 因为有本工具创建的链接待撤销

#### Scenario: 撤销后目标为空

- **WHEN** 撤销已完成且目标路径上没有备份可恢复, 再次执行 `undo`
- **THEN** 全部条目报告 `ok` 并计入汇总的"已是最新"列, 退出码 0

### Requirement: 退出码 2 的两重含义靠标准错误区分

清单校验失败与调用方式错误 (子命令缺失, 拼错或未知选项) MUST 共用码值 2. 二者的可区分性 MUST 由标准错误的内容承担: 前者是单行说明, 后者是含 `Usage:` 与解析错误的用法说明. 调用方式错误 MUST 在读取清单之前被拒绝.

#### Scenario: 拼错的子命令

- **WHEN** 用户执行 `dotlink statu`
- **THEN** 标准错误是用法说明并含 `unrecognized subcommand`, 退出码 2, 不读取清单

#### Scenario: 清单不存在

- **WHEN** 当前目录没有 `dotlink.toml`
- **THEN** 标准错误只有一行"无法读取清单"说明, 退出码 2

### Requirement: `--help` 文本自带退出码语义

每个子命令的 `--help` MUST 写明退出码的含义, 使用户仅凭命令行输出即可判断每个条目的结果与下一步动作. `--help` 文本 MUST 至少说明: 退出 1 可能表示有待办也可能包含跳过或失败; 退出码 2 的两重含义及其区分方式; 只读命令在有待办时同样退 1.

#### Scenario: 查看子命令帮助

- **WHEN** 用户执行 `dotlink preview --help`
- **THEN** 输出包含退出码 0 / 1 / 2 / 3 的含义说明

#### Scenario: 帮助提示脚本用法

- **WHEN** 用户查看任一子命令的 `--help`
- **THEN** 输出提示在 `set -e` 脚本中不应裸调只读命令, 并给出按退出码分支的示例

### Requirement: token 词汇固定为十二个

报告行 MUST 使用固定的 token 词汇, 且 MUST NOT 出现其他取值: `create`, `ok`, `replace`, `conflict`, `absent`, `missing`, `foreign`, `wrong`, `undo`, `restore`, `skip`, `fail`. token 的文本 MUST 稳定, 使脚本可依赖.

#### Scenario: token 集合封闭

- **WHEN** 遍历全部命令与全部状态的组合
- **THEN** 每个条目输出且仅输出上述十二个 token 中的一个

### Requirement: 部署类 token 由状态与动作共同决定

`preview` 与 `apply` MUST 使用同一张 token 表, 二者只在措辞上区分将来时与完成时: 未部署且动作是创建 -> `create`; 已是最新 -> `ok`; 冲突类且动作是备份后替换 -> `replace`; 冲突类且动作是拒绝冲突 -> `conflict`. 该表约束判定导出的动作预测; 动作在执行期失败时, `apply` 按"撤销类与失败 token 的适用面"把该行改写为 `fail`, 不属于表外 token.

#### Scenario: 预览与执行共用 token

- **WHEN** 同一份清单上依次执行 `preview` 与 `apply`
- **THEN** 两次输出的 token 序列完全相同 (动作执行失败的行除外: `apply` 按 `fail` 的适用面报告), 只有说明措辞不同

#### Scenario: 允许覆盖的冲突

- **WHEN** 目标被普通文件占用且该条目允许覆盖
- **THEN** token 为 `replace`

### Requirement: 检查类 token 只由状态决定 [FR-011]

`status` MUST 只渲染状态, MUST NOT 渲染动作: 未部署 -> `absent`; 已是最新 -> `ok`; 指向错误 -> `wrong`; 目标被普通文件, 目录或别人的链接占用 -> `foreign`; 源缺失 -> `missing`; 无法判定 -> `fail`.

#### Scenario: 检查不提动作策略

- **WHEN** 一条允许覆盖的条目在 `status` 下目标被普通文件占用
- **THEN** token 为 `foreign`, 与不允许覆盖的条目一致

#### Scenario: 指向错误与占用物的分工

- **WHEN** 同一条目在 `status` 下分别是"本工具的链接指错"与"别人的链接"
- **THEN** 前者为 `wrong`, 后者为 `foreign`

### Requirement: 撤销类与失败 token 的适用面

`undo`, `restore`, `skip` MUST 只出现在撤销命令中, 其细分条件由撤销语义定义. `missing` MUST 出现在全部四条命令中并一律表示源缺失. `fail` MUST 出现在全部四条命令中, 表示"无法判定"或"执行动作失败", 且 MUST 附带原因.

#### Scenario: 无法判定在所有命令下都是失败

- **WHEN** 某条目的目标路径读不到, 分别执行四条命令
- **THEN** 四次都报告 `fail`, 并给出底层原因

#### Scenario: 源缺失在所有命令下都是 missing

- **WHEN** 某条目的源不存在, 分别执行四条命令
- **THEN** 四次都报告 `missing`

### Requirement: 汇总分列为四列且是 token 的划分

末尾汇总 MUST 输出条目总数与四列计数: 成功 / 已是最新 / 未处理 / 失败. 四项 MUST 互斥且相加恒等于总条目数. 分列 MUST 是 token 的划分: 成功收 `create` / `replace` / `undo` / `restore`; 已是最新收 `ok`; 未处理收 `conflict` / `skip` / `foreign` / `absent` / `wrong`; 失败收 `missing` / `fail`.

#### Scenario: 四项之和等于总数

- **WHEN** 一份含四个条目的清单分别产生一个 `create`, 一个 `ok`, 一个 `conflict`, 一个 `missing`
- **THEN** 汇总行为"4 个条目: 1 成功, 1 已是最新, 1 未处理, 1 失败"

#### Scenario: 第三列不得命名为跳过

- **WHEN** 检查汇总行措辞
- **THEN** 第三列名为"未处理", 因为它同时收容没有动作可执行的 `foreign` / `absent` / `wrong`

### Requirement: 报告行格式 [FR-016]

每条目 MUST 输出一行, 依次包含: `<序号>/<总数>`, token, 用户书写的原始目标字符串, 人类可读说明. 除这四项外 MUST NOT 输出其他列: 尤其不得增加"源条目"列 (它会让行需要折行, 而源条目在 `missing` 一类说明中已以文字给出) 或"动作措辞"列 (`status` 与 `undo` 只渲染现状, 不存在四命令共有的动作列). 目标列 MUST 展示清单中的原始写法而不是解析后的绝对路径, 以便用户逐行对照自己的清单.

#### Scenario: 不增加额外列

- **WHEN** 检查任一条目行的字段
- **THEN** 只出现上述四项, 不出现源条目或动作措辞的独立列

#### Scenario: 展示原始写法

- **WHEN** 清单中某条目写 `target = "~/.bashrc"`
- **THEN** 该行展示 `~/.bashrc`, 而不是主目录展开后的绝对路径

### Requirement: 必现信息

输出 MUST 包含以下信息: 发生备份时给出备份文件的完整路径; 失败, 跳过, 冲突与源缺失给出原因; status 的 `wrong` 与 `foreign` 行在占用物是符号链接时给出实际指向, `foreign` 行同时给出占用物类型 (符号链接 / 普通文件 / 目录): 撤销对这三类占用物一律不移除, 该信息使用户能判断该路径是否需要自行处理. `preview` 与 `apply` 的 `replace` 行只要求给出"冲突占用"这一状态粒度, 是否细分占用类型不影响本要求.

#### Scenario: 备份位置

- **WHEN** 某条目备份后替换成功
- **THEN** 该行给出备份文件的完整路径

#### Scenario: 占用物类型

- **WHEN** 检查命令发现目标被占用
- **THEN** 说明指出是符号链接, 普通文件还是目录

### Requirement: 报告写出失败不改变命令结局

报告写入标准输出失败时, 系统 MUST NOT panic, MUST NOT 以 101 等契约外退出码结束. 报告写出失败 MUST NOT 参与现有的 0/1 两条件判据: 退出码仍只由命令的目标谓词与逐条结果决定. 标准输出返回 `BrokenPipe` 时 MUST 静默结束, MUST NOT 向标准错误输出诊断; 其他写错误 MUST 尽力向标准错误输出一行诊断. 标准错误不可写时 MUST 静默忽略, 退出码仍按该情形原本的结局计算.

#### Scenario: 标准输出被提前关闭

- **WHEN** 通过管道运行任一子命令, 消费方在报告写完之前关闭标准输出
- **THEN** 进程不 panic, 标准错误不出现诊断, 退出码仍按该命令的目标谓词与逐条结果计算

#### Scenario: 标准输出出现其他写错误

- **WHEN** 报告写入标准输出时返回 `BrokenPipe` 之外的错误
- **THEN** 进程不 panic, 标准错误输出一行说明写失败, 退出码不变

#### Scenario: 标准错误不可写

- **WHEN** 需要写标准错误时标准错误已关闭
- **THEN** 进程不 panic, 退出码仍按该情形原本的 0/1/2/3 结局计算

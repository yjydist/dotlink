# Design

## Context

本变更的两个主题都围绕"报告是否如实描述将要发生的事", 因此边界相互牵连.

特殊文件一侧的现状:

- `src/classify.rs:119-131` 把非符号链接, 非目录的目标统一判为 `ForeignFile`, 说明写"目标被普通文件占用". FIFO, socket 与设备文件因此被描述成普通文件.
- `src/backup.rs:291-296` 明确拒绝复制特殊文件. 目标是特殊文件时, `preview` 仍按 `ForeignFile` 与 `on_conflict` 承诺 `replace`, `apply` 却在备份阶段失败.
- 目录递归备份同样会在子项上失败. 实测目标目录内存在 `S.gpg-agent` 形态的 socket 时, 整个目录备份必然失败, 而 `preview` 仍承诺 `replace`. 这意味着 `~/.gnupg` 这类目录在当前实现下无法被 `replace_and_backup` 替换.
- `openspec/specs/entry-classification/spec.md` 的 8 个状态没有特殊文件; `openspec/specs/cli-contract/spec.md` 的"必现信息"只列符号链接, 普通文件与目录三种占用物.

报告一侧的现状:

- `src/render.rs:189-199` 直接输出 `{}/{}`, 条目数达到 10 时前 9 行的 token 列左移一格; target 用 `{:<width$}` 按 `char` 数补齐, CJK 每个字符少算 1 列.
- `src/render.rs:456-463` 的 `non_empty` 返回 trim 后的文本, `wrong` / `foreign` / `skip` 行的实际指向因此丢失首尾空格.
- `src/classify.rs:83-95` 的源侧故障只写"源侧读取失败: {error}", 而 `src/render.rs:446-452` 固定拼接目标的原始字符串, 输出只见到无关的 target.
- `src/cli.rs:23-41` 用 Rust 字符串续行写 help, 反斜杠后的换行会剥掉下一行的前导空白, `--help` 的 if/code/fi 与退出码条目全部顶格. README 的退出码 2 判据只提 `unrecognized subcommand`, 未知选项的标准错误实际是 `error: unexpected argument` 加 `Usage:`, 不含该短语.
- `src/render.rs:42-52` 只转义 C0, C1 与 DEL; U+2028, U+2029 与常见 bidi / 零宽字符仍可改变视觉顺序或折行.
- `src/render.rs:359-364` 的 `replace` 行括号放 `entry.source`, `src/render.rs:386-389` 的 `conflict` 行同位置放占用物描述, 读者无法用同一约定理解两行.

约束: 只支持 macOS 与 Linux, 但代码仍需在非 Unix 目标上编译; 无新增依赖; token 集合与退出码值不变; 报告仍是四列.

## Goals / Non-Goals

**Goals:**

- 为特殊文件建立独立状态与精确类型, 使 `status` / `preview` / `apply` / `undo` 对它的结论一致且可预测.
- 保证任何"无法完整备份"的对象都不会被删除或覆盖, `preview` 不再承诺做不到的 `replace`.
- 让目录内特殊文件的失败在 `apply` 中可诊断: 给出特殊文件路径与下一步动作, 且原目录完整保留.
- 让 10 条以上与 CJK target 的报告列保持对齐, 并让实际指向逐字保真.
- 让失败说明按侧别给出路径, 让退出码 2 的帮助文本与真实 clap 输出一致, 恢复 help 缩进.
- 扩大显示层转义集合, 关闭 U+2028 / U+2029 与常见 bidi / 零宽字符的视觉欺骗面.
- 统一同一行内括号的语义, 并把无法在代码层修复的边界写入 README 已知限制.

**Non-Goals:**

- 不改变 token 集合 (仍为 12 个), 不改变退出码判据与码值, 不改变报告的四列结构.
- 不为 `preview` 增加目录内容的递归预检; 目录内特殊文件仍由备份层在 `apply` 时发现.
- 不改变 `undo` 对特殊文件的既有 `skip` 语义, 不尝试移除, 备份或恢复特殊文件.
- 不把特殊文件转成普通文件, 不截断 FIFO 数据, 不打开设备节点.
- 不引入 `unicode-width` 或其它依赖, 不实现完整的 Unicode 字素簇或 East Asian Ambiguous 宽度表.
- 不修复恢复失败说明缺少备份路径的问题, 那属于另一个变更.
- 不改变非 UTF-8 路径的 lossy 显示行为, 只在 README 记录.
- 不修改 `manifest-schema`, `path-normalization`, `link-teardown`, `backup-lifecycle` 的 delta.

## Decisions

### D1. 第九状态用 `Current::ForeignOther`, 类型标签由 classify 写入 detail

在 `Current` 增加 `ForeignOther`. `classify_entry` 的非符号链接分支改成三路: `is_dir()` -> `ForeignDir`; `is_file()` -> `ForeignFile`; 其余 -> `ForeignOther`. 求值顺序保持"两侧环境故障 -> 源缺失 -> 未部署 -> 符号链接归属 -> 占用物三路"不变.

精确类型由 `std::os::unix::fs::FileTypeExt` 判定, 依次检查 `is_fifo`, `is_socket`, `is_char_device`, `is_block_device`, 在 `detail` 中写入 `FIFO`, `socket`, `字符设备`, `块设备`; 全部不命中写 `其他特殊文件`. `render` 的 `occupied` 统一加 `"特殊文件: "` 前缀, 因此 `status` 输出例如 `被非本工具创建的内容占用 (特殊文件: FIFO)`, 不需要在多个分支重复类型判断. 非 Unix 目标使用 `#[cfg(unix)]` 与 fallback `特殊文件 (类型未知)`, 使 Windows 目标仍可编译, 但运行时由平台门禁拒绝.

备选: 复用 `ForeignFile` 并在 `detail` 里写特殊类型. 否决理由是 `status` 无法只靠 `Current` 区分特殊文件, `undo` 与动作映射会被迫读取自由文本, 编译器也无法强制每个 `match` 处理新增语义.

备选: 在判定阶段递归扫描目录内容. 否决理由见 D3: 代价与竞态都不可接受.

### D2. 特殊文件固定拒绝冲突, 复用 `foreign` / `conflict` / `skip`

requirement 标题按 MODIFIED 规则逐字保留, 尽管正文的冲突类状态由四种扩为五种. `action_for` 为 `ForeignOther` 增加一条固定分支 `RefuseConflict`, 不读取 `on_conflict`. `render` 的三张 token 表分别映射:

- `status`: `ForeignOther` -> `foreign` (只渲染状态).
- `preview` / `apply`: `ForeignOther` -> `conflict`. 即使未来某个调用方错误地构造出 `ReplaceAndBackup`, token 也保持 `conflict` 而不是 `replace`, 使报告不会承诺无法完成的备份.
- `undo`: `ForeignOther` -> `skip`; `undo_entry` 继续走"非本工具创建的内容不触碰"的分支.

说明文本: 特殊文件的 `conflict` 行写 `冲突占用 (特殊文件: FIFO; 特殊文件不支持备份, 不会覆盖)`, 不再展示与该状态无关的 `on_conflict`; 特殊文件的 `skip` 行沿用 `occupied` 的描述. `status` 的 `foreign` 行使用同一 `occupied` 结果. token 集合不变, 汇总分列也不变: `conflict` 与 `foreign` 归"未处理", `skip` 归"未处理", 退出码仍由同一个 `exit_code_for` 导出.

备选: 新增 `special` 或 `unbackupable` token. 否决理由是 12 个 token 是公开契约, 增 token 会破坏脚本与现有测试, 而现有 `foreign` / `conflict` / `skip` 的语义已能准确表达.

备选: `replace_and_backup` 时仍让特殊文件走 `replace`. 否决理由是备份层无法复制特殊文件, 承诺必然落空, 正是本次要修复的缺陷.

备选: 特殊文件统一报 `fail`. 否决理由是它是确定可判定的状态, 不是环境故障; `fail` 表示"无法判定"或"执行动作失败", 会误导用户.

### D3. 目录内特殊文件由备份层整体失败, 预览不做递归预检

`backup::copy_object` 已经在遇到非文件, 非目录, 非符号链接的对象时返回 `Unsupported`. 保留"整体失败"的行为, 不改成跳过: 备份的产品是原对象的完整副本, 跳过任何子项后仍删除原目录会把未备份内容变成数据丢失.

强化错误文本: 特殊文件分支的错误写成 `无法复制特殊文件 <完整路径>; 请先手工移走该特殊文件再重试`. 该错误沿目录递归上抛, `finish_copy` 清理暂存对象, 不发布最终名, 原目录保持原样; `apply` 的 `fail_row` 原样携带该说明, 用户因此拿到具体路径与下一步动作.

`preview` 只消费 `classify` 的单次 `lstat`, 不知道目录内部有什么: 目标目录仍判为 `ForeignDir`, `on_conflict = replace_and_backup` 时仍预测 `replace` 与备份路径. 这是刻意的取舍, 不增加递归预检. 用户手移走特殊文件后重跑 `apply` 即可, 备份失败是安全失败.

备选: `preview` 递归遍历目录并预先报冲突. 否决理由是代价与目录树大小成正比, 预览应保持廉价; 遍历与执行之间仍有竞态, 预检通过也不能保证备份成功; 而且会让 `classify` 的结果依赖目录内容, 破坏"同一份判定结果被四条命令共享"的简单性.

备选: 备份时跳过特殊文件, 只复制其余内容. 否决理由是不完整备份加上删除原对象会造成不可恢复的数据丢失, 违反 `backup-fidelity` 的完整性前提.

备选: 把含特殊文件的目录直接判为 `ForeignOther`. 否决理由是判定需要递归, 且会让所有目录条目的语义依赖内容类型, 超出本变更范围.

### D4. 报告列: 序号按位数右对齐, target 按显示宽度补齐

序号: 取 `width = total.to_string().len()`, 用 `{:>width$}/{total}` 输出. 总数 12 时得到 ` 1/12` 到 `12/12`, token 列在所有行都从同一个显示列开始. 总数小于 10 时宽度仍为 1, 现有 README 的两条 ASCII 示例逐字不变.

target: 先转义, 再按显示宽度补齐. 新增 `display_width(&str) -> usize`, 只依赖字符区间:

- 宽度 0: 组合记号 U+0300-036F, U+1AB0-1AFF, U+1DC0-1DFF, U+20D0-20FF, U+FE20-FE2F; 变体选择符 U+FE00-FE0F; 以及已在显示层转义的 U+200B-200F, U+202A-202E, U+2060-2064, U+2066-2069, U+FEFF.
- 宽度 2: U+1100-115F, U+2E80-303E, U+3041-33FF, U+3400-4DBF, U+4E00-9FFF, U+A000-A4CF, U+AC00-D7A3, U+F900-FAFF, U+FE10-FE19, U+FE30-FE6F, U+FF00-FF60, U+FFE0-FFE6, U+1F300-1F64F, U+1F900-1F9FF, U+20000-3FFFD.
- 其余宽度 1.

补齐时计算 `pad = TARGET_WIDTH - 1 - display_width(target_display)`; `pad` 为正时补空格, 为 0 或负数时只保留一列分隔空格. 长 target 不截断: 报告必须展示原始写法, 截断会破坏对照; 其代价是说明列会右移, 作为已知行为保留.

近似之处必须在注释中写清: 不按字素簇分段, emoji ZWJ 序列与区域指示符按码点近似; East Asian Ambiguous 按 1 列; 未分配码点按 1 列; 转义后的 `\u{...}` 文本按字面字符计宽. 这些近似不影响文件系统操作, 只影响列对齐.

备选: 引入 `unicode-width`. 否决理由是本仓库不新增依赖, 且该 crate 仍需处理版本与终端差异; 本次只要求 CJK 与 10 条以上场景.

备选: 继续用 `{:<width$}` 按字符数补齐. 否决理由是 CJK 每字少算 1 列, 已实测说明列错开.

备选: 按终端宽度截断或查询 `ioctl`. 否决理由是输出可能被管道接走, 结果不确定, 且截断违反原始 target 的展示要求.

### D5. detail 只用 trim 判断是否为空, 非空时原样返回

`non_empty` 改为: 若 `detail.trim().is_empty()` 返回 `None`, 否则返回 `Some(detail)`. 所有调用点 (`actual_target`, `occupied`, `missing_detail`, `unreadable_detail`, `fail_note`) 因此保留首尾空白; 控制字符仍由 `render` 最后统一转义, 所以首尾空白不会破坏单行结构.

备选: 只对"实际指向"保留原始 detail, 其他字段继续 trim. 否决理由是需要两套助手, 且 `missing` / `fail` 的路径同样可能有意义空格, 会留下同类缺陷.

备选: 只去掉换行, 保留空格. 否决理由是换行已经由转义层处理, trim 本来只用于判空, 不应改变内容.

### D6. classify 的 Unreadable detail 自带侧别与路径

`classify_entry` 的 `Unreadable` 分支改为构造带侧别和路径的说明:

- 仅源侧: `源侧读取失败: 路径 <source_path>: <error>`.
- 仅目标侧: `目标侧读取失败: 路径 <target_path>: <error>`.
- 双侧: `源侧与目标侧读取失败: 源侧路径 <source_path>: <error>; 目标侧路径 <target_path>: <error>`.
- 目标侧链接读取失败: `目标侧读取链接失败: 路径 <target_path>: <error>`.

`render` 的 `unreadable_detail` 不再拼接 `条目 target = ...`, 只输出 `无法判定: <detail>`; `detail` 为空时回退为 `判定阶段读取失败`. 这样源侧故障一定出现源路径, 目标侧故障一定出现目标路径, 两侧同时故障时两条路径都在.

备选: 给 `EntryState` 增加 `Side` 字段并在 render 中分支. 否决理由是这会把纯显示信息提升为共享结构, 需要改所有构造点与测试; detail 本来就是面向说明的自由文本.

备选: 在 render 中根据 detail 前缀猜测侧别. 否决理由是字符串前缀不是稳定接口, 容易再次错配.

### D7. 退出码 2 只改判据文字, 不改码值或运行时分支

运行时已经是正确的: 参数解析先于清单读取, clap 对缺失, 拼错与未知选项写 `error: ...` 与 `Usage:`, 清单错误写单行 `dotlink: ...`. 本次只同步契约文字, README 与 help:

- 契约写"标准错误同时含 `error:` 与 `Usage:` 的 clap 用法说明, 或单行 `dotlink: ...` 说明".
- README 的判据句不再要求 `unrecognized subcommand`, 增加 `dotlink status --json` 的 `unexpected argument` 示例.
- `cli.rs` 的根帮助与四条命令的共用段落写明"子命令缺失, 拼错或未知选项"都属调用方式错误.

不增加代码内的错误分类, 因为退出码 2 的两个来源本来就由"clap 先退出"与"清单载入失败"在进程层面分开, 再加一层分类只会制造第二个来源.

备选: 继续以 `unrecognized subcommand` 作为判据. 否决理由是未知选项不含该短语, 已实测会把用法错误误读成清单错误.

备选: 只以 `error:` 为判据. 否决理由是 `Usage:` 是 clap 用法说明的稳定组成部分, 两者同时要求更不容易误判; 单行清单说明以 `dotlink:` 开头, 不会命中.

### D8. help 文本用 `concat!` 逐行写出, 不再用续行反斜杠

`long_about!` 与根 `long_about` 改成 `concat!` 加多个独立字符串字面量, 每行把缩进写在引号内, 例如 `"  0  该命令的目标已完全达成, 且无遗留待办\n"` 与 `"    code=$?; case \"$code\" in 1) dotlink apply ;; *) exit \"$code\" ;; esac\n"`. 这样编译器不会剥掉下一行的前导空白.

`about` 仍只服务 `-h`, `long_about` 仍只服务 `--help`; `-h` 是否包含退出码语义维持现状, 不扩大. 修正 `cli.rs` 模块注释中"两处都写"的暗示, 明确 `-h` 只显示短描述, 退出码语义只承诺在 `--help` (规格也只要求 `--help`).

备选: 用 raw string 保留缩进. 否决理由是源码自身的缩进会进入字符串, 除非把内容顶格书写, 可读性差; 宏还要拼接 `$head`, `concat!` 更直接.

备选: 接受顶格输出. 否决理由是 `if/code/fi` 与退出码条目失去结构, 用户难以扫读, 且与 help 的预期格式不符.

### D9. 转义集合用显式区间谓词, 统一走 `escape_default`

`escape_controls` 的判定改为 `character.is_control() || is_layout_control(character)`. `is_layout_control` 覆盖:

- U+2028, U+2029.
- U+200B-200F, U+202A-202E, U+2060-2064, U+2066-2069, U+FEFF.

命中后继续用 `char::escape_default`, 对非 ASCII 字符输出 `\u{2028}` 形式的小写十六进制. 这在显示层完成, 不改变清单, 路径, 链接或退出码. 转义后的文本再参与 D4 的宽度计算, 因此 `\u{...}` 按字面宽度对齐.

README 的已知限制改成准确措辞: 已覆盖上述集合; 其他格式字符 (例如 U+00AD, U+061C 与标签字符) 不在集合内, 仍按原样显示. 不声称覆盖全部 Cf, 避免制造虚假保证.

备选: 用 Unicode 通用类别 Cf 的全表. 否决理由是标准库没有类别 API, 引入全表会显著增加代码与维护成本, 而本次只要求列出的常见区间.

备选: 把可疑字符替换为 U+FFFD 或直接删除. 否决理由是转义应可逆且保留原始字符串的可辨认性, 删除会让 target 与清单不再可对照.

### D10. `replace` 与 `conflict` 的括号统一描述占用物, README 补边界

`replace` 行的括号改为描述被替换的占用物:

- `ForeignFile` -> `冲突占用 (普通文件)`; `ForeignDir` -> `冲突占用 (目录)`; `ForeignOther` -> `冲突占用 (特殊文件: FIFO)`.
- `LinkedWrong` 保持 `本工具的链接指向错误 (实际指向: <实际指向>)`, 因为这里的占用物是一个指错的链接, 描述它比描述 source 更有信息量.

`conflict` 行沿用同一 `occupied` 结果; 特殊文件额外写"特殊文件不支持备份, 不会覆盖", 其他占用物继续写 `on_conflict` 取值. 不在括号里再放 `entry.source`: 报告只有四列, source 已在清单中, 且"被什么占用"才是冲突行需要回答的问题.

README 的已知限制补四条: 非 UTF-8 路径的 lossy 显示; `undo` 不清理 `apply` 创建的父目录; 目标同目录的备份与暂存命名空间由本工具保留; 备份开始后对目标的并发原地修改不会进入该备份. 恢复失败说明缺少备份路径明确留给另一个变更, 本变更不碰.

备选: `replace` 与 `conflict` 都放 source. 否决理由是冲突行会失去占用物类型, 无法满足"必现信息", 用户也无法判断是否需要自行处理占用物.

备选: 增加"源条目"列. 否决理由是与四列契约冲突, 且会让行折行.

## Risks / Trade-offs

- **目录内特殊文件的 preview 仍可能承诺 replace** -> 这是不递归预检的直接代价. 备份整体失败且原目录保留, `apply` 给出特殊文件路径与"先手工移走再重试", README 记录该限制.
- **显示宽度是近似值** -> East Asian Ambiguous, emoji ZWJ 序列与未分配码点可能与真实终端差 1 列. 已覆盖常见 CJK 与 10 条以上场景, 近似规则写进注释; 不追求终端级精确.
- **设备节点的分支缺少端到端测试** -> 在无特权测试环境中创建字符/块设备不可移植. 用 FIFO 与 UnixListener 覆盖真实路径, 设备标签由类型判定分支的单元测试或代码审查覆盖, 并在风险中注明.
- **转义扩大后输出与旧版本不再逐字相同** -> 含 U+2028 或 bidi 字符的 target 现在显示为 `\u{...}`. 这是刻意的安全收紧; token 与退出码不变, 脚本不应依赖这些字符的原始显示.
- **新增 `ForeignOther` 迫使所有匹配点更新** -> 编译器的穷尽性检查会暴露遗漏, 但测试中的手工状态数组不会; 任务要求逐处更新并用 `cargo test --locked` 全量验证.
- **help 文本的完整措辞易碎** -> 测试只断言稳定标记 (`error:`, `Usage:`, 缩进后的 `0`/`if`/`fi`) 而不钉全文, 降低后续改写的维护成本.
- **`non_empty` 保留首尾空白会让说明出现视觉空隙** -> 这是保真要求; 路径中的首尾空格本来就有意义, 显示层不应擅自删除.
- **README 的备份命名空间说明可能不完整** -> 只描述 `<原名>.dotlink-backup.` 与三类 `.partial` 暂存名, 不承诺覆盖所有内部临时名; 用户不应占用这些前缀.
- **退出码 2 的文档判据仍依赖 clap 输出** -> 用 `statu` 与 `status --json` 两个端到端用例锁定, 并在 `--help` 中写明两种例子, 避免未来 clap 升级后文字漂移.

## Migration Plan

无数据迁移, 无清单格式变化, 备份文件与备份命名规则不变. 行为变化只发生在特殊文件目标上: 以前 `preview` 可能承诺 `replace` 而 `apply` 失败, 现在 `preview` 与 `apply` 都报 `conflict`; 要把链接部署到该路径, 用户必须先手工移走特殊文件. 目录内特殊文件仍走原来的备份路径, 只是失败说明更具体.

回滚时删除 `ForeignOther` 与相应匹配臂, 恢复 `render` 的列格式与文案, 恢复 `cli.rs` 的 help 文本与 README 段落即可; 已存在的链接与备份不受影响. 本变更不写任何持久化状态.

## Open Questions

- 是否在后续变更中把 `preview` 的目录预检做成可选的诊断动作? 当前不做, 以免把只读命令变重.
- 是否把转义集合扩展到完整的 Unicode Cf 类别? 当前只覆盖列出的区间, 其余格式字符在 README 记录.
- 是否在未来的报告格式中给 `replace` 行补 source 信息? 当前受四列契约限制, 不增加列.

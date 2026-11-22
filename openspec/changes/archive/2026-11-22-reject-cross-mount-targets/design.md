# Design

## Context

动机与复现结果见 `proposal.md`. 当前生产路径是:

- 备份: `replace_and_backup` -> `backup::write_backup` -> `copy_to_staging` -> `copy_object` 递归复制 (`src/backup/copy.rs:193`).
- 替换: 备份发布后 `stage_and_verify`; 目录暂存对象由 `remove_staged` 调用 `fs::remove_dir_all` 删除 (`src/apply.rs:336`).
- 清理: `copy_to_staging` 失败后由 `remove_any` 清理本次暂存树, `make_tree_writable` 递归恢复目录权限 (`src/backup/copy.rs:362`, `src/backup/copy.rs:380`).
- 恢复: `restore_backup` 复用 `copy_to_staging` (`src/backup/copy.rs:99`), 因此新的复制边界同样作用于撤销恢复.

`copy_object` 每个条目只做一次 `symlink_metadata`, 没有任何设备或挂载标识比较; `remove_staged` 直接使用 `remove_dir_all`, 会递归进入目录挂载点. 现有 `ObjectIdentity` (设备号, inode, 对象类型) 服务于替换前的身份复核, 不承担挂载边界判定.

约束: 无新增依赖, 无 `unsafe`, 不写状态文件; `rustix` 1.1 的 `fs` 特性已可用, Linux 可用 `statx`; macOS 没有等价的 `statx`; 非 Unix 平台只要求可编译, 运行时由平台门禁拒绝.

## Goals / Non-Goals

**Goals:**

- 在复制与删除之前识别目标自身及其后代中的挂载点, 使该条目失败, 且不发布最终备份名, 不删除或修改原对象.
- 覆盖备份, 恢复, 复制失败清理与目录替换的删除阶段.
- Linux 识别同一设备上的 bind mount; macOS 至少识别设备号差异; 平台能力不足时明确记录漏检边界.
- 保持 token 词汇, 退出码取值, 备份命名, 查找, 身份复核顺序与报告行格式不变.
- 提供可在无特权 CI 中跳过的端到端测试, 并用线程局部接缝覆盖删除与清理分支.

**Non-Goals:**

- 不在清单校验阶段拒绝挂载路径; 挂载是运行时状态, 清单只描述目标路径.
- 不让 `preview` 递归扫描目录内容; 它仍可能预测 `replace`, 由 `apply` 在动作阶段失败.
- 不识别 btrfs subvolume 等非挂载边界的文件系统子结构.
- 不自动卸载, 移动或修复挂载点; 用户必须先卸载或移走.
- 不改变现有 `Unsupported` 元数据降级, 备份同步, 暂存名上限或 token/退出码契约.

## Decisions

### D1. 新增挂载身份原语, 而不是复用 `ObjectIdentity`

在 `src/os` 增加 `MountIdentity` 与 `mount_identity(path, metadata)`, 至少保存设备号, Linux 额外保存 `statx` 的挂载标识. 比较规则: 两侧都取得挂载标识时比较标识; 否则比较设备号. Linux 使用 `rustix::fs::statx` 与 `StatxFlags::MNT_ID`, 仅在 `stx_mask` 含 `MNT_ID` 时采用 `stx_mnt_id`; `statx` 失败或内核不支持时回退设备号. macOS 只使用 `dev()`, 非 Unix 提供可编译 stub.

理由: bind mount 与父目录共享设备号, 只比较 `st_dev` 无法识别; 挂载标识是同一设备上区分 bind mount 的最小信息. 复用 `ObjectIdentity` 会把替换前身份复核与挂载边界判定混成一个概念, 二者生命周期和失败语义不同.

被否决的备选:

- 只比较 `st_dev`: 会漏掉 bind mount, 而 bind mount 正是把源仓库删掉的复现路径.
- 解析 `/proc/self/mountinfo`: 能覆盖旧内核, 但要处理 `\040` 等转义, `/proc` 可能不可用, 且只适用于 Linux; 留作未来兼容性增强, 不进入本次实现.
- `openat2` 的 `RESOLVE_NO_XDEV`: Linux-only, 需要重写复制与删除遍历, 且会把合法的非根目录挂载目标一律拒绝; 不作为主方案.
- 新增 crate 或直接使用 `libc`: 违反不新增依赖与禁止 `unsafe` 的约束.

### D2. 复制阶段在进入每个对象前判定边界

`copy_to_staging` 为顶层源计算一次父目录挂载身份, 并把它传给 `copy_object`. `copy_object` 对普通文件与目录先计算自身挂载身份, 与传入的父挂载身份比较; 不同则返回 `CopyFailure::Fatal`, 在任何目标对象创建或内容读取之前失败. 目录通过比较后, 以自身挂载身份递归 `copy_child`. 符号链接只复制链接文本, 不参与边界判定, 因为链接不是挂载点且不会被跟随.

顶层源没有父目录 (`/`) 时跳过比较; 目标 `/` 已由 V-9 拒绝, 该分支不可达. 顶层源自身是挂载点时, 与父目录比较即可在创建暂存对象前失败.

`CopyFailure::Fatal` 不触发暂存名换名重试, 因此不会重复复制; 失败后 `finish_copy` 清理本次暂存对象, 原对象保持原样, 不发布最终名. 说明使用单行 `io::Error::other`, 包含挂载点完整路径与"先卸载或移走"的提示.

同时检查普通文件: 文件挂载点 (bind-mounted file) 若被复制, 后续删除会因 `EBUSY` 失败并可能留下部分删除的兄弟项; 提前失败可以保持目标完整. 代价是 Linux 上每个条目多一次 `statx`; 备份本身是 I/O 密集操作, 该成本可接受, 在 50 条目性能用例中观察.

### D3. 删除阶段先复核, 再使用不跨边界的递归删除

`remove_staged` 对目录暂存对象:

1. 先调用 `find_mount_boundary(staged)` 扫描暂存树; 使用与复制相同的挂载身份判定. 发现边界时不删除任何对象, 尝试 `rename_noreplace(staged, target)` 把暂存目录移回原路径, 报告 `fail` 并给出挂载点路径; 移回失败时报告暂存目录完整路径.
2. 未发现边界时调用 `remove_tree(staged)` 递归删除. 该函数在进入每个子目录或删除每个子项前重新比较挂载身份; 发现边界立即返回 `MountBoundary`, 不再进入该挂载点. 如果边界是在扫描之后才出现, 可能已删除部分兄弟项; 此时保留暂存目录, 报告挂载点与暂存路径, 不回滚部分删除的树. 静态边界由第 1 步在删除前处理.

被否决的备选:

- 直接保留 `remove_dir_all` 并在外层预扫描: 扫描与删除之间的竞态仍可能跨边界; 且无法在删除过程中发现新挂载.
- 遇到边界就跳过挂载点继续删除其余内容: 会让目标树部分保留, 且无法确定挂载点之后是否还有用户数据; 与"失败时保持原状"冲突.
- 备份后把原目录整体改名到备份名 (move 而不是 copy): 会改变备份语义, 把挂载点及其活动内容带进备份, 且与原对象的生命周期绑定; 否决.

`remove_staged` 对普通文件与符号链接保持现有逻辑; 文件挂载点已由复制阶段拒绝, 且 `remove_file` 对挂载点返回 `EBUSY`, 不会删除挂载内容.

### D4. 清理路径复用同一删除原语

`remove_any` 用于清理本次复制产生的暂存树; 它改用 `remove_tree`, 遇到挂载边界时停止清理并保留残渣, 不把"清理失败"包装成新的用户可见错误. `make_tree_writable` 在挂载边界前停止, 不 chmod 挂载点之后的内容. 这样失败清理不会成为第二条跨挂载边界的删除路径.

理由: 清理对象通常不含挂载点, 但并发挂载或外部修改仍可能让它出现边界; 复用同一原语比在清理路径单独写一套判断更不容易漂移.

### D5. 平台降级与已知漏检

- Linux: 设备号加 `statx` 挂载标识; 内核不支持 `STATX_MNT_ID` 或 `statx` 返回 `NOSYS` 时只比较设备号.
- macOS: 只比较设备号; 同设备 bind mount 或 nullfs 的漏检写入 README 与规格的已知限制.
- 非 Unix: `mount_identity` 返回不可用, 复制与删除路径不会运行, 平台门禁先退 3.

不把"无法取得挂载标识"变成备份失败的通用理由: 设备号仍然可用, 对大多数挂载足够; 把平台能力差异写成文档与规格边界, 而不是在用户路径上制造新的拒绝面.

### D6. 测试接缝与端到端策略

- `MountIdentity` 的比较与回退用单元测试覆盖, 不依赖真实挂载.
- 在 `src/os` 增加线程局部测试接缝, 让下一次目录挂载身份比较返回模拟的"不同挂载"; 用它覆盖 `remove_staged` 的回滚分支, `remove_tree` 的中止分支与 `remove_any` 的清理停止分支.
- Linux 端到端测试在 `unshare -Ur -m sh -c` 中建立临时根, 用 `mount -t tmpfs` 与 `mount --bind` 构造挂载点, 运行真实的 `dotlink apply`; `unshare` 或 `mount` 不可用时打印原因并跳过. 断言: 退出码 1, 该行 `fail`, 目标与挂载内容仍在, 不出现最终备份名, 源仓库 bind 场景中的仓库文件仍在.
- macOS 无 user namespace, 端到端挂载用例跳过; 保留 `MountIdentity` 单元测试与 README 边界.
- 50 条目性能用例仍运行, 默认不强制墙钟预算; 需要时用 `DOTLINK_PERF=1` 观察 `statx` 开销.

### D7. `preview` 不递归预检

`preview` 仍只消费 `classify` 的结果, 不读取目标目录内容; 它可能预测 `replace`, 随后 `apply` 因挂载边界失败. 这与既有"预览报告冲突/替换, 执行期失败时报告 fail 并保留原状"的契约一致, 也与目录内特殊文件的既有行为一致. 在 README 的已知限制中补一条, 避免用户误以为 preview 的 `replace` 一定可执行.

若未来需要预览阶段发现挂载点, 可以复用 `find_mount_boundary`, 但那会增加一次完整目录遍历并引入新的竞态, 不在本变更范围.

## Risks / Trade-offs

- **兼容性收紧**: 含挂载点的目录目标从"可能成功并删除内容"变为 `fail`; 需要 README 与发布说明明确写出. 这是有意的安全行为.
- **Linux 旧内核或 macOS 同设备挂载漏检**: 只比较设备号时 bind mount 仍可能被复制和删除. 规格与 README 将其列为已知限制; 未来可增加 `/proc/self/mountinfo` 或平台挂载列表.
- **每个条目多一次 `statx`**: 目录树很大时增加系统调用; 备份本身是 I/O 主导, 以 50 条目性能用例观察, 不以硬墙钟作为默认门禁.
- **扫描与删除之间的竞态**: `find_mount_boundary` 之后新挂载仍可能出现; `remove_tree` 会在进入边界前停止, 但可能已删除部分兄弟项. 保留暂存目录并报告路径, 备份仍是完整副本; README 的并发边界适用.
- **`remove_tree` 的实现风险**: 需要保持不跟随符号链接, 处理只读目录与清理失败语义; 以现有 `remove_any`/`make_tree_writable` 测试为回归基线, 新增边界测试接缝.
- **错误说明长度**: 挂载点路径可能很长; 既有单行报告与 `escape_controls` 已处理, 不新增输出列.
- **btrfs subvolume 等非挂载边界**: 不在检测范围; 与 `rm -rf` 语义一致, 备份仍保留内容, 写入 README 的非目标边界.

## Migration Plan

无数据迁移. 升级后, 含挂载点的目录目标会从可能执行替换变为 `fail`; 用户需要先卸载或移走挂载点, 再运行 `apply`. 回滚只需回退代码; 本变更不写状态文件, 不改变既有备份命名或查找, 旧备份仍可被 `undo` 使用. 旧备份中若出现挂载点 (外部创建), 恢复会按新的复制边界安全失败.

## Open Questions

无. Linux 旧内核的 mountinfo 回退与 macOS 同设备 mount 列表属于后续兼容性增强, 不改变本次规格, 已在风险与已知限制中记录.

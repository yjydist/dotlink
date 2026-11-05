# Proposal

## Why

特殊文件与报告层的缺陷让"预览承诺"和"实际执行"脱节: 目标本身是 FIFO, socket 或设备文件时, `status` 与 `preview` 都把它报成普通文件, `preview` 进而承诺 `replace` 并预测备份路径, `apply` 却在备份阶段失败; 目标目录内含 FIFO 或 socket 时 (例如 `~/.gnupg/S.gpg-agent` 形态), 整个目录备份必然失败, 而 `preview` 仍承诺 `replace`. 报告层另有列错位, 实际指向被裁掉首尾空白, 源侧故障指向目标路径, 退出码 2 判据过窄, `--help` 缩进丢失与控制字符转义不完整等问题. 两组问题都要改 `cli-contract` 的同几条 requirement, 特殊文件还牵连 `entry-classification`, `link-deployment` 与 `backup-fidelity`; 拆成两个变更会在归档时产生冲突, 因此合并为一个变更.

## What Changes

- `entry-classification` 新增第 9 个状态 "目标被特殊文件占用" (`ForeignOther`): 既非普通文件, 非目录, 也非符号链接的占用物; 说明给出精确类型 (FIFO / socket / 字符设备 / 块设备 / 其他特殊文件).
- 动作映射: 特殊文件占用不论 `on_conflict` 一律"拒绝冲突"; `status` 报 `foreign`, `preview` 与 `apply` 报 `conflict` 并说明"特殊文件不支持备份, 不会覆盖", `undo` 维持既有 `skip` 语义; token 词汇仍为 12 个.
- 目录内含特殊文件: 备份整体失败, 不得跳过; `apply` 报 `fail` 并给出特殊文件路径与"先手工移走该特殊文件再重试"; `preview` 不递归预检, 作为已知限制写入 README.
- 报告列对齐: 序号按总条目数的位数右对齐; target 按终端显示宽度补齐, 内置只依赖字符区间的 East Asian 宽度近似助手, 不新增依赖; 增加 10 条以上与 CJK target 的回归测试.
- 实际指向保真: `detail` 只用 trim 判断是否为空, 非空时原样返回, 保留首尾空白; 失败说明按侧别给出源路径或目标路径.
- 退出码 2 判据改为标准错误是否含 clap 的用法说明 (`error:` 与 `Usage:`) 或单行的 `dotlink: ...` 清单说明; README 与各 `--help` 文本同步, 未知选项与拼错子命令都属调用方式错误.
- `--help` 文本去掉会吞掉下一行前导空白的 Rust 字符串续行写法, 恢复 if/code/fi 示例与退出码条目的缩进; `-h` 是否含退出码语义维持现状, 只修正模块注释与行为一致.
- 控制字符转义集合扩展到 U+2028, U+2029 与 U+200B-200F, U+202A-202E, U+2060-2064, U+2066-2069, U+FEFF, 统一以 `\u{...}` 形式显示.
- 统一 `replace` 行括号语义为冲突占用物描述; README 补充非 UTF-8 路径的 lossy 显示, `undo` 不清理 `apply` 创建的父目录, 备份命名空间被本工具保留, 并发原地修改目标不会进入备份.

## Capabilities

### New Capabilities

无.

### Modified Capabilities

- `entry-classification`: 当前状态由 8 个扩为 9 个; 特殊文件的动作映射固定为拒绝冲突.
- `cli-contract`: 检查类 token 的 `foreign` 覆盖特殊文件; 必现信息增加特殊文件精确类型, 实际指向不裁剪与失败侧路径; 控制字符转义扩展; 退出码 2 的判别覆盖未知选项等用法错误.
- `link-deployment`: 新增"不可备份的占用物不建立链接", `preview` 与 `apply` 报 `conflict` 而不承诺 `replace`.
- `backup-fidelity`: 新增"特殊文件不可复制且目录备份整体失败", 失败说明给出路径与重试提示.

## Impact

- 代码: `src/classify.rs` (第 9 状态与精确类型), `src/render.rs` (列对齐, 显示宽度, 原始 detail, 转义集合, 文案), `src/cli.rs` (help 文本与判据), `src/backup.rs` (特殊文件失败说明), `src/undo.rs` (匹配臂与注释); 预计不改 `src/lib.rs` 的编排.
- 测试: `src/classify.rs` / `src/render.rs` / `src/backup.rs` 单测; `tests/status.rs`, `tests/preview.rs`, `tests/apply.rs`, `tests/undo.rs`, `tests/output.rs`, `tests/unreadable.rs`, `tests/usage_errors.rs` 增加特殊文件, 列对齐, 实际指向, 源侧路径, 未知选项与转义用例.
- 文档: `README.md` 的退出码 2 段落, 控制字符说明, 报告示例附近的对齐说明与已知限制; 只改与本变更相关的段落.
- 依赖与平台: 不新增依赖; 特殊文件类型识别使用 `std::os::unix::fs::FileTypeExt`, 非 Unix 目标保留可编译的降级分支; 平台门禁与退出码值不变.

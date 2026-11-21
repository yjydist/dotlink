# Design

## Context

本变更只改测试与本地验证脚本, 动机与缺口清单见 `proposal.md` 的 Why. `.openspec.yaml` 设置 `skip_specs: true`, 因为没有产品行为变化; 但当前 CLI 2.1.1 不支持 `skip_design`, `openspec status` 会把 design 列为 `ready`, `isPlanningComplete` 为 false. 归档要求所有制品为 done 或 skipped, 因此需要一份 design.md 使规划完整.

当前测试套件的具体形态:

- `tests/support/mod.rs` 的 `snapshot` / `collect` 只记录路径与"文件内容, 链接文本或 `dir`"三类值, 不记录 mode, 对象类型, 设备号与 inode, 也不记录 mtime. 因此 chmod, 对象替换与 touch 不会改变快照.
- `tests/apply.rs` 的 `fifty_entries_preview_and_apply_finish_under_two_seconds_with_fifty_rows` 对 `preview` 与 `apply` 各断言墙钟耗时小于 `Duration::from_secs(2)`.
- 多处零改动断言使用 `snapshot_all` 在四条命令前后各取一次总快照; 后一条命令可以抵消前一条命令的写入, 使 net-zero 通过.
- `scripts/test_check_punctuation.py` 只覆盖 `.github`, `openspec/config.yaml` 与参数错误; 没有覆盖 `src`, `tests`, `scripts`, `openspec/specs`, `openspec/changes`, `AGENTS.md`, `README.md`, 也没有符号链接跳过与非法 UTF-8 的回归用例.
- `tests/support/mod.rs` 的 `running_as_root` 使用 `std::env::temp_dir()` 加进程号的固定探测路径, 同一进程内并发调用会互相干扰.
- 生产代码没有提供跨目标本地门禁; `scripts/check.sh` 只做本机目标的 fmt, test, clippy, doc 与 release 构建.

## Goals / Non-Goals

**Goals:**

- 让测试能发现只改 mode, 对象类型, 身份或 mtime 的真实回归.
- 让每条只读命令的零改动断言独立成立, 不依赖其他命令的副作用抵消.
- 让标点检查的回归覆盖实际扫描范围与失败语义.
- 移除默认测试对墙钟的硬依赖, 并让 root 探测在并发下稳定.
- 提供可选的本机跨目标与 `cargo audit` 门禁, 不引入 CI.

**Non-Goals:**

- 不修改 `src/` 的任何生产行为.
- 不引入 CI, 不改动远端流水线.
- 不承诺 Windows 或其他平台的运行时行为; 跨目标检查只做编译验证.
- 不为测试引入新的 crate; `tempfile` 已是开发依赖.
- 不把性能预算设为默认必过门槛.

## Decisions

### D1. 用 design.md 记录纯测试变更

备选是留空 design 或依赖 `skip_design`; CLI 2.1.1 没有该能力, 会让 design 一直处于 `ready`. 这里补一份简短 design.md, 记录测试策略与验证方式, 不写测试代码. 这样 `openspec status` 与 `openspec validate --strict` 能通过, 归档不会因规划不完整被阻塞.

### D2. 快照记录 mode, 类型, 设备号与 inode, mtime, 不记录 atime

`snapshot` / `collect` 在 `symlink_metadata` 结果上记录: mode (至少权限位), 对象类型, 设备号与 inode, mtime. 文件仍记录内容, 符号链接仍记录链接文本, 目录仍会递归. 不记录 atime, 因为读取内容或列举目录本身就会改变 atime, 会把只读命令误判为有改动.

这样 chmod 改变 mode, 类型替换与删除重建改变类型或设备号/inode, `touch` 改变 mtime, 都能被零改动断言发现. 备选是继续只比较内容; 否决理由是当前缺口正是内容不变而元数据变化.

平台差异由测试夹具控制: 需要精确 mtime 的用例显式设置时间, 不依赖文件创建时刻或文件系统的时间戳精度; 断言记录到的值, 不与墙钟比较.

### D3. 2 秒墙钟断言改为显式 opt-in

默认测试保留行数, token 与汇总断言, 时间预算只在显式 opt-in 时检查, 例如由环境变量控制. 这样默认 `cargo test --locked` 在负载下不会因机器快慢偶发失败, 同时在本地仍可主动验证性能.

备选是直接删除时间断言; 否决理由是删除会丢失本机性能信号. 备选是把 opt-in 预算做得更宽; 否决理由是预算值仍依赖机器, 不能作为默认门禁. opt-in 的运行不属于 `scripts/check.sh` 的默认路径.

### D4. 零改动比较改为逐命令独立快照

把 `assert_all_commands_reject` 等处的 before/after 比较移入命令循环, 每条命令执行前后各取一次快照并比较. 共享总快照只能证明"四条命令的净效果为零", 无法发现一条命令写入而另一条命令撤销的情况. 逐命令比较让每条只读命令的零改动独立成立.

备选是保留总快照再做逐命令抽查; 否决理由是总快照不再提供额外保证, 逐命令比较已覆盖同一范围且失败信息更精确.

### D5. 标点回归覆盖完整白名单与失败语义

`scripts/test_check_punctuation.py` 为 `src`, `tests`, `scripts`, `openspec/specs`, `openspec/changes`, `AGENTS.md`, `README.md` 各加至少一条命中用例; 增加符号链接跳过与非法 UTF-8 返回 1 的用例. `.github` 用例保留为未来模板防护的回归, 与 README 的说明一致. 用例继续通过 `--root` 指向 `tempfile.TemporaryDirectory`, 不触碰真实仓库.

备选是只测试脚本的禁止码点表; 否决理由是扫描范围与文件遍历才是回归高发处, 必须按真实目录结构验证.

### D6. root 探测改用临时目录

`running_as_root` 改用 `tempfile::tempdir()` 生成每次调用独立的探测目录, 用 chmod 000 目录能否被读取判断是否以 root 运行. 目录随 `TempDir` 析构清理. 依赖 `EACCES` 的用例在 root 环境下跳过, 并在跳过时说明原因.

备选是继续用进程号路径并加线程序号; 否决理由是仍需手工清理且容易与崩溃残留冲突, `tempfile` 已是开发依赖, 没有新增成本.

### D7. 可选跨目标脚本独立于 check.sh

新增 `scripts/check-cross.sh` 作为可选本地门禁, 在宿主目标可用时运行 Linux 与 Windows GNU 的 `cargo check --locked`, 以及 `cargo audit`; 目标或工具缺失时跳过并打印原因. 不把该脚本接入 `scripts/check.sh`, 也不引入 CI. 备选是 `check.sh --full`; 否决理由是默认脚本应保持可预测, 独立脚本更容易按需运行并明确跳过原因.

### D8. 稳定性用例保持可观察断言

非法或未设置 `TZ` 的备份命名用例断言时间戳仍是 15 字符 UTC; symlink manifest 用例断言 V-9 退出 2; 并发 apply 用例用启动屏障制造重叠; 重复 socket 用例合并后保持原覆盖. 这些断言只观察文件系统与 CLI 契约, 不依赖内部实现细节.

## Risks / Trade-offs

- **快照记录更多字段使测试变慢** -> 元数据读取与现有 `symlink_metadata` 共用一次调用, 增量主要来自字符串格式化; 只读命令用例规模有限, 可接受.
- **mtime 精度与文件系统差异** -> 需要精确时间的用例显式设置 mtime, 断言记录值; 不把创建时刻当作期望值.
- **root 环境无法验证 EACCES** -> 依赖权限拒绝的用例在 `running_as_root` 为真时跳过并说明原因, 其余用例照常运行.
- **opt-in 性能预算默认不执行** -> 默认测试只保证结构与输出, 性能回归需在本地显式开启; README 会说明这一点.
- **跨目标工具缺失** -> `check-cross.sh` 跳过缺失目标或 `cargo audit` 并打印原因, 不把跳过当作通过.
- **设备号与 inode 在特殊文件系统上不稳定** -> 测试只使用本地临时目录; 若未来在 network/FUSE 上运行, 可退化为只比较类型, mode 与 mtime, 但默认范围不受影响.

## Migration Plan

无产品迁移. 只修改测试夹具, 测试用例与本地脚本; 回滚即还原这些文件, 不涉及用户数据或运行时状态. 新增脚本不进入默认门禁, 移除它不会影响现有 `scripts/check.sh`.

## Open Questions

无.

# Proposal

## Why

`specs/001-dotfile-link-management/` 已在本轮删除 (规格改由 OpenSpec 管理), 但该目录承载的 6 份文档是本项目**唯一**的行为契约来源: 退出码的两条件判据与目标谓词表, 12 个状态 token 与 4 个汇总列, 清单校验 V-1 到 V-11, 8 种 `Current` 与 6 种 `Action` 的判定表, 非严格 realpath 的三段语义, 备份命名与"取最新"的排序规则, `undo` 的分支表与幂等条件. 目前 `openspec/specs/` 只覆盖 `backup-fidelity` 一个能力, 其余契约没有任何权威落点.

删除留下了两类悬空引用:

- 源码, 测试与 README 中 **68 处**按文件名引用 (`research.md` 24 处, `cli.md` 19 处, `data-model.md` 13 处, `tasks.md` 7 处, `manifest.md` 4 处, `quickstart.md` 1 处); 其中 README 的 `quickstart.md` 链接是坏链接, README 另有一处指向已删除目录 `specs/001-dotfile-link-management/` 的坏引用;
- **65 处** `FR-n`, **101 处** `V-n`, **32 处** `R-n`, **84 处** `T-nnn` 编号引用 (验证时实测, 统计范围为 `src/`, `tests/` 与 `README.md`; `T-nnn` 只计独立的任务编号, 备份时间戳中的 `T000000` 形态不计), 全部指向已不存在的文档或已完成的任务清单.

这些编号在实现过程中反复被用来锁定行为 (例如 `V-5` 的目标重复检测, `R2` 的规范化陷阱), 失去据点的代价是下一次改动无法再判断"我有没有违反契约". 因此需要在 OpenSpec 中重建权威落点, 并把引用改指过去.

## What Changes

- 在 `openspec/specs/` 下新增 7 个 capability, 承接被删文档中的**行为契约**: 清单格式与校验, 路径规范化, 条目状态判定, 命令行接口契约, 部署语义, 撤销语义, 备份生命周期.
- 契约中的实现约束 (例如"必须用 `symlink_metadata` 而非 `metadata`", "不得用 `Path::canonicalize`", "暂存名必须携带进程号") 一并写入 capability 的 requirement, 不只放在 `design.md`: 它们的失效模式可被外部观察到 (状态误判, `undo` 拒删自己的产物), 且源码需要长期稳定的引用路径.
- 保留 `V-n` 与 `FR-n` 作为 requirement 名称的后缀标注 (`[V-4]`, `[FR-014]`), 使代码中已有的 96 处 `V-n` 与 64 处 `FR-n` 引用仍可定位; 丢弃 `R-n` (论证过程编号, 仅 `R2` 作为溯源标注保留) 与 `T-nnn` (已完成任务编号).
- **不迁移**: `plan.md` (技术方案), `tasks.md` (已完成的任务清单), `checklists/requirements.md` (质量检查清单), `spec.md` 的 User Stories 与 Clarifications 会话记录, `quickstart.md` 的独立章节形态. 这些内容属于过程记录, 由 git 历史保留; 其中的实质行为已压缩进各 capability 的 Scenario.

**引用改写不在本变更内**: README 与源码/测试中的 68 处文件名引用与 198 处编号引用 (`FR-n` / `V-n` / `R-n`), 由**归档之后**的后续变更承接. 原因是 OpenSpec 的主 spec 由 delta 在 `archive` 时生成, 本变更的 apply 阶段 `openspec/specs/<capability>/spec.md` 尚不存在, 此刻改指过去会制造一批指向空的引用. 本变更的交付物是 capability 本身, 外加 5.1 的 `~//` 行为修正, 7.1 的 V-9 主目录符号链接形态修正, 5.5 的测试接缝, 以及一组补齐既有 Scenario 断言的集成测试与验证报告建议落实的断言补强及一处说明文本细化 (见 `tasks.md` 第 8 节), 外加 `README.md` 第 5 行与 `src/render.rs` 注释中的 undo 幂等表述修正 (文档措辞, 非引用改写, 见 `tasks.md` 第 9 节), 以及第二轮验证报告建议落实的措辞与断言补强 (见 `tasks.md` 第 10 节).

**行为修正之一 (评审反馈)**: `~//.bashrc` 这类 "`~/` 之后多写分隔符" 的目标此前会解析为 `/...` (`PathBuf::join` 丢弃主目录), 与 V-6 已声明的 "`~/...` 展开为主目录" 冲突, 一次笔误即可把链接与备份挪到主目录之外. 本变更修正 `expand_target` 并在 V-6 补 Scenario 与端到端用例 (见 `design.md` D7).

**行为修正之二 (验证反馈)**: `HOME` 自身是符号链接时 (如 `/home/u -> /mnt/u`), 把 `target` 写成该主目录的物理路径会漏过 V-9 的主目录落点检查 -- `os::normalize` 不解析末段, 书写形态与物理形态因此不等, 一次 `replace_and_backup` 即可备份并替换整个主目录. 本变更让 V-9 同时比较 `HOME` 的书写形态与解析其后符号链接的物理形态, 并在该 requirement 补 Scenario 与端到端用例 (见 `design.md` D7). 除此之外不修任何可执行逻辑, 不改变退出码或文件系统操作.

**输出细化之三 (验证报告建议)**: `wrong` 与 `foreign` 行的"实际指向"此前回显目标链接文本的原始写法; 当该文本是相对路径时, 说明只能给出相对形态, 用户仍需自行拼接前缀. 本变更改为展示 `link_destination` 已解析出的规范化绝对目标 (只影响这两类行的说明文本, 不改变 token, 动作, 退出码或文件系统行为), 并补相对链接文本的单元与端到端断言 (见 `tasks.md` 第 8 节).

**文档表述修正 (验证反馈)**: `README.md` 第 5 行与 `src/render.rs` 的 `undo_token` 注释称"上一次恢复过备份时第二次 `undo` 报 `skip`", 但若恢复出的内容本身是指向仓库内的链接, 实现会按归属规则重新归类并再次报 `restore` (见 `link-teardown` 的 Scenario). 本变更按实现修正这两处措辞, 不改变任何可执行行为 (见 `tasks.md` 第 9 节).

**为无法端到端验证的分支留接缝**: 平台门禁与 `now_local()` 退化在本机走不到. 本变更拆出 `run_on(supported)` / `platform_gate(supported, stderr)` 与 `timestamp_at(local)` 两个参数化入口, 使"退 3 且先于参数解析", "门禁标准错误恰一行", "同一秒内的序号消歧"与"UTC 退化仍保持时间戳形态"成为确定性单元测试, 而不是只能依赖代码审阅.

## Capabilities

### New Capabilities

- `manifest-schema`: `dotlink.toml` 的定位规则, 字段定义, `target` 的三种形态解析, 以及校验 V-1 到 V-11 的整体拒绝语义与说明要求.
- `path-normalization`: 非严格 realpath 的两段语义 (已存在段解析符号链接, 缺失尾段词法规整), 末段不解析, 深度上限, 以及"归属判定与相等判定共用同一函数"的约束.
- `entry-classification`: 8 种 `Current` 的判定条件与求值顺序, 链接归属的划分依据, 6 种 `Action` 的映射表, 以及目标谓词对 `Absent` / `SourceMissing` / `Unreadable` 的计数规则.
- `cli-contract`: 四条子命令的命令面, 不维护持久化状态, 平台门禁的时机与退出码 3, 退出码的两条件判据与各命令的目标谓词, 12 个状态 token 与 4 个汇总列的归类表, 以及必现信息的输出要求.
- `link-deployment`: `preview` 与 `apply` 的语义: 父目录自动创建, 幂等, 单条目失败不中断, 预览与执行共用同一判定, 预览同时给出当前状态与将执行的动作, 整目录链接.
- `link-teardown`: `undo` 的语义: 仅处理本工具创建的链接, 按 `Current` 与"同目录是否有备份"决定 token 的分支表, 两种幂等情形, 并存语义, 暂存名消歧与失败回滚.
- `backup-lifecycle`: 备份的位置, 命名, 时间戳与消歧序号, "取最新"的 `(时间戳, 序号数值)` 排序, `.partial` 的排除规则, 以及备份不被自动回收的边界.

### Modified Capabilities

无. `backup-fidelity` 的 requirement 不改变: 它约束备份的**内容与元数据保真**, 而 `backup-lifecycle` 约束备份的**命名, 查找与保留**. 二者边界已在 `design.md` 中固定, 避免同一行为出现两处权威定义.

## Impact

- `openspec/specs/`: 新增 7 个目录, 每个含一个 `spec.md`; 由归档步骤从本变更的 delta 生成.
- `openspec/changes/migrate-spec-kit-contracts/`: proposal, 7 份 delta spec, design, tasks.
- 生产代码只做四处改动: 5.1 的 `expand_target` 修正 (`~//` 形态不再逃出主目录), 7.1 的 V-9 主目录符号链接形态修正 (`src/manifest.rs` 同时比较 `HOME` 的书写形态与物理形态), 与 5.5 的四个可测试接缝 (`run_on` / `timestamp_at` / `backup_candidates_at` / `write_backup_at`) 及验证反馈补充的 `platform_gate`, 以及 8.9 的 `wrong` / `foreign` 实际指向细化 (`src/classify.rs`); 其余生产代码, 依赖, 构建配置与 `scripts/check.sh` 均不改动. `README.md` 仅修正第 5 行的 undo 幂等表述, `src/render.rs` 仅修正 `undo_token` 注释. 旧引用在此期间保持原样, 不会新增悬空引用.
- 新增一组补齐既有 Scenario 断言的集成测试 (preview 备份命名形态与不建父目录, 清单不可读, 执行期失败续跑, 二次 undo, 只有 `.partial` 残留, 标准输入不阻塞, 符号链接 HOME 下物理形态主目录落点被拒等) 与 `tests/support` 的取路径助手; 除 5.1 与 7.1 的缺陷修正与 8.9 的说明文本细化外不改变任何可执行行为. 验证报告建议补强的测试 (四命令一致性, 源侧故障, 部署 / 撤销 e2e 与备份生命周期断言等) 见 `tasks.md` 第 8 节; 第二轮验证报告建议的措辞与断言补强见 `tasks.md` 第 10 节.

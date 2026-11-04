# Design

## Context

被删除的 `specs/001-dotfile-link-management/` 是 spec-kit 的产物: 规格, 契约, 数据模型, 研究与任务清单分处六个文件, 同一行为常在多处出现 (例如"目标谓词"同时写在 `contracts/cli.md`, `data-model.md` 3.2a 与 `research.md` R10). 其内容已被实现与测试吸收, 现在是反过来的: 实现是权威, 文档需要与之一致.

OpenSpec 的承载单位不同: 一个 capability 一份 `spec.md`, 每条 requirement 带可验证的 Scenario. 因此本设计的基本动作不是"把六个文件搬过去", 而是**按能力重新切分**, 并把重复表述收敛到唯一落点. 动机见 `proposal.md`.

约束:

- `openspec/specs/` 现有 1 个 capability (`backup-fidelity`), 布局是扁平单层 (`openspec/specs/<capability>/spec.md`), 本设计沿用该布局, 不引入新的域层级.
- 主 spec 由 delta 在归档时生成. 归档之前 `openspec/specs/<capability>/spec.md` 不存在, 因此任何指向主 spec 的引用都不能在本变更的 apply 阶段落地.
- 项目 context 要求产物使用简体中文; 图表优先用 mermaid, ASCII 图只用 ASCII 字符.

## Goals / Non-Goals

**Goals:**

- 让每一条**当前实现遵守**的行为契约都有唯一的权威落点, 且该落点可用 `openspec show <capability> --type spec` 读取.
- 让被删文档中的**实现约束** (不使用会穿透链接的元数据读取, 不使用 `Path::canonicalize`, 暂存名排他创建等) 保留为 requirement, 因为它们的失效模式可被外部观察到, 而不只是内部风格.
- 保留 `V-n` / `FR-n` 编号的可定位性, 使代码中已有的引用不必全部重写才能找到对应契约.

**Non-Goals:**

- 不修文档与实现之间可能存在的既存偏差. 迁移以**当前实现**为准; 发现偏差时记录到 Open Questions, 不在本变更中裁决. 例外是两处可被外部观察的缺陷 -- `~//` 目标逃出主目录的展开缺陷, 以及 `HOME` 自身是符号链接时 V-9 漏掉物理形态主目录落点 -- 它们被直接修正而不是写进 spec (见 D7).
- 不定义并发保证. `research.md` R8 的结论是"不保证串行化, 不隔离外部并发修改", 这是一条限制而非可验证契约; 它留在 README 的"已知限制"章节, 不写成 requirement.
- 不迁移验收指标 (SC-00x) 与性能断言: 它们描述验收与文档目标, 不是可验证的行为契约; 若要长期约束性能, 应由独立变更引入基准测试.
- 不保留 spec-kit 的过程产物: 用户故事, 澄清会话, 技术方案, 任务清单, 质量检查清单.
- 不定义特殊文件 (FIFO, 设备) 与不可读链接文本的处理: 实现把前者归入"目标被普通文件占用"一格 (替换时备份阶段拒绝并报告失败), 后者按环境故障判为"无法判定". 这两条边界不对应稳定契约, 留待后续裁决, 因此不写成 requirement.
- 不改写 README 与源码注释中的**引用** (见 D6; README 第 5 行的 undo 表述修正属措辞而非引用, 见 D5).

## Decisions

### D1. 按能力切分, 不按旧文档切分

七个 capability 的划分依据是"一个能力一段契约", 与旧文件边界没有一一对应:

| capability | 承接的旧内容 |
|---|---|
| `manifest-schema` | `contracts/manifest.md` 全部; `data-model.md` 1-2 节; FR-001/002/013/015 |
| `path-normalization` | `research.md` R2 与 R5 的规范化部分; `data-model.md` 2 节"两侧同源"与 3.1 节"两段语义"; FR-012 |
| `entry-classification` | `data-model.md` 3.1-3.3; `research.md` R7; FR-017 |
| `cli-contract` | `contracts/cli.md` 命令面, 平台门禁, 退出码, 输出契约; FR-011/016/019/020; SC-009 |
| `link-deployment` | `contracts/cli.md` 的 `preview` / `apply` 小节; FR-003/004/005/006/007/008/014/018 |
| `link-teardown` | `contracts/cli.md` 的 `undo` 小节与分支表; FR-009/010 |
| `backup-lifecycle` | `data-model.md` 4 节的命名与查找; `research.md` R4 |

备选方案是每个旧文件一个 capability (`manifest`, `cli`, `data-model`, `research`). 否决理由: `data-model.md` 同时含清单校验, 状态判定与备份, 命名成 capability 会得到一个跨三个关注点的宽泛能力; 而 `research.md` 是论证记录, 不是行为契约的天然边界. 备选方案之二是只做一个 `dotlink-contract` capability. 否决理由: 约 55 条 requirement 挤在一个文件里, 无法按需读取, 也无法让代码注释指向具体段落.

### D2. 编号以 requirement 名称后缀的形式保留

`V-n` (101 处引用) 与 `FR-n` (65 处引用) 写成 requirement 名称内的短标识, 例如 `### Requirement: 清单校验 V-4: 字段必填且为非空字符串` 与 `### Requirement: 单条目失败不中断其余条目 [FR-014]`. `V-n` 同时承担"校验编号即 requirement 顺序"的作用, 因此放在名称中部, 只有与 FR 标注并列的 `V-1` 写成 `[V-1]`; `FR-n` 只作溯源, 一律作为方括号后缀放在末尾.

`R-n` 丢弃: 它编号的是论证过程 (被否决的替代方案, 实测记录), 不是行为. 例外是 `R2`: 它保留为 `path-normalization` 两条 requirement 的溯源标注 (`[R2]`), 因为它是代码中引用最多的论证编号 (17 处), 且其结论本身就是一条行为约束; 其余 `R-n` 仍然丢弃. `T-nnn` 丢弃: 它指向已完成的实现任务, 不是长期契约.

备选方案是彻底丢弃全部编号, 代码引用改为 requirement 名称. 否决理由: 166 处 `V-n` / `FR-n` 引用 (另有 17 处 `R2`) 全部重写才能恢复可定位性, 而 `V-5` 这类短标识在代码里比一段中文名称更易读, 也更容易在评审中引用.

代价: `FR-n` (以及并列标注的 `V-1`) 使 requirement 名称包含方括号. 未来若用 `MODIFIED` 修改它, 名称必须逐字匹配; 若需改名, 应使用 `RENAMED`, 并把方括号 (`FR-n`) 或名称中部的 `V-n` 视为名称的一部分.

### D3. 实现约束写成 requirement, 不放进 design

判断标准是"失效模式是否可被外部观察到". 符合的例子:

- 使用会穿透链接的元数据读取 -> 三种状态塌缩成一种, 用户看到 `status` 把"别人的链接"报成"目标被普通文件占用";
- 使用 `Path::canonicalize` -> 悬空链接被判为落在仓库外, `undo` 拒绝清理自己的产物;
- 允许暂存名覆盖 -> 用户文件被静默替换.

这些都不是风格问题, 而是用户可见的后果, 所以属于 spec. 反之, "用哪个模块承载"之类不影响行为的内部结构留在本文件.

放在 spec 而不是本文件的第二个理由: 代码注释需要长期稳定的引用目标. 本文件归档后路径会变成 `openspec/changes/archive/<date>-<name>/design.md`, 不适合被 100 余处注释指向.

### D4. `backup-fidelity` 与 `backup-lifecycle` 的边界

既有 `backup-fidelity` 的 Purpose 明确排除所有权, ACL, 硬链接与链接自身元数据, 并覆盖"内容, 类型, 权限, mtime, 扩展属性"的保真以及"完整复制后才发布最终名". 本变更**不修改它**, 新增的 `backup-lifecycle` 只覆盖位置, 命名, 时间戳, 消歧, 中间名的排他创建与原子发布, 查找排序, `.partial` 的排除与保留边界.

这条边界让"发布前提"只出现一次: 它属于保真 (内容尚未完整时不得发布), 不属于生命周期. `backup-lifecycle` 定义 `.partial` 这个中间名的命名, 排他创建与可见性规则, 以及发布必须原子不覆盖; 它不复述"内容完整"这一前提, 只引用. 边界说明只写在新 capability 的 Purpose 里: `backup-fidelity` 的 Purpose 保持原样, 因为本变更不修改既有主 spec (见任务 2.2 的 `git status --porcelain openspec/specs` 无输出).

### D5. 引用改写的目标形态 (后续变更执行)

代码注释写成仓库相对路径加编号, 例如 `openspec/specs/manifest-schema/spec.md 的 V-5`. 选择完整仓库相对路径而不是简称, 是因为编辑器的路径跳转与 `rg` 定位都依赖它; 编号则接在路径之后, 避免每条引用重复中文名称.

README 第 268 行改为指向 `openspec/specs/` 并列出能力清单; 第 277 行的坏链接不修补为"端到端验证指南" (该文档已随 `specs/` 删除), 改为指向 `cli-contract` 的 Scenario 与 README 既有示例.

README 第 5 行的 undo 幂等表述与本变更新增的 `link-teardown` Scenario 冲突 (恢复出的仓库内链接第二次 `undo` 报 `restore`), 属文档措辞而非引用改写, 在本变更中一并修正; `src/render.rs` 的 `undo_token` 注释同步. 其余 README 引用仍留给后续变更.

### D6. 两阶段落地, 本变更只产出 capability

OpenSpec 的主 spec 由 delta 在归档时生成. 因此本变更的 apply 阶段无法创建 `openspec/specs/<capability>/spec.md`; 若此时改注释指向过去, 会得到一批指向空的引用, 且 `cargo test` 之外的任何检查都发现不了.

因此拆为两个变更:

1. 本变更: 7 份 delta spec, 外加 5.1 的 `~//` 行为修正, 7.1 的 V-9 主目录符号链接形态修正 (`src/manifest.rs` 同时比较 `HOME` 的书写形态与物理形态), 5.5 的四个测试接缝 (`run_on` / `timestamp_at` / `backup_candidates_at` / `write_backup_at`) 与验证反馈补充的 `platform_gate`, 以及一组补齐既有 Scenario 断言的集成测试 (preview 备份命名形态, 清单不可读, 执行期失败续跑, 二次 undo 等), 还有 `tests/support` 的取路径助手, 以及 README 第 5 行与 `src/render.rs` 注释的 undo 幂等表述修正. 验收标准是 `openspec validate --strict` 通过, 每份 delta 逐条对应到旧文档 (见下方对照表), 且 `cargo test --locked` 通过.
2. 后续变更 (归档后): README 的两处坏引用; 68 处文件名引用; 198 处编号引用 (65 处 `FR-n` / 101 处 `V-n` / 32 处 `R-n`); 84 处 `T-nnn`. 验收标准是 `grep` 不再命中已删文档名, 且 `cargo test --locked` 与 `cargo clippy -D warnings` 保持通过.

备选方案是在 apply 阶段直接写主 spec, 再让归档覆盖一次. 否决理由: 同一份内容被两个来源写入, 归档时的 delta 应用结果不可预期, 且违背 capability 只由 delta 演进的模型.

### D7. 迁移以当前实现为权威

旧文档是 2026-09 到 10 月间逐步演进的, 代码在其后经历多次修复. 当文档与实现不一致时, spec 写实现的行为, 不写文档的说法; 若该行为看起来是缺陷, 记录到 Open Questions 供后续裁决, 本变更不改任何代码 (三处例外见下一条).

**例外一 (评审反馈)**: `~//.bashrc` 这类"`~/` 之后多写分隔符"的目标此前解析为 `/...`, 因为 `PathBuf::join` 在剩余段是绝对路径时会**丢弃主目录** (`src/entry.rs` 的 `expand_target`). 这不是文档与实现的措辞差异, 而是一次笔误就把链接与备份挪到主目录之外: 与 V-6 已声明的 "`~/...` 展开为主目录" 直接冲突, 且修复只在解析入口一行. 把该行为写进 spec 会让一个可被外部观察的缺陷变成契约, 因此本变更直接修正 `expand_target`, 在 V-6 补 "主目录简写后多写斜杠" 的 Scenario, 并用 `tests/apply.rs` 的端到端用例锁定. 其余偏差仍按上一条处理.

**例外二 (验证反馈)**: `HOME` 自身是符号链接时, 同一主目录有书写形态 (如 `/home/u`) 与解析符号链接后的物理形态 (如 `/mnt/u`) 两种写法. `os::normalize` 刻意不解析末段, 于是 V-9 原先只比较 `HOME` 的书写形态, 写在物理形态下的主目录落点会漏过: 一次 `replace_and_backup` 就会备份并替换整个主目录, 与 V-9 要防的失效模式完全相同, 且缺陷可被外部观察. 因此 V-9 的判别同时覆盖两种形态 (物理形态用"探针子项 + 父目录"取得, 不引入新的路径语义), 并在 `manifest-schema` 补 Scenario 与 `tests/manifest_errors.rs` 的端到端用例; 同时补一条"主目录之下的合法落点不受误伤"的反向用例. 这仍属于"可被外部观察的缺陷不写成契约"的同一判断标准, 因此是 D7 的第二处例外.

**例外三 (验证报告建议)**: `wrong` 与 `foreign` 行的"实际指向"此前回显链接文本的原始写法. 用户自建的符号链接可以写相对文本, 此时说明停留在相对形态, 与"说明给出实际指向"的可读性目标不一致. 本变更改为展示 `link_destination` 已解析出的规范化绝对目标: 只影响这两类行的说明文本, token, 动作, 退出码与文件系统行为均不变, 因此不属于行为修正, 但记录为第三处例外. 相应补相对链接文本的单元测试与 `tests/status.rs` 的端到端断言.

## 对照: 旧章节到 capability

下表是逐节归属, 用作迁移完整性的核对基准. "不迁"表示该内容属于过程记录.

| 旧文件 | 章节 | 归属 |
|---|---|---|
| `contracts/manifest.md` | 定位规则, 仓库根, 当前工作目录不可用 | `manifest-schema` 清单定位与仓库根 |
| | 格式与字段 | `manifest-schema` V-4, V-7 |
| | 未知键 | `manifest-schema` V-8 |
| | `target` 解析规则 | `manifest-schema` V-6 |
| | 落点不得退化 | `manifest-schema` V-9 |
| | 相对目标路径与自指 | `manifest-schema` V-10 |
| | 校验 V-1 到 V-7 | `manifest-schema` |
| | 源目标隔离 (V-11) | `manifest-schema` V-11 |
| | 不校验源存在 | `manifest-schema` 源条目存在性不属于清单缺陷 |
| `contracts/cli.md` | 命令面, 非交互约束与刻意不提供的选项 | `cli-contract` 命令面 |
| | 平台门禁 | `cli-contract` 平台门禁 |
| | 退出码, 两条件, 目标谓词, 调用方式错误 | `cli-contract` 退出码相关四条 |
| | `preview` / `apply` 语义 | `link-deployment` |
| | `status` 语义 | `cli-contract` 检查类 token + `entry-classification` |
| | `undo` 语义, 分支表, 幂等, 并存语义, 撤销期间的冲突 | `link-teardown` |
| | 输出契约, token 表, 汇总列, 必现信息 | `cli-contract` |
| `data-model.md` | 1 节 Manifest 与 V-1 到 V-11 | `manifest-schema` |
| | 2 节 Entry 与解析产物 | `manifest-schema` + `path-normalization` 三处同源 |
| | 3.1 节八种状态与求值语义 | `entry-classification` |
| | 3.1 节 non-strict realpath 两段语义 | `path-normalization` |
| | 3.2 节动作映射 | `entry-classification` |
| | 3.2a 谓词求值基准 | `entry-classification` |
| | 3.3 节命令与状态的消费关系 | `entry-classification` + 部署/撤销两个能力 |
| | 3.4 节状态迁移 | `link-deployment` 与 `link-teardown` 的 Scenario (迁移是动作的效果); 状态本身与恢复后的重新判定归 `entry-classification` |
| | 4 节 Backup 的位置, 命名, 中间名排他创建, 查找, 完整性 | `backup-lifecycle`; 内容与元数据保真归 `backup-fidelity` (本变更不改) |
| | 5 节 Report | `cli-contract` |
| `research.md` | R1 清单格式 | 约束进 `manifest-schema`, 取舍理由不迁 |
| | R2 链接归属与规范化 | `path-normalization` + `entry-classification` |
| | R3 冲突粒度 | `link-deployment` |
| | R4 备份位置与命名 | `backup-lifecycle` |
| | R5 目标解析 | `manifest-schema` |
| | R6 平台门禁 | `cli-contract` |
| | R7 单一判定入口 | `entry-classification` |
| | R8 并发策略 | 不迁 (限制, 见 Non-Goals) |
| | R9 不做机器可读输出 | `cli-contract` 命令面 |
| | R10 只读命令退出码 | `cli-contract` 退出码 |
| `spec.md` | FR-001 到 FR-020 | 分散到上列 capability, 编号以 `[FR-n]` 保留 |
| | US1 到 US5, Clarifications 会话 | 不迁 |
| | SC-001 到 SC-010 | 部分成为 Scenario, 不保留编号; SC-003 (性能), SC-008 与 SC-010 (文档目标) 不迁 |
| `quickstart.md` | 14 个验证场景 | 拆入各 capability 的 Scenario (场景 3 的性能断言不迁); 整体流程不迁 |
| `plan.md`, `tasks.md`, `checklists/` | 全部 | 不迁 |

## Risks / Trade-offs

- **压缩导致细节丢失** -> 上方逐节对照表是核对基准; `openspec validate --strict` 检查每条 requirement 至少一个 Scenario, 使"漏写验收条件"变成可检测的. 迁移完成后用 `rg '^### Requirement:'` 与旧文档的条目数做一次人工比对.
- **requirement 名称含 `[V-4]` 之类后缀, 未来修改时必须逐字匹配** -> 在 `AGENTS`/README 之外, 由 D2 记录约定; 改名一律走 `RENAMED`.
- **`backup-lifecycle` 与 `backup-fidelity` 边界漂移** -> 两边 Purpose 各写一句边界, 并在 `backup-lifecycle` 的"发布前提"处引用而非复述.
- **`V-n` 与 `FR-n` 混用两种括号位置, 可能被误认为可以任意移动** -> 对照表固定每个编号的归属; 编号只作为名称的一部分, 不参与排序或匹配逻辑.
- **迁移后 `specs/` 仍可能被其他地方 (脚本, CI, 编辑器配置) 引用** -> 已在仓库内搜索过: 仅 README 与接口文件注释引用; `.specify/feature.json` 仍指向旧路径, 但它属于 spec-kit 工具链, 不在本变更范围.
- **`openspec validate --strict` 对 requirement 描述长度 (500 字符) 的约束可能逼出过度拆分** -> 已按"一条 requirement 一个行为"拆分; 若出现警告, 优先合并 Scenario 而不是合并 requirement.
- **平台门禁与时区退化无法在当前主机上端到端验证** -> 前者需要运行在不支持的操作系统上, 后者需要让 `now_local()` 失败; 端到端测试走不到这两条分支. 评审反馈后已拆出接缝, 使它们可被单元测试锁定: `platform_gate(supported, stderr)` (`src/lib.rs`) 让"退 3, 标准错误恰一行, 不写标准输出"与"受支持时门禁无副作用"可直接断言, `run_on(false)` 在测试进程的非法 argv 下取到 3 证明门禁先于参数解析且与清单无关; `timestamp_at(Option<OffsetDateTime>)` (`src/backup.rs`) 让 UTC 退化与给定时刻的格式化可被确定性地测试. 备份名与 15 位时间戳形态仍由 `src/backup.rs` 的单元测试锁定. 端到端验证仍不可得: 若后续需要真正的跨平台回归, 应在 CI 上增加受支持平台之外的构建任务, 而不是在测试里伪造 `cfg!`.

## 验证层级与后续项 (验证报告建议落实)

各类"覆盖"的证据层级固定如下, 归档说明据此解释验证范围:

- **端到端 (真实二进制)**: 大部分 Scenario. 本轮新增 `tests/consistency.rs` (同一份七状态清单上四条命令的逐条结论一致), 并在既有套件中补强: 备份成功但替换失败时的备份路径, 备份不可读时的 fail 与重试, 同目录三目标各自取最新备份, 源侧-only 的 ENOTDIR 故障, 相对链接文本的实际指向.
- **单元测试层**: 真实二进制无法注入的竞态与不可达分支 -- 动手前复核目标被替换 (`src/undo.rs`), 备份不可读时的链接回滚 (`src/undo.rs`), 回滚落点被占用 (`src/undo.rs`), 裸文件名与目录不存在的备份查找 (`src/backup.rs`), 平台门禁 (`src/lib.rs`) 与本地时区退化 (`src/backup.rs`).
- **代码审阅**: "平台不支持不覆盖移动"的失败分支 (`src/os/mod.rs` 的 `rename_noreplace`) 在受支持平台上不可达, 只有实现与注释作为证据; `entry-classification` 的 "撤销只读取状态" 是结构性断言 (撤销只消费判定结果, 不重新判定链接归属), 没有可观测行为, 同样由代码审阅覆盖.
- **结构保证**: 平台门禁的标准输出为空与不触碰文件系统由 `platform_gate` 的签名 (只接受 stderr writer, 无文件系统调用) 保证; 单元测试锁定退出码 3 与标准错误恰一行.
- **运行身份**: 依赖 chmod 的用例 (备份失败原子性, 备份目录不可读, 备份不可读的恢复, 源侧权限故障) 在 root 下会跳过并打印原因. 质量门应以非 root 用户运行, 验证报告需记录运行身份.

后续项 (不在本变更内): 仓库当前没有 CI 配置. 若要补齐平台门禁的交叉验证与两种运行身份, 应由后续变更引入 CI, 增加一次非 macOS/Linux 目标的 `cargo check` 与非 root 的 `cargo test`.

## Migration Plan

1. 归档本变更, 使 7 个 capability 落入 `openspec/specs/`.
2. 归档后逐份 `openspec show <capability> --type spec` 复核: Purpose 非 `TBD`, Scenario 未被丢弃, 编号后缀完整. 归档说明同时记录质量门的运行身份 (非 root) 与未跟踪工作区文件 `AGENTS.md` 的归属, 避免归档时误纳入.
3. 起第二个变更处理 README 与注释引用; 该变更的 tasks 必须包含"改完后 `rg` 旧文档名零命中"与"全套测试通过"两条验收.
4. 回滚策略: 删除 `openspec/changes/migrate-spec-kit-contracts/`, 还原 `README.md` 第 5 行与 `src/render.rs` 的 `undo_token` 注释, `src/entry.rs` 的 `~//` 修正, `src/manifest.rs` 的 V-9 主目录形态修正, `src/classify.rs` 的 `wrong` / `foreign` 实际指向细化及其单元与端到端断言, `tests/manifest_errors.rs` 的两个主目录用例, `src/lib.rs` / `src/backup.rs` 的测试接缝及其单元测试, 并撤销 `tests/` 下本次新增的断言, 助手与 `tests/consistency.rs`; 归档之前主 spec 尚未生成, 这些改动不影响任何外部契约.

## Open Questions

- 撤销期间目标被换成**另一个符号链接**: 实现只复核"仍是符号链接", 因此会移除该链接并恢复备份. 是否应进一步复核归属 (规范化后的链接文本落在仓库根内或等于本条目的源), 留待后续裁决; 本变更按当前实现记录于 `link-teardown`.
- 旧 `quickstart.md` 的端到端验证流程 (14 个场景) 已拆散进各 capability 的 Scenario, 但"按顺序手工验证一遍"这种整体流程没有等价的落点. 是否需要一份独立的验收文档, 留待后续变更决定.
- 旧 `contracts/cli.md` 的幂等段落声称第二次 `undo` 对恢复出的内容一律报告 `skip`; 实现会把恢复出的仓库内链接按归属规则重新归类并再次恢复 (token `restore`). 本变更按实现记录于 `link-teardown`, 是否需要识别"刚恢复"以避免重复写入, 留待后续裁决.

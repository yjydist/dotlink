---
description: "Task list for dotlink implementation"
---

# Tasks: 声明式 dotfile 软链接管理

**Input**: Design documents from `/specs/001-dotfile-link-management/`

**Prerequisites**: `plan.md`, `spec.md`, `research.md`, `data-model.md`, `contracts/`, `quickstart.md`

**Tests**: 本清单**包含**测试任务.规格本身未强制 TDD, 但 `plan.md` 的 Technical Context 已确定测试栈, 且本功能的多数操作会写入用户主目录 (`undo` 甚至涉及删除), 因此把"先写会失败的测试"作为硬性顺序要求, 而非可选项.

**Organization**: 任务按用户故事分组, 使每个故事可独立实现与独立验证.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: 可并行 (不同文件, 且不依赖未完成的任务)
- **[Story]**: 所属用户故事 (US1..US5)
- 每项任务都给出确切文件路径

## Path Conventions

单一 crate, `src/` 与 `tests/` 位于仓库根 (见 `plan.md`).
所有 Rust 源码使用 **edition 2024**.
常量: 清单位置为**当前工作目录**下的 `dotlink.toml` (不向上搜索); 仓库根 == 该文件所在目录.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: 项目初始化与基本结构

- [X] T001 初始化 Rust 项目: 创建 `Cargo.toml` (package name `dotlink`, edition 2024), 依赖 `clap` 4.6 (features `derive`), `serde` 1.0 (features `derive`), `toml` 1.1, `thiserror` 2.0, `time` 0.3 (features `formatting`, `local-offset`); dev-dependencies `assert_cmd` 2.2, `predicates` 3.1, `tempfile` 3.27.声明 `[lib]` 与 `[[bin]]` 两个 target, bin 名为 `dotlink`
- [X] T002 创建模块骨架并使其可编译: `src/main.rs` (薄入口, 调用 `dotlink::run()` 并以返回值 `std::process::exit`), `src/lib.rs`, `src/cli.rs`, `src/error.rs`, `src/manifest.rs`, `src/entry.rs`, `src/classify.rs`, `src/backup.rs`, `src/apply.rs`, `src/undo.rs`, `src/render.rs`, `src/os/mod.rs`, `src/os/unix.rs`.每个模块先放最小占位 (空函数/空枚举), 保证 `cargo build` 通过
- [X] T003 配置 lint 与格式化 (依赖 T001, 因其修改 T001 新建的 `Cargo.toml`): 在 `Cargo.toml` 中加 `[lints.clippy]` (至少 `all = "warn"`) 与 `[lints.rust]` (`unsafe_code = "forbid"`), 新增 `rustfmt.toml` (edition 2024)

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: 所有用户故事共用的核心设施.**任何一个用户故事都不能在它完成前开始.**

**⚠️ CRITICAL**: 本阶段未完成前, Phase 3 起的任何任务都不得开始.

**⚠️ 本阶段的 TDD 顺序约束 (宪章第三条)**: `manifest`, `classify` 与退出码计算是全项目风险最高的三处, 它们的**失败测试必须先于实现存在**. 因此本阶段内部有三条硬性先后关系, 不得颠倒:

```
T048 (退出码测试, 失败)   → T004 (实现 exit_code_for)
T046 (清单校验测试, 失败) → T008 (实现 manifest)
T047 (判定测试, 失败)     → T009 (实现 classify)
```

T048 / T046 / T047 只需写测试并**确认失败**, 不要求此时全绿 -- 它们对应的实现就在紧邻的下一项. T040 在 Phase 8 只做回归与不变量补充, 不与本节重复.

**T048 为何在此阶段而不在 US1**: 它约束的 `exit_code_for` 是四个命令共用的共享设施 (T004), 因此该测试与 T046 / T047 同属"基础阶段的红", 必须位于 Phase 2 之内 -- 若留在 Phase 3, 它会因为"Foundational 阻塞全部用户故事"而无法真正先于 T004 (它反倒在 T004 之后). 它的**端到端链** (经真实二进制断言四条命令的退出码) 要等到各命令接入后才可能全绿, 这是预期的: 红测试只需在 T004 之前存在并确认失败, 不要求立刻转绿.

**红阶段"失败"的形态**: 三者断言的都是 T002 留下的占位实现 (返回固定值或 `unimplemented!()` 之类), 因此测试应当**能编译并运行**, 以断言失败 (而非编译错误) 的形式变红. 若为了让测试编译而需要在占位处补签名, 只补签名, 不补行为 -- 把行为留给 T004 / T008 / T009.

- [X] T048 **先于 T004**: 编写 `tests/exit_codes.rs`, 锁定 `contracts/cli.md` 的退出码契约 (FR-020).这是全清单唯一一处跨故事的两个条件规则 ("目标谓词成立" **且** "无遗留待办"), 必须在实现退出码计算之前有失败测试. 该测试的**第一条链**直接对 T004 的 `exit_code_for` 做断言 (喂入构造的逐条结果), 因此它在 T004 之前即为红并在 T004 之后立即转绿; 端到端链约束 T015 / T016 / T031 / T034 / T036 的真实命令接入, 在本阶段之后才逐步转绿:
  - `preview` 与 `apply` 共用同一目标谓词但求值时刻不同: 未部署的清单上 `preview` 退 1 而紧随其后的 `apply` 退 0; 已符合期望的清单上二者同为 0
  - `undo` 的目标谓词与 `apply` 相反: 刚部署好的清单上 `apply` 退 0 而 `undo` 退 1; 撤销后目标为空时再运行 `undo` 退 0 (幂等)
  - **条件 2 单独生效的场景**: 清单含 `SourceMissing` 条目时, 四条命令一律退 1 (该条目计入"失败"列), 即使 `undo` 的目标谓词在它身上已成立
  - `undo` 遇到真实目录占用 (`skip`) 时退 1, 即使目标谓词成立
  这些断言不得依赖自由文本, 只用退出码与 token
- [X] T004 实现错误类型与退出码映射 (FR-020) 于 `src/error.rs`: 定义 `Error` 枚举 (`thiserror`), 至少区分 `ManifestInvalid` (清单/校验类, 退出码 **2**),`EntryFailed` (条目级失败或被拒绝, 退出码 **1**),`UnsupportedPlatform` (退出码 **3**).提供 `fn exit_code(&self) -> i32`.退出码语义为对外契约, 见 `contracts/cli.md` 的"退出码"小节.
  **同一任务内实现"由结果导出退出码"的单一函数** (`fn exit_code_for(command, &[EntryOutcome]) -> i32`): 这是四个命令**共用**的唯一的退出码计算路径, 由"逐条结果 + 该命令的目标谓词"经两个条件 (目标谓词成立 **且** 无遗留待办) 导出.它的唯一判据来源为 `contracts/cli.md` 的"退出码"小节与"各命令的目标谓词"表.之所以放在 Phase 2 而非某个用户故事里: 它是四个命令的**共同前置**, 若留到最后一个故事才实现, 则会强迫前四个故事的接入任务各自临时拼凑一份退出码逻辑 (与"不得各写一份"直接冲突), 也会让 T048 的红阶段测试跨越四个阶段无法转绿.此函数只依赖退出码所引用的**汇总分列**, 因此它是**红灯测试可以先行断言**的: T048 在本函数存在之前即为红.它与 `EntryOutcome` 的定义一起属于本阶段交付的共享设施.此时 `EntryOutcome` 只需能表达"某条目落在哪一个汇总列", 不要求接入任何真实命令
- [X] T005 [P] 定义命令行接口于 `src/cli.rs` (FR-015): 使用 `clap` derive, 子命令为 `preview` / `apply` / `status` / `undo` (无参数), 仅有 `-h/--help` 与 `-V/--version`.**不得**提供 `--repo`,`--manifest`,`--force`,`--home`,`--json` 或任何对应的环境变量. **子命令缺失或拼错**由 `clap` 的参数解析直接拒绝 (在读取清单之前), 向标准错误打印用法说明并退 2 -- 与清单校验失败共用码值, 可区分性由标准错误承担, 见 `contracts/cli.md` 的"调用方式错误". 该路径**不得**被 T004 的四类结局映射改写: `clap` 对 stderr 类错误固定退 2 且不提供自定义出口 (已由实测确认), 实现时让其直接终止进程即可, 不要试图把它包进 `Error` 枚举
- [X] T006 [P] 实现条目与路径解析于 `src/entry.rs` (FR-002, `research.md` R5): `Entry { source: String, target: String, on_conflict: OnConflict }`, 其中 `OnConflict` 取值为 `Skip` (默认) 与 `ReplaceAndBackup`.解析规则按顺序: 以 `~` 开头 → 展开为 `HOME`; 绝对路径 → 原样; 相对路径 → 相对**当前工作目录下 `dotlink.toml` 所在目录**.产出 `source_path` (仓库根 + source) 与 `target_path`, **二者都必须过 `os::normalize`** (见 `data-model.md` 2 节与 `research.md` R2 的"三个调用点"). 只规范化 `target_path` 而让 `source_path` 保持字面拼接结果, 会使 `classify` 的 `L == S` 比较落在两套路径语义上: 当 `$PWD` 与相对 `source` 拼出的路径含多层 `..` 或符号链接段时 (例如仓库根经 `/tmp` 别名形态取得, 或从含 `..` 的路径进入), 两侧即无法判等. `HOME` 无法确定时返回 `ManifestInvalid`. **取当前工作目录失败** (目录已被删除) 时同样返回 `ManifestInvalid`, 但错误说明须区分于"清单不存在" (FR-015). **不计算 `source_kind`**: 本工具的链接建立对文件与目录是同一个 `os::create_symlink` 调用, token 与说明措辞也明确排除类型标签 (`data-model.md` 2 节), 因此该字段无任何消费者; 计算它只会多一次 `symlink_metadata` 与一个无用的公开字段, 与宪章第一条"接口是成本"及第六条"禁止投机抽象"相悖.若将来出现确实依赖源类型的判定 (例如目录条目的递归语义), 在**那时**随该判定一并引入
- [X] T007 [P] 实现符号链接,路径规范化与归属判定的平台层于 `src/os/mod.rs` 与 `src/os/unix.rs` (FR-012, `research.md` R2): 需要 `create_symlink(&Path, &Path)`,`read_link(&Path)` (见下条关于 `is_symlink` 的说明).对外提供 `fn link_is_inside_repo(link_path: &Path, repo_root: &Path) -> bool` 与 `fn normalize(path: &Path) -> PathBuf`.要点:
  - `link_is_inside_repo`: 读取链接原始文本 → 解析为绝对路径 (相对链接以父目录为基准) → **两侧都规范化** → 判断是否等于或落于其下.**仅做字面前缀比较是错的**: macOS 上 `/tmp` 会规范化为 `/private/tmp`, 必须覆盖 `research.md` R2 的实测情形
  - `normalize` **不得直接使用 `Path::canonicalize`**: 后者要求路径存在, 对悬空链接 (指向已删除文件的链接) 返回 `Err` (实测已确认). 必须实现逐段消除 `.` 与 `..` 的**非严格**规范化, 不要求路径存在. 否则一个指向仓库内但已失效的链接会被误判为 `ForeignLink`, 使 `undo` 拒绝清理自己创建的悬空链接 (见 `data-model.md` 3.1 的"悬空链接"段)
  - 同一判定内部必须**自始至终使用同一个 `normalize`**, 不可一处用 `canonicalize` 一处用自实现, 否则比较两侧语义不同会重新引入 R2 的陷阱
  - `os/unix.rs` 用 `#[cfg(unix)]`; 非 unix 平台提供可编译的空实现, 使二进制在 Windows 上仍能构建成功 (`research.md` R6)
  - **不提供 `is_symlink`** (2026-09-15 第六次 `/speckit-implement` 的 T044 审阅修正): 本任务最初要求它, 但实现完成后确认它**无任何消费者** -- `classify` 直接读一次 `symlink_metadata` 的 `file_type()`, 以便同一次 lstat 同时判定"存在"与"是链接"; 再提供一个 `is_symlink` 会使调用处 lstat 两次. 保留它只会是一个无消费者的公开接口, 与宪章第一条"接口是成本"及第六条"禁止投机抽象"相悖 (`data-model.md` 2 节对 `source_kind` 的处置同理). 将来确有调用点时随该调用点一并引入
- [X] T046 **先于 T008**: 为清单校验规则编写单元测试 (FR-013), 覆盖 `data-model.md` 的 V-1 至 **V-8** 逐条 (V-8 = 只出现已定义的键, 两层各自校验).以纯函数形式直接调用 `manifest` 的校验入口, 逐个断言返回的 `Error::ManifestInvalid` 与其中的条目下标 (V-4 / V-5 / V-6 / V-7 必须指出下标; V-8 必须指出键名与所在层级).**必须确认这些测试在 T008 之前全部失败** (宪章第三条).不含退出码与"零改动"断言 -- 那两条需要完整二进制, 由 T038 在 Phase 8 以端到端形式覆盖; 本任务只锁定校验逻辑本身
- [X] T008 实现清单解析与全量校验于 `src/manifest.rs` (FR-001, FR-013), 依赖 T006.用 `serde` + `toml` 反序列化 `src/` 无状态,无全局默认值的结构: 顶层仅 `entry` 数组表.实施 `data-model.md` 的 **V-1 至 V-8** 全部规则, 逐字对齐: V-1 清单文件存在且可读 (且当前工作目录可用 -- 否则无法构造清单路径, 见 FR-015); V-2 TOML 语法合法 (报告出错位置与原因); V-3 `entry` 非空; V-4 每条目 `source` 与 `target` 均非空; V-5 不存在两个条目声明**同一个规范化后的目标路径**; V-6 每条目 `target` 可解析为绝对路径; V-7 `on_conflict` 取值属于 `{skip, replace_and_backup}`; **V-8** 只出现已定义的键 (顶层仅 `entry`, 条目内仅 `source` / `target` / `on_conflict`), 违反时指出键名与所在层级 (理由见 `contracts/manifest.md`: 拼错 `on_conflict` 静默变成 `skip` 比报错危险得多).所有违反均返回 `ManifestInvalid` 并指出条目下标, **且不得产生任何文件系统改动**
- [X] T047 **先于 T009**: 为条目状态判定编写单元测试 (FR-017), 覆盖 `data-model.md` 3.1 全部 8 种 `Current` 取值的判定与 3.2 中每个 `Current` × `on_conflict` 组合映射到的 `Action`.这是本项目风险最高的一处判定, 其失败测试必须先于实现存在 (宪章第三条; 本任务即 T040 中"必须先于 T009"的那部分).必含的边界断言, 每条都对应一处已知实现陷阱:
  - **判定顺序**: 源缺失且目标侧存在冲突时必须是 `SourceMissing`
  - **归属分界**: 链接落在仓库外 → `ForeignLink`; 链接落在仓库内但指向别的源 → `LinkedWrong`.二者不得混淆, 且必须互斥穷尽
  - **悬空链接**: 链接文本指向仓库内一个**不存在**的文件时必须是 `LinkedWrong` (而非 `ForeignLink` 或 `SourceMissing`), 以此验证 T007 的非严格规范化生效
  - **R2 规范化**: 链接文本含 `..` 且路径位于 macOS `/tmp` 下时仍须归类为 `LinkedCorrect`
  - **`NotFound` 与 `Unreadable` 不得混淆**: 路径不存在 (`NotFound`) 必须归入 `SourceMissing` / `Absent`, 而非 `Unreadable`; 仅当 `lstat` 因 `EACCES` 之类失败时才归入 `Unreadable`. (测试可用 chmod `000` 的父目录构造; 注意以 root 运行时该权限位无效, 需在非 root 环境执行或跳过)
  - **`Unreadable` 的优先权与两侧独立**: 源侧 `lstat` 因 `EACCES` 失败而目标侧正常时, 该条目必须是 `Unreadable` (而非 `SourceMissing` -- 后者只在源侧返回 `NotFound` 时成立), 说明中指出失败的是源侧; 目标侧因 `EACCES` 失败而源侧缺失时同样是 `Unreadable` (验证它优先于判定顺序, 而非表末兜底). 两条断言覆盖 `data-model.md` 3.1 的"求值语义"1 与 2
  测试直接构造临时文件系统并调用 `classify`, **必须确认全部失败**后再开始 T009
- [X] T009 实现唯一的条目状态判定于 `src/classify.rs` (FR-017, FR-006, FR-011), 依赖 T006 与 T007.这是本设计中最关键的模块, 见 `data-model.md` 第 3 节:
  - 枚举 `Current`: `SourceMissing` | `Absent` | `LinkedCorrect` | `LinkedWrong` | `ForeignFile` | `ForeignDir` | `ForeignLink` | `Unreadable`
  - 枚举 `Action`: `ReportMissingSource` | `Create` | `Noop` | `RefuseConflict` | `ReplaceAndBackup` | `ReportUnreadable`
  - 函数签名 `fn classify(manifest: &Manifest) -> Vec<EntryState>`, 每个 `EntryState` 同时携带 `Current` 与 `Action` 两个字段
  - **判定顺序必须严格按 `data-model.md` 3.1 的表格自上而下取首个命中者**: `SourceMissing` 最先, 否则源缺失时会给出误导性的"已是最新"或"冲突"
  - 必须用 `symlink_metadata` (lstat), 否则 `ForeignLink` 与 `LinkedWrong` 无法区分
  - **`LinkedWrong` 与 `ForeignLink` 必须互斥且穷尽**: `LinkedWrong` = 链接规范化后落在**仓库内**但 `L != S`; `ForeignLink` = 链接落在**仓库之外**. 二者的分界是链接归属 (`os::link_is_inside_repo` / `os::normalize`), 不是"是否看起来指向正确". **不得**把 `LinkedWrong` 实现为"指向别处 (无论内外)", 那样 `ForeignLink` 将永不可达, 且 `undo` 会错误地移除用户自己的外部链接
  - 动作映射严格按 3.2 表: `SourceMissing` → `ReportMissingSource`; `Absent` → `Create`; `LinkedCorrect` → `Noop`; 其余**四种** (`LinkedWrong` / `ForeignFile` / `ForeignDir` / `ForeignLink`) 按 `on_conflict` 分别映射到 `RefuseConflict` 或 `ReplaceAndBackup`
  - **本函数必须完全只读**, 不得有任何写操作
- [X] T010 实现报告渲染于 `src/render.rs` (FR-016), 依赖 T009.每条目一行, 格式 `<序号>/<总数> <token> <用户书写的 target> <人类可读说明>`, 末尾一行汇总 `<N> 个条目: <a> 成功, <b> 已是最新, <c> 未处理, <d> 失败`. 汇总的四项计数**互斥**, 相加必须恒等于 N; 归类与 token 取值**唯一来源为 `contracts/cli.md` 的"汇总列 / token"对照表与"状态 token 表"**, 下面的复述仅为便于对照, 若与 `contracts/cli.md` 冲突一律以 `contracts/cli.md` 为准 (成功 = `create`+`replace`+`undo`+`restore`; 已是最新 = `ok`; 未处理 = `conflict`+`skip`+`foreign`+`absent`+`wrong`; 失败 = `missing`+`fail`). 第三列取"未处理"而非"跳过": 它是 token 分桶名, 同时收容"有意不处理"(`conflict` / `skip`) 与"没有动作可执行"(`foreign` / `absent` / `wrong`), 后者出现在只读的 `status` 上, 用"跳过"作列名会造成误读 -- 不得出现同一条目计入两列.**token 取值**: `create` / `ok` / `replace` / `conflict` / `absent` / `missing` / `foreign` / `wrong` / `undo` / `restore` / `skip` / `fail`, 不得另起命名. token 由 `(命令, Current, Action)` **三元组**决定, 不是仅由 `Current` 决定: `Current::Absent` 在 `apply` / `preview` 为 `create`, 在 `status` 为 `absent`, 在 `undo` 为 `ok`; `Current::LinkedWrong` / `ForeignLink` / `ForeignFile` / `ForeignDir` 在 `preview` / `apply` 为 `conflict` 或 `replace`, 在 `status` 为 `wrong` / `foreign`, 在 `undo` 为 `skip` 或 `undo` (`missing` 专指 `SourceMissing`; `foreign` 专指链接落在仓库外或非链接占用物). 必现信息: `replace` 与 `restore` 行必须给出备份文件的**完整路径**; `fail` / `skip` / `conflict` / `missing` 行必须说明原因; `wrong` 与 `foreign` (占用物为链接时) 行必须给出**实际指向**; `foreign` 行必须说明占用物是链接,文件还是目录. 展示的 target 必须是用户书写的原始字符串 (不是解析后的绝对路径), 便于对照清单
- [X] T011 实现命令编排与平台门禁于 `src/lib.rs`, 依赖 T004-T010.`pub fn run() -> i32` 的顺序必须严格为: (1) **平台检查放最前**, 早于参数解析与清单读取; 不受支持时向 stderr 输出单行说明 (如 `dotlink: 不支持当前平台 (仅支持 macOS 与 Linux)`), stdout 保持为空, 返回 3, 不触碰文件系统 (FR-019, `research.md` R6); (2) 解析参数; (3) **从当前工作目录**读取并校验 `dotlink.toml`, 不向上搜索父目录 (FR-015); (4) 调用 `classify` 一次; (5) 按子命令分发到 `preview` / `apply` / `status` / `undo` 的实现; (6) 渲染报告并返回退出码.**四个子命令必须消费同一个 `classify` 返回值** —— 这是 FR-017 一致性的结构保证
- [X] T012 [P] 建立端到端测试夹具于 `tests/support/mod.rs`: 提供 `TempEnv` 结构, 创建一个临时目录同时充当仓库根与 `HOME`, 并把子进程的当前工作目录设为仓库根,`HOME` 环境变量设为临时主目录.封装: 写入清单,在仓库中创建源文件/目录,`dotlink` 命令调用 (用 `assert_cmd::Command::cargo_bin("dotlink")`),断言某目标路径是指向某源的符号链接,断言某路径的内容等于给定字符串,列出目录以检测多出的备份文件.夹具必须隔离真实主目录

**Checkpoint**: 基础设施就绪.此后 US1 到 US5 可以并行推进 (若有多人).

---

## Phase 3: User Story 1 - 一条命令把仓库中的配置部署到目标位置 (Priority: P1) 🎯 MVP

**Goal**: 执行一次 `dotlink apply`, 清单中所有条目的链接被创建到正确位置, 必要时自动创建缺失的父目录; 重复执行幂等.

**Independent Test**: 在临时仓库放 2-3 个源文件与对应清单, 执行一次 `apply`, 检查每个目标路径都成为指向仓库内文件的链接; 再执行一次, 确认无任何文件系统改动.

### Tests for User Story 1 ⚠️

> **先写这些测试, 确认它们在实现前失败**

- [X] T013 [P] [US1] 编写 `tests/apply.rs`: (a) 清单含 3 个条目而目标全部不存在时, `apply` 退出码 0 且三行均为 `create`; (b) 每个目标路径都是正确的符号链接, 且 `cat` 能读到仓库内内容; (c) 目标父目录不存在 (如 `~/.config/nvim`) 时被自动创建 (FR-005); (d) 清单含目录条目时建立的是**整目录**软链接 (FR-018); (e) **幂等** (SC-002): 连续执行两次, 第二次全部为 `ok`,退出码 0, 且用 `find`/目录列举确认未产生任何新文件 (含备份文件); (f) 增量: 部署后往清单新增一个条目再执行, 只有新条目为 `create`, 已有条目保持 `ok`; (g) 多个条目声明**同一个源**但目标不同时, 两侧都成功 (合法用法)

- [X] T014 [US1] 实现部署动作于 `src/apply.rs`, 依赖 T012: 对 `Action::Create` 执行"创建缺失的各级父目录 → 建立符号链接"; 对 `Action::Noop` 与 `Action::ReportMissingSource` 不做任何写操作.链接创建必须直接调用 `os::create_symlink`.注解: 单个条目的创建在文件系统层面是原子的 (`research.md` R8), 不需要临时名加 rename
- [X] T015 [US1] 在 `src/lib.rs` 中接入 `apply` 子命令: 逐条目执行 `Action`, **单个条目失败不中断其余条目** (FR-014), 收集每条的结果供渲染. 退出码交由 T004 的 `exit_code_for` 计算 (任一 `fail` 条目使条件 2 为假, 故退 1), **不得在本任务内另写一份退出码判定**
- [X] T016 [US1] 实现逐条目的失败处理与汇总于 `src/lib.rs` 与 `src/render.rs`: 创建失败 (权限不足,父目录无法创建等) 时该行输出 `fail` token 并附带原因, 其余条目继续执行 (FR-014). 退出码由 T004 的 `exit_code_for` 给出, 本任务只负责保证 `fail` 条目被归入"失败"列. 本任务的 `fail` 只覆盖**执行动作时的失败**; 判定阶段的 `Unreadable` 也产出 `fail`, 由 T009 归类, 二者的 token 相同而原因措辞不同 (见 `contracts/cli.md` 的状态 token 表)
- [X] T017 [US1] 在仓库根运行 `cargo test --test apply` (对应 `tests/apply.rs`) 与 `cargo clippy`, 确认 T013 全部通过且无警告

**Checkpoint**: US1 可独立交付.此时 `dotlink apply` 已是一个可用的 MVP.

---

## Phase 4: User Story 2 - 部署前预览将发生的变更 (Priority: P2)

**Goal**: `dotlink preview` 在不修改文件系统的前提下, 报告每条目将会发生什么, 且结论与随后 `apply` 的实际行为一致.

**Independent Test**: 构造"新条目"与"目标已被无关文件占用"两种情况, 执行预览, 确认输出准确区分二者, 且文件系统逐字节未变.

### Tests for User Story 2 ⚠️

> **先写这些测试, 确认它们在实现前失败**

- [X] T018 [P] [US2] 编写 `tests/preview.rs`: (a) **只读** (SC-006): 在含"未部署""已是最新""冲突"三种状态的清单下执行 `preview`, 用 `find` 比对执行前后, 必须完全一致; (b) 目标被普通文件占用时该行为 `conflict` 并说明原因; (c) 目标已是最新的正确链接时该行为 `ok`; (d) **一致性** (FR-017, SC-006): 同一状态下把 `preview` 每行的 token 与随后 `apply` 的实际结果逐条比对, 必须 100% 对应; (e) `preview` 之后文件系统中不得出现备份文件; (f) 空清单 (无条目) 时退出码为 2 并说明 (V-3)

### Implementation for User Story 2

- [X] T019 [US2] 在 `src/lib.rs` 中接入 `preview` 子命令: 调用 `classify` 后渲染 `Current` 与 `Action` **两个字段**, 不执行任何 `Action`.该命令路径下不得出现任何文件系统写操作 (含创建父目录). 退出码交由 T004 的 `exit_code_for` 计算, 谓词与 `apply` **相同**但求值时刻在动手**之前**: 未部署的清单上 `preview` 退 1 而 `apply` 退 0, 这不是不一致 (见 `contracts/cli.md` 的"只读命令与写入命令的对偶")
- [X] T020 [US2] 在 `src/render.rs` 中为 `Action` 增加面向"将来时"的措辞 (如 `create` 行说明"未部署 -> 将创建链接"), 使 `preview` 与 `apply` 在同一 token 下语义清晰可辨 (FR-016)
- [X] T021 [US2] 在仓库根运行 `cargo test --test preview` (对应 `tests/preview.rs`), 确认 T018 全部通过, 特别是 (d) 一致性断言

**Checkpoint**: US1 + US2 均可独立工作.预览可信后, 用户才敢于对真实主目录执行 `apply`.

---

## Phase 5: User Story 3 - 安全地处理已有文件 (Priority: P2)

**Goal**: 目标路径已被非本工具内容占用时默认拒绝覆盖; 当条目声明 `on_conflict = "replace_and_backup"` 时, 先把原内容备份到可定位的位置, 再建立链接.

**Independent Test**: 在目标路径预置含已知内容的文件, 使用允许备份的条目执行部署, 确认链接已建立,原内容在备份位置完整可读; 再验证默认情况下该条目被拒绝且原内容未变.

### Tests for User Story 3 ⚠️

> **先写这些测试, 确认它们在实现前失败**

- [X] T022 [P] [US3] 编写 `tests/conflict.rs`: (a) 目标为普通文件且 `on_conflict` 缺省时, 该行为 `conflict`, 退出码 1, **原文件内容逐字未变**, 且无备份文件产生 (FR-007, SC-004); (b) 目标为普通文件且 `on_conflict = "replace_and_backup"` 时, 该行为 `replace`, 输出**给出备份文件的完整路径**, 该路径内容等于原内容, 且目标已成为正确链接 (FR-008); (c) 目标为**目录** (非链接) 时默认为 `conflict`, 目录及其内容未被删除; (d) 目标是**指向仓库之外**的符号链接时默认为 `conflict` (FR-007); (e) 目标是**指向错误位置**的符号链接且允许备份时, 备份的是**链接本身**而非其目标 (`readlink` 备份文件可验证), 且备份后目标成为正确链接; (f) 目标为目录且允许备份时, 备份为递归副本, 目录内文件在备份位置可读, 且**不残留 `.partial` 文件** (备份一步到位, 目录下只有最终名); (g) 同一清单中"冲突跳过"的条目不影响其他条目正常完成, 退出码为 1 (FR-014); (h) **备份同名消歧** (`data-model.md` 第 4 节的"唯一性"): 对同一目标路径连续两次 `replace_and_backup`, 断言之**前一个备份的内容未被覆盖**且两个备份文件并存 (第二个名称带序号); (i) **备份失败的原子性** (`data-model.md` 第 4 节的"先复制后删除的硬顺序"): 构造一个**复制必然失败**的目录条目 (例如目录内含一个无读权限的子目录, 或其体积超过一个 `ulimit -f` 限制 -- 夹具中优先用前者), 断言该条目报告 `fail`, **原对象完整保留在原路径**, 且磁盘上**不存在**名为 `<原名>.dotlink-backup.*` 的最终备份 (中断残留只能是 `.partial`). 该断言是"顺序颠倒即数据丢失"这一风险的唯一回归防线

### Implementation for User Story 3

- [X] T023 [US3] 实现备份路径构造与查找于 `src/backup.rs` (`research.md` R4): 备份位于**被覆盖路径的同一目录**, 名称为 `<原名>.dotlink-backup.<YYYYMMDDThhmmss>` (本地时区, 秒级, 用 `time` crate).提供 `fn backup_path_for(target: &Path) -> PathBuf` 与 `fn find_latest_backup(target: &Path) -> Result<Option<PathBuf>>` (取同目录下名称以 `<原名>.dotlink-backup.` 开头者中**最新**的一个: 先比时间戳, 同一秒内再比序号数值, 不按整名字典序).**命名消歧** (见 `data-model.md` 第 4 节的"唯一性"): `backup_path_for` 若发现算出的名字已存在, 必须追加序号 `.2`, `.3`... 直到未占用, 绝不覆盖既有备份; 同一秒内对同一路径的两次替换因此产生两个并存备份, 而非后者覆盖前者
- [X] T024 [US3] 实现"复制被覆盖对象"于 `src/backup.rs`: 原对象是文件/目录时递归复制; 原对象是**符号链接时必须复制链接本身而不跟随** (`std::fs::read_link` + `create_symlink`), 否则会备份到链接指向的内容. **写入必须是两步** (见 `data-model.md` 第 4 节的完整性规则): 先复制到进程级唯一的暂存名 `<原名>.dotlink-backup.<YYYYMMDDThhmmss>.<pid>.partial`, 全部复制成功后再原子改名为最终名; 任何中途失败都不得留下一个被误认为完整的备份. `find_latest_backup` 只接受已完整命名的备份 (不含 `.partial`)
- [X] T025 [US3] 实现 `Action::ReplaceAndBackup` 于 `src/apply.rs`: 顺序严格为"先备份 → 确认备份**完整落定并被改名** → 再删除/替换原对象 → 建立新链接"."完整落定"指 `data-model.md` 第 4 节的两步写入已完成 (复制到 `.partial` 后成功改名为最终名); 它未成立之前**绝不允许**删除原对象. 本任务不变量 (由 `tests/conflict.rs` 的 T022(h) 之一断言): 该顺序颠倒时, 递归复制**目录**中途失败会让唯一副本同时消失, 直接违反 SC-004. **若备份失败则不得删除原对象**, 该条目报告 `fail` 并保留原状
- [X] T026 [US3] 在 `src/lib.rs` 中接入 `Action::RefuseConflict`: 不产生任何文件系统改动, 报告 `conflict` 并说明原因是"目标被非本工具创建的内容占用", 退出码 1 (FR-007)
- [X] T027 [US3] 在仓库根运行 `cargo test --test conflict` (对应 `tests/conflict.rs`), 确认 T022 全部通过

**Checkpoint**: 用户可以在真实机器上放心运行 `apply` —— 任何已有内容要么被保留到可定位的备份, 要么跳过并报告, 不存在静默丢弃路径 (SC-004).

---

## Phase 6: User Story 4 - 撤销部署, 让系统回到未管理状态 (Priority: P3)

**Goal**: `dotlink undo` 只移除由本工具创建的链接, 并对有备份的条目恢复原有内容; 绝不触碰其他任何路径.

**Independent Test**: 在"已部署且发生过一次备份"的环境中执行 `undo`, 确认链接被移除,原始内容被恢复,其他文件与目录未被改动.

### Tests for User Story 4 ⚠️

> **先写这些测试, 确认它们在实现前失败**

- [X] T028 [P] [US4] 编写 `tests/undo.rs`: (a) 已部署且曾有备份的条目: `undo` 后链接被移除, 原内容被恢复到原路径, 该行为 `restore` 且给出备份来源路径 (FR-009); (b) 已部署但无备份的条目: 链接被移除, 该行为 `undo`, 原路径不再存在; (c) **安全反例** (SC-005): 部署后手工把目标换成一个真实目录 (非本工具创建), `undo` 必须 `skip` 并说明原因, 目录及其内容**完整保留** (FR-010); (d) 目标被替换成指向仓库外的链接时, `undo` 必须 `skip`, 不删除该链接; (e) 目标被替换成普通文件时 `undo` 只 `skip`, **且不消费备份** (`contracts/cli.md` 的"并存语义"); (f) **幂等** (SC-002 在 undo 上的对偶): 连续执行两次 `undo`, 第二次不得产生错误或多余删除, 且第二次**每行 token 为 `ok`, 退出码为 0** (`Current` 为 `Absent` 归入"已是最新"列, 见 `contracts/cli.md` 的"undo 分支"; 若实现把 `Absent` 归为 `skip`, 本断言必失败); (g) 混合场景: 同一清单中有的条目可撤销,有的须跳过, 可撤销的被处理而跳过的不受影响, 退出码为 1; (h) **归属判定不依赖状态文件**: 整个测试过程中不存在任何 dotlink 自建的状态/索引文件

### Implementation for User Story 4

- [X] T029 [US4] 实现撤销动作于 `src/undo.rs`, 依赖 T023 与 T024: 仅当 `Current` 为 `LinkedCorrect` 或 `LinkedWrong` 时才移除链接. 其余情况**绝不删除**, 但**必须分别取 token**, 不得一律归为 `skip` (`contracts/cli.md` 的"undo 分支"表):
  - `ForeignLink` / `ForeignFile` / `ForeignDir` -> `skip` (该路径非本工具创建)
  - `Absent` -> `ok` (该路径本就没有本工具创建的链接; 这是幂等第二次运行的情形, 必须退 0 而非 1)
  - `SourceMissing` -> `missing` (计入"失败"列)
  - `Unreadable` -> `fail`
  注意: `Current` 的取值**已经**编码了链接归属 -- `LinkedWrong` 蕴含"链接落在仓库内", `ForeignLink` 蕴含"落在仓库外". 因此 `undo` 只需匹配 `Current`, **不需要**再次调用 `os::link_is_inside_repo` (重复判定既冗余, 又可能因两处规范化语义不同而引入分歧)
- [X] T030 [US4] 在 `src/undo.rs` 中实现备份恢复: 移除链接后若同目录存在匹配 `<原名>.dotlink-backup.*` 的备份, 用 `backup::find_latest_backup` 取最新一个并恢复到原路径, 该行报告 `restore` 与备份来源路径 (FR-009).恢复成功后不删除备份文件本身
- [X] T031 [US4] 在 `src/lib.rs` 中接入 `undo` 子命令: 消费同一个 `classify` 返回值, 走撤销分支, 逐条目处理. 退出码交由 T004 的 `exit_code_for` 计算, 输入为 `undo` 的目标谓词 (在动手**之前**求值: "本次要撤销的清单上是否还有本工具创建的链接") 与逐条结果, **不得另写一份**. 谓词在动手前求值这一点由 `contracts/cli.md` 的"`undo` 的谓词在动手之前求值"规定; T036 做最终核对
- [X] T032 [US4] 在仓库根运行 `cargo test --test undo` (对应 `tests/undo.rs`), 确认 T028 全部通过, 尤其是 (c) 与 (e) 两条安全断言

**Checkpoint**: 全部生命周期可用 —— 部署,预览,撤销, 且撤销的破坏面被限制在"本工具自己创建的东西".

---

## Phase 7: User Story 5 - 发现清单与实际状态的不一致 (Priority: P3)

**Goal**: `dotlink status` 只读地报告清单与实际状态的偏离, 区分未部署,指向错误,被非本工具创建的内容占用,源缺失.

**Independent Test**: 构造"部分部署,部分被外部破坏"的环境, 执行 `status`, 确认输出准确分类每种偏离且未修改文件系统.

### Tests for User Story 5 ⚠️

> **先写这些测试, 确认它们在实现前失败**

- [X] T033 [P] [US5] 编写 `tests/status.rs`: (a) 声明了但目标路径不存在 → 输出 token 为 `absent` (`Current::Absent`); 源条目不存在 → token 为 `missing` (`Current::SourceMissing`).两者是不同的 token, 必须分别断言 (FR-011); (b) **归属分界必须分别断言**: 链接落在仓库**外** → token 为 `foreign` 并给出实际指向; 链接落在仓库**内**但源不对 → token 为 `wrong` 并给出实际指向. 二者必须是**不同**的 token (FR-011, `contracts/cli.md` 的必现信息); (c) 目标被普通文件/目录占用 → 报告为 `foreign` 并说明占用物类型; (d) 全部符合时退出码 0, 存在任一偏离时退出码 1 (FR-020); (e) **只读**: `status` 前后文件系统完全一致; (f) 规范化路径陷阱 (`research.md` R2): 链接文本含 `..` (如 `$D/repo/../repo/shell/bashrc`) 或路径位于 macOS 的 `/tmp` 下时, `status` 仍须报告 `ok` 而非 `foreign`. 若实现只做字面前缀比较, 此条必然失败; (g) **悬空链接**: 链接文本指向仓库内一个不存在的文件时, 须报告 `wrong` 而非 `foreign` (验证 T007 的非严格规范化)

### Implementation for User Story 5

- [X] T034 [US5] 在 `src/lib.rs` 中接入 `status` 子命令: 调用 `classify` 后**只渲染 `Current`, 不渲染 `Action`**; 该路径下不得有任何写操作. 退出码交由 T004 的 `exit_code_for` 计算 (谓词为"无任何偏离"), **不得另写一份**
- [X] T035 [US5] 在 `src/render.rs` 中为 `status` 增加 `foreign` / `wrong` / `absent` 三个 token 的渲染, 其中 `wrong` 必须输出链接的实际指向, `absent` 用于"目标路径不存在" (FR-016)
- [X] T036 [US5] 在 `src/lib.rs` 中把四条命令的**实际退出码计算接到 T004 的单一函数上** (FR-020): 各命令把自身的逐条结果与目标谓词交给 `exit_code_for`, **不得另写一份退出码逻辑**. **退 0 的判据唯一来源为 `contracts/cli.md` 的"退出码"小节** (两个条件必须同时成立); 下面的展开仅说明本任务需要为每条命令提供的**目标谓词**输入, 若与之冲突一律以 `contracts/cli.md` 为准:
  - **条件 1 (目标谓词)**: 四个谓词严格按 `contracts/cli.md` 的"各命令的目标谓词"表, 其**求值基准**按 `data-model.md` 3.2a: 谓词以 `Current` 为输入. `Absent` 计为"尚未成为正确链接"; **`SourceMissing` 与 `Unreadable` 不计数** (它们的残留待办只由条件 2 表达). 因此 `preview` 行的"等价于每个条目的 `Action` 均为 `Noop`"必须读作"在**可执行**条目中 `Action` 均为 `Noop`" -- 若把 `SourceMissing` 也算作"Noop", 含源缺失条目的清单上 `preview` 会错退 0 (`quickstart.md` 场景 14 的判别点). **注意 `undo` 的谓词与 `apply` 相反** -- 一份刚部署好的清单上 `apply` 退 0 而 `undo` 退 1, 这是预期行为, 不得为了让四者"看起来一致"而把 `undo` 改成按动作数计. `undo` 的谓词**在动手之前求值** (问"本次要撤销的清单上是否还有本工具创建的链接"), 否则撤销后再求值将恒为 0, "刚部署好则退 1" 与 "撤销干净则退 0" 无法同时成立 (见 `contracts/cli.md` 的"`undo` 的谓词在动手之前求值"). 本命令是四条中**最后**接入的一条, 因此本任务顺带核对前三条接入时的谓词求值时刻是否与本节一致
  - **条件 2 (无遗留待办)**: 全部条目都落在汇总的"成功"或"已是最新"两列, 即不存在任何 `skip` / `conflict` / `foreign` / `absent` / `wrong` / `missing` / `fail` 条目. 该条件独立于条件 1: `SourceMissing` 条目会让四条命令一律退 1 (即使 `undo` 的谓词在它身上已成立), `undo` 遇到真实目录占用 (`skip`) 时同样退 1
  - **不在此函数内的退出码**: `Error::exit_code` (T004) 负责派发**之前**的结局 -- 清单校验失败 (含当前工作目录不可用) 与 `HOME` 无法确定均退 **2**, 平台不受支持退 **3**; `exit_code_for` 只负责逐条结果侧的 0/1. 二者不重复硬编码同一个数字: `Error::exit_code` 是这两个码的唯一来源, `exit_code_for` 不得返回 2 或 3
  - `preview` 与 `apply` 共用条件 1 的同一个谓词, 差别只在求值时刻 (动手前 / 动手后), 因此未部署的清单上 `preview` 退 1 而 `apply` 退 0 -- 这不是不一致
  - 由 T048 的 `tests/exit_codes.rs` 锁定; 实现后必须让其全绿 (该测试的逐条结果链在 T004 后即已转绿, 本任务需让它针对**真实命令**的端到端链同样全绿)
- [X] T037 [US5] 在仓库根运行 `cargo test --test status` (对应 `tests/status.rs`), 确认 T033 全部通过, 尤其 (f)

**Checkpoint**: 五个用户故事全部可独立工作且相互一致.

---

## Phase 8: Polish & Cross-Cutting Concerns

**Purpose**: 跨故事的收尾与规格级验证

- [X] T038 [P] 编写 `tests/manifest_errors.rs` 覆盖 FR-013 的整体拒绝 (V-1 至 V-8 逐条): 清单缺失,TOML 语法错误,`entry` 为空,缺少 `target`,缺少 `source`,两个条目声明同一目标路径,`on_conflict = "yes"` (非法取值),条目内未知键名 (如拼错的 `onconflict`),**顶层未知表** (如 `[foo]`).每条断言退出码为 **2** **且文件系统零改动** (执行前后 `find` 比对完全一致).另补三条:
  - **FR-015 不向上搜索**: 在仓库的子目录中执行 (`cwd` 下无清单, 父目录有), 断言退出码 **2** 且零改动 -- 这是 FR-015 唯一的自动化断言, `quickstart.md` 场景 11 只做手工覆盖
  - **当前工作目录不可用** (FR-015 的边界, `research.md` R5): 子进程在启动前把 `cwd` 设为一个**随后被删除**的目录时, 断言退出码 **2** 且零改动, 且标准错误指出"当前工作目录不可用"而非"找不到清单". 实现手法: 先在临时目录中 `chdir` 再删除该目录 (`std::process::Command::current_dir` 配合预先 `remove_dir_all`), 或在 `tests/support` 中用 `pre_exec` 完成 chdir (该断言只验证"分类正确", 不验证 `clap` 的用法错误路径)
  - **主目录无法确定** (FR-002 的边界, `research.md` R5): 清单含 `~` 开头的 `target` 而子进程未设置 `HOME` 时, 断言退出码 **2** (V-6) 且零改动
  - **`Unreadable` 不是校验失败**: 目标父目录 chmod `000` 使 `lstat` 返回 `EACCES` 时, 断言退出码为 **1** (非 2/3), 该行为 `fail` token, 其余条目正常处理
- [X] T039 在 `tests/apply.rs` 中补充收尾用例 (与 T013 同文件, 故不加 `[P]` 且须在 T013/T017 之后):
  - **SC-003 性能断言** (本清单中唯一需要构建工作量的成功标准, 此前只由 T043 手工覆盖): 生成 50 个条目的清单与源文件, 对 `preview` 与 `apply` 分别断言墙钟时间 < 2 秒, 且输出恰好 50 行逐条目结果. 用 `std::time::Instant` 而非 shell `time`, 使断言可回归
  - 单条目失败不阻塞其余条目 (FR-014): 清单同时含源缺失与正常条目, 正常条目仍被创建且退出码为 1
  - 并发调用不产生指向错误位置的链接 (`research.md` R8 的已知限制, 只断言"不静默产生错误结果", 不断言备份文件的唯一性)
- [X] T040 [P] 为 `src/classify.rs` 补充判定不变量的回归测试.`data-model.md` 3.1 与 3.2 的逐格覆盖已由 T047 在 Phase 2 完成 (先于 T009, 见宪章第三条), 本任务**不重复**那批测试, 只补充在实现过程中暴露出的新边界与不变量:
  - **互斥且穷尽**: 对同一份文件系统状态, 8 种 `Current` 恰好命中一种; 任意符号链接情形不得同时落入 `LinkedWrong` 与 `ForeignLink`; `Unreadable` 只吸收 `lstat` 的**非 `NotFound`** 错误, 不吸收"路径不存在"
  - **判定顺序的负面断言**: 若实现被改成先判目标侧, 则"源缺失 + 目标被占用"会退化为 `conflict`; 用该反例断言实现未被改动
  - **迁移表自洽**: `data-model.md` 3.4 的迁移路径逐条验证 (`Absent` --Create--> `LinkedCorrect`; `LinkedCorrect` --手工改坏--> `ForeignFile` / `ForeignDir` / `ForeignLink`; 回到 `LinkedCorrect`)
  - **`os::normalize` 的三个调用点同源** (`research.md` R2): `source_path` 与 `target_path` 必须经同一函数规范化. 夹具必须走 `manifest::parse` 的真实解析路径, 并让**仓库根路径本身含 `..` 段** (例如仓库真实位于 `<tmp>/repo`, 而传给 `parse` 的 `repo_root` 写作 `<tmp>/repo/../repo`), 链接文本取规范化后的源路径, 断言链接仍为 `LinkedCorrect` 而非 `wrong` -- 若 `source_path` 未过 `normalize`, 该用例必失败.
    **不得**用"把仓库 `mv` 到另一个临时根后断言链接仍为 `ok`"这一形态: 移库后链接文本仍指向旧路径, 按 `data-model.md` 3.1 的 `ForeignLink` 行正确结论是 `ForeignLink`, 该期望与规格冲突且不可实现 (2026-09-15 第六次 `/speckit-analyze` 修正)
  若 T047 已覆盖某项, 则本任务只保留其回归价值, 不新增等价断言
- [X] T041 [P] 编写 `README.md`: 快速上手 (clone → cd 到仓库 → 预览 → 部署), 清单格式说明, 四条命令与**退出码语义表**, 以及 `set -e` 脚本中的推荐写法 `if dotlink preview >/dev/null; then ...; fi` (避免裸调只读命令导致中断, 见 `research.md` R10 的代价说明).**同一任务内为 `--help` 补上同一份说明**: `research.md` R10 要求该代价"必须在 `--help` 与 README 中显式说明", 而四个子命令由 clap 自动生成的帮助文本只含命令与参数, 不含退出码语义.因此需在各子命令的 `about` / `long_about` 中写明三点: (1) 退出 0 当且仅当该命令的目标已达成且无遗留待办; (2) 只读命令在有待办时也退 1, 故 `set -e` 脚本中应使用上面的 `if ... then ... fi` 写法而非裸调; (3) **退出码 2 有两重含义** -- "清单校验失败" 与 "子命令缺失或拼错", 前者报"无法读取清单 ...", 后者是 `clap` 的用法说明 (`unrecognized subcommand`), 二者靠标准错误的内容区分 (见 `contracts/cli.md` 的"调用方式错误"). 第 3 点是 SC-009 "无需查阅文档即可从退出码判断下一步"得以成立的**唯一**依据, 不得省略.该落点此前无任务承接, 本任务即其唯一归属 (FR-020 的 CLI 侧); 同时验证三条须人工判断的成功标准: SC-008 (只读 README 的新用户能否在 5 分钟内完成全流程), SC-007 ("仅凭命令行输出即可判断每个条目的结果与下一步动作") 与 **SC-010** ("解释命令该在哪里运行所需的文字不超过一句话") -- 后两者是主观标准, SC-007 唯一可自动化的部分是"每条 `fail` / `skip` / `conflict` / `missing` 行都含原因", 该部分已由 T010 的渲染要求与 T022 / T038 的断言覆盖; SC-010 由本任务审阅 README 的快速上手段落时逐句确认. 二者的人工确认结果须在 README 审阅记录中留痕
- [X] T042 验证平台门禁 (FR-019) 的两个层面:
  - **可编译性 (本机可验证)**: 运行 `cargo check --target x86_64-pc-windows-gnu` 必须通过 (证明非 unix 空实现有效). **不得**用 `cargo build` 该 target 作为判据 -- 链接 Windows 二进制需要 mingw-w64 工具链 (`dlltool`), 本机实测未安装且失败原因是缺工具而非代码问题 (见 `quickstart.md` 场景 13)
  - **运行时拒绝 (需 CI)**: 在 Windows CI job 中运行二进制, 断言四条命令各自的 stdout **为空**, stderr 含平台说明, 退出码为 **3**, 且文件系统零改动. 这是 FR-019 中"不产生部分输出误导用户"唯一可执行的落点; 若 CI 暂不可用, 必须在 `README.md` 中记录该行为"未在任何环境验证", 不得仅仅因为 `cargo check` 通过就宣称门禁已验证
- [X] T043 按 `specs/001-dotfile-link-management/quickstart.md` 逐场景手工执行全部 **14 个场景**, 记录每个场景的实际退出码与输出, 与文档中的"期望"逐条比对. **场景 4, 7, 9, 10 与 12 开头有状态重置命令与"起始状态"说明, 不得跳过**: 场景 3 残留的 50 条清单会让场景 4 的 `conflict` 断言不成立, 场景 6 留下的两个偏离会让场景 7 / 10 / 12 的"期望 0"不成立, 场景 7 建立的 `$HOME/.config/nvim` 链接会让场景 9 的 `create` 断言不成立. **任一处不符即为缺陷**, 修正代码或该文档后再继续 -- 不得只改文档以迎合实现. 特别复核四处**命令本身是否正确执行** (而非只是输出对上了): 场景 9 (须先 `rm -rf` 掉残留链接, 否则 `create` 断言与 FR-018 的证明都是假阳性), 场景 10 (须确认 `ln -sfn` 成功且 `readlink` 输出确含 `..` -- 若是含 `/private/tmp` 与 `..` 的物理路径形态也算通过, 本场景只要求"链接文本与 `S` 规范化后不同"这一条件成立; 若 `ln` 报 `File exists` 则本场景无效), 场景 6 (确认 `rm -f` 未失败; 该处用 `-f` 是为容忍路径不存在, 不再以"BSD `rm` 拒绝删除目录链接"为由 -- 实测 Darwin 27.0.0 上不带 `-f` 也能删除链接本身), 场景 11 (确认 `cd` 到已删除目录后命令确实以 2 结束). 另复核 `readlink` 的期望值形态与前置说明一致 (逻辑 `cd` 的 shell 下会是 `/tmp/...`, 同样通过). 另需补做三处文档未覆盖的边界: (1) 悬空链接 (链接指向仓库内一个已删除的文件) 在 `status` / `undo` 下须归类为 `wrong` 并允许移除; (2) `quickstart.md` 场景 2 的 `find -cnewer` 断言确实为"无变更"; (3) **备份同名消歧** (`data-model.md` 第 4 节的"唯一性"): 对同一目标路径连续两次 `replace_and_backup` (间隔可短至同一秒, 例如 apply → undo → 再 apply), 断言产生**两个**并存的备份文件 (第二个名字带序号), 先前的备份内容未被覆盖
- [X] T049 逐项复查 `plan.md` 的"实现前门禁" (宪章治理条要求门禁在进入实现前复查一次, 而非只在生成计划时核对). 重点复核 TDD 组四项是否真的成立: 失败测试是否先于实现 (T046/T047/T048), 测试是否经用户验证并确认失败, 实现是否为通过测试的最小代码, 重构是否在测试通过后发生. 未通过项必须修正规范或设计, 而不是在实现中绕过; 无法满足的项记入 `Complexity Tracking` 并说明恢复方案. 同时核对 `plan.md` 的 Constitution Check 表结论与当前设计实况是否仍一致
- [X] T044 在仓库根运行 `cargo test` 全量测试与 `cargo clippy --all-targets -- -D warnings`, 确认零失败零警告; 审阅最终 diff, 确认无规格外的功能,无未使用的公开接口 (内部类型 `Entry` / `EntryState` / `Action` 不应成为 `pub` 对外契约, 见 `src/lib.rs`)
- [X] T045 复核 `specs/001-dotfile-link-management/plan.md` 的 Technical Context 与实际实现一致 (该处的"命令面"描述已在生成任务时修正, 本任务只做逐项复核: 依赖版本,目标平台,性能目标,结构决策是否与实际交付相符), 若有出入则更新 plan.md 或代码使二者一致
- [X] T057 [P] 新增两个横切契约的端到端测试文件:
  - `tests/unreadable.rs` (`contracts/cli.md` 的 `Unreadable` 行, FR-011): 用 chmod `000` 的父目录构造 `lstat` 的 `EACCES`, 断言**四条命令**下该条目 token 均为 `fail`, 汇总"失败"列为 1, 退出码为 **1** (非 2/3), 且文件系统零改动. 以 root 运行时该权限位无效, 需在非 root 环境执行或跳过 (与 T047 的处理相同). 本任务补的是**跨命令**的一致性与退出码, T047 只覆盖 `classify` 的内部判定
  - `tests/usage_errors.rs` (`contracts/cli.md` 的"调用方式错误", FR-020): 断言 `dotlink` (无子命令), `dotlink statu` (拼错) 与 `dotlink --bogus` 三者均退出 **2**, 标准错误含用法说明 (`unrecognized subcommand` 或等价的 clap 措辞), 标准输出为空, 文件系统零改动; 并断言 `dotlink -h` 与 `dotlink -V` 退出 **0** 且标准输出非空. 该文件使 "退出码 2 的两重含义靠标准错误区分" 这一 SC-009 的依据成为可回归的断言

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: 无依赖, 可立即开始
- **Foundational (Phase 2)**: 依赖 Setup 完成 —— **阻塞所有用户故事**
- **User Stories (Phase 3+)**: 均依赖 Foundational 完成.此后可按优先级顺序推进, 或并行
- **Polish (Phase 8)**: 依赖所需用户故事完成

### Critical Path (阻塞关系最紧的链条)

```
T001 → T002 → T006 ─┬→ T046 → T008 ┐
                    ├→ T047 → T009 ─┼→ T010 → T011 → T012 → T013 → T014 → T015 → ... → T044
        T007 ──────┘               │
        T004, T005 ────────────────┘
```

`T009` (`classify`) 是全项目的收窄点: 它之后的**每一个**故事都依赖它, 且它自身依赖 T006 (路径解析) 与 T007 (归属判定).若进度紧张, 优先保证 T006/T007/T009 正确 —— 它们承载了 `research.md` R2 与 R5 两处最容易出错的判定, 而 T047 是它们的第一道防线.

`T046` / `T047` / `T048` 是红阶段测试, 均位于各自实现之前 (宪章第三条).其中 `T048` 的逐条结果链约束 T004 的 `exit_code_for` (先写测试, 该函数尚不存在即为红), 端到端链约束 T015 / T016 / T031 / T034 / T036 的真实命令接入: 因此它在 T004 之后即部分转绿, 其余部分随各命令的接入逐步转绿.

### User Story Dependencies

- **US1 (P1)**: 依赖 Foundational.**不依赖任何其他故事** —— 单独交付即为可用 MVP
- **US2 (P2)**: 依赖 Foundational.**不依赖 US1**: `preview` 只需 `classify` 的 `Action` 字段, 完全不调用 `apply`
- **US3 (P2)**: 依赖 Foundational.修改 `src/apply.rs` (与 US1 同文件, 故 T025 必须在 T014 之后), 但功能上独立可测
- **US4 (P3)**: 依赖 Foundational 与 `src/backup.rs` (T023/T024 由 US3 引入).**这是唯一一处跨故事的真实依赖**: 若无备份可恢复, US4 的 `restore` 分支无从验证.若必须让 US4 先行, 可只实现 T029 (移除链接) 而不实现 T030 (恢复备份)
- **US5 (P3)**: 依赖 Foundational.**不依赖任何其他故事**: `status` 只渲染 `Current`

### Within Each User Story

- 测试任务必须**先写并确认失败**, 再实现
- `src/lib.rs` 被多个故事修改 (接入各自子命令), 因此 T015 / T019 / T031 / T034 之间**不可并行**, 须串行
- `src/render.rs` 同理: T010 / T020 / T035 须串行

### Parallel Opportunities

- Setup 阶段: T003 与 T002 可并行 (不同文件)
- Foundational 阶段: T005,T006,T007,T012 可并行 (各自独立文件); 测试任务 T046,T047 写完后也可与彼此的后续并行 (两者是不同文件, 且都改不到 `src/`)
- **红阶段不参与并行**: T046 必须早于 T008, T047 必须早于 T009, T048 必须早于 T004 的退出码函数与 T014/T015/T016.它们可与"另一个模块的绿灯工作"并行 (如 T047 在跑时去做 T046→T008 这条链), 但绝不可与被它约束的实现任务并行
- **各故事的测试文件天然可并行**: T013 / T018 / T022 / T028 / T033 是五个不同文件, 可同时编写
- 各故事的**领域模块**可并行: `src/apply.rs`,`src/undo.rs`,`src/backup.rs` 互不相同
- Polish 阶段: T038 / T039 / T040 / T041 / T057 可并行

### Files Touched by More Than One Task (串行约束来源)

| 文件 | 涉及任务 | 约束 |
|------|----------|------|
| `src/lib.rs` | T011, T015, T016, T019, T026, T031, T034, T036 | 全部串行 |
| `src/render.rs` | T010, T020, T035 | 全部串行 |
| `src/apply.rs` | T014, T025 | T025 在 T014 之后 |
| `src/backup.rs` | T023, T024 | T024 在 T023 之后 |

---

## Parallel Example: User Story 1

```bash
# 先并行写好 US1 与 US2 的测试 (两者是不同文件, 且都改不到 src/):
Task: "编写 tests/apply.rs 覆盖部署, 幂等, 增量, 整目录链接"
Task: "编写 tests/preview.rs 覆盖只读性, 冲突识别, 与 apply 的结论一致性"

# 再并行实现 US1 与 US2 各自的领域模块 (不碰 lib.rs):
Task: "实现 src/apply.rs 的 Action::Create"
Task: "在 src/render.rs 中增加 preview 的将来时措辞"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. 完成 Phase 1: Setup
2. 完成 Phase 2: Foundational (**关键** —— 阻塞所有故事)
3. 完成 Phase 3: US1
4. **停下来验证**: 用 `tests/apply.rs` 单独验证 US1, 并手工跑 `quickstart.md` 场景 1,2,3
5. 此时已可对自己的一小部分 dotfile 试用

### Incremental Delivery

1. Setup + Foundational → 基础就绪
2. + US1 → 可部署 (MVP)
3. + US2 → 部署前可预览, **此时才建议对真实主目录使用**
4. + US3 → 已有文件被安全处理, 可在有历史配置的机器上使用
5. + US4 → 可回滚
6. + US5 → 可巡检
7. 每个故事均可独立验证, 不破坏之前的故事

### 推荐的完成顺序

严格按 P1 → P2 → P2 → P3 → P3.US3 与 US2 同为 P2, 但建议 **US2 先于 US3**: `preview` 的实现成本远低于备份逻辑, 却能让后续所有关于"备份是否正确"的调试都变得可观察.

---

## Notes

- `[P]` 表示不同文件且无未完成依赖
- `[Story]` 标签用于追溯任务到具体用户故事
- 每个用户故事都应可独立完成与独立验证
- 实现前先确认测试失败
- 每完成一个任务或一组逻辑相关任务即提交 (遵循项目的 Conventional Commits 规则)
- 可在任意 Checkpoint 停下, 独立验证该故事
- **避免**: 模糊任务,同文件冲突,破坏故事独立性的跨故事依赖
- 规格约束提醒: 不得引入状态/索引文件 (FR-012), 不得提供 `--repo` / `--manifest` / `--force` / `--home` (FR-015, `contracts/cli.md`), 不得向上搜索清单 (FR-015), 不得静默降级平台支持 (FR-019).关于 `--json`: 不提供它的依据是宪章第二条 (CLI 支持 JSON) 的**已记录偏离**, 见 `plan.md` 的 Complexity Tracking 与 `research.md` R9 -- 不是 FR 层面的禁令, 故恢复方案是"真实需求出现时以加法形式引入"
- 逐条结果侧的退出码只有一处实现: `exit_code_for` (T004) 是四个命令共用的**唯一**路径, 任何命令的接入任务都不得另写一份 (T015 / T016 / T031 / T034 / T036). 派发**之前**的结局另有唯一来源 `Error::exit_code` (T004 同一任务, 退 2 或 3), 二者分工见 T036: `exit_code_for` 不得返回 2 或 3, `Error::exit_code` 只处理派发前的错误
- 退出码 2 有**两重含义** (清单校验失败 / 调用方式错误), 二者的可区分性由标准错误承担, 由 T057 的 `tests/usage_errors.rs` 锁定; 这是 SC-009 得以成立的依据, 不得省略

## 2026-10-13 审查回归修复

- [X] T060 [US4] 添加暂存名称占用, 回滚目标占用, 恢复落位占用与无权限依赖的失败重试回归测试, 确认旧行为失败 (FR-009 / FR-010 / SC-004).
- [X] T061 [US4] 引入平台原子不覆盖移动, 暂存名称冲突递增消歧, 回滚冲突保留链接并报告原因与位置 (FR-009 / FR-010 / FR-014).

---

## Phase 9: Convergence

**来源**: `/speckit-converge` 对 `src/` 与 `contracts/` 的现状复核 (`/speckit-implement` 之后). 两条发现皆为"该规则在契约与代码中的表述不一致", 与既有任务不重复, 故不修改上文任何一项.

- [X] T058 修正 `target` 中"以 `~` 开头但**不**以 `~/` 开头"这一形态的解析规则 per FR-002 / `contracts/manifest.md` 的 "`target` 解析规则" 表 (contradicts). **实况**: 该表首行为 "以 `~` 开头 -> 展开为当前用户主目录 (`HOME`)", 而 `src/entry.rs` 的 `expand` 只识别 `~/...` 与裸 `~`, 其余形态落进 "相对路径 -> 相对仓库根" 分支. **后果 (已实测)**: `target = "~.bashrc"` (漏写斜杠的常见笔误) 会在**仓库根内**静默创建一个名为 `~.bashrc` 的路径并报告 `create` / 退出码 0; `target = "~foo/bar"` 同样在仓库内建出 `~foo/` 目录. 报告第三列展示的是用户书写的原始字符串, 因此输出里看不出落点已经错了 -- 而落点恰是用户受版本控制的 dotfile 仓库. 这与本项目在 V-8 处确立的立场 ("拼错的键静默退化比报错危险得多") 直接冲突. **处置 (二者择一, 选定后须回写 `contracts/manifest.md` 的该表, `data-model.md` 的 V-6, `README.md` 的 "`target` 的三种形态" 表, 并补一条端到端断言)**: (a) 视该形态为主目录简写并展开为 `HOME` (严格按契约表的字面); 或 (b) 视其为无法解析为绝对路径, 按 V-6 整体拒绝并指出条目下标与非法形态. **不得**保持现状. `source` 侧共用同一函数, 但不受本条影响 -- 见 T059
  - **2026-10-11 落实**: 选定 (b). `src/entry.rs` 新增 `expand_target`, 对 `written.starts_with('~')` 且非 `~/...` / 裸 `~` 的形态返回 `ManifestInvalid`. 已回写 `contracts/manifest.md` 的 `target` 解析规则表 (新增"其他以 `~` 开头的形态 -> 拒绝"行), `data-model.md` 的 V-6, `README.md` 的形态表说明. 端到端断言: `tests/manifest_errors.rs::v6_a_tilde_without_slash_target_is_rejected` (四条命令退 2 + 零改动 + 指出下标); 单测 `entry::tests::target_tilde_without_slash_is_rejected`
- [X] T059 处置 `source` 侧超出契约的路径形态 per `contracts/manifest.md` 的字段表 / `data-model.md` 第 2 节 (unrequested). **实况**: `src/entry.rs` 对 `source` 与 `target` 调用同一个 `expand`, 因此 `source` 也接受 `~/src/rc` 与 `/etc/hosts` 这类形态 (`src/entry.rs` 的单元测试 `source_side_also_supports_tilde_and_absolute_paths` 已把它们锁定为期望行为, 实测 `source = "/etc/hosts"` 亦能建成链接), 而两份契约都把 `source` 定义为 "仓库内的相对路径", `data-model.md` 2 节更把 `source_path` 描述为 "仓库根 + `source`". 该能力既未在规格中要求, 也未在任何契约中记录, 属宪章第八条所称的 "规范与实现不一致". **处置 (二者择一)**: (a) 在 `contracts/manifest.md` 与 `data-model.md` 中显式记录该形态及其语义, 并说明它为何不破坏 "条目来自仓库" 这一心智模型; 或 (b) 让 `source` 只走 "相对仓库根" 一条分支, 对 `~` 与绝对形态按 V-4 / V-6 拒绝. **不得**保持 "代码支持而文档不提" 的状态
  - **2026-10-11 落实**: 选定 (b). `src/entry.rs` 新增 `expand_source`, 对 `~` 前缀与绝对路径返回 `ManifestInvalid`. 删除旧单测 `source_side_also_supports_tilde_and_absolute_paths`, 新增 `entry::tests::source_must_be_a_relative_path_inside_the_repo`; 端到端断言: `tests/manifest_errors.rs::v4_a_source_outside_the_repo_shape_is_rejected`. `contracts/manifest.md` 字段表与 `data-model.md` 第 2 节同步注明 `source` 不接受 `~` 前缀或绝对路径. 另: `src/manifest.rs` 的错误包装去掉了 "无法解析为绝对路径" 前缀 (它只适用于 target), 改由 `expand_target` / `expand_source` 自带说明

# Implementation Plan: 声明式 dotfile 软链接管理

## 2026-10-13 审查回归修复

- macOS/Linux 通过 `rustix` 的 `renameat_with(..., NOREPLACE)` 执行撤销暂存, 恢复落位及失败回滚; 其他平台提供拒绝实现. 暂存名称冲突时递增序号重试, 其他错误直接上报, 不使用可覆盖的回退操作.
- 回滚冲突保留占用物与暂存链接, 同时报告恢复原因, 回滚原因及暂存位置. 回滚分支先原样提取为私有函数, 以真实文件系统稳定验证冲突, 不依赖线程计时或权限模拟.
- 回归测试先确认旧行为失败; 完成后运行全套测试, clippy, 格式检查及 Linux/Windows 交叉编译检查.

**Branch**: `001-dotfile-link-management` | **Date**: 2026-09-14 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/001-dotfile-link-management/spec.md`

**Status**: 已吸收六轮一致性审查的修正 (未决项: 无; 宪章第二条 `--json` 偏离已记录于 Complexity Tracking).本轮 (2026-09-15 第六次 `/speckit-implement` 的收尾复核) 修正了 2 类共 4 处 (详见下方"设计修正"表的第六次批次): `os::normalize` 的语义与 R2 实测要求自相矛盾 (`research.md` R2 的 Decision 只描述词法规范化, 但 R2 自己引用的 macOS `/tmp` 别名实测要求解析符号链接, 二者不可由纯词法实现同时满足), 以及 T040 的移库夹具期望与 `data-model.md` 3.1 冲突且不可实现.同时把 plan.md 的 `clap` 版本由 4.6.6 校正为 `Cargo.lock` 实测的 4.6.7.本轮 (2026-09-15 第五次 `/speckit-analyze`) 修正了 4 类共 10 处 (详见下方"设计修正"表的第五次批次): 三处 `quickstart.md` 中**命令本身无法按期望执行**的缺陷 (场景 10 的 `ln -s` 缺 `-f` 使 R2 规范化陷阱的端到端验证从未被触发, 场景 6 的 `rm` 在目录链接上必然失败, 场景 9 的 `create` 断言因残留链接而不成立从而未证明 FR-018 / FR-014), 累积状态导致的期望偏差 (场景 4 / 5 / 7 / 10 / 12 / 14 的逐行期望与起始状态), 四处规格与契约缺口 (`source_path` 未要求过 `os::normalize`; 目标谓词对 `Absent` / `SourceMissing` / `Unreadable` 的计数基准未定义; 子命令缺失或拼错的退出码路径与"清单校验失败"共用码值 2 却未被定义, 使 SC-009 不成立; 备份的"先复制后删除"顺序未钉死), 以及未定义键规则缺少 V-8 条目.本轮新增 `tasks.md` 的 T057 (两个横切契约测试文件 `tests/unreadable.rs` 与 `tests/usage_errors.rs`), 并为 `spec.md` 的 FR-015 / FR-020 / SC-009 与两个 Edge Case 补上对应约束.本轮 (2026-09-14 第二次 `/speckit-analyze`) 修正了 token 归属的四处相互矛盾 -- `undo` 下的 `Absent` 取 `ok`, `conflict` 仅出现于 `preview` / `apply`, token 由 `(命令, Current, Action)` 三元组唯一决定, 以及新增 `Unreadable` 判定以覆盖"判定阶段读取失败"; 同时修正了 `quickstart.md` 场景 10 与场景 12 因场景 9 残留源缺失清单而无法成立的"期望 0".本轮 (2026-09-14 第三次 `/speckit-analyze`) 修正了 `quickstart.md` 中三处无法按文档期望执行的场景 -- 场景 4 未复原场景 3 的 50 条清单且 `printf` 会穿过场景 1 残留的链接写进仓库源 (应为 `conflict` 却得 `ok`), 场景 7 未清除场景 6 留下的普通文件 `$HOME/.bashrc` (其"期望 0"不成立; 原行 231 的说明是错位遗留, 已移到场景 7 并补上对应命令), 以及场景 10 / 12 起始状态描述与实际不符; 同时补齐 `data-model.md` 中 `undo` 下 `Absent -> ok` 这一决定撤销幂等的映射 (此前只存在于 `contracts/cli.md`), 把 `data-model.md` 与 `tasks.md` 中重复的 token / 汇总 / 退出码表述改为对 `contracts/cli.md` 的引用, 澄清 FR-018 的落点为"处理方式不同"而非输出类型标签, 并新增备份命名消歧规则 (同一秒内两次替换不得覆盖既有备份; 落点 `data-model.md` 第 4 节, `research.md` R4, `tasks.md` T023 / T022(h) / T043).本轮 (2026-09-14 第四次 `/speckit-analyze`) 修正了 12 处 (详见下方"设计修正"表的第四次批次): `Unreadable` 的求值语义 (两次 `lstat` 与它的优先权), `undo` / `restore` token 机制的三处互相矛盾, FR-017 的一致性范围 (三条路径 -> 四条), T048 红阶段测试跨越四个阶段的排期, `--help` 的退出码说明无任务承接, `source_kind` 无消费者, 汇总列名"跳过"对只读命令的误导, `preview` 谓词中"需要执行动作"未定义, 相对 `target` 路径形态未被样例覆盖, 以及 `quickstart.md` 场景 10 与场景 12 各一处无法按文档期望执行的命令.上一轮已落地的修正为: 退出码契约的两条件规则, 备份完整性, 以及 `manifest` / `classify` 的测试先于实现 (T046/T047).

## Summary

实现一个名为 `dotlink` 的命令行工具: 读取仓库根目录下的一份声明式清单 (TOML), 将清单中每个条目在目标路径上建立指向仓库内源条目的符号链接.核心机制是"先判定,后动作": 一个共享的条目状态判定模块把所有条目归类为 未部署 / 已是最新 / 指向错误 / 冲突占用 / 源缺失, 再由 `preview` (只读),`apply` (执行),`status` (只读),`undo` (撤销) 四个动作消费同一判定结果, 从而保证四条路径的结论一致 (FR-017).工具不维护任何状态文件: "某个链接是否由本工具创建"完全由"该路径是符号链接且其指向落在仓库根目录内"推导得出 (FR-012), 这也使撤销天然只触碰自己的产物 (FR-010).

## Technical Context

**Language/Version**: Rust 1.97.0 (edition 2024).本机实测 `rustc 1.97.0 (2d8144b78 2026-07-07)`

**Primary Dependencies**: `clap` 4.6 (derive, CLI 解析), `serde` 1.0 + `toml` 1.1 (清单解析), `thiserror` 2.0 (库错误类型), `time` 0.3 (备份文件名的可读时间戳).已在本机解析确认可用的精确版本 (`Cargo.lock` 实测): `clap` 4.6.7, `toml` 1.1.8+spec-1.1.0, `serde` 1.0.229, `thiserror` 2.0.21, `time` 0.3.55 (features `formatting`, `local-offset`)

**Storage**: 无数据库,无状态文件.唯一持久化产物是被覆盖内容的备份副本, 存放在被覆盖路径的同一目录下, 文件名形如 `<原名>.dotlink-backup.<YYYYMMDDThhmmss>`.清单位置与仓库根目录也没有配置面: 二者都由"当前工作目录下的 `dotlink.toml`"唯一确定 (FR-015)

**Testing**: `cargo test`.`assert_cmd` 2.2 + `predicates` 3.1 驱动二进制端到端测试, `tempfile` 3.27 提供临时仓库与临时 HOME; `classify` 模块由单元测试直接覆盖.已核实版本: `assert_cmd` 2.2.2, `predicates` 3.1.4, `tempfile` 3.27.0

**Target Platform**: macOS 与 Linux (本机为 `aarch64-apple-darwin`).Windows 为完全排除项: 二进制在 Windows 上必须可编译, 但运行时立即以退出码 3 拒绝执行 (FR-019).可编译性用 `cargo check --target x86_64-pc-windows-gnu` 验证 -- 本机实测该命令无需 mingw-w64 工具链即可通过, 而 `cargo build` 该 target 需要 `dlltool` 因而在本机不可用

**Project Type**: 单一可执行程序 (single binary CLI), 采用 `lib.rs` + 薄 `main.rs` 结构以便单元测试直接调用内部模块

**Performance Goals**: 含 50 个条目的清单, `preview` 与 `apply` 均在 2 秒内完成并输出逐条目结果 (SC-003).条目处理为逐条文件系统元数据操作, 无网络与无进程外调用, 该目标有数量级余量

**Constraints**: 零网络访问; 不读取任何外部服务; 所有文件系统改动必须可被单个条目级操作原子替换完成; 无交互式提示 (全部行为由清单决定, 以支持脚本化); 退出码是脚本可依赖的对外契约 (FR-020), 只读命令也用它表达"有待办"

**Scale/Scope**: 单用户,单机,单仓库; 典型清单 10 到 50 个条目; 命令面为 4 个子命令 (`preview` / `apply` / `status` / `undo`), 无命令行参数 (仅 `-h` / `-V`); 配置面仅"当前工作目录下的 `dotlink.toml`"一处

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

`.specify/memory/constitution.md` 已于 2026-09-14 批准为 **1.0.0** (八条核心原则 + 七组"实现前门禁").下表逐条核对, 结论以设计与规格的实况为依据, 不以意图为依据.

### 与宪章逐条核对

| 原则 | 门禁要点 | 结论 | 依据 |
|------|----------|------|------|
| I. 深模块 | 公开接口最小; 复杂度内藏; 功能先作为独立库 | PASS | `classify` 是唯一判定入口, 签名 `classify(&Manifest) -> Vec<EntryState>`; 调用者只需读结论, 不需要知道"什么算冲突".`lib.rs` + 薄 `main.rs` 使全部逻辑位于可被单元测试直接调用的库中 |
| II. CLI 验证接口 | 提供轻量 CLI; 文本进出; 支持 JSON 用于结构化交换 | **DEVIATION** | 本交付物本身即为 CLI, 文本进出与"逐条目结论 + 退出码"构成的稳定契约由 `contracts/cli.md` 的状态 token 表承担, agent 无需解析自由文本.**但未提供 `--json`** -- 理由与恢复方案见 `Complexity Tracking` |
| III. TDD (不可协商) | 测试先于实现并确认失败; 合同测试先于实现 | PASS | `tasks.md` 把每个故事的测试任务置于该故事实现之前; `tests/` 直接断言 `contracts/cli.md` 的退出码与状态 token 契约.**顺序约束**: 覆盖 `classify` 的单元测试与覆盖清单校验 (V-1 到 V-7) 的契约测试必须先于 `src/classify.rs` / `src/manifest.rs` 完成并确认失败 |
| IV. 代码即文档 | 命名表达意图; 函数单一职责; 结构替代注释 | PASS | 模块按"变化原因"而非"行数"命名 (解析 / 判定 / 动作 / 呈现); `backup_path_for` / `find_latest_backup` / `link_is_inside_repo` 等函数名自解释 |
| V. 最小注释 | 只解释代码表达不了的"为什么" | PASS | 设计中确实需要注释的只有两处非显而易见的"为什么" -- `research.md` R2 的规范化陷阱与 R6 的"可编译 + 运行时拒绝"组合; 二者记于设计文档, 不散落为代码注释 |
| VI. 简单性与反抽象 | 不超过 3 个项目; 不包装框架; 无投机抽象 | PASS | 单一 crate (lib + bin), 1 个项目; 直接使用 `clap` / `serde` / `toml`, 未做包装; 无 trait 抽象层,无工厂,无输出格式开关 |
| VII. 集成优先测试 | 真实实例而非 mock; 合同测试先于实现; 验证真实行为 | PASS | 测试用真实文件系统与真实二进制进程 (`assert_cmd` + 临时 `HOME`), 不 mock 文件系统; 本工具无外部服务可 mock; `tests/manifest_errors.rs` 逐条验证 `cli.md` 的退出码契约 |
| VIII. 持续精炼 | 规范 / 计划 / 代码持续一致; 歧义回写最上游 | PASS | 本次规划已修正并回写 spec / data-model / cli / quickstart 之间的四处实质矛盾, 见下方"设计修正" |

### 规格自身的范围约束

| 检查项 | 结论 | 依据 |
|--------|------|------|
| 未实现规格之外的功能 | PASS | 范围严格对齐 FR-001 到 FR-020; 模板渲染,加密,Git 操作,反向生成清单,Windows 支持均已在规格中显式排除 |
| 不预建假想扩展点 | PASS | 不引入插件机制,不预留输出格式开关,不设计状态数据库; 唯一超出最小集的抽象是共享判定模块, 而它由 FR-017 强制要求 |
| 依赖显式,副作用集中 | PASS | 所有文件系统写入集中在 `apply` / `undo` 两个模块; `classify` 与 `preview` / `status` 为纯只读 |
| 失败语义属于接口 | PASS | 退出码与逐条目结果构成对外契约 (见 `contracts/cli.md`); 单条目失败不吞错, 携带上下文上抛 (FR-014) |
| 内部类型不外泄 | PASS | `Entry` / `EntryState` / `Action` 为 crate 内部类型, 仅 `run()` 的退出码与标准输出为公开契约 |
| 不新建平行实现 | PASS | 四条命令共用同一 `classify` 结果与同一 `render` 渲染器, 不存在第二套判定或输出逻辑 |

**Gate 结论**: 除第二条第 2 款 (CLI 支持 JSON) 一项有据可查的偏离外, 全部门禁 PASS.该偏离已记入 `Complexity Tracking`.

### 设计修正 (Phase 0/1 复核产出)

复核 `spec.md` / `data-model.md` / `contracts/` / `quickstart.md` 后修正了下述实质矛盾.修正均回写到受影响的最上游文档 (宪章第八条):

| 修正 | 原状 | 处置 |
|------|------|------|
| 链接归属的状态归类 | `data-model.md` 3.1 把 `ForeignLink` 排在 `LinkedWrong` 之后, 而前者的判定条件被后者完全覆盖, 使 `ForeignLink` 永不可达; 同时 `spec.md` US5 场景 2 把"链接指向仓库外"称为"指向错误", `cli.md` 的 `wrong` token 却描述为"指向仓库之外的别处" | `LinkedWrong` 收窄为"链接规范化后落在**仓库内**但目标不对", `ForeignLink` 收窄为"链接落在**仓库之外**"因而可达; 二者分别对应 token `wrong` 与 `foreign`; FR-011 与 US5 场景按此改写 |
| 退出码与汇总格式 | `cli.md` 的样例汇总行把同一条目同时计入两列 (1 成功 + 1 已是最新 = 2, 总数却是 4, 而该例实际 1 成功 / 1 已是最新 / 1 未处理 / 1 失败 需要 5); 且 `data-model.md` 把退出码 0 描述为"新建数为 0"与 `cli.md` 的"有待办则 1"互相矛盾 | 汇总行定为四项**互斥**计数 (相加等于总条目数) 并附 token 归类表; 退出码统一为"结束时刻该命令的目标是否达成, 且无遗留待办", 与逐行 token 解耦 (后续对"无遗留待办"这一条件的完整化见下一行) |
| quickstart 中不可执行的命令 | 场景 2 用 `find "$HOME" -newer dotlink.toml` 断言"无新增文件", 但 `$HOME` 目录自身的 mtime 必然晚于清单, 该断言恒为假; 场景 4 的 `sed -i.bak` 在 GNU sed 下会把 `i.bak` 解析为脚本而非备份后缀 | 场景 2 改用 `find "$HOME" -cnewer dotlink.toml`; 场景 4 改用 heredoc 直接写出清单, 不经 `sed` |
| Windows 可编译性的验证手段 | `quickstart.md` 场景 13 与 `tasks.md` T042 要求 `cargo build --target x86_64-pc-windows-gnu`, 该命令需要 mingw-w64 工具链 (`dlltool`) | 改为 `cargo check --target x86_64-pc-windows-gnu` (本机实测无需交叉工具链即可通过), 并保留"运行时拒绝行为需在 Windows 环境或 CI 中确认"的说明 |
| 标点规范 | 9 个产物共 534 处全角标点 (U+3002 与 U+3001), 违反用户全局规则 | 全部规范化为半角; 所有 fenced code block 内原本即无违规, 故属纯散文层面的机械修正 |
| 退出码语义未覆盖"无法通过动作消除"的条目 | `cli.md` 把退出码 1 描述为"存在未达成目标的条目", 而 `undo` / `preview` 的目标谓词对 `SourceMissing` 条目天然成立 (它上面没有本工具创建的链接, 也没有动作待执行); 同时 `cli.md` 断言 `preview` 与 `apply` 的退出码"必然相同", 该断言在含 `missing` 条目的清单上不成立. `undo` 遇到真实目录占用而 `skip` 时也无从表达 | 退出码改为**两个条件同时成立**才退 0: 条件 1 是该命令的目标谓词, 条件 2 是无遗留待办 (全部条目落在"成功"或"已是最新"两列). `preview` / `apply` 共用同一谓词但求值时刻不同 (动手前 / 动手后); `SourceMissing` 恒使退出码为 1; 只读命令的"有待办"语义不变. 已补 `quickstart.md` 场景 14 专门验证条件 2 单独生效 |
| 备份的完整性无保证 | `data-model.md` 第 4 节与 R4 只规定"取最新者 (时间戳 + 序号数值)", 但递归复制一个目录可能中途中断, `find_latest_backup` 无法分辨完整与不完整的备份, `undo` 会把半成品当作完整备份恢复, 直接威胁 SC-004 | 备份写入改为两步: 先复制到 `.partial` 临时名, 完成后原子改名; `find_latest_backup` 只接受已完整命名的备份. 中断残留计入 R4 的已知限制 |
| (2026-09-14 第四次 `/speckit-analyze`) `Unreadable` 的求值语义缺失 | `data-model.md` 3.1 的"自上而下取首个命中者"约束无法表达 `Unreadable` -- 它是**错误驱动**的行 (非 `NotFound` 的 `lstat` 失败), 其条件在源侧或目标侧失败时既非真亦非假. 按字面实现会把 `EACCES` 读作"路径不存在"从而落入 `Absent` / `SourceMissing`, 与 FR-011 的"无法判定"结局冲突 | 3.1 补"求值语义": 明确两次 `lstat` (源侧 / 目标侧) 与 `Unreadable` 的**优先权** -- 任一非 `NotFound` 失败立即产出 `Unreadable` 并标明失败侧, 先于表中所列各行; "顺序取首个命中者"只在两次 `lstat` 均成功时成立. T047 补两条断言 (源侧 `EACCES` + 目标侧正常; 目标侧 `EACCES` + 源侧缺失) |
| (同上) `undo` / `restore` 的 token 机制三处互相矛盾 | `contracts/cli.md` 的 token 表把二者写成"`Action` 为移除 / 移除并恢复", 而 `data-model.md` 3.2 与 T009 的 `Action` 枚举只有 6 个取值, **不含移除类**; `3.3` 又明说"`undo` 的 token 不按 `Action` 导出" | token 表改为按 `(Current, 同目录是否有备份)` 定义, 并新增一段显式声明 `undo` / `restore` 不由 `Action` 导出, 消除对不存在的 `Action` 取值的引用 |
| (同上) FR-017 的一致性范围只写三条路径 | `spec.md` FR-017 写"预览,检查与部署三条路径", 而 `plan.md` Summary,`data-model.md` 3.3 与 T011 一致声明**四条**命令消费同一个 `classify` 返回值 (含 `undo`); `research.md` R7 的 Rationale 同样写"三条" | FR-017 改为四条, 并说明 `undo` 的"结论"指它据以决定是否移除链接的那项判定; `research.md` R7 与 `plan.md` Summary 同步改为四条 |
| (同上) T048 的红阶段测试跨越四个阶段 | T048 是退出码契约的红测试, 标为 `[P] [US1]` 置于 Phase 3, 但它要约束的"逐条结果 + 目标谓词导出退出码"函数 (T036) 在 Phase 7. 于是它从 T015 起直到 Phase 7 长期为红, 与宪章第三条的红-绿闭环及 T048 自称的"先于 T015/T016"矛盾 | 把单一退出码函数 (`exit_code_for`) 与 `EntryOutcome` 的定义前置到 Phase 2 的 T004, 作为四命令共用的共享设施; T015 / T016 / T031 一律调用它, 不得另写一份; T036 收窄为"把真实命令的逐条结果与目标谓词接到该函数上", 并负责核对四条命令的谓词求值时刻. 于是 T048 的逐条结果链在 T004 后立即转绿, 端到端链在 T036 后转绿 |
| (同上) `--help` 的退出码说明无任务承接 | `research.md` R10 要求"`set -e` 中裸调只读命令会中断"这一点**必须在 `--help` 与 README 中显式说明**, 但 tasks.md 中 9 处 `--help` 只出现在 T005 的命令面定义, 无任何任务负责其文本内容 | T041 并入该落点: 四个子命令的 `about` / `long_about` 须含退出码语义与 `if ... then ... fi` 推荐写法 |
| (同上) `source_kind` 字段无消费者 | T006 强制计算 `source_kind` (`File` / `Dir`, 额外一次 `symlink_metadata`), 但 `os::create_symlink` 对文件与目录是同一调用, 输出 token 与措辞明确排除类型标签, `classify` 也不使用它. 删除该字段不会有任何测试失败 | 删除该字段与 T006 中相应要求; `data-model.md` 2 节的表格行改为一段说明, 记录"为何不设"以及将来确有需要时随该判定一并引入 |
| (同上) `quickstart.md` 场景 12 的 `rm "$HOME/.bashrc"` 必然报错 | 该行紧接第二次 `undo` (已移除链接) 且其上文的 `rm -f` 已清掉备份, 因此该路径不存在, `rm` 必非零退出; 在文档自身推荐的 `set -e` 写法下会中断整个场景 | 删除该行, 改为注释说明两次 `undo` 已移除全部链接, 因此 `preview` 仍按期望退 1 (两个条目均为 `Absent` -> `create`) |
| (同上) `quickstart.md` 场景 10 的反事实不成立 | 文中说"若场景 7 漏掉那一步 `apply`"会使该条目按 `skip` 报 `conflict` 并退 1, 但场景 7 同时做了 `rm -f "$HOME/.bashrc"`; 只漏 `apply` 会让两条目为 `Absent`, 本场景的 `apply` 会照常建好并退 0. 该反事实实际描述的是"漏掉 `rm -f`" | 改写为"二者缺一不可", 并分别说明只漏 `apply` (退 0) 与只漏 `rm -f` (退 1) 两种后果 |
| (同上) 汇总列名"跳过"对只读命令有误导 | 该列由四条命令共同输出, 但 `preview` / `status` 不执行动作, 谈不上"跳过" (`foreign` / `absent` / `wrong` 均计入该列), 而 SC-007 要求"仅凭输出判断结果与下一步" | 列名改为"未处理", 并注明它是 token 分桶名而非动作陈述; `contracts/cli.md` 的 `undo` 分支表,`data-model.md` 的 Report 节与 T010 同步改名 |
| (同上) `preview` 的"需要执行动作"未定义 | `contracts/cli.md` 的"各命令的目标谓词"表中 `preview` 行写作"没有任何条目需要执行动作", 但 `RefuseConflict` / `ReportMissingSource` / `ReportUnreadable` 都是无副作用的动作, 是否算"需要执行"取决于 `Action != Noop` 这一未写明的解读 | 该行补为"等价于每个条目的 `Action` 均为 `Noop`", 并说明其与条件 2 的重叠区间 |
| (同上) 相对 `target` 路径形态未被样例覆盖 | `contracts/manifest.md` 的三个样例全为 `~` 开通, 而 `data-model.md` V-6 与 `research.md` R5 都要求支持相对路径, 且未说明"相对 target 与某 source 指向同一路径"这一自指清单的处置 | `contracts/manifest.md` 补一节: 给出相对 `target` 的样例, 并说明自指清单仍**合法** (校验不比较 `target` 与 `source`), 其后果由 `conflict` + 退出码 1 暴露; 首版不为它增加校验规则 |
| (同上) 标点规范 | 本轮新增/修改的文本 | 全部使用半角标点 (已扫描确认无 U+3000-U+303F 与 U+FF01-U+FF5E 区段字符) |
| (2026-09-15 第六次 `/speckit-analyze`) `os::normalize` 的语义与 R2 的实测要求自相矛盾 | `research.md` R2 的 Decision 把 `os::normalize` 描述为"逐段消除 `.` 与 `..`"的**纯词法**规范化, 但同一节引用的实测证据 (macOS 上 `/tmp` -> `/private/tmp`) 要求它**解析符号链接**, 否则一条以 `/tmp` 别名形态书写的本工具链接与物理形态的仓库根永不判等, 会被误判为 `ForeignLink`, 使 `undo` 拒绝清理自己的产物. 两个要求不可能同时由纯词法实现满足, 且 `tests/status.rs` 中已有锁死该缺陷的失败用例 | `research.md` R2 的 Decision 改写为**非严格 realpath** 的两段语义 (已存在的段解析符号链接; 不存在的尾段按词法规整), 并补"二者为何不可只取其一"的说明; `data-model.md` 3.1 的"悬空链接处理"同步. 据此 `os::normalize` 由纯词法实现改为非严格 realpath (保留根前缀与相对前导 `..` 语义, 符号链接环由深度上限截断) |
| (同上) T040 的移库夹具期望不可实现 | T040 要求"在临时根 A 建仓库并部署, 把整个目录 `mv` 到 B, 再在 B 中运行 `status`, 断言链接仍为 `ok`". 移库后链接文本仍指向 A 下的旧路径, 按 `data-model.md` 3.1 的 `ForeignLink` 行正确结论是 `ForeignLink`, 该期望与规格直接冲突 (同一节的注释与函数体亦互相矛盾) | 夹具改为**可实现的同源属性**形态: 走 `manifest::parse` 真实解析路径, 让**仓库根路径本身含 `..` 段**, 链接文本取规范化后的源路径, 断言 `LinkedCorrect`. 回归价值不变 -- 若 `source_path` 未过 `os::normalize`, 该用例仍必失败 |
| (2026-09-15 第五次 `/speckit-analyze`) 文档中的命令无法按期望执行, 因而其承载的证明是假阳性 | `quickstart.md` 有三处命令本身会失败或未达成所声称的效果: (1) 场景 10 用 `ln -s`(无 `-f`) 覆盖**已存在**的 `$HOME/.bashrc` -- 实测该命令以 `File exists` 退 1 且**不改动**链接, 链接文本不会是含 `..` 的形态, 紧随的 `status` 仍报 `ok` 退 0. 本场景是 `research.md` R2 规范化陷阱**唯一**的端到端验证, 实际从未被触发, 且在文档推荐的 `set -e` 写法下会中断; (2) 场景 6 用 `rm "$HOME/.config/nvim"` 删除 dotlink 创建的目录链接 -- 该行未加 `-f`, 路径不存在时会以非零退出, 在 `set -e` 下中断 (当时记为"BSD `rm` 拒绝删除目录链接"; 2026-09-15 T043 在 Darwin 27.0.0 上实测该说法不成立 -- 不带 `-f` 也能删除链接本身, 只删链接不跟随 -- 故订正为上述站得住的理由); (3) 场景 9 断言 `2/2` 为 `create`, 但该链接由场景 7 建立后一直未被破坏 (场景 8 退 2 零改动), 实际为 `ok` -- 于是场景 9 自称要证明的 **FR-018 (整目录链接)** 与 **FR-014** 都没有被证明 | 全部订正: 场景 10 改用 `ln -sfn` 并新增 `readlink` 断言"文本确含 `..`"; 场景 6 改用 `rm -f` 并说明原因 (理由为容忍路径不存在, 非"BSD `rm` 会拒绝"); 场景 9 补起始状态 `rm -rf "$HOME/.config/nvim"` 使 `create` 断言成立, 并补 `[ -L ]` 与 `ls` 两条断言显式证明整目录链接. 另新增 `quickstart.md` 的"起始状态"总表与三类累积清单 (多出"备份文件"一类), 把前置的 `cd` 改为 `cd -P` 并说明 `$D` 的物理路径形态 (使 `readlink` 期望值有确定答案), 场景 3 末尾补 `undo` + `rm -rf` 收尾以免 50 条链接污染后续场景 |
| (同上) 累积状态导致后续场景的"期望"不成立 | 沿时间轴复核每个场景的起始状态后发现: 场景 4 的逐行期望忽略被管理的 `nvim` 条目 (实际是 `ok`); 场景 7 / 10 / 12 的固定行号引用 (如"行 231") 在文本被编辑后错位, 且场景 10 的反事实把 `$HOME/.bashrc` 的状态归因于场景 7 的 `apply`, 而它实际由**场景 7 情形 C** 的 `ln -sfn` 决定; 场景 12 的起始状态描述把场景 10 的 `..` 链接说成"已删除"; 场景 14 的"判别点"标在 `undo` 上, 但本例四个条目同为 `missing`, 真正能揭示"目标谓词误把 `SourceMissing` 计入"的是 `preview` | 逐条改写为与实况一致的期望, 并把固定行号引用改为按**场景编号 / 情形**引用; 场景 14 的判别点改为 `preview` 并说明 `SourceMissing` 是终态 (`data-model.md` 3.4) 故它不参与谓词计数; 场景 5 补一段真正的 SC-006 断言 (制造 `create` + `conflict` 的混合状态, 断言 `preview` 预测与 `apply` 实际逐条一致) |
| (同上) 若干边界与判定基准未被规格或契约覆盖 | 四处缺口: (1) `data-model.md` 只要求 `target_path` 规范化, 未要求 `source_path` 也过同一 `os::normalize` -- 于是 `L == S` 的比较可能落在两套路径语义上, 重新引入 R2 陷阱; (2) 四条命令的目标谓词以"每个条目的目标路径"为量词, 但未说明 `Absent` / `SourceMissing` / `Unreadable` 如何计数, 与 T036 引用的"`Action` 均为 `Noop`"表述存在张力; (3) **子命令缺失或拼错**会由 `clap` 以退出码 2 结束, 与"2 = 清单校验失败"共用码值, 使 SC-009 的"无需查阅文档即可区分四类结局"不成立, 而契约与任务均未定义该路径; (4) 备份的"先复制后删除"顺序未被钉死 -- 递归复制目录可能中途失败, 顺序颠倒会让一次失败变成不可恢复的数据丢失, 直接威胁 SC-004 | 分别补入: `data-model.md` 2 节与 `research.md` R2 (含"`os::normalize` 仅有三个调用点"); `data-model.md` 3.2a (谓词求值基准, 并声明"谓词只依赖 `Current`"); `contracts/cli.md` 的"调用方式错误"小节 + `spec.md` FR-020 / FR-015 / Edge Cases 两行 + SC-009 的补句 (clap 对 stderr 类错误固定退 2 且不提供自定义出口, 已由实测确认, 故可区分性由标准错误承担); `data-model.md` 第 4 节与 `research.md` R4 |
| (同上) 未定义的键规则只覆盖了拼错 `on_conflict` 这一种形态 | `contracts/manifest.md` 只说"出现未定义的键时拒绝", 未说明 `[foo]` 这类顶层未知表是否也拒绝; `data-model.md` 的 V-1 到 V-7 也没有对应条目, 而 T008 / T046 已按"顶层仅 `entry`"实现 | `contracts/manifest.md` 补两层合法键表, `data-model.md` 补 **V-8**(只出现已定义的键), `tasks.md` 的 T046 / T038 改为引用 V-1 到 **V-8** |

## Project Structure

### Documentation (this feature)

```text
specs/001-dotfile-link-management/
├── plan.md              # 本文件
├── research.md          # Phase 0 输出: 关键技术决策与取舍
├── data-model.md        # Phase 1 输出: 实体,校验规则,状态迁移表
├── quickstart.md        # Phase 1 输出: 端到端可执行的验证场景
├── contracts/           # Phase 1 输出: 对外契约
│   ├── cli.md           #   命令面,参数,退出码,输出字段
│   └── manifest.md      #   清单文件格式
├── checklists/
│   └── requirements.md  # /speckit-specify 产出
└── tasks.md             # Phase 2 输出 (/speckit-tasks 生成, 本命令不创建)
```

`tasks.md` 新增两个测试文件 (见其 T057): `tests/unreadable.rs` 覆盖 `Unreadable` 在四条命令下同为 `fail` 并退 1 的横切契约, `tests/usage_errors.rs` 覆盖"子命令缺失或拼错"与"清单校验失败"共用退出码 2 而可区分于标准错误.二者均为跨故事的端到端测试, 与 `tests/exit_codes.rs` 同属模块级契约.

### Source Code (repository root)

```text
Cargo.toml
rustfmt.toml       # T003 产出: edition 2024
README.md          # T041 产出: 快速上手, 清单格式, 退出码语义表
src/
├── main.rs          # 薄入口: 调用 dotlink::run(), 用其返回值作为进程退出码
├── lib.rs           # run() 编排: 平台门禁 -> 解析参数 -> 载入并校验清单 -> 分发命令 -> 渲染报告
├── cli.rs           # clap 命令与参数定义, 解析为内部 Command 枚举
├── error.rs         # Error 枚举 (thiserror) 与"退出码"映射
├── manifest.rs      # TOML 反序列化 + 全量校验 (FR-001/002/013), 产出已解析的 Manifest
├── entry.rs         # Entry 与路径解析: 仓库根 / 主目录展开 / 相对目标路径
├── classify.rs      # 唯一的条目状态判定 (FR-017), 纯只读
├── backup.rs        # 备份路径的构造与"最近一次备份"的查找
├── apply.rs         # 部署: 创建父目录,建立链接,备份后替换
├── undo.rs          # 撤销: 移除本工具创建的链接,恢复备份
├── render.rs        # 人类可读输出 (FR-016)
└── os/
    ├── mod.rs       # 符号链接与"链接是否落在仓库内"的平台抽象
    └── unix.rs      # std::os::unix::fs::symlink 实现 (#[cfg(unix)])
    # 非 unix 平台提供空实现, 使 cargo build 在 Windows 上仍能成功,
    # 以便运行时给出可读的"平台不受支持"提示而非编译失败.

tests/
├── support/
│   └── mod.rs       # 夹具: 临时仓库 + 临时 HOME + 清单写入 + 链接断言
├── apply.rs         # US1: 部署,幂等,增量
├── exit_codes.rs    # FR-020: 退出码契约 (两个条件规则), 跨故事
├── preview.rs       # US2: 只读,结论与实际一致
├── conflict.rs      # US3: 备份替换,拒绝覆盖
├── undo.rs          # US4: 恢复备份,不触碰外部路径
├── status.rs        # US5: 不一致分类
├── manifest_errors.rs # FR-013: 校验失败时零改动的整体拒绝
├── unreadable.rs    # FR-011: Unreadable 的横切契约 (四命令同为 fail, 退 1)
└── usage_errors.rs  # FR-020: 调用方式错误 (与清单校验失败同码 2, 靠 stderr 区分)
```

**Structure Decision**: 采用单一 crate (lib + bin), 不拆分 workspace.理由是首个版本只有一个交付物 (一个二进制), 拆 workspace 只会增加构建与版本同步成本而无任何独立性收益.`classify.rs` 与 `apply.rs` / `undo.rs` 的分离是结构上的关键决策: 判定知识只存在一处, 使 FR-017 由结构而非约定保证; 同时因为 `preview` 与 `status` 只需读写分离的前者, 它们天然不可能产生副作用.`os/` 子模块的存在是为了让 Windows 上的编译与运行时拒绝得以共存 (FR-019), 这不是为未来平台预留的扩展点.

## 实现前门禁复查 (T049)

宪章治理条要求"实现前门禁"不仅生成计划时核对, 还须在**进入实现前复查一次**.本表为交付完成后的复查结论, 依据是**代码实况**而非设计意图.

| 门禁 | 复查结论 | 代码实况依据 |
|------|----------|--------------|
| 深模块 | PASS | `src/lib.rs` 的对外表面收窄为 `run()` / `Command` / `EntryOutcome` / `exit_code_for` 四项 (10 个模块均由 `pub mod` 改为 `mod`), `Entry` / `EntryState` / `Action` / `Manifest` / `Current` 等内部类型不再是外部契约.复杂度集中在 `classify` (唯一判定入口) 与 `os::normalize` 内部 |
| CLI 验证接口 | **DEVIATION** (同前, 见 Complexity Tracking) | 交付物即 CLI; 退出码与 12 个状态 token 构成可断言的稳定契约; 未提供 `--json` 的依据与恢复方案已记录 |
| TDD (不可协商) | PASS | `T048` / `T046` / `T047` 三批红测试均先于其实现 (`exit_code_for` / `manifest` / `classify`) 完成并确认失败, 顺序约束记录于 `tasks.md` Phase 2 的硬性先后关系 |
| 代码即文档 | PASS | `backup_path_for` / `find_latest_backup` / `link_is_inside_repo` / `exit_code_for` 等命名自解释; 模块按"变化原因"划分 (解析 / 判定 / 动作 / 呈现) |
| 最小注释 | PASS | 注释集中于两处非显而易见的原因: `os::normalize` 的"为何不得用 `canonicalize`"与"为何不解析末端段", 以及 `os/mod.rs` 的"可编译空实现 + 运行时拒绝"组合 |
| 简单性与反抽象 | PASS | 单一 crate (1 个项目); 直接使用 `clap` / `serde` / `toml`, 无包装; 无 trait 抽象层与工厂.复查中移除了无消费者的 `os::is_symlink` (T007 曾要求), 与 `source_kind` 的处置同理 |
| 集成优先测试 | PASS | 测试用真实文件系统与真实二进制进程 (`assert_cmd` + 临时 `HOME`), 不 mock 文件系统; 无外部服务可 mock |
| 持续精炼 | PASS | 六轮审查的修正均回写至最上游文档; 本轮 (`os::normalize` 语义与 T040 夹具) 已同步回写 `research.md` / `data-model.md` / `tasks.md` |

**未通过项**: 仅宪章第二条的 `--json` 一项, 已记入 Complexity Tracking 并给出恢复方案.其余门禁 PASS.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| 宪章第二条: CLI 未提供 `--json` | 本工具唯一的程序化消费面是"退出码 + 逐条目结论".退出码已在 `contracts/cli.md` 中构成完整契约 (0/1/2/3 四类结局), 状态 token 表给出 12 个可断言的稳定取值, 二者叠加已足以支撑 agent 交互与自动化判断, 无需解析自由文本 | 为一个**尚未出现**的脚本化消费需求引入输出格式开关, 属宪章第六条明令禁止的"投机抽象 / 未来预留"; 且 JSON schema 一旦发布即成为第二种必须维护的对外契约.**恢复方案**: 真实需求出现时以加法形式引入 `--json` (不破坏既有文本输出契约), 无需迁移 |

宪章第五条的合规性说明: 本设计确实需要两处非显而易见的注释 -- `research.md` R2 的规范化陷阱 (macOS 上 `/tmp` 会规范化为 `/private/tmp`) 与 R6 的"可编译空实现 + 运行时拒绝"组合.二者符合"只解释代码完全表达不了的为什么", 不构成违规."不得超过 3 个项目"这一条 (简单性与反抽象门禁第 1 项) 由单一 crate 满足.

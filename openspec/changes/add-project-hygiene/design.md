# Design

## Context

当前状态 (详见 proposal.md 的 Why):

- 仓库在 `main` 分支公开, 没有 `.github/` 目录, 也没有 `LICENSE`; `Cargo.toml` 的 `[package]` 只有 `name` 与 `version` 等构建字段, 缺 `license` / `description` / `repository`.
- `Cargo.toml` 声明 `edition = "2024"`, `rust-version = "1.97"`; `rustix` 依赖只对 macOS 与 Linux 生效; 运行时没有异步执行器, 也没有状态文件.
- `scripts/check.sh` 依次执行 `cargo fmt --check`, `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings`, rustdoc (`-D warnings`) 与 `cargo build --locked --release`, 任一步失败即停止.
- `README.md` 在 10 行里共有 21 处 U+3001, 其中 1 处位于 fenced shell 代码块的 `echo` 字符串内; 已知限制缺四条.
- `src/manifest.rs:56` 把 `parse` 称为 "纯函数", 但它经 `os::normalize` 读取符号链接元数据; `src/backup.rs:32` 的暂存名示例漏了进程号.
- `openspec/config.yaml` 已有 `schema: spec-driven`, context 里已有 ASCII Art 与"所有 artifact 使用中文"两条约束, rules 仍是注释示例. 目前 `openspec instructions proposal` 输出的就是这两条, 不是本项目的 MSRV, 平台与验证约束; 本变更保留这两条并追加技术约束.
- 本变更不改变任何可观察行为, `.openspec.yaml` 已设置 `skip_specs: true`.

约束:

- 许可证是维护者的法律决定, 不是工程默认值.
- 本地检查必须不新增必须联网的步骤; 只有显式运行 CI 的审计 job 才依赖 audit 网络访问.
- Windows 不在支持平台内, 只做编译检查, 不承诺运行时.
- 不使用异步运行时, 不写状态文件, 不新增依赖.

## Goals / Non-Goals

**Goals:**

- 每次 push 与 PR 都在 macOS 与 Linux 上得到与本地 `./scripts/check.sh` 完全一致的检查结果.
- 用独立 job 固定 README 已承诺的 MSRV 命令与两个交叉编译命令.
- 让新克隆的开发环境自动落到 1.97.0, 并自带 `rustfmt` 与 `clippy`.
- 用可执行的检查脚本长期阻止非 ASCII 标点回归, 当前 21 处一次性清零.
- 把四条已知限制写入 README, 并修正与实现不符的两处注释.
- 让 OpenSpec 制品拿到真实项目约束, 且与 AGENTS.md 不冲突.
- 在维护者确认许可证后交付 `LICENSE` 与 Cargo 元数据.

**Non-Goals:**

- 不改变任何 spec 级行为, 不新增 capability, requirement 或 scenario.
- 不新增 Windows runner 或 Windows 运行时支持; 交叉编译不等于运行时验证.
- 不由代理替维护者选择许可证, 不提供法律建议; 不伪造版权署名.
- 不把 `cargo audit` 加入本地默认检查, 不新增 Rust 依赖.
- 不在本变更增加 stable 工具链的前瞻 job (见 Open Questions).
- 不重排或重写 README, 只做标点替换, 四条限制补充与一处措辞衔接.
- 不修改其他并行变更目录, `Cargo.lock`, 测试语义或运行时代码.

## Decisions

### D1. 一个 CI 工作流, 三个 job, 审计可选单独成流

`.github/workflows/ci.yml` 定义:

- `check` job: 矩阵 `ubuntu-latest` 与 `macos-latest`, 直接运行 `./scripts/check.sh`.
- `msrv` job: `ubuntu-latest`, 运行 `cargo +1.97.0 test --locked`.
- `cross-check` job: `ubuntu-latest`, 安装 `x86_64-pc-windows-gnu` 目标后运行 README 承诺的两条 `cargo check --locked --target ...`.

触发条件为 `push` 到 `main` 与所有 `pull_request`; `permissions: contents: read`; 顶层 `concurrency` 用 workflow 与 ref 分组并 `cancel-in-progress: true`; 每个 job 设 `timeout-minutes` (完整检查 30, 其余 15), 防止 runner 卡住. `check` 用 `fail-fast: false`, 让两个 OS 的结果都能看到.

供应商选择: `actions/checkout@v4`, `dtolnay/rust-toolchain@master` (传入 `toolchain: 1.97.0`), `Swatinem/rust-cache@v2`. 缓存 key 按 job 与 OS 区分, 缓存 cargo registry 与 `target/`; 缓存未命中时也能完成, 只是更慢. 这些 action 都按主版本标签固定, 升级需单独评审; 不采用提交 SHA 固定是为了避免每次升级都改 hash, 代价是上游主版本可移动的供应链风险, 由定期评审覆盖.

`cargo audit` 放在可选的独立 `.github/workflows/audit.yml`: 每周 cron 加 `workflow_dispatch`, 使用 `rustsec/audit-check@v2` 或等价的 cargo-audit 步骤. 它依赖网络与 RustSec 数据库, 失败可能来自上游公告而不是本仓库代码, 因此不进入 PR 必需检查, 也不进入 `scripts/check.sh`. 本地检查只保留编译与测试, 避免把网络可用性变成提交前置条件.

备选方案: 每个 OS 一个 workflow 文件, 或把 MSRV 与交叉编译并入矩阵. 否决理由是 job 目的不同, 混在矩阵里会让 MSRV 失败与平台失败难以区分, 也会让 macOS 跑不需要的 MSRV 与 Windows 交叉检查.

### D2. CI 复用 `scripts/check.sh`, 不复制五条命令

主 job 只做 checkout, 安装工具链与缓存, 然后执行脚本. 这样本地与 CI 的检查集合由同一个文件定义: 以后在 `check.sh` 增加步骤, CI 自动获得, 不会出现两边漂移. 备选方案是在 YAML 里逐条写 `cargo fmt`, `cargo test` 等; 否决理由是已经存在脚本, 复制会制造第二份真相, 且 README 明确以脚本为入口.

代价是 CI 日志里看不到脚本内部的步骤划分; 脚本本身已有 `set -euo pipefail` 与逐步输出, 足够定位. 交叉编译与 MSRV 是脚本之外的独立命令, 按 README 的承诺保留在 job 里.

### D3. MSRV job 独立保留

即使主 job 也使用 1.97.0, 仍保留 `cargo +1.97.0 test --locked` 的独立 job:

- 它验证 README "开发" 段承诺的确切命令, 包括 `--locked` 对 `Cargo.lock` 的约束.
- 未来若把开发工具链改为 stable, 该 job 仍然守着 MSRV, 不需要重新引入.
- 测试在 MSRV 上通过是发布声明的一部分, 与 fmt/clippy 的平台检查目的不同.

代价是一个短 job 的重复计算; 相对其保护作用可以接受.

### D4. 交叉编译只在 Linux 上做编译检查

`cross-check` 放在 `ubuntu-latest`:

- `x86_64-unknown-linux-gnu` 是宿主目标, 该命令仍按 README 原样执行, 验证文档承诺的命令可用.
- `x86_64-pc-windows-gnu` 只安装 std 目标; `cargo check` 不做链接, 因此不需要 MinGW 链接器, 也不会把 Windows 运行时行为当作已验证.
- macOS 的原生检查已覆盖 Apple 平台, 不再为交叉检查消耗 macOS 分钟.

否决方案: 在 Windows runner 上跑测试. 平台门禁明确拒绝 Windows, 加 runner 会扩大支持面并产生误导读者的绿色徽章; 本变更只在 README 保留"Windows 运行时未验证"的说明.

### D5. 固定 1.97.0, 不固定 stable

选择 `rust-toolchain.toml`:

```toml
[toolchain]
channel = "1.97.0"
components = ["rustfmt", "clippy"]
profile = "minimal"
```

理由:

- README 与 `Cargo.toml` 都把 MSRV 定为 1.97; 开发环境落在 MSRV 上, 能在提交前发现"只在新 stable 可用"的写法, 而不是等 CI 的 MSRV job 事后报警.
- `rust-toolchain.toml` 让新克隆执行 `./scripts/check.sh` 时自动安装并选用 1.97.0 与两个组件, 不需要手工 `rustup override`; CI 安装同一版本, 本地与 CI 可复现.
- `profile = "minimal"` 减少首次下载; 两个组件仍显式列出, 避免依赖 rustup 默认 profile 的变化.

否决方案: 固定 `stable`. 它更早暴露新编译器警告, 但会让本地检查随 stable 漂移, 与 MSRV 声明脱节; 而且 CI 仍要额外安装 1.97. 代价是本变更不捕获 stable-only 的新 lint. 接受该代价, 并在 Open Questions 留下后续增加 stable 前瞻 job 的选项.

### D6. 标点检查用独立 Python 脚本, 不设逐行豁免

新增 `scripts/check-punctuation.py`, 由 `scripts/check.sh` 最先调用.

- 扫描范围 (相对仓库根): `AGENTS.md`, `README.md`, `openspec/specs/**/*.md`, `openspec/changes/**/*.md`, `src/**/*.rs`, `tests/**/*.rs`, `scripts/**/*.sh`, `scripts/**/*.py`. 使用显式路径而不是全仓遍历, 因此天然不进入 `.agents`, `target`, `.git`; 脚本仍会跳过这些目录名作为防御. 变更产物与 `AGENTS.md` 也纳入: 二者都受 AGENTS.md 的 ASCII 标点规则约束, 不纳入会让后续变更与规则文本本身回归.
- 禁止码点: U+3000-303F, U+FF00-FFEF, U+2018-201F, U+2014, U+2026, U+00B7. 只检查这些范围, 不把中文汉字或其他非 ASCII 字符本身当作违规.
- 输出 `path:line:column: U+XXXX` 与一行上下文; 有命中退 1, 无命中退 0, 参数错误退 2. 支持 `--root` 指向临时目录, 便于用夹具测试失败路径.
- 不跳过 fenced code block, 也不跳过 inline code. 理由: README 第 241 行的 U+3001 位于 shell 代码块的 `echo` 字符串里, 属于自然语言文本; 若跳过代码块会漏掉它. 代码中确需这些字符时使用语言转义 (Rust 用 `\u{3001}`, shell 用 `printf` 构造), 或在文档里写成 `U+3001`, 让字面字符不出现在源码中.
- 不提供逐行忽略注释. 豁免机制会随时间积累, 最终让检查失效; 需要豁免时应在后续变更中讨论范围, 而不是在单行上开口子.
- 文件必须是 UTF-8; 解码失败按错误报告并退 1, 不静默跳过. 不跟随符号链接, 避免循环.
- 性能不构成问题: 上述路径下的文本远小于一次 cargo 构建.

否决方案: 在 `check.sh` 内嵌 heredoc Python. 可读性与测试性都更差; 独立脚本可以用 `--root` 夹具验证命中与退出码. 也否决 shell + `grep -P`: macOS 自带 BSD grep 不支持 `-P`, Unicode 范围行为不一致.

### D7. README 的机械替换与四条限制

21 处 U+3001 的分布按行: 55 (1), 112 (1), 116 (1), 120 (4), 122 (1), 136 (7), 140 (1), 218 (1), 241 (1), 262 (3). 全部替换为 ASCII 逗号, 沿用正文已有的 ", " 间距. 第 241 行在 shell 代码块内, 但它是 `echo` 的中文文本, 仍按自然语言处理; 这正是不跳过代码块的原因.

四条已知限制的落点:

- **`undo` 不清理父目录**: `apply` 通过 `src/apply.rs:48-61` 的 `create_dir_all` 建立缺失父目录; `undo` 只移除链接并恢复备份, 不删除空目录. README 快速上手的 "想回到原状时执行 `dotlink undo`" 需要补一句指向该限制, 避免读者以为目录也会回退.
- **备份命名空间被保留**: `src/backup.rs:111-136` 的 `scan_backups` 把同目录下任何 `<原名>.dotlink-backup.<YYYYMMDDThhmmss>` (可带 `.序号`) 当作候选备份. 用户文件若采用该形态, 可能被 `undo` 当作备份恢复. 限制条目要明确这是保留命名空间, 不是普通文件命名.
- **并发原地修改不进入备份**: 替换路径用 `(dev, ino, file_type)` 复核身份; 对同一 inode 的原地写入不改变身份, 因此复核通过后仍会删除暂存对象. 备份复制完成之后的原地写入不会进入本次备份. 该条与现有"并发"条目相邻, 但单独列出, 因为用户容易误以为备份覆盖所有并发修改.
- **非 UTF-8 路径 lossy 显示**: `src/classify.rs:164` 用 `actual.display().to_string()` 生成 `wrong` / `foreign` 行的"实际指向"; `Path::display` 对不可解码字节输出 U+FFFD. 只影响报告文本, 不影响分类, 建链或恢复.

CI 徽章可选: 若维护者需要, 在 H1 之后加入指向 `actions/workflows/ci.yml` 的 badge; 不加也不影响其他任务. 徽章只反映 CI, 不代表 Windows 运行时支持.

### D8. 两处注释按实现改写

- `src/manifest.rs:56`: 把 "纯函数" 改为 "只读, 不写文件系统", 并保留 "不产生任何文件系统改动" 的后半句. 原因: `parse` 会调用 `os::normalize` 对 target 逐段 `lstat`, 读取符号链接元数据; 它确实不改文件系统, 但不是不访问文件系统的纯函数. 同时把 `src/manifest.rs:428` 测试注释里的 "`parse` 是纯函数" 改为 "`parse` 只读", 避免同一文件留下矛盾说法. 用 `rg -n '纯函数' src` 验证零命中.
- `src/backup.rs:32`: 把 `x.dotlink-backup.20260101T000000.partial` 改为 `x.dotlink-backup.20260101T000000.<pid>.partial`, 并把 "加在时间戳之后" 调整为 "加在时间戳与进程号之后". 模块文档 (`src/backup.rs:11`) 已经写明 `<时间戳>.<pid>.partial`, 常量注释只是漏写. 只改注释, 不改 `PARTIAL_SUFFIX` 或命名逻辑.

### D9. `openspec/config.yaml` 写入真实约束

现状是 context 已有两条约束 (ASCII Art 与简体中文), rules 仍是注释. 本变更保留这两条, 追加技术约束, 并写入未注释的 `rules`:

```yaml
context: |
  - ASCII Art 请只使用 ASCII 字符, 不要使用中文字符, 因为中文字符和 ASCII 不是严格 2:1 的宽度; 并且如果可以, 尽量用 mermaid 代替 ASCII Art
  - 所有 artifact 的自然语言使用简体中文
  - 使用 Rust 2024 edition.
  - MSRV 为 1.97 (以 Cargo.toml 的 rust-version 为准), 不使用 nightly 特性.
  - 仅支持 macOS 与 Linux; 其他平台在参数解析前退 3, 不承诺运行时支持.
  - 不使用异步运行时, 命令同步执行.
  - 不写状态文件, 运行状态由清单与文件系统决定.
rules:
  proposal:
    - 自然语言使用简体中文, 标点仅用 ASCII, 与 AGENTS.md 一致.
    - 与 README.md 及 openspec/specs/ 的现有行为保持一致; 有意改变行为时先写明差异.
  specs:
    - 自然语言使用简体中文, 标点仅用 ASCII, 与 AGENTS.md 一致.
    - 每个 Scenario 给出可执行命令或可观察结果, 不写无法验证的意图.
    - 与 README.md 及现有 openspec/specs/ 的行为描述一致; 新增行为必须明确标记.
  design:
    - 自然语言使用简体中文, 标点仅用 ASCII, 与 AGENTS.md 一致.
    - 记录被否决的备选方案与否决理由.
    - 决策必须能落到 tasks 的具体步骤.
    - 与 README.md 及 openspec/specs/ 的现有行为保持一致.
  tasks:
    - 自然语言使用简体中文, 标点仅用 ASCII, 与 AGENTS.md 一致.
    - 每个任务写明验证命令或可观察结果.
    - 每组包含该组的验证任务.
    - 与 README.md 及 openspec/specs/ 的现有行为一致, 不安排与规格冲突的改动.
```

`context` 放会影响方案的项目级约束, 不复述 README 的安装, 命令与退出码说明; 两条既有约束 (ASCII Art 与简体中文) 继续保留并生效, 技术约束追加其后. `rules` 中的语言条款是 AGENTS.md 在 OpenSpec 工作流里的投影: OpenSpec 的 instructions 不读取 AGENTS.md, 所以必须写入配置; 详细标点判定表仍以 AGENTS.md 为唯一来源, 配置只给操作性规则, 冲突时以 AGENTS.md 为准. `specs` 规则要求验证场景可执行, 与本项目现有 spec 的写法一致.

禁止把 `context` 写成项目简介或文件清单; 也禁止在配置里另立一套与 AGENTS.md 不同的标点规则.

### D10. 许可证由维护者确认后再落地

实现顺序上, 先向维护者确认三件事: 许可证 (建议 MIT), 版权署名 (个人或组织), 首次发布年份. 确认后:

- 新增 `LICENSE`, MIT 时使用标准正文与 `<year> <copyright holder>` 版权行.
- 在 `Cargo.toml` 的 `[package]` 增加 `license = "<SPDX>"`, `description` (英文 ASCII 一行, 例如 `Declarative dotfile symlink manager`) 与 `repository = "https://github.com/yjydist/dotlink"`.
- 使用 SPDX `license` 字段而不是 `license-file`, 让 crates.io 与 GitHub 都能识别; 仓库根只保留一个 `LICENSE`.

不把许可证选择放进 Open Questions: 它改变交付物 (文件内容与元数据值), 不是可以安全延后的实现细节. 未确认前 tasks 的许可证组保持未勾选, 不得用 "建议 MIT" 冒充维护者决定. 若维护者选择其他 SPDX 许可证, 只替换 `LICENSE` 正文与 `license` 值, 其余 CI, 工具链与文档任务不受影响.

### D11. 实施顺序与依赖

1. 维护者确认许可证 (可与其余工作并行, 但阻塞许可证落地).
2. 固定工具链; 之后本地与 CI 使用同一版本.
3. 修正 README 的 21 处标点与四条限制; 此时标点扫描应归零.
4. 加入标点脚本并接入 `check.sh`; 若先接入, 在 README 修正前 `check.sh` 会红, 这是预期状态, 但会让实现期验证混淆.
5. 增加 CI; 它依赖 2 与 4 的脚本和工具链.
6. 修正两处注释; 与 CI 无依赖, 可提前.
7. 填写 `openspec/config.yaml`; 独立于代码, 最后验证 instructions 输出.
8. 集成验证: `./scripts/check.sh`, `openspec validate --strict`, CI 首次运行.

## Risks / Trade-offs

- **CI 时间与 macOS 计费** -> 只用一个 macOS runner; 启用 rust-cache, `concurrency` 取消过期运行, 30 分钟超时; 首次无缓存较慢, 以实际日志评估是否需要缩减.
- **固定 1.97.0 错过新 stable 的警告** -> 接受; Open Questions 记录后续增加 stable 前瞻 job. 当前优先级是 MSRV 可复现.
- **MSRV job 与主 job 重复** -> 保留为短 job; 它的职责是守住 README 承诺的命令, 不是重复完整检查.
- **交叉编译给出错误的安心感** -> job 名与 README 都写明 compile-only; 不添加 Windows runner, 不宣称运行时支持.
- **审计告警噪声或网络失败** -> 每周独立运行, 非合并门禁; 手动触发可复现; 依赖网络时失败不阻塞 PR.
- **许可证未确认** -> 许可证组阻塞, 其余可交付; 不擅自选 MIT, 不伪造版权行. 归档前若仍未确认, 需拆分或推迟归档.
- **标点检查误伤测试夹具** -> 用转义构造字符; 不设行内豁免. 若确实需要字面字符, 先讨论是否扩展白名单, 不在本变更开口子.
- **README 机械替换破坏示例** -> 只改 10 行的标点; 用检查脚本与 `git diff` 复核; 检查 Markdown fenced block 数量未变.
- **配置与 AGENTS.md 漂移** -> 配置只写操作规则并指向 AGENTS.md; 修改语言规则时两处一起改; 用 instructions 输出验证生效.
- **首次克隆下载工具链** -> README 开发段说明固定版本; CI 安装同一版本并缓存.
- **`python3` 成为本地检查前置** -> macOS 与 Linux 默认提供; 只使用标准库; README 开发段补一句.

## Migration Plan

无数据迁移, 无运行时行为变化, 无需用户操作.

实施顺序按 D11. 回滚策略:

- 删除 `.github/workflows/`, `rust-toolchain.toml`, `scripts/check-punctuation.py`, `LICENSE`.
- 还原 `Cargo.toml`, `scripts/check.sh`, `README.md`, 两处注释与 `openspec/config.yaml`.
- CI 可单独删除而不影响其他交付; 环境变量, 清单格式, 备份命名与退出码均未改变.

若 CI 首次运行失败, 修正工作流或环境, 不放宽 `check.sh` 的检查; 否则会违背"CI 与本地等价"的目标.

## Open Questions

以下问题都不改变本变更的交付物与任务分解:

- 是否在后续变更增加 stable 工具链的前瞻 job, 以便更早发现新 lint. 本变更固定 1.97.0, 主 job 与 MSRV job 都使用 1.97; 增加 stable job 才会覆盖新版本.
- 是否把 `cargo audit` 的告警升级为阻断. 当前只在每周工作流中报告, 不进入 PR 门禁.

许可证选择不在此列: 它改变最终交付物, 已作为 tasks 的前置确认项 (1.1), 未确认前不落地.

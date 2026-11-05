# Tasks

## 1. 前置确认: 许可证

- [x] 1.1 (阻塞) 向维护者确认许可证 (建议 MIT), 版权署名 (个人或组织) 与首次发布年份, 并把答复记录在变更讨论或 PR 描述中; 未确认前不得创建 `LICENSE`, 不得在 `Cargo.toml` 写 `license`. 验证: 记录中明确给出 SPDX 标识与版权行 (例如 `MIT` 与 `<year> <copyright holder>`), 且没有代理替维护者选定的表述; 若选择非 MIT, 记录实际 SPDX 标识供 2.1 与 2.2 使用.
- [x] 1.2 组验证: 人工复核 1.1 的记录包含许可证, 署名与年份三项; 运行 `rg -n '^license|^description|^repository' Cargo.toml` 确认 2.2 之前没有提前写入元数据.

## 2. 许可证与 Cargo 元数据

- [x] 2.1 按 1.1 的确认新增根目录 `LICENSE` (MIT 时使用标准正文, 版权行为 `<year> <copyright holder>`). 验证: `test -s LICENSE` 成功, `head -5 LICENSE` 显示标准开头与版权行, 正文中的许可证名称与 1.1 记录的 SPDX 标识一致.
- [x] 2.2 在 `Cargo.toml` 的 `[package]` 增加 `license = "<1.1 的 SPDX>"`, `description = "Declarative dotfile symlink manager"` 与 `repository = "https://github.com/yjydist/dotlink"`. 验证: `cargo metadata --no-deps --offline --format-version 1` 的输出中三项均非空, `license` 与 `LICENSE` 一致; `cargo check --locked --offline` 仍通过.
- [x] 2.3 组验证: 用 `python3 -c` 读取 `cargo metadata` 的 JSON, 打印并核对 `license`, `description`, `repository`; 运行 `git status --short` 确认只新增 `LICENSE` 与修改 `Cargo.toml`; 人工确认仓库根没有第二个许可证文件或 `license-file` 字段.

## 3. 工具链固定

- [x] 3.1 新增 `rust-toolchain.toml`, 内容为 `[toolchain]`, `channel = "1.97.0"`, `components = ["rustfmt", "clippy"]`, `profile = "minimal"`. 验证: 在仓库内运行 `rustup show active-toolchain` 输出包含 `1.97.0`; `rustc --version` 为 1.97.0; `cargo fmt --version` 与 `cargo clippy --version` 成功; `rustup component list --installed` 含 `rustfmt` 与 `clippy`.
- [x] 3.2 在 README 的 "开发" 段补一句: 开发工具链由 `rust-toolchain.toml` 固定为 1.97.0, MSRV 仍为 1.97. 验证: `rg -n 'rust-toolchain|1\\.97' README.md Cargo.toml rust-toolchain.toml` 三处版本一致; 人工阅读该段, 确认没有暗示默认使用任意 stable.
- [x] 3.3 组验证: 运行 `rustup show`, `cargo --version` 与 `rustup component list --installed` 确认 1.97.0 生效且组件存在; 记录本机首次安装耗时, 供 6.4 的 CI 缓存评估参考.

## 4. README 合规与已知限制

- [x] 4.1 替换 21 处 U+3001: 行 55 (1 处), 112 (1), 116 (1), 120 (4), 122 (1), 136 (7), 140 (1), 218 (1), 241 (1), 262 (3) 全部改为 ASCII 逗号, 并沿用正文已有的 ", " 间距; 第 241 行位于 shell 代码块内也要改. 验证: 运行 `python3 -c 'import pathlib,sys; t=pathlib.Path("README.md").read_text(); bad=[(i+1,hex(ord(c))) for i,l in enumerate(t.splitlines()) for c in l if 0x3000<=ord(c)<=0x303F or 0xFF00<=ord(c)<=0xFFEF or 0x2018<=ord(c)<=0x201F or ord(c) in (0x2014,0x2026,0x00B7)]; print(bad); sys.exit(1 if bad else 0)'`, 输出空列表并以 0 退出; `git diff --word-diff README.md` 只显示标点替换; fenced block 数量与修改前一致.
- [x] 4.2 在 "已知限制" 增加四条: `undo` 不清理 `apply` 创建的父目录, 备份命名空间由 dotlink 保留, 并发原地修改不会进入备份, 非 UTF-8 链接文本在报告中 lossy 显示; 每条给出现有行为边界 (分别对照 `src/apply.rs:48-61`, `src/backup.rs:111-136`, 替换路径的身份复核, `src/classify.rs:164`). 验证: 人工逐条对照代码位置; `rg -n '父目录|dotlink-backup|UTF-8|原地' README.md` 四条均可定位.
- [x] 4.3 调整快速上手 "想回到原状时执行 `dotlink undo`" 一句, 补充自动创建的父目录不会被删除并指向 4.2 的限制条目. 验证: 人工阅读该段与已知限制, 确认不矛盾; `rg -n '想回到原状|父目录' README.md` 两处措辞一致.
- [x] 4.4 组验证: 运行 4.1 的扫描命令确认 0 命中; `git diff --stat README.md` 只涉及 README; 人工通读 diff, 确认示例命令, 表格与代码块未被改写.

## 5. 标点检查与 check.sh

- [x] 5.1 新增 `scripts/check-punctuation.py`, 仅用标准库, 按 design D6 实现扫描范围 (`AGENTS.md`, `README.md`, `openspec/specs/**`, `openspec/changes/**`, `src/**`, `tests/**`, `scripts/**`), 六个码点范围, `path:line:column: U+XXXX` 输出, 命中退 1, 无命中退 0, 参数错误退 2, 支持 `--root`. 验证: `python3 -m py_compile scripts/check-punctuation.py`; 在 4.1 之后运行 `python3 scripts/check-punctuation.py` 退 0 (本次创建的 7 个变更目录与 `AGENTS.md` 当前均已合规); 运行 `tmp=$(mktemp -d)`, 用 `python3 -c` 的 `\u3001` 转义写入 `$tmp/openspec/changes/sample/spec.md` 与 `$tmp/AGENTS.md`, 运行 `python3 scripts/check-punctuation.py --root "$tmp"` 应退 1 且两处路径都出现在 `文件:行:列: U+3001` 输出中; 清理夹具.
- [x] 5.2 在 `scripts/check.sh` 的最前面调用 `python3 scripts/check-punctuation.py`, 保留 `set -euo pipefail`. 验证: `bash -n scripts/check.sh` 通过; `./scripts/check.sh` 从标点检查开始并全部通过 (依赖 4.1); 人工确认脚本仍使用锁文件且没有新增联网步骤.
- [x] 5.3 在 README 的 "开发" 段说明新增的标点检查与 Python 3 前置, 并说明 `cargo audit` 不在本地脚本中; 若采纳 6.5, 同时说明由每周 CI 运行. 验证: `rg -n '标点|python3|audit' README.md` 能定位这些说明; `rg -n 'cargo audit' scripts/check.sh` 零命中.
- [x] 5.4 组验证: 运行 `./scripts/check.sh` 确认标点检查加五条 cargo 检查全部通过; 记录耗时; 检查 `git diff scripts/check.sh` 只新增调用与必要的注释, 没有放宽既有检查.

## 6. CI

- [x] 6.1 新增 `.github/workflows/ci.yml`: 触发 `push` 到 `main` 与 `pull_request`; `permissions: contents: read`; 顶层 `concurrency` 用 workflow 与 ref 分组并 `cancel-in-progress: true`; `check` 矩阵包含 `ubuntu-latest` 与 `macos-latest`, 两个 job 都安装 1.97.0 与 `rustfmt`/`clippy` 组件, 运行 `./scripts/check.sh`; 使用 `Swatinem/rust-cache@v2`, `fail-fast: false`, `timeout-minutes: 30`. 验证: `ruby -ryaml -e 'YAML.load_file(".github/workflows/ci.yml")'` 解析成功; 安装 `actionlint` 时运行 `actionlint .github/workflows/ci.yml`; 首次推送后两个 check job 均绿.
- [x] 6.2 在同一工作流增加 `msrv` job: `ubuntu-latest`, 安装 1.97.0, 运行 `cargo +1.97.0 test --locked`, `timeout-minutes: 15`. 验证: job 日志显示确切命令且通过; 本地运行 `cargo +1.97.0 test --locked` 通过.
- [x] 6.3 增加 `cross-check` job: `ubuntu-latest`, 安装 1.97.0 与 `x86_64-pc-windows-gnu` 目标, 依次运行 `cargo check --locked --target x86_64-unknown-linux-gnu` 与 `cargo check --locked --target x86_64-pc-windows-gnu`, `timeout-minutes: 15`; job 名称或注释注明 compile-only. 验证: 日志显示两条命令通过; 工作流中没有 Windows 测试步骤或 Windows runner.
- [x] 6.4 配置缓存与时长: 每个 job 使用按 job 与 OS 区分的 rust-cache key, 记录首次无缓存与第二次有缓存的时长; 触发第二次运行. 验证: 第二次运行日志出现 cache hit; 总时长不高于首次; 若 macOS job 超过 30 分钟, 记录原因并调整缓存或检查范围.
- [x] 6.5 新增可选的 `.github/workflows/audit.yml`: 每周 cron 与 `workflow_dispatch`, 使用 `rustsec/audit-check@v2` 或固定版本的 cargo-audit 步骤, 明确不进入 PR 必需检查. 验证: 若采纳, 手动 dispatch 一次并确认运行完成; 若不采纳, 在 PR 描述记录决定并说明依赖审计仍由维护者手工执行. `rg -n 'audit' scripts/check.sh` 在两种情况下都应零命中.
- [x] 6.6 在 README 增加 CI 徽章 (可选) 并在开发段列出 `check`, `msrv`, `cross-check` 三个 job 与 "Windows 仅交叉编译" 的边界. 验证: 若采纳徽章, URL 中的 workflow 文件名与 6.1 一致且 `rg -n 'badge|ci.yml|Windows' README.md` 可定位; 若不采纳, 在 PR 描述记录决定并保留 job 说明, 视为完成.
- [x] 6.7 组验证: 推送分支或开 PR, 确认 `check (ubuntu-latest)`, `check (macos-latest)`, `msrv`, `cross-check` 全部成功; 记录 run URL; 若采纳 6.5, 手动 dispatch audit 成功; 确认没有把交叉编译写成 Windows 运行时验证.

## 7. 代码注释

- [x] 7.1 把 `src/manifest.rs:56` 的 "纯函数" 改为 "只读, 不写文件系统", 保留后续说明; 同步把 `src/manifest.rs:428` 测试注释的 "纯函数" 改为 "只读". 验证: `rg -n '纯函数' src` 零命中; `RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps --document-private-items` 通过.
- [x] 7.2 把 `src/backup.rs:32` 的暂存名示例补上 `.<pid>`, 改为 `x.dotlink-backup.20260101T000000.<pid>.partial`, 并把 "加在时间戳之后" 改为 "加在时间戳与进程号之后". 验证: `sed -n '29,34p' src/backup.rs` 显示 `.<pid>`; `rg -n 'pid' src/backup.rs` 与模块文档的 `<时间戳>.<pid>.partial` 一致.
- [x] 7.3 组验证: 运行 `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings` 与 7.1 的 rustdoc 命令; `git diff src` 只含注释行, 不改变代码或测试断言.

## 8. openspec/config.yaml

- [x] 8.1 在 `openspec/config.yaml` 现有的 context 两条约束 (ASCII Art 只使用 ASCII 字符并优先 mermaid; 所有 artifact 使用简体中文) 之后追加技术约束: Rust 2024 edition 与 MSRV 1.97 (以 `Cargo.toml` 的 rust-version 为准), 仅 macOS 与 Linux, 无异步运行时, 无状态文件; 保留文件里的说明注释, 不删除说明性段落, 不删除既有两条约束. 验证: `openspec instructions proposal --change add-project-hygiene --json` 的 `context` 同时包含既有两条 (ASCII Art 与简体中文) 与新增五条; 人工核对 `openspec/config.yaml` 的 context 共七条且顺序为先旧后新.
- [x] 8.2 取消 `rules` 的注释示例并写入 per-artifact 规则 (proposal, specs, design, tasks): 简体中文与仅 ASCII 标点, 与 README 及 openspec/specs 一致, Scenario 可执行, 每个 task 有验证; 明确以 AGENTS.md 为语言规则的上位来源. 验证: `openspec instructions proposal --change add-project-hygiene --json` 的 `rules` 字段包含 proposal 条款; 对 design 与 tasks 运行同一命令, `rules` 字段分别包含对应条款; `openspec validate add-project-hygiene --strict` 通过.
- [x] 8.3 与 AGENTS.md 对照: 不新增与 AGENTS.md 冲突的标点白名单, 不复制完整标点判定表; 配置中的语言条款只是 AGENTS.md 的操作性投影. 验证: 人工逐条对照 AGENTS.md 的 "OpenSpec 规则" 与 "输出原则", 记录一致的结论; `rg -n 'AGENTS' openspec/config.yaml` 至少一处指向.
- [ ] 8.4 组验证: 运行 `openspec validate add-project-hygiene --strict` 与 `openspec validate --all --strict`; 运行 `openspec status --change add-project-hygiene --json` 确认 specs 仍为 skipped; 人工检查 YAML 缩进与引号.

## 9. 集成验证

- [x] 9.1 运行 `./scripts/check.sh`, 确认标点检查与五条 cargo 检查全部通过. 验证: 退出码 0, 输出无 warning, 耗时在 6.4 记录的范围内.
- [x] 9.2 运行 `cargo +1.97.0 test --locked`, `cargo check --locked --target x86_64-unknown-linux-gnu` 与 `cargo check --locked --target x86_64-pc-windows-gnu`, 确认与 CI job 一致. 验证: 三条命令均通过; 交叉编译只证明可编译.
- [x] 9.3 运行 `openspec validate add-project-hygiene --strict`. 验证: 输出通过且没有要求新增 spec; `openspec status --change add-project-hygiene --json` 的 proposal, design, tasks 为 done, specs 为 skipped.
- [x] 9.4 扫描本变更目录与 README: 使用 `scripts/check-punctuation.py` 或等价的 `python3` 命令确认无禁止码点. 验证: 0 命中; 本变更的三个 artifact 均不包含 U+3000-303F, U+FF00-FFEF, U+2018-201F, U+2014, U+2026 或 U+00B7.
- [x] 9.5 检查工作区: `git status --short` 与 `git diff --stat` 只包含本变更目录以及 2.x, 3.x, 4.x, 5.x, 6.x, 7.x, 8.x 涉及的文件. 验证: 人工核对没有改动 `harden-path-identity` 等其他变更目录, `Cargo.lock` 与 `tests/` 的测试语义.
- [x] 9.6 CI 首次运行: 推送或开 PR 后四个 job 全绿, 记录 run URL; 若采纳 6.5, audit 手动 dispatch 成功. 验证: GitHub Actions 页面可观察; README 仍写明 Windows 运行时行为未验证.

## Workflow follow-up

- 归档前重新运行 `openspec validate add-project-hygiene --strict` 与 `openspec validate --all --strict`, 确认 `skip_specs: true` 仍生效.
- 归档前确认 1.1 的许可证确认记录可引用; 若仍未确认, 不要把 2.x 勾选为完成, 并决定是否把许可证拆为后续变更.
- 归档后确认 `openspec instructions proposal/design/tasks` 已输出新的 context 与 rules.
- 归档后确认 CI 徽章 (若采纳) 指向 `ci.yml`, 且默认分支上的首次运行成功.
- 若后续增加 stable 前瞻 job 或扩大标点检查范围, 另开变更, 不在本变更归档时夹带.

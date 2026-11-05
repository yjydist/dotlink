# Proposal

## Why

dotlink 已在 GitHub 公开 (github.com/yjydist/dotlink), 但仓库没有 `.github/workflows`, 也没有 `LICENSE` 与 `Cargo.toml` 的 `license` / `description` / `repository` 元数据: 任何改动都不会自动经过 fmt, 全量测试, clippy, rustdoc 与 release build 校验, 使用者也看不到使用条款. 本地检查还漏掉 AGENTS.md 要求的 ASCII 标点规则, README 目前有 21 处 U+3001 (顿号); 开发工具链未固定, 新环境可能落在不同 rustc 上; 四条已知限制未写入 README; `openspec/config.yaml` 的 context 只有 ASCII Art 与中文两条既有约束, rules 仍全部是注释, OpenSpec 工作流因此拿不到本项目的 MSRV, 平台与验证约束.

## What Changes

- CI: 新增 `.github/workflows/ci.yml`. 主 job 在 macOS 与 Linux 上运行 `./scripts/check.sh` (fmt, 全量测试, clippy `-D warnings`, rustdoc `-D warnings`, release build); 独立 MSRV job 运行 `cargo +1.97.0 test --locked`; 交叉编译 job 运行 `cargo check --locked --target x86_64-unknown-linux-gnu` 与 `cargo check --locked --target x86_64-pc-windows-gnu`. 使用 cargo/target 缓存, `concurrency` 取消过期运行并设置超时. 交叉编译只证明源码可编译, 不验证 Windows 运行时行为.
- 依赖审计: 新增可选的每周 `cargo audit` 工作流 (支持手动触发), 需要网络, 不进入 `scripts/check.sh`, 本地检查不因审计新增联网依赖.
- LICENSE: 新增根目录 `LICENSE`, 并在 `Cargo.toml` 增加 `license`, `description` 与 `repository`. 许可证由维护者确认 (建议 MIT), 本变更不替维护者选定; 未确认前不创建 `LICENSE`, 不写 `license` 值.
- rust-toolchain.toml: 固定开发工具链为 1.97.0 (与 MSRV 一致), 声明 `rustfmt` 与 `clippy` 组件; stable 备选方案在 design 中记录并说明否决理由.
- 标点检查: `scripts/check.sh` 增补非 ASCII 标点检查, 覆盖 `AGENTS.md`, `README.md`, `openspec/specs/**`, `openspec/changes/**`, `src/**`, `tests/**` 与 `scripts/**` 的自然语言; 排除 `.agents`, `target`, `.git`. 检查 U+3000-303F, U+FF00-FFEF, U+2018-201F, U+2014, U+2026 与 U+00B7. 代码或夹具中确需这些字符时使用语言转义, 不设逐行豁免. 变更产物本身也是 OpenSpec 制品, 同样受 AGENTS.md 约束, 因此纳入同一检查, 防止后续变更回归.
- README 合规: 把 10 行中的 21 处 U+3001 替换为 ASCII 逗号; 新增四条已知限制 (`undo` 不清理 `apply` 创建的父目录, 备份命名空间由 dotlink 保留, 并发原地修改不会进入备份, 非 UTF-8 链接文本在报告中 lossy 显示); 可选增加 CI 状态徽章.
- 源码注释: `src/manifest.rs:56` 的 "纯函数" 改为 "只读, 不写文件系统" (它经 `os::normalize` 读取符号链接元数据); `src/backup.rs:32` 的暂存名注释补上 `.<pid>`.
- OpenSpec 配置: 保留 `openspec/config.yaml` 的 context 现有两条约束 (ASCII Art 只使用 ASCII 字符并优先 mermaid, 以及 artifact 使用简体中文), 追加 Rust 2024, MSRV 1.97, 仅 macOS 与 Linux, 无异步运行时, 无状态文件五项技术约束, 并启用 per-artifact rules (简体中文与仅 ASCII 标点, 与 README/specs 一致, 验证场景可执行).
- 本变更没有 spec 级行为变化: 不新增或修改任何 requirement 与 scenario, `.openspec.yaml` 设置 `skip_specs: true`.

## Capabilities

### New Capabilities

无.

### Modified Capabilities

无. 本变更只涉及 CI, 许可证, 工具链, 检查脚本, 文档与注释, 不改变可观察行为; `.openspec.yaml` 已设置 `skip_specs: true`, 不伪造 requirement 来满足校验.

## Impact

- 新增文件: `.github/workflows/ci.yml`, `.github/workflows/audit.yml` (可选), `LICENSE` (待维护者确认), `rust-toolchain.toml`, `scripts/check-punctuation.py`.
- 修改文件: `Cargo.toml`, `scripts/check.sh`, `README.md`, `src/manifest.rs`, `src/backup.rs`, `openspec/config.yaml`.
- 许可证待定: `LICENSE` 正文与 `Cargo.toml` 的 `license` 值取决于维护者对许可证 (建议 MIT), 版权署名与年份的确认. 未确认前这两项不落地, 其余任务不受影响; 若选择其他 SPDX 许可证, 只替换这两处.
- 运行时: 不修改逻辑, 测试语义, 依赖, MSRV, 平台门禁或退出码; `Cargo.lock` 不变; Windows 运行时行为仍未验证.
- CI 成本: 两个 OS 的完整检查加 MSRV 与交叉编译依赖缓存; macOS runner 计费高于 Linux, 通过缓存, 超时与 `concurrency` 控制时长. 首次运行需要下载固定工具链.
- 网络: 首次工具链安装与每周 audit 需要网络; 本地 `./scripts/check.sh` 不新增必须联网的步骤.
- 其他变更: 只改本变更目录与上述文件, 不触碰 `harden-path-identity` 等并行变更目录.

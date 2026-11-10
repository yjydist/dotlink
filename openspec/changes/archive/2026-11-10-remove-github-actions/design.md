# Design

## Context

当前质量门分两层:

- 本地 `scripts/check.sh`: 标点检查及其回归测试, `cargo fmt --check`, `cargo test --locked`, `cargo clippy -D warnings`, rustdoc `-D warnings`, release 构建.
- 远程 GitHub Actions: `check` 在 ubuntu 与 macOS 上复跑 `check.sh`; `msrv` 在 ubuntu 上跑 `cargo +1.97.0 test --locked`; `cross-check` 在 ubuntu 上编译 Linux 与 Windows GNU 目标; `audit` 每周运行 `cargo audit` 并支持手动触发.

当前环境下 workflow 无法启动, 定时任务在启动前即失败. 仓库没有 branch protection, rulesets 与 open PR, 删除配置不会阻塞任何流程. `rust-toolchain.toml` 把开发工具链钉在 1.97.0, 与 MSRV 相同, 因此本地 `cargo test` 已覆盖最低版本. 仓库是 public, 归档后的 artifact 长期可见.

现有的 8 个 capability 全部描述 CLI 运行时行为, 没有任何规格要求 CI 或托管服务, 因此本次变更不产生 delta spec, 见 `proposal.md` 的 Capabilities 段.

## Goals / Non-Goals

**Goals:**

- 让 README 与实际一致: 不再宣称存在可运行的托管 CI.
- 保持本地检查为唯一必需门禁, 且不削弱它的检查项.
- 保留配置的恢复路径与其余手工验证命令.

**Non-Goals:**

- 不引入替代的托管 CI, 自托管 runner 或其他 CI 供应商.
- 不新增 Linux 容器脚本或独立的 audit 脚本.
- 不修改 CLI 行为, 测试, 本地检查脚本的检查项, 依赖或工具链配置.
- 不在公开仓库记录账户级的具体原因.

## Decisions

### D1: 删除两个 workflow 文件, 而不是 disable

删除 `.github/workflows/ci.yml` 与 `.github/workflows/audit.yml`. 两个文件由 `cbb7c01` 一次性加入, git 历史完整保留, 恢复只需 `git show` 或 revert.

备选: `gh workflow disable` 停止调度. 否决理由: 留下两具不再运行的 YAML, 读者仍需猜测它们为何存在; README 无论如何都要改写, 保留文件节省的工作量为零, 却增加长期误导.

备选: 保留文件不动. 否决理由: `audit.yml` 每周继续触发并失败, badge 在下次 push 后变红, README 与事实持续不符.

### D2: 删除 README 顶部的 CI badge

badge 指向 `actions/workflows/ci.yml`; 文件删除后该 URL 不再对应任何 workflow, 保留只会显示 no status.

备选: 换成静态的 "local checks" badge. 否决理由: 静态 badge 不携带可验证的状态, 是纯装饰.

### D3: 用本地质量门段落替换 CI job 列表

新段落需要说明三件事, 使读者在删除 CI 段后仍知道推送前该做什么:

- `./scripts/check.sh` 是唯一必需门禁, 其检查项由紧邻的既有段落描述, 不重复罗列.
- 最低 Rust 版本与跨平台编译不进入该脚本, 按紧随其后的既有命令块手工执行.
- 依赖审计不进入 `check.sh`, 以免网络可用性成为提交前置 (沿用 `add-project-hygiene` 的既定理由), 由维护者安装 `cargo-audit` 后按需运行 `cargo audit`.

备选: 删除 CI 段且不补说明. 否决理由: 读者会以为仓库没有任何质量门, 或不知道 audit 已被移除.

### D4: 保留 `check-punctuation.py` 的 `.github/**` 扫描范围

该条目描述扫描范围, 不描述仓库当前清单; 目录不存在时零成本. 将来新增 issue 模板或 dependabot 配置时, 这些文件仍会被自动检查. 回归测试同样不改.

备选: 同步删除代码条目, docstring 与相关测试. 否决理由: 为了一致性去改动测试的覆盖声明, 收益为零, 还会让下一次出现的 `.github` 文件漏检.

### D5: 不把账户级原因写进公开 artifact

proposal 与 design 归档后长期公开. 用"当前环境下 workflow 无法启动"描述事实, 不写账单, 账户或供应商工单细节.

备选: 写明具体失败信息. 否决理由: 泄露个人财务信息, 且与变更的技术内容无关.

### D6: 不新增 Linux 验证脚本

删除后唯一实质缺口是 Linux 上的真实执行. Linux 专属代码只有 `src/os/metadata.rs` 的 `ENODATA` 免错分支与 `tests/metadata.rs` 中 `cfg(not(target_os = "macos"))` 的断言, 其余是通用 Unix 代码. README 记录这一缺口, 由维护者按需在 Linux 机器或容器中运行 `cargo test --locked`.

备选: 新增 `scripts/check-linux.sh`, 用容器运行 `cargo test`. 否决理由: 引入一套平时很少执行的基础设施, 依赖本地容器运行时, 维护成本高于它消除的回归风险.

备选: 把 `cargo audit` 加进 `check.sh`. 否决理由: 把网络可用性变成提交前置, 与 `add-project-hygiene` 的既有决策冲突.

### D7: 不迁移到其他托管 CI

单维护者项目, 本地门禁已覆盖编译, 测试与 lint. 迁移会新增账号, 配置与另一套外部可用性依赖, 而收益仅是恢复"在别人的机器上复跑一遍".

备选: 迁移到其他 CI 平台. 否决理由: 维护面增大, 对当前项目规模不成比例.

## Risks / Trade-offs

[Linux 回归无法自动发现] -> README 记录按需验证方式; 发布前在 Linux 上手工运行 `cargo test --locked` 或容器内运行 `check.sh`.

[依赖公告无人监控] -> README 写明手工 `cargo audit`; 维护者在升级依赖或发布前执行.

[将来恢复托管 CI 需要重写配置] -> 从 `cbb7c01` 取回原文件; 恢复时重新核对 action 版本与 runner 标签, 例如运行注解已提示 `ubuntu-latest` 将迁移.

[归档 artifact 与 README 描述不一致] -> 归档 artifact 是历史记录, 保持原样; README 是当前事实来源.

## Migration Plan

1. 删除两个 workflow 文件, 确认 `git ls-tree -r --name-only HEAD .github` 在提交后无输出.
2. 更新 README: 删除 badge, 替换开发段的 CI 段落.
3. 运行 `./scripts/check.sh` 与 `openspec validate remove-github-actions --strict`, 确认无回归.
4. 回滚策略: revert 提交即可恢复 workflow; 若只需停止定时任务而不改仓库, 可先执行 `gh workflow disable` 作为临时手段.

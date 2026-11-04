# Design

## Context

`specs/001-dotfile-link-management/` 已在迁移中删除, OpenSpec 主规格位于 `openspec/specs/<capability>/spec.md`. 当前工作区仍有 61 处旧文件名引用, 84 处 `T-nnn`, 19 处 `SC-n` 与 15 处非 R2 的 `R-n`; README 第 270 行与第 279 行各有一个坏链接. `FR-n` 与 `V-n` 仍保留在 requirement 名称中, 因此编号本身可定位, 但缺少文件路径.

## Goals / Non-Goals

**Goals:**

- 所有旧文件名引用改为对应的 OpenSpec capability 路径; 无法对应的编号删除或改写为 requirement 名称.
- README 的两个坏链接改为 `openspec/specs/` 与 `cli-contract` 的 Scenario.
- 不改变任何可执行行为, 不新增依赖.
- 用 `rg` 与既有质量门作为验收.

**Non-Goals:**

- 不新增 CI; 不新增 GitHub Actions.
- 不改变规格内容, 不新增 capability, 不修改 requirement 名称.
- 不引入自动引用检查脚本; 验收命令保留在 tasks 中, 是否加入 `scripts/check.sh` 另行决定.

## Decisions

### D1. 文件名到 capability 的映射

| 旧引用 | 新落点 |
|---|---|
| `contracts/manifest.md` | `openspec/specs/manifest-schema/spec.md` |
| `contracts/cli.md` 的命令面/退出码/输出 | `openspec/specs/cli-contract/spec.md` |
| `contracts/cli.md` 的 preview/apply | `openspec/specs/link-deployment/spec.md` |
| `contracts/cli.md` 的 undo | `openspec/specs/link-teardown/spec.md` |
| `data-model.md` 1-2 节 | `openspec/specs/manifest-schema/spec.md` |
| `data-model.md` 3 节 | `openspec/specs/entry-classification/spec.md` |
| `data-model.md` 4 节 | `openspec/specs/backup-lifecycle/spec.md` |
| `data-model.md` 5 节 | `openspec/specs/cli-contract/spec.md` |
| `research.md` R2 | `openspec/specs/path-normalization/spec.md` |
| `research.md` R4 | `openspec/specs/backup-lifecycle/spec.md` |
| `research.md` R5 | `openspec/specs/manifest-schema/spec.md` |
| `research.md` R6 / R10 | `openspec/specs/cli-contract/spec.md` |
| `research.md` R7 | `openspec/specs/entry-classification/spec.md` |
| `research.md` R8 | 删除引用, 或改为 README 的"已知限制" |
| `quickstart.md` | `openspec/specs/cli-contract/spec.md` 的 Scenario 与 README 示例 |

`T-nnn` 指向已归档的任务清单, 删除; 如该句仍需解释行为, 改写为对应的 requirement 语句. `SC-n` 未迁移: `SC-006` (preview 只读) 改写为 `link-deployment` 的要求, `SC-009` (help) 改写为 `cli-contract` 的要求, 其余删除. 非 R2 的 `R-n` 删除; 若该句是唯一的行为说明, 改为 requirement 名称.

### D2. 引用写法

统一为"仓库相对路径 + 编号", 例如:

- `openspec/specs/manifest-schema/spec.md` 的 V-9
- `openspec/specs/cli-contract/spec.md` 的 FR-016

同一段落多次引用同一 capability 时, 首次给完整路径, 后续只给编号. 这样编辑器可跳转, `rg` 也可按路径或编号定位.

### D3. README 的两处坏链接

第 270 行改为:

```text
设计文档 (规格, 契约, 数据模型, 验证场景) 在 `openspec/specs/`, 按能力分为 8 个目录; 每份 `spec.md` 含 requirement 与 Scenario.
```

第 279 行删除 `quickstart.md` 链接, 改为指向 `openspec/specs/cli-contract/spec.md` 的 Scenario 与 README 既有示例, 并说明端到端验证流程已拆入各 capability.

### D4. 验收

```sh
rg 'research\.md|contracts/cli\.md|data-model\.md|contracts/manifest\.md|quickstart\.md' src tests README.md
rg '\bT[0-9]{3}\b' src tests README.md
rg 'SC-[0-9]+' src tests README.md
rg '\bR(?!2\b)[0-9]+\b' src tests README.md --pcre2
cargo fmt --check
cargo test --locked
cargo clippy --all-targets -- -D warnings
```

前四条零命中, 后三条通过. 时间戳中的 `T000000` 不应被 `T-nnn` 规则误伤; 使用单词边界, 并在改写后抽查备份命名相关注释.

## Risks / Trade-offs

- **机械改写丢失论证细节** -> 只替换引用, 保留原有解释; 对 `R8` 这类"限制而非契约"的引用改为 README 的已知限制, 不硬塞进 spec.
- **同一旧文件对应多个 capability** -> 按小节映射, 不把整个 `contracts/cli.md` 指向单一 capability, 避免错误归属.
- **`rg` 规则误伤** -> 排除时间戳 `T000000`, 并对 `R2` 单独放行; 验收命令在 tasks 中固定.
- **注释行长度变化** -> `cargo fmt --check` 只约束代码, 不约束注释; 人工检查可读性.

## Migration Plan

无运行时迁移. 回滚时用 git 还原 `src`, `tests` 与 `README.md` 的注释改动即可; 本变更不触碰可执行代码.

## Open Questions

- 是否把四条 `rg` 验收命令加入 `scripts/check.sh`; 本变更暂不加入, 避免与"不构造 CI"的范围混淆.

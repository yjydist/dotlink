# Design

## Context

见 `proposal.md` 的 Why. 现状由 `scripts/check-punctuation.py` 的两组白名单决定:

- `TEXT_FILES`: 逐个点名 `AGENTS.md` 与 `README.md`;
- `TEXT_TREES`: 目录加后缀, 覆盖 `openspec/specs`, `openspec/changes`, `src`, `tests`, `scripts`;
- `SKIP_DIRS` 跳过 `.agents` / `.git` / `target`, 目录与文件都不跟随符号链接.

当前实际扫描 115 个文件. 未纳入的 `.github/workflows/*.yml` 与 `openspec/config.yaml` 都含中文,
其中前者是 CI 说明性注释, 后者是 OpenSpec 的项目上下文与 per-artifact 规则.

## Goals / Non-Goals

- Goal: AGENTS.md 的标点规则在"实际承载中文自然语言"的文件上都有执行点.
- Goal: 负向用例无需在仓库里留下违规样例即可复现.
- Non-Goal: 不做全仓库文本扫描, 不引入第三方 lint 框架, 不检查二进制或构建产物.
- Non-Goal: 不改退出码语义与输出格式.

## Decisions

### D1. 继续用白名单目录加后缀, 不改成全仓库扫描

备选: 遍历仓库所有文件并按 UTF-8 解码成功与否决定是否检查. 否决: 需要另一张排除表来跳过
`target/`, `Cargo.lock`, 图片与其它生成物; 排除表本身会漂移, 且误报一次就会让人绕过检查.
白名单的失败模式是"漏检一个新增位置", 只影响新增位置的覆盖, 不会产生噪音.

### D2. `openspec/config.yaml` 用两个显式文件名, 不把 `openspec/` 顶层加入 `TEXT_TREES`

OpenSpec 允许项目配置名为 `config.yaml` 或 `config.yml`, 两个名字都点名即可, 不存在的自动跳过
(`iter_files` 已对每个 `TEXT_FILES` 成员做 `is_file()` 判断).

备选: 把 `openspec` 加入 `TEXT_TREES` 并收 `.yaml` / `.yml`. 否决: `TEXT_TREES` 是递归的, 会连带
覆盖将来出现在 `openspec/**` 下的工具元数据 (例如机器生成的状态或缓存描述), 那些文件里的非自然语言
内容会造成无意义命中. 显式点名把覆盖面限定在承载自然语言的那一个文件上.

### D3. `.github/` 一并收 `.md`

工作流用 `.yml`, 但 `.github` 下还可能出现 issue 模板与 PR 模板 (`*.md`), 它们同样是中文密集的自然
语言载体. 现在没有这类文件, 收进后缀表是为了位置被新增时不需要再改脚本.

### D4. 负向验证用 `--root` 指向临时树

脚本已支持 `--root`, 因此在 `mktemp -d` 建出的临时根目录里放一个含全角逗号的
`.github/workflows/probe.yml`, 就能断言"命中并退 1".

上述临时树验证已固化为 `scripts/test_check_punctuation.py`, 由 `scripts/check.sh` 在标点检查之后运行,
额外断言白名单外的后缀被忽略.

备选: 在 `tests/` 或 `scripts/` 里放一个含全角标点的夹具文件. 否决: 夹具本身会被扫描器命中, 只能靠
`SKIP_DIRS` 排除, 等于为了测试而削弱被测的覆盖面.

### D5. `Cargo.toml` 与 `.gitignore` 不纳入

两者当前不含中文自然语言, 也没有承载说明性文案的意图; 纳入后没有覆盖收益, 只增加"将来有人加中文
注释"这一假设的维护成本. 若确实出现中文注释, 随那次改动一并加入白名单, 与 D1 的失败模式一致.

## Risks / Trade-offs

- **新覆盖立刻暴露既有违规** -> 落地前先跑一次; 若命中, 在同一变更内修正该文件, 不得用排除绕开.
  本次探索已预检: `.github/**` 与 `openspec/config.yaml` 当前都是合规的, 预期不会命中.
- **`.github` 将来出现生成的 `.yml`** -> 按 `SKIP_DIRS` 的既有做法加最小排除, 并在 docstring 记录原因.
- **白名单与 README 枚举再次漂移** -> 本变更把 README 的枚举写进任务验证条件, 要求两边逐项一致.

## Migration Plan

无. 无违规时 `./scripts/check.sh` 的行为与退出码不变.

## Open Questions

无.

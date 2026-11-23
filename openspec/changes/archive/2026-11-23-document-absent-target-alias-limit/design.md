# Design

## Context

`manifest::parse` 的目标去重同时比较规范化字符串, 比较形态与文件系统身份. 当两个 target 都不存在时, `os::target_identity` 返回错误, 因此只剩字符串比较; 大小写别名与 Unicode 规范化别名在大小写不敏感或规范化不敏感的文件系统上无法被判为同一个目标. `apply` 按清单顺序执行, 第一条建成链接后, 第二条的建链会得到 `EEXIST` 并报告 `fail`. 这是既有校验口径的直接结果, 不是本次要修复的缺陷.

## Goals / Non-Goals

**Goals:**

- 把该边界写进 README, 并用一个平台门控测试锁定当前行为, 避免后续无意改变或误以为已经支持.
- 测试只在大小写不敏感卷上运行, 大小写敏感卷上跳过并说明原因.

**Non-Goals:**

- 不在清单校验阶段做只读的卷大小写探测.
- 不做 Unicode 规范化折叠.
- 不改变 `preview` 与 `apply` 的判定或执行顺序.

## Decisions

### D1. 用测试锁定"当前限制", 而不是把它当作要通过的行为

测试断言 `preview` 报两个 `create` 且 `apply` 第二条 `fail`, 并在注释中明确这是已知限制. 备选方案是直接拒绝这类清单; 否决理由是那需要额外的平台探测, 会改变清单接受面, 超出本变更的范围.

### D2. 测试放在 `tests/manifest_errors.rs`

该文件已有 `case_insensitive_volume`, `TempEnv`, `run`, `line_for` 与 `token_of` 等辅助, 能在同一处同时观察清单校验与 apply 结果. 备选方案是新建测试文件; 否决理由是新增文件只为一条平台门控测试, 收益不足.

### D3. 不新增规格 delta

这是已知限制的记录与回归锁定, 没有新的行为承诺, 因此 `.openspec.yaml` 设置 `skip_specs: true`.

## Risks / Trade-offs

- [测试在大小写敏感卷上跳过] -> 打印跳过原因, 与现有 macOS 专用用例一致.
- [限制被误解为支持] -> README 与测试注释都写明"不保证被拒绝".

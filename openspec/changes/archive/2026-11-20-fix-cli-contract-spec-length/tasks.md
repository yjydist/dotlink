# Tasks

## 1. 规格拆分

- [x] 1.1 在 `openspec/changes/fix-cli-contract-spec-length/specs/cli-contract/spec.md` 写入 "报告列宽按终端显示宽度计算" 的 `REMOVED` 块, 以及 "报告列宽按显示宽度补齐" 与 "宽度表的整块保留与近似规则" 的 `ADDED` 块. 验证: `openspec show fix-cli-contract-spec-length --json --deltas-only` 列出 1 个 removed 与 2 个 added requirement.
- [x] 1.2 核对原 requirement 的全部 MUST 片段与 6 个 Scenario 都已保留. 验证: 用脚本分别提取原 `openspec/specs/cli-contract/spec.md` 中该 requirement 的 MUST 片段与 Scenario 名称, 以及 delta 中两条新 requirement 的并集, 两组差集均为空.
- [x] 1.3 确认两条新 requirement 的正文均不超过 500 字符. 验证: 用脚本统计 delta 中每个 `### Requirement:` 到其首个 `#### Scenario:` 之间的文本长度, 两条均 <= 500.
- [x] 1.4 运行变更级严格校验. 验证: `openspec validate fix-cli-contract-spec-length --strict` 退出 0, 无正文长度 warning.

## 2. 一致性核对

- [x] 2.1 核对拆分未改变行为措辞. 验证: 脚本确认原正文拼接结果与两条新正文拼接结果除组织和连接词外无差异, 且所有含 `MUST` 或 `MUST NOT` 的片段逐条出现在新正文中.
- [x] 2.2 核对 Scenario 归属与设计一致. 验证: `openspec show fix-cli-contract-spec-length --json --deltas-only` 显示 "报告列宽按显示宽度补齐" 含 4 个 Scenario, "宽度表的整块保留与近似规则" 含 2 个 Scenario, 名称与主规格原文逐字一致.
- [x] 2.3 核对未触碰禁止修改的文件. 验证: `git status --short` 中本变更只修改 `openspec/changes/fix-cli-contract-spec-length/`; 同一工作区中其它 change 的规划改动不计入本变更; 主规格, `src/`, `tests/`, `README.md` 与 `openspec/config.yaml` 无改动.

## 3. 全量验证

- [x] 3.1 运行全量校验并记录基线. 验证: `openspec validate --all --strict` 中 `spec/cli-contract` 仍报告 requirements[15] 的长度 warning (主规格尚未归档更新), 其余 spec 与 change 无本变更引入的新增失败.
- [x] 3.2 运行状态检查. 验证: `openspec status --change fix-cli-contract-spec-length --json` 显示 proposal, specs, design, tasks 四个制品均为 done.

## Workflow follow-up

- 归档 `fix-cli-contract-spec-length` 并同步主规格.
- 重新运行 `openspec validate --all --strict`, 确认 `spec/cli-contract` 的 requirements[15] 长度 warning 消失.

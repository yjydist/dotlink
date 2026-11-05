# Tasks

## 1. 规格拆分

- [x] 1.1 在 `openspec/changes/split-manifest-v9-requirement/specs/manifest-schema/spec.md` 写入 V-9 的 REMOVED 块与 V-9, V-9a, V-9b 的 ADDED 块. 验证: `openspec show split-manifest-v9-requirement --json --deltas-only` 列出 1 个 removed 与 3 个 added requirement.
- [x] 1.2 核对原 V-9 的全部 MUST 与 8 个 Scenario 都已保留. 验证: 用脚本比较原主规格 V-9 与三项新增 requirement 的 MUST 片段与 Scenario 名称, 差集为空.
- [x] 1.3 确认三条 requirement 的正文均不超过 500 字符. 验证: 用脚本统计 `### Requirement:` 到首个 `#### Scenario:` 之间的文本长度, 三条均 <= 500.
- [x] 1.4 运行变更级严格校验. 验证: `openspec validate split-manifest-v9-requirement --strict` 退出 0.

## Workflow follow-up

- 归档 `split-manifest-v9-requirement` 并同步主规格.
- 重新运行 `openspec validate --all --strict`, 确认 V-9 长度 warning 消失.
- 回到 `add-project-hygiene` 勾选 8.4 并提交.

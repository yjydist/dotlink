# Tasks

## 1. 增量与校验

- [ ] 1.1 保持 `specs/backup-lifecycle/spec.md` 只有 `## RENAMED Requirements` 一节, 且 FROM 与 TO
      紧邻成对; 验证: `openspec change show align-backup-timestamp-spec --json --deltas-only` 显示出
      一条 `RENAMED`, 旧名与新名与主规格和 README 的措辞一致.
- [ ] 1.2 验证 FROM 与主规格标题逐字相同; 验证: `grep -F '### Requirement: 备份名携带秒级本地时间戳'
      openspec/specs/backup-lifecycle/spec.md` 有输出, 而 delta 的 FROM 行与之完全一致.
- [ ] 1.3 验证 delta 未携带任何需求正文; 验证: delta 文件里 `### Requirement:` 只出现在 FROM/TO 行的
      反引号内, `grep -c '^### ' openspec/changes/align-backup-timestamp-spec/specs/backup-lifecycle/spec.md`
      输出 0.

## 2. 一致性与集成

- [ ] 2.1 确认仓库内没有需要同步的其他引用; 验证: `grep -rn '备份名携带秒级' --include=*.md .` 只命中
      主规格, 本变更的制品与归档目录.
- [ ] 2.2 运行 `openspec validate --all`; 验证: 全部通过, 其中 `change/align-backup-timestamp-spec`
      为通过而不是 skip_specs 说明.
- [ ] 2.3 运行 `./scripts/check.sh`; 验证: 退出 0 (本变更不改代码, 该步骤确认新增制品未破坏标点检查与
      文档构建).

## Workflow follow-up

- 归档时执行 `openspec archive align-backup-timestamp-spec`, 归档后用
  `grep -n '^### Requirement: 备份名携带秒级' openspec/specs/backup-lifecycle/spec.md` 确认标题已是
  "备份名携带秒级 UTC 时间戳", 且旧名称不再出现在主规格.
- 归档后重跑 `openspec validate --all` 与 `./scripts/check-punctuation.py`.

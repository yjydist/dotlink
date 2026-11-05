# Tasks

## 1. 时间戳改为 UTC

- [ ] 1.1 在 `src/backup.rs` 把 `timestamp()` 改为 `OffsetDateTime::now_utc()`, 将格式化收敛为接收 `OffsetDateTime` 的纯函数, 删除 `timestamp_at(Option<OffsetDateTime>)` 的 `None` 退化分支; 改写 `src/backup.rs:378-401` 的注释, 删除 "退化只影响时间戳的字面值, 不影响任何不变量", 写明 UTC 是排序可比较性的前提. 验证: `cargo test --locked backup` 通过, 且 `grep -nE "now_local|timestamp_at" src/backup.rs` 在非测试代码中无结果.
- [ ] 1.2 更新 `src/backup.rs` 的时间戳单测: 用固定的 `2026-11-04T23:17:19+08:00` 断言纯格式化函数输出 `20261104T151719`, 证明带偏移的输入被归一到 UTC; 另保留一个 epoch (`19700101T000000`) 用例; 删除依赖 `Option` 的退化用例, 保留 `backup_name_carries_a_second_precision_timestamp` 的 15 字符断言. 验证: `cargo test --locked backup` 通过, 断言消息写明 UTC 期望.
- [ ] 1.3 在 README 的备份命名说明中注明时间戳是 UTC, 与 delta 的 "备份名形态" 一致. 验证: 人工比对 README 与 `openspec/changes/harden-backup-ordering/specs/backup-lifecycle/spec.md`, 并运行 `python3` 扫描该段无禁用标点.

## 2. 依赖收缩

- [ ] 2.1 从 `Cargo.toml` 删除 `time` 的 `local-offset` feature, 保留默认 feature, 不新增依赖. 验证: `cargo tree -e features -i time` 输出不含 `local-offset`, 且 `cargo check --locked` 通过.
- [ ] 2.2 复核移除 feature 后 `Cargo.lock` 无版本漂移: 运行 `git diff -- Cargo.lock`, 只允许出现不再需要的可选依赖条目被删除, 不允许任何版本或校验和变化; 若有变化, 回退并查明原因. 验证: 上述命令输出符合预期, 且 `cargo test --locked` 通过.
- [ ] 2.3 在 README 的平台或备份说明中注明生成备份不读取本地时区, 容器或缺少 tzdata 的环境不再影响时间戳. 验证: 人工核对与 design D2 一致, 且 `grep -nE "本地时区|tzdata" README.md` 的措辞不与该说明冲突.

## 3. 排序语义与跨 TZ 回归

- [ ] 3.1 更新 `src/backup.rs:111-147` 与 `src/backup.rs:442-446` 的注释, 写明时间戳来自 UTC, 定长 15 字符, 因此字符串序即时间序; 保留 "先时间戳, 再序号数值, 再文件名兜底" 与 `.partial` 排除规则. 验证: `cargo test --locked backup` 通过, 注释不再声称本地时间戳可跨时区比较.
- [ ] 3.2 增加查找单测: 同目录并存两个不同 UTC 时间戳, 以及同一时间戳下的 `.9` 与 `.10`, 断言跨时间戳取较晚者, 同时间戳取 `.10`. 验证: `cargo test --locked backup` 通过, 断言消息写明取的备份名.
- [ ] 3.3 在 `tests/undo.rs` 或新增 `tests/backup_ordering.rs` 增加子进程端到端用例: 先以 `TZ=Asia/Shanghai` 生成旧内容备份, 等待至少 1 秒后以 `TZ=UTC` 生成新内容备份; 断言两个 15 位时间戳的绝对差小于 60 秒而不是 8 小时, 且 `undo` 恢复新内容. 验证: 修复前该用例因 8 小时差与恢复旧内容而失败, 修复后 `cargo test --locked --test undo` 通过, 若用例落在新文件则运行 `cargo test --locked --test backup_ordering`.
- [ ] 3.4 更新 README 已知限制中的 "`undo` 只恢复最新备份" 条目, 说明时间戳是 UTC, 排序先比时间戳再比消歧序号的数值. 验证: 人工比对与 `backup-lifecycle` delta 的 "取最新按时间戳再按序号数值" 一致.

## 4. 已知限制与迁移说明

- [ ] 4.1 在 README 已知限制中增加时钟回拨: NTP 校时或手动改表后, 新备份可能携带更早的 UTC 时间戳, 仍会被误判为较旧; 说明本工具不做自动修正. 验证: 人工核对与 design D4 及 Risks 第一条一致.
- [ ] 4.2 在 README 已知限制中增加旧备份混放: 升级前的本地时区备份与新的 UTC 备份同形但可能不可比, 建议清理或归档旧备份; 明确不自动迁移, 不自动删除. 验证: 人工核对与 design D5 及 Migration Plan 第 2 条一致.
- [ ] 4.3 用临时目录和真实二进制按 README 的 TZ 场景手动复现: `TZ=Asia/Shanghai` 生成备份后再用 `TZ=UTC` 生成更晚备份, 断言名字时间戳同轴且 `undo` 恢复较新内容. 验证: 按 README 步骤执行并记录两次备份名与 undo 结果, 证明文档步骤可执行.

## 5. 集成验证

- [ ] 5.1 运行 `./scripts/check.sh`, 确认 fmt, test, clippy, rustdoc 与 release build 全部通过. 验证: 脚本退出码为 0 且末尾输出无失败项.
- [ ] 5.2 运行 `cargo check --locked --target x86_64-unknown-linux-gnu` 与 `cargo check --locked --target x86_64-pc-windows-gnu`, 确认移除 `local-offset` 后这两个目标仍可编译. 验证: 两条命令退出码均为 0.
- [ ] 5.3 运行 `git status --short` 与 `git diff --stat` 复核改动只落在 `src/backup.rs`, `Cargo.toml`, `tests/undo.rs` 或新增的 `tests/backup_ordering.rs`, 以及 `README.md`; 确认未改动其他 capability 的 delta 与其他变更目录. 验证: 人工检查文件清单与 `openspec validate "harden-backup-ordering" --strict` 通过.
- [ ] 5.4 运行 python 扫描本变更目录与 README 改动段落, 确认无 U+3000-303F, U+FF00-FFEF, U+2018-201F, U+2014, U+2026 与 U+00B7. 验证: 扫描输出为空.

## Workflow follow-up

- 归档前复核 `backup-lifecycle` 两个 MODIFIED requirement 的合并结果, 确认 "命名消歧", "暂存名排他创建且只清理本次产物" 与 "只有完整命名的备份参与查找" 未被本变更触碰.
- 归档后确认 README 的 UTC 命名, 时钟回拨与旧备份混放说明仍与主规格一致.
- 评估是否用独立变更以 RENAMED 修正 "备份名携带秒级本地时间戳" 的标题措辞.

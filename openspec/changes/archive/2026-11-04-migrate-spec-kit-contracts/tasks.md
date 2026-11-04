# Tasks

## 1. 逐份核对 delta 与旧文档

- [x] 1.1 核对 `specs/manifest-schema/spec.md` 覆盖 `contracts/manifest.md` 全部章节与 V-1 到 V-11; 验证: `git show HEAD:specs/001-dotfile-link-management/contracts/manifest.md` 与 `git show HEAD:specs/001-dotfile-link-management/data-model.md` 逐节对照, 每个 V 编号在 delta 中恰好出现一次 (`rg -o 'V-[0-9]+' specs/manifest-schema/spec.md | sort -u`)
- [x] 1.2 核对 `specs/path-normalization/spec.md` 覆盖 `research.md` R2 的两段语义, 末段不穿透, 环上限与三处调用点同源; 验证: 与 `git show HEAD:specs/001-dotfile-link-management/research.md` 的 R2 段落逐条比对, 确认无遗漏与矛盾
- [x] 1.3 核对 `specs/entry-classification/spec.md` 覆盖 `data-model.md` 3.1 到 3.3 与 R7, 3.4 的状态迁移由 `link-deployment` 与 `link-teardown` 的 Scenario 覆盖; 验证: 八种状态在 delta 中全部出现, 动作映射覆盖 8 x 2 组合 (`rg -c 'Scenario:' specs/entry-classification/spec.md` 不少于 8)
- [x] 1.4 核对 `specs/cli-contract/spec.md` 覆盖退出码两条件, 四命令目标谓词, 12 个 token 与 4 个汇总列; 验证: 12 个 token 字面量在 delta 中全部出现, 且汇总列的四组归类相加等于 12
- [x] 1.5 核对 `specs/link-deployment/spec.md` 与 `specs/link-teardown/spec.md` 覆盖 `contracts/cli.md` 的四命令语义小节; 验证: 撤销分支表的六行与 `git show HEAD:specs/001-dotfile-link-management/contracts/cli.md` 的 `undo` 分支表逐行一致
- [x] 1.6 核对 `specs/backup-lifecycle/spec.md` 只约束位置, 命名, 查找与保留, 未复述内容与元数据保真; 验证: `rg -i '扩展属性|xattr|mtime|权限' specs/backup-lifecycle/spec.md` 零命中, 或仅出现指向 `backup-fidelity` 的边界说明
- [x] 1.7 核对 `design.md` 的逐节对照表与七份 delta 的实际覆盖一致, 无"声明归属但实际未写"的章节; 验证: 对表中每个"不迁"条目确认确实未出现在任何 delta 中

## 2. 交叉一致性与规范检查

- [x] 2.1 确认 requirement 名称在全部 delta 中唯一, 且 `[V-n]` / `[FR-n]` 标注无重复; 验证: `LC_ALL=C rg --no-filename '^### Requirement:' openspec/changes/migrate-spec-kit-contracts/specs | LC_ALL=C sort | LC_ALL=C uniq -d` 无输出
- [x] 2.2 确认 `backup-lifecycle` 与 `backup-fidelity` 的边界无重复定义, 且 `backup-fidelity` 未被修改; 验证: `git status --porcelain openspec/specs` 无输出
- [x] 2.3 确认未迁移的内容 (用户故事, 澄清会话, 技术方案, 任务清单, 质量检查清单) 没有出现在任何 delta 中; 验证: `rg 'US[0-9]|Clarifications|会话|plan\.md|checklists|SC-0' openspec/changes/migrate-spec-kit-contracts/specs/` 零命中 (只扫 `specs/`, 提案与设计文档会合法地提到这些名字)
- [x] 2.4 运行严格校验, 确认无无 Scenario 的 requirement, 无超长描述警告; 验证: `openspec validate migrate-spec-kit-contracts --strict` 输出 `is valid`

## 3. 项目级验证

- [x] 3.1 确认全部 planning artifact 已就绪; 验证: `openspec status --change migrate-spec-kit-contracts --json` 的 `artifacts` 四项均为 `done`, `isPlanningComplete` 为 `true`
- [x] 3.2 确认本变更未改动文档引用, 且生产代码改动仅限 5.1 的修正, 7.1 的修正, 5.5 的接缝与 8.9 的实际指向细化; 改动面只有 `openspec/changes/` 下的新文件, proposal 已声明的 `specs/001-dotfile-link-management/` 删除, `tests/` 下新增的断言与助手 (3.4, 4.x 与 8.x), 以及 `src/` 内的测试与 5.x / 7.x 列出的接缝与 8.9 的 `src/classify.rs` 细化, 外加第 9 节的注释与文档措辞修正 (README 第 5 行, `src/render.rs` / `src/error.rs` / `src/entry.rs` / `src/manifest.rs` 的注释). 验证: `git diff -- src` 只含 `src/entry.rs` 的 5.1 修正, 7.1 的 `src/manifest.rs` 修正 (`physical_home` 与 V-9 的两个主目录比较基准), 5.5 的参数化入口 (`src/lib.rs` 的 `run_on` / `platform_gate`, `src/backup.rs` 的 `timestamp_at` / `backup_candidates_at` / `write_backup_at`), 8.9 的 `src/classify.rs` 实际指向细化, README 第 5 行与 `src/render.rs` 注释的 undo 幂等表述修正, 以及 `src/backup.rs` / `src/os/mod.rs` / `src/undo.rs` 的 `#[cfg(test)]` 用例与 `src/lib.rs` / `src/error.rs` / `src/manifest.rs` 的测试与注释; `git status --porcelain -- README.md` 只出现一处 M, 且 `git diff -- README.md` 只改第 5 行, 且 `git status --porcelain` 中除 `openspec/changes/migrate-spec-kit-contracts/`, proposal 已声明的 `specs/` 删除, `tests/` / `src/` / `README.md` 的修改与未跟踪的 `AGENTS.md` (工作区既有文件, 与本变更交付无关) 外不出现其他路径
- [x] 3.3 确认既有测试保持通过, 作为"未改动行为"的旁证; 验证: `cargo test --locked` 全部通过
- [x] 3.4 补齐两条既有 Scenario 缺失的断言: preview 给出的备份路径与 apply 遵循同一命名规则 (同目录, 同一前缀, 15 位时间戳形态), 且 preview 不创建备份; 清单存在但不可读时四条命令都退 2 且零改动; 验证: `cargo test --locked --test preview preview_reports_the_backup_path_it_would_use` 与 `cargo test --locked --test manifest_errors an_unreadable_manifest_is_rejected_with_exit_code_two` 通过

## 4. 验证反馈修复

- [x] 4.1 补齐 `cli-contract` 的两条基线规则: 报告行除四列外不得增加列; 系统不维护数据库/状态文件/持久化索引 (含 Scenario). 验证: `openspec validate migrate-spec-kit-contracts --strict` 输出 `is valid`
- [x] 4.2 补齐 `link-deployment` 的 "预览行同时给出当前状态与将执行的动作" requirement 与两个 Scenario. 验证: `cargo test --locked --test preview preview_reports_the_current_state_and_the_planned_action`
- [x] 4.3 补齐 quickstart 场景 10/14 的 Scenario: 链接文本含 `..` 仍判 `ok`; 源缺失清单上的 `preview` 退 1. 验证: 两份 delta 各新增一个 Scenario 且 `openspec validate --strict` 通过
- [x] 4.4 修正设计/规范措辞: V-6 绝对路径不拼接仓库根; V-7 非字符串给出 TOML 类型; 删除 link-teardown 不可达的"恢复后目标仍为空"; cli-contract 条件 2 明确引用汇总列; D1 补 FR-008; D2 的 `V-n` 格式与实例一致.
- [x] 4.5 新增集成测试: 执行期失败不中断其余条目; 二次 undo 的两种恢复后情形; 只有 `.partial` 残留的 undo; 备份目录不可读时其余条目照常; preview 不建父目录; status 忽略覆盖策略; undo 的 `missing`; 四条命令不写状态文件. 验证: `cargo test --locked`
- [x] 4.6 强化既有断言: 帮助文本的 0/1/2/3 与 `set -e`; 标准输入不阻塞; 清单不可读的单行与底层原因; V-3/V-4/V-6/V-8 的精确组合; 备份暂存名被占用的直接断言. 验证: `cargo test --locked`
- [x] 4.7 记录无法端到端验证的分支 (平台门禁/时区退化) 与后续可注入接缝建议. 验证: `design.md` 的 Risks / Trade-offs 含该条

## 5. 验证报告反馈修复

- [x] 5.1 修正 `~//...` 形态的目标逃出主目录: `expand_target` 剔除 `~/` 之后多余的分隔符; V-6 补 "主目录简写后多写斜杠" 的 Scenario. 验证: `cargo test --locked --lib duplicate_separators_after_tilde_still_expand_under_home` 与 `cargo test --locked --test apply a_duplicate_separator_after_tilde_still_lands_in_home` 通过
- [x] 5.2 补齐 `link-deployment` 的 "替换别人的符号链接" 端到端用例, 并修正 `foreign_link_is_refused_by_default` 的夹具 (原夹具的链接落在仓库内, 实际走的是 `wrong` 分支). 验证: `cargo test --locked --test conflict` 全部通过
- [x] 5.3 在 `path-normalization` 的 R2 写明相对路径只做词法规整, 并用 `relative_paths_are_normalized_lexically_only` 锁定该边界. 验证: `cargo test --locked --lib relative_paths_are_normalized_lexically_only` 通过
- [x] 5.4 强化撤销的动手前复核断言: 判定后被替换成真实文件时, 同目录的备份不得被消费或删除. 验证: `cargo test --locked --lib a_real_file_that_replaced_the_link_is_refused_and_preserved` 通过
- [x] 5.5 拆出可参数化的时钟与平台门禁接缝 (`timestamp_at` / `backup_candidates_at` / `write_backup_at`, `run_on(supported)`), 使同秒序号消歧, UTC 退化与 "退 3 且先于参数解析" 可确定性测试. 验证: `cargo test --locked --lib two_replacements_in_one_second_get_serial_two_deterministically`, `... --lib timestamp_falls_back_to_utc_without_changing_the_shape`, `... --lib an_unsupported_platform_exits_three_before_parsing_arguments` 通过
- [x] 5.6 修正 `design.md` 的逐节归属表 (FR-006/007 归 `link-deployment`, FR-011 归 `cli-contract`, FR-012 归 `path-normalization`) 与 D4 的边界措辞; 在 D7 / Non-Goals / Risks 记录本次行为修正与新增接缝.

## 6. 验证报告反馈修复

- [x] 6.1 锁定平台门禁的输出契约: 抽出 `platform_gate(supported, stderr)`, 断言退 3, 标准错误恰一行, 不写标准输出, 并保留"门禁先于参数解析且与清单无关"的断言. 验证: `cargo test --locked --lib tests::the_platform_gate_writes_exactly_one_stderr_line_and_nothing_to_stdout` 与 `cargo test --locked --lib tests::an_unsupported_platform_exits_three_before_parsing_arguments` 通过.
- [x] 6.2 补齐调用方式错误与命令面回归: "用法错误先于清单读取"的判别性用例; `--force` 与 `gc` / `clean` / `cleanup` / `prune` 子命令或参数被拒; 根帮助不出现回收能力; 字面 `status --json` 与帮助文本中的 `case` / `dotlink apply` 示例. 验证: `cargo test --locked --test usage_errors` 全部通过.
- [x] 6.3 备份发布路径的悬空占用: 在最终名上摆悬空链接, 断言 `write_backup_at` 改用 `.2` 且占用物原样保留. 验证: `cargo test --locked --lib backup::tests::write_backup_retries_when_the_final_name_is_occupied_by_a_dangling_symlink` 通过.
- [x] 6.4 悬空链接撤销端到端: 仓库内悬空链接被移除 (`undo`), 仓库外悬空链接被跳过且链接文本保留 (`skip`). 验证: `cargo test --locked --test undo undo_removes_a_dangling_link_that_points_inside_the_repo` 与 `cargo test --locked --test undo undo_skips_a_dangling_link_that_points_outside_the_repo` 通过.
- [x] 6.5 谓词与只读/幂等断言强化: `goal_reached` / `no_pending_work` 对 `missing` / `fail` 不计数; 只读夹具预置既有备份; 第二次 `apply` 断言链接 inode 不变. 验证: `cargo test --locked --lib error::tests::special_states_are_excluded_from_the_target_predicate`, `cargo test --locked --test status status_never_touches_the_filesystem`, `cargo test --locked --test apply a_second_apply_is_idempotent_and_produces_no_new_files` 通过.
- [x] 6.6 同步文档表述: proposal 的交付面与引用计数改为实测值, design 的 D2/D6/D7/Risks/Migration Plan 与 tasks 3.2 的改动面描述与实现一致. 验证: `rg -n '生产代码未改动' openspec/changes/migrate-spec-kit-contracts/proposal.md openspec/changes/migrate-spec-kit-contracts/design.md` 零命中, 且 `openspec validate migrate-spec-kit-contracts --strict` 输出 `is valid`.
- [x] 6.7 撤销动手前复核的措辞断言: `a_real_file_that_replaced_the_link_is_refused_and_preserved` 额外断言说明含"判定之后被替换". 验证: `cargo test --locked --lib undo::tests::a_real_file_that_replaced_the_link_is_refused_and_preserved` 通过.

## 7. 验证报告反馈修复 (第二轮)

- [x] 7.1 修正 V-9 漏点: `HOME` 自身是符号链接时, 把主目录写成物理路径会漏过主目录落点检查. `src/manifest.rs` 的 `physical_home` 取解析末段符号链接后的物理形态, V-9 同时比较书写形态与物理形态; `manifest-schema` 的 V-9 补 "主目录路径本身是符号链接" Scenario, 并补一条 "主目录之下的合法落点不受误伤" 的反向用例. 验证: `cargo test --locked --lib manifest::tests::physical_home_resolves_a_symlinked_final_segment` 与 `cargo test --locked --test manifest_errors v9_a_symlinked_home` 通过.
- [x] 7.2 `cli-contract` 的 "各命令的目标谓词" 补特殊状态计数基准与 `preview` 冲突计数的交叉引用, 消除与 `entry-classification` 的字面歧义. 验证: `openspec validate migrate-spec-kit-contracts --strict` 输出 `is valid`.
- [x] 7.3 `link-deployment` 明确预览预测备份名跨秒可以不同 (一致性只约束状态与动作结论), 并把 "无副作用的动作不创建路径" 的场景改写为执行层不变量. 验证: `openspec validate --strict` 通过, 且 `cargo test --locked --lib apply::tests::actions_without_side_effects_touch_nothing` 通过.
- [x] 7.4 `manifest-schema` 的 V-11 补 "目标包含本条目的源" Scenario (实现与端到端用例已覆盖该形态). 验证: `openspec validate --strict` 通过.
- [x] 7.5 强化 `backup-lifecycle` 的两处覆盖: 排序兜底 (同时间戳, 同数值序号, 不同写法) 的直接单测; 失败清理夹具预置既有备份与用户文件并断言原样保留. 验证: `cargo test --locked --lib backup::tests::find_latest_backup_tiebreaks_equal_serials_by_file_name` 与 `cargo test --locked --lib backup::tests::a_copy_failure_cleans_only_its_own_partial_directory` 通过.
- [x] 7.6 在 `design.md` 的 Non-Goals 记录特殊文件与不可读链接文本的边界; proposal / design 的 "唯一行为修正" 改为两处, design 的回滚清单与 tasks 3.2 的改动面补上 `src/manifest.rs`. 验证: `rg -n '唯一的行为修正' openspec/changes/migrate-spec-kit-contracts/proposal.md openspec/changes/migrate-spec-kit-contracts/design.md` 零命中, 且 `openspec validate --strict` 通过.
- [x] 7.7 重新运行全套质量门: `openspec validate migrate-spec-kit-contracts --strict`, `cargo test --locked`, `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings` 全部通过. 验证: 四条命令均成功.

## 8. 验证报告建议落实

- [x] 8.1 补 `manifest-schema` V-7 非字符串取值的 TOML 类型断言: `v7_illegal_on_conflict_reports_index_and_value` 额外断言 `实际为 integer`. 验证: `cargo test --locked --lib manifest::tests::v7_illegal_on_conflict_reports_index_and_value` 通过.
- [x] 8.2 补 V-11 两个 Scenario 的说明断言: "目录目标下的子目标" 与 "目标包含本条目的源" 在拒绝时同时给出条目下标与重叠路径. 验证: `cargo test --locked --test manifest_errors overlapping_source_and_target_paths_are_rejected_before_writes nested_targets_are_rejected_regardless_of_order_or_existing_aliases` 通过.
- [x] 8.3 补 "主目录解析" 的 HOME 未设置/为空文案断言, 并把 "不用 ~ 的清单不受影响" 强化为 apply 退 0 且链接建立. 验证: `cargo test --locked --test manifest_errors` 通过.
- [x] 8.4 补命令面排他性断言: `dotlink help` 与拼错子命令同样以 2 被拒. 验证: `cargo test --locked --test usage_errors help_subcommand_is_rejected` 通过.
- [x] 8.5 补 token 适用面排他断言: `undo` / `restore` / `skip` 只属于撤销命令, 撤销命令不产出部署/检查专属 token. 验证: `cargo test --locked --lib render::tests::token_table_is_covered_cell_by_cell` 通过.
- [x] 8.6 在 `design.md` 的 "验证层级与后续项" 记录平台门禁的结构保证层级与缺少非 macOS/Linux 目标 CI 的缺口.
- [x] 8.7 新增 `tests/consistency.rs`: 同一份七状态清单上四条命令逐条结论一致, 并回收仓库外夹具目录. 验证: `cargo test --locked --test consistency` 通过.
- [x] 8.8 新增 "仅源侧不可读" 端到端用例 (ENOTDIR): 四条命令均为 fail, 只透出源侧原因, 退出码 1 且零改动. 验证: `cargo test --locked --test unreadable a_source_side_only_failure_is_fail_under_every_command` 通过.
- [x] 8.9 `wrong` / `foreign` 行的 "实际指向" 改为 `link_destination` 的规范化绝对目标; 补相对链接文本的单元断言 (`relative_link_text_is_reported_as_its_normalized_destination`) 与端到端断言 (`status_reports_the_normalized_destination_of_a_relative_link`). 验证: 两条测试通过, 且既有 status / render 用例保持通过.
- [x] 8.10 补部署/撤销端到端证据: 备份成功但替换失败时 fail 行给出存在的备份路径且原对象保留; 备份不可读时 undo 报 fail, 恢复权限后可重试. 验证: `cargo test --locked --test conflict a_failed_replacement_reports_the_backup_path_and_keeps_the_original` 与 `cargo test --locked --test undo undo_reports_fail_and_can_be_retried_when_the_backup_is_unreadable` 通过.
- [x] 8.11 补备份生命周期断言: 同目录三目标各自取最新备份; 混合 `.partial` 残留原样保留; 十份备份内容不被消费. 验证: `cargo test --locked --test undo` 通过.
- [x] 8.12 在 `design.md` 记录测试证据层级 (端到端 / 单元 / 代码审阅 / 结构保证) 与不可达分支.
- [x] 8.13 在 `design.md` 记录运行身份要求 (chmod 类用例在 root 下跳过) 与 CI 后续项; 同步 proposal (输出细化之三, 四处生产代码改动), design (D7 例外三, 回滚清单) 与 tasks 3.2 的改动面.
- [x] 8.14 修复测试卫生: `src/apply.rs` 的替换失败单测恢复备份副本内只读子目录权限, 避免 TMPDIR 残留; 新增一致性测试回收仓库外夹具. 验证: 全量测试后 TMPDIR 不再出现 `.tmp*` 备份残留与 `dotlink-consistency-outside-*`.
- [x] 8.15 统一本变更制品与触及文件的非 ASCII 标点: proposal / design / tasks 与 `src/` / `tests/` 内本变更触及的行只保留 ASCII 标点 (AGENTS.md 规则). 验证: 扫描三份制品与 `src/**/*.rs` / `tests/**/*.rs`, 无 CJK 标点或全角字符.
- [x] 8.16 重跑全套质量门: `openspec validate migrate-spec-kit-contracts --strict` 输出 `is valid`; `cargo fmt --check` 通过; `cargo test --locked` 通过 (当次 317 个测试通过, 1 个 ignored 辅助用例); `cargo clippy --locked --all-targets -- -D warnings` 通过.

## 9. 验证报告 WARNING 与 SUGGESTION 落实

- [x] 9.1 修复 W-1: `path-normalization` 的"仓库内的悬空链接" Scenario 补"该文件不是本条目的 `source`"限定, 并在 requirement 中交叉引用 `entry-classification` 的求值顺序. 验证: `openspec validate migrate-spec-kit-contracts --strict` 输出 `is valid`.
- [x] 9.2 修复 W-2: `README.md` 第 5 行与 `src/render.rs` 的 `undo_token` 注释按 `link-teardown` 的"恢复出的内容本身是仓库内的链接" Scenario 改写, 并同步 proposal / design / 本文件 3.2 的改动面描述. 验证: `rg -n '报 `skip` 且同样不改动文件系统' README.md` 零命中, `cargo test --locked` 通过.
- [x] 9.3 落实 S-1 / S-2: `entry-classification` 的"源缺失先于目标侧"补"两侧均无环境故障时", "目标谓词对特殊状态的计数基准"限定为部署 / 预览 / 检查的谓词并注明撤销的相反谓词. 验证: `openspec validate migrate-spec-kit-contracts --strict` 输出 `is valid`.
- [x] 9.4 落实 S-3 / S-4 / S-8: `link-teardown` 的备份保留要求改为交叉引用 `backup-lifecycle`, Purpose 补快照与目录失败约束的归属, 补恢复侧暂存名命名规则, 并把"暂存名被占用" Scenario 改名为"撤销暂存名被占用"; `manifest-schema` 的清单选项声明改为交叉引用 `cli-contract`. 验证: `openspec validate migrate-spec-kit-contracts --strict` 输出 `is valid`.
- [x] 9.5 落实 S-5: 补齐测试证据 -- 不依赖权限位的双侧故障, apply 侧 conflict 原因, missing 原因, 四列各 1 的汇总, 清单不存在单行标准错误, 缺失尾段不解析别名, on_conflict 大小写混写, V-10 归一化自指; 另把 `status --json` 纳入用法错误先于清单读取的断言. 验证: `cargo test --locked` 全部通过.
- [x] 9.6 落实 S-6: 把 `tests/manifest_errors.rs` 的四个 `v4_` 前缀用例改为 `v6_`, 更正 `src/entry.rs` 陈旧的函数名引用, 为 V-2 的"位置未知"回退补注释. 验证: `rg -n 'v4_a_source|v4_parent_segments|link_is_inside_repo' tests src` 零命中.
- [x] 9.7 落实 S-7: 注明 `Error::EntryFailed` 的 `exit_code` 映射不参与顶层进程退出码. 验证: `cargo clippy --locked --all-targets -- -D warnings` 通过.
- [x] 9.8 落实 S-9: design 的 Migration Plan 与本文件的 Workflow follow-up 记录归档说明需要写明质量门运行身份 (非 root) 与未跟踪 `AGENTS.md` 的归属. 验证: `design.md` 与 `tasks.md` 含该说明.
- [x] 9.9 重跑全套质量门: `openspec validate migrate-spec-kit-contracts --strict`, `cargo test --locked`, `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings` 全部通过.

## 10. 验证报告 WARNING 与 SUGGESTION 落实 (第二轮)

- [x] 10.1 修复 W-1: `cli-contract` 的"预览与执行共用 token" Scenario 与 `link-deployment` 的"预览与执行使用同一判定" requirement 限定为"动作执行未失败时", 并注明执行期失败由 `fail` 的适用面表达; `tests/apply.rs` 的只读父目录用例先跑 `preview` 断言 `create`, 再跑 `apply` 断言 `fail`, 锁定该例外. 验证: `cargo test --locked --test apply a_link_creation_failure_does_not_block_the_other_entries` 通过.
- [x] 10.2 修复 W-2: `src/cli.rs` 的 `undo` `long_about` 按 `link-teardown` 的"恢复出的内容本身是仓库内的链接" Scenario 改写; `tests/usage_errors.rs` 增加"仓库内链接"与 `restore` 措辞断言. 验证: `cargo test --locked --test usage_errors undo_help_mentions_the_restore_again_case` 通过.
- [x] 10.3 落实 S-1 / S-2: `manifest-schema` 的 V-10 说明改为"符号链接环"语义, V-2 补"位置未知"防御分支的限定. 验证: `openspec validate migrate-spec-kit-contracts --strict` 输出 `is valid`.
- [x] 10.4 落实 S-3 / S-4: `backup-lifecycle` 的"备份名形态" Scenario 区分有无消歧序号, Purpose 的列举补"保留". 验证: `openspec validate migrate-spec-kit-contracts --strict` 输出 `is valid`.
- [x] 10.5 落实 S-5 / S-6: `cli-contract` 的"撤销后目标为空" Scenario 改为报告 `ok` 并计入"已是最新"列; 必现信息限定为 status 的 `wrong` / `foreign` 行, `replace` 行只要求"冲突占用"粒度. 验证: `openspec validate migrate-spec-kit-contracts --strict` 输出 `is valid`.
- [x] 10.6 落实 S-7: `tests/usage_errors.rs` 的标准输入用例保留标准错误并断言为空; 未知选项用例补 `--manifest` / `--repo` / `--home`; 新增环境变量不影响清单来源与主目录的用例. 验证: `cargo test --locked --test usage_errors` 通过.
- [x] 10.7 落实 S-8 / S-9: `link-teardown` 的"动手前复核" requirement 写明目标被换成另一个符号链接时复核通过; `design.md` 的验证层级把"撤销只读取状态"记为代码审阅证据. 验证: `openspec validate migrate-spec-kit-contracts --strict` 输出 `is valid`.
- [x] 10.8 落实 S-10: 本文件 8.16 的测试计数标注为当次快照.
- [x] 10.9 重跑全套质量门: `openspec validate migrate-spec-kit-contracts --strict` 输出 `is valid`; `cargo test --locked` 通过 (324 个测试通过, 1 个 ignored 辅助用例); `cargo fmt --check` 通过; `cargo clippy --locked --all-targets -- -D warnings` 通过.

## Workflow follow-up

- 评审通过后归档本变更, 使七个 capability 落入 `openspec/specs/`.
- 归档说明记录质量门的运行身份 (非 root) 与未跟踪的工作区文件 `AGENTS.md` 的归属, 避免归档时误纳入.
- 归档后逐份复核主 spec: `openspec show <capability> --type spec` 确认 Purpose 不是 `TBD`, Scenario 未被丢弃, 编号后缀完整.
- 起后续变更处理 README 两处与 68 处文档名引用, 198 处编号引用, 84 处 `T-nnn` 引用. 该变更的验收包含"`rg` 对已删文档名零命中"与"`cargo test --locked` / `cargo clippy -D warnings` 通过".
- 评估 `undo` 在判定后目标被换成他人符号链接时的归属复核 (见 `design.md` 的 Open Questions); 如需收紧, 作为独立变更处理.

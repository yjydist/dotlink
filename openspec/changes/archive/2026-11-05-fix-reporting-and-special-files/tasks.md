# Tasks

## 1. 特殊文件状态与动作

- [x] 1.1 在 `src/classify.rs` 增加 `Current::ForeignOther`, 把非符号链接分支改为目录 / 普通文件 / 其余三路, 并用 `std::os::unix::fs::FileTypeExt` 生成 `FIFO`, `socket`, `字符设备`, `块设备`, `其他特殊文件` 标签; 非 Unix 目标保留 fallback. 增加单测: 用 `mkfifo` 与 `std::os::unix::net::UnixListener::bind` 各造一个目标, 断言状态与 detail, 且 `skip` 与 `replace_and_backup` 都导出 `RefuseConflict`; 运行 `cargo test --locked classify` 验证.
- [x] 1.2 更新 `action_for`, `undo::undo_entry` 与 `render` 的 `deploy_token` / `status_token` / `undo_token` / `occupied` / `note_for_token`: 特殊文件在 `status` 为 `foreign`, 在 `preview` 与 `apply` 为 `conflict`, 在 `undo` 为 `skip`; 说明包含精确类型与"特殊文件不支持备份, 不会覆盖". 增加单测断言 12 个 token 未增加, 且特殊文件的 token 不受 `on_conflict` 影响; 运行 `cargo test --locked render` 与 `cargo test --locked undo` 验证.
- [x] 1.3 在 `tests/status.rs`, `tests/preview.rs`, `tests/apply.rs`, `tests/undo.rs` 增加端到端用例: 目标是 FIFO 与 socket 时, `status` 报 `foreign` 且含类型, `preview` 报 `conflict` 且不承诺备份, `apply` 报 `conflict` 且目标 inode 不变, `undo` 报 `skip` 且不触碰; 运行 `cargo test --locked --test status --test preview --test apply --test undo` 验证.
- [x] 1.4 在 `README.md` 补充特殊文件目标的语义: `status` 报 `foreign` 并给类型, `preview` / `apply` 报 `conflict`, `undo` 跳过; 人工对照 `tests/status.rs` 与 `tests/apply.rs` 的断言, 确认措辞与实际输出一致.

## 2. 目录内特殊文件与备份失败

- [x] 2.1 在 `src/backup.rs` 的特殊文件错误中给出完整路径与"请先手工移走该特殊文件再重试"; 增加单测: 目录含普通文件与 FIFO, 以及目录含 socket, 断言 `copy_object` 失败且错误含该子项路径与提示, 暂存对象被清理, 最终备份名不出现, 原目录与特殊文件保持原样; 运行 `cargo test --locked backup` 验证.
- [x] 2.2 在 `tests/apply.rs` 与 `tests/conflict.rs` 增加端到端用例: 目标目录内含 socket, 条目 `replace_and_backup`, `preview` 仍按当前分类预测 `replace` (不递归预检), `apply` 报 `fail`, 说明含 socket 路径与"先手工移走", 不建立链接, 目录与 socket 不变; 运行 `cargo test --locked --test apply --test conflict` 验证.
- [x] 2.3 在 `README.md` 的已知限制补充: 目录内含特殊文件时备份整体失败, `preview` 不递归预检, 需先手工移走再重试; 人工核对错误文本与 `2.1` 的单测一致, 且没有把该失败描述成可自动跳过.

## 3. 报告列对齐

- [x] 3.1 在 `src/render.rs` 把序号改为按总数位数右对齐, 增加只依赖字符区间的 `display_width` 并按显示宽度补齐 target; 辅助函数注释写明近似之处. 增加单测: 12 条的行 token 列一致, `~/.abc` 与 `~/.配置` 的说明列都从同一显示列开始, 组合记号与长 target 不 panic; 运行 `cargo test --locked render` 验证.
- [x] 3.2 在 `tests/output.rs` 增加端到端用例: 生成 12 条含 ASCII 与 CJK target 的清单, 断言每行 token 起始显示列相同, 说明列在显示宽度 20 处开始, 汇总行仍是最后一行; 运行 `cargo test --locked --test output` 验证.
- [x] 3.3 在 `README.md` 的报告示例附近补一句序号按位数右对齐, target 按显示宽度补齐, CJK 按 2 列, 超长 target 不截断但会使说明列右移; 人工按 README 示例跑一次 `preview`, 确认示例与实际输出一致.

## 4. 实际指向保真与失败侧路径

- [x] 4.1 把 `src/render.rs` 的 `non_empty` 改成只用 `detail.trim().is_empty()` 判空, 非空时返回原始 detail; 增加单测: `wrong`, `foreign`, `skip`, `fail` 的 detail 含首尾空格时, 说明保留这些空格; 运行 `cargo test --locked render` 验证.
- [x] 4.2 在 `tests/status.rs` 增加端到端用例: 目标链接指向一个名称尾部含空格的路径, `status` 的 `wrong` 或 `foreign` 说明保留该空格, 不显示 trim 后的版本; 运行 `cargo test --locked --test status` 验证.
- [x] 4.3 在 `src/classify.rs` 的 `Unreadable` detail 中写入失败侧与对应路径 (源侧给 source, 目标侧给 target, 双侧都给, 读链接失败给 target), 并让 `src/render.rs` 的 `unreadable_detail` 不再拼接 `条目 target`; 更新 `tests/unreadable.rs` 的源侧用例断言输出含源路径, 增加双侧与目标侧用例; 运行 `cargo test --locked --test unreadable` 与 `cargo test --locked classify` 验证.
- [x] 4.4 在 `README.md` 的报告说明中补充失败原因会给出失败侧路径, 实际指向按解析结果原样显示; 人工对照 `tests/unreadable.rs` 的一条源侧失败输出, 确认只出现源路径而不是无关的目标路径.

## 5. 退出码 2 判据与 help 缩进

- [x] 5.1 把 `src/cli.rs` 的根 `long_about` 与 `long_about!` 改为 `concat!` 逐行字面量, 恢复退出码与 if/code/fi 的缩进, 并把判据改成"`error:` 与 `Usage:` 的用法说明, 或单行 `dotlink: ...`"; 修正模块注释说明 `-h` 只显示短描述. 在 `tests/usage_errors.rs` 增加缩进断言 (`  0`, `  if`, `    code`, `  fi`) 与判据断言; 运行 `cargo test --locked --test usage_errors` 验证.
- [x] 5.2 在 `tests/usage_errors.rs` 把未知选项与拼错子命令的断言改为同时检查 `error:` 与 `Usage:`, 保留"先于清单读取"的断言, 并断言 `dotlink status --json` 不读取清单; 运行 `cargo test --locked --test usage_errors` 验证.
- [x] 5.3 更新 `README.md` 的退出码 2 段落: 判据改为 `error:` 加 `Usage:` 或单行 `dotlink:`, 增加 `dotlink status --json` 的 `unexpected argument` 示例, 明确未知选项与拼错子命令都属调用方式错误; 在临时目录实际运行一次 `dotlink status --json` 与 `dotlink statu`, 对照 README 示例与 `--help` 文本.

## 6. 控制字符转义扩展

- [x] 6.1 在 `src/render.rs` 的 `escape_controls` 增加 U+2028, U+2029, U+200B-200F, U+202A-202E, U+2060-2064, U+2066-2069, U+FEFF 的判定, 命中后继续用 `escape_default`; 增加单测断言这些字符显示为 `\u{2028}`, `\u{2029}`, `\u{202e}`, `\u{200b}`, `\u{feff}`, 普通中文与反斜杠不被二次转义; 运行 `cargo test --locked render` 验证.
- [x] 6.2 在 `tests/output.rs` 或 `tests/preview.rs` 增加端到端用例: target 含 U+2028 与 U+202E, 输出不含原始字符, 每条目仍只占一行, 汇总行仍是最后一行; 运行 `cargo test --locked --test output` 验证.
- [x] 6.3 更新 `README.md` 的控制字符说明与已知限制: 列出新增转义集合, 说明其他格式字符 (例如 U+00AD, U+061C 与标签字符) 仍按原样显示; 人工用含 U+2028 的 target 跑一次 `preview`, 确认输出为字面 `\u{2028}`.

## 7. 文案统一与 README 边界

- [x] 7.1 统一 `src/render.rs` 的 `replace` 与 `conflict` 括号语义: 外部占用物一律描述占用物 (普通文件 / 目录 / 特殊文件: FIFO), `LinkedWrong` 保留"本工具的链接指向错误 (实际指向: ...)", 不再在 `replace` 括号里放 `entry.source`; 更新受影响的 render 单测与 `tests/preview.rs` / `tests/apply.rs` 断言. 运行 `cargo test --locked render` 与 `cargo test --locked --test preview --test apply` 验证.
- [x] 7.2 在 `README.md` 的已知限制补充四条: 非 UTF-8 路径的 lossy 显示, `undo` 不清理 `apply` 创建的父目录, 备份与暂存命名空间由本工具保留, 备份开始后的并发原地修改不会进入该备份; 人工核对只改这些条目, 不触碰并行变更会修改的其它 README 小节, 并用 `git diff -- README.md` 复核.

## 8. 集成验证

- [x] 8.1 运行 `./scripts/check.sh`, 确认 fmt, test, clippy, rustdoc 与 release build 全部通过.
- [x] 8.2 执行手工验收矩阵并记录输出: FIFO 目标的 `status` / `preview` / `apply` / `undo`; 含 socket 目录的 `preview` 与 `apply`; 12 条含 CJK target 的报告列; `dotlink status --json` 的退出码 2; target 含 U+2028 的转义; 确认特殊文件未被删除或覆盖.
- [x] 8.3 运行 `openspec validate "fix-reporting-and-special-files" --strict`, 并检查 `entry-classification`, `cli-contract`, `link-deployment` 与 `backup-fidelity` 四份 delta 的标题与主规格逐字匹配.

## Workflow follow-up

- 实现完成并通过评审后归档本变更; 归档前复核四份 delta 的合并结果.
- 归档时确认 `link-deployment` 只新增"不可备份的占用物不建立链接", 不覆盖并行变更正在修改的"本工具的错误链接在部署时自愈"与"替换删除前必须原子复核身份".
- 归档时确认 `backup-fidelity` 只新增"特殊文件不可复制且目录备份整体失败", 不覆盖并行变更正在修改的"备份的发布仍以完整复制为前提".
- 归档时把 `openspec/specs/entry-classification/spec.md` 的 Purpose 中"八种"同步为"九种"; 主规格 Purpose 不在本变更目录的编辑范围.
- 归档后检查 README 的特殊文件, 退出码 2, 转义与已知限制段落仍与主规格一致.

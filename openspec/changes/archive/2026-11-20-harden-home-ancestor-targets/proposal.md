# Proposal

## Why

V-9 只拒绝主目录本身, 不拒绝主目录的祖先. 当仓库不在 `HOME` 之下时, `target = "~/.."` (以及等价的绝对路径形式) 会被清单接受; `preview` 预测 `replace` 并给出主目录父目录的备份路径, 在 `replace_and_backup` 下会先备份再删除整棵包含主目录的树. 仓库位于 `HOME` 之下时, V-9 的仓库祖先检查会偶然挡住这类落点, 但那是仓库位置的副作用, 不是契约保证.

复现证据 (只运行只读的 `preview`, 未执行 `apply`): 临时目录中令 `HOME=/tmp/dotlink-evidence.X/home`, 仓库为 `/tmp/dotlink-evidence.X-other/repo` (不在 `HOME` 之下), 清单含一条 `target = "~/.."` 且 `on_conflict = "replace_and_backup"`:

```text
$ dotlink preview
1/1 replace   ~/..                冲突占用 (目录) -> 将备份到 /tmp/dotlink-evidence.X.dotlink-backup.20261120T060408 并建立链接
1 个条目: 1 成功, 0 已是最新, 0 未处理, 0 失败
exit=1
```

同一环境下 `target = "~//../x"` 也被接受 (预测 `create` 或 `replace`), 绝对形式的 `HOME` 父目录同样被接受. 这些落点一旦落地, 毁掉的是包含主目录的整个祖先目录.

## What Changes

- **BREAKING**: 主目录祖先的 `target` 从接受改为整体拒绝 (退出码 2, 零改动). 旧版接受的 `~/..`, `~//../x` 与绝对路径形式的 `HOME` 父目录现在都会退回 2.
- 新增 V-9c: 规范化后是 `HOME` 祖先 (同时覆盖 `HOME` 的书写形态与解析末段符号链接后的物理形态) 时整体拒绝; `HOME` 不可用时该比较不适用, 清单不得因此被拒.
- 以 `~` 简写书写且**按路径组件词法折叠后**经 `..` 离开主目录的落点 (如 `~//../x`) 一并拒绝: `~` 形态只表达主目录及其之下的词法路径. 该判定不解析祖先符号链接, 因此 `~/symlinked-dir/file` 这类有意指向主目录外部的符号链接子目录仍按既有规则处理.
- 落点存在时按文件系统身份判等, 覆盖祖先目录的大小写别名与其它指向同一对象的写法; 取不到身份时退回规范化字符串比较, 不因此拒绝清单.
- 错误说明指出条目下标与书写形态.
- 更新 `src/manifest.rs` 的内联单测, `tests/manifest_errors.rs` 端到端矩阵与 README 的 V-9 段落.

## Capabilities

### New Capabilities

无.

### Modified Capabilities

- `manifest-schema`: V-9 的受保护落点从"主目录本身"扩展为"主目录本身及其祖先" (新增 V-9c), 并明确 `~` 简写不得经 `..` 离开主目录.

## Impact

- 代码: `src/manifest.rs` 的 `reject_degenerate_target` 与 `BaselineIdentities::collect`.
- 测试: `src/manifest.rs` 单测, `tests/manifest_errors.rs` 的 V-9 端到端矩阵 (需使用仓库位于 `HOME` 之外的夹具).
- 文档: `README.md` 的 V-9 段落与兼容性说明.
- 规格: `openspec/specs/manifest-schema/spec.md` 在归档后新增 V-9c.
- 依赖与运行状态: 无新增依赖, 不引入状态文件; 变化只影响清单校验.
- 回滚: 撤销新增校验即可恢复旧版接受面, 不涉及数据迁移.

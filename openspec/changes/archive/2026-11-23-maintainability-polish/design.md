# Design

## Context

当前 `dotlink` 不写状态文件, 同步执行, 平台层依赖 `rustix`. 上一轮探索留下 6 个独立维护项; 本设计把它们放在一个 change 内, 但按任务组隔离, 让 apply 可以在高风险组之前停下并汇报. 参考资料: `openspec/changes/archive/2026-11-22-reject-cross-mount-targets/` 与 `openspec/specs/` 的现有要求.

## Goals / Non-Goals

**Goals:**

- 让同一清单上的并发 `apply` / `undo` 串行化, 只读命令使用共享锁.
- 让递归复制与删除不跟随遍历期间被替换的目录项.
- 去掉 `--help` 的 markdown 强调符, 拆分过长的 `manifest::parse`, 移除只为时间戳存在的 `time` 依赖.
- 在清单校验阶段使用运行期 `f_namemax` 收紧生成名预算.
- 保持 token, 退出码, 备份命名与现有失败语义.

**Non-Goals:**

- 不引入异步运行时或后台锁服务.
- 不新建状态文件或锁文件.
- 不用 `unsafe`; 不引入 `libc` 或新的 Unicode 属性依赖.
- 不实现运行期 PATH_MAX 探测; 没有可移植的只读接口.
- 不改变 `preview` 的递归预检范围.

## Decisions

### D1. 在清单文件描述符上使用 `flock`

`manifest::load` 改为先打开 `dotlink.toml`, 取得锁后再读取内容. `apply` / `undo` 取排他锁, `preview` / `status` 取共享锁. 锁 guard 由 `run_on` 持有到进程结束; 锁不创建新文件, 符合"不写状态文件". `flock` 返回 `ENOTSUP`, `EOPNOTSUPP`, `ENOLCK` 或 `ENOSYS` 时降级为不加锁并继续; 其他错误按清单读取失败处理 (退出码 2). 备选方案是锁一个专用锁文件; 否决理由是那会新增状态文件, 违反 `cli-contract` 的持久化状态约束.

### D2. 递归遍历改用 fd 相对原语

使用 `rustix::fs` 的 `openat`, `fstatat`, `readlinkat`, `mkdirat`, `unlinkat`, `renameat`, `fchmod`, `futimens`, `flistxattr` / `fgetxattr` / `fsetxattr`. 目录以 `O_DIRECTORY | O_NOFOLLOW` 打开; 子项以 `fstatat(..., SYMLINK_NOFOLLOW)` 判定; 若判定为目录后 `openat` 返回 `ELOOP` 或 `ENOTDIR`, 视为遍历期间被替换, 使该条目失败, 不跟随目标. 删除使用 `unlinkat` 的 `REMOVEDIR` 标志; 符号链接只删除链接本身. 备选方案是保留路径式遍历并只加文档; 否决理由是无法关闭"目录项被替换为符号链接后递归跟随"的窗口.

### D3. 帮助文本去 markdown

`src/cli.rs` 中 `**且**` 与 `动手**之前**` 等改成不加标记的纯文本. 不改变退出码段落, 子命令集合或 `-h` / `-V` 行为.

### D4. 拆分 `manifest::parse`

按现有校验顺序抽取 `parse_entries`, `validate_entry_paths`, `register_target` 等私有函数, 保留错误文本, 报错优先级与 V-5 到 V-13 的顺序. 这是纯重构, 不新增规格.

### D5. 移除 `time` 依赖

用 `SystemTime::now().duration_since(UNIX_EPOCH)` 取得 Unix 秒, 用 civil-from-days 算法格式化为 `YYYYMMDDThhmmss`; `is_timestamp` 用同一套闰年与月天数规则. 测试改用固定的 Unix 秒值, 不再构造 `OffsetDateTime`. 备选方案是保留 `time`; 否决理由是它只为时间戳引入 4 个传递依赖, 而所需行为很小.

### D6. 运行期 NAME_MAX

在 V-13 之后新增 V-14: 对每个 target, 从 `target_path` 向上找最近的已存在祖先; 若该祖先可读且 `statvfs` 成功, 取 `f_namemax`; 当 `target_basename_bytes + MAX_GENERATED_SUFFIX_BYTES > f_namemax` 时整体拒绝, 说明给出条目下标, 实际值, 预算与文件系统上限. 祖先不存在或 `statvfs` 失败时跳过运行期检查, 仍使用静态预算. PATH_MAX 继续使用平台常量. 备选方案是移到运行期失败; 否决理由是那会让清单先被接受, 备份或暂存时才报 `ENAMETOOLONG`.

## Risks / Trade-offs

- [flock 在 NFS 等文件系统不可靠] -> 不支持时降级并记录; 本变更只承诺本地文件系统上的串行化.
- [锁住清单后长时间阻塞] -> 锁覆盖整个命令执行; 这是串行化的代价, 与"不提供超时选项"一致.
- [fd 相对重写引入回归] -> 按 `remove_tree` / `find_mount_boundary` / `copy_object` / 元数据分批, 每组先跑现有挂载与备份测试.
- [macOS 编译分支无法在当前 Linux 环境运行] -> 用 `cargo check` 与代码审阅为证据, 在 README 记录平台边界.
- [运行期 NAME_MAX 误拒] -> 只在 `statvfs` 成功且值大于 0 时生效; 最近祖先与最终父目录不同文件系统时可能偏保守, 在 README 记录.
- [时间算法错误] -> 以现有合法/非法时间戳测试为回归基线, 增加闰日与月末用例.

## Migration Plan

无数据迁移. 唯一兼容性收紧是 V-14: 在 `f_namemax` 小于 250 的文件系统上, 过去接受的清单现在退 2. 回滚只需还原代码与 `Cargo.lock`; 备份与链接不受影响.

## Open Questions

无.

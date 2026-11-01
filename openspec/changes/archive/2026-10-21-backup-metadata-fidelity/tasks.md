# Tasks

## 1. 平台元数据原语

- [x] 1.1 在 `src/os` 增加元数据模块: macOS/Linux 使用 `rustix::fs` 实现扩展属性复制与 mtime 设置, 其他平台提供返回 `Unsupported` 的 stub; 验证 `cargo check --locked` 与 `cargo check --locked --target x86_64-pc-windows-gnu` 均通过.
- [x] 1.2 实现扩展属性复制: 列出源属性, 逐属性读取并以相同名称与值写入目标, 缓冲区不足 (`ERANGE`) 时扩容重试; 验证单元测试能复制一个 8 KiB 的用户扩展属性.
- [x] 1.3 实现逐属性错误分类: `ENOTSUP` / `EOPNOTSUPP` / `ENOSYS` / `EPERM` / `EACCES` / `ENODATA` 归为可跳过, 其他错误归为致命; 验证纯分类函数的单元测试覆盖这两类.
- [x] 1.4 实现 mtime 设置: 使用 `utimensat`, atime 传 `UTIME_OMIT`, mtime 取源对象的秒与纳秒; 验证单元测试中设置并读回一个已知时间戳.

## 2. 备份与恢复复制路径

- [x] 2.1 在 `src/backup.rs` 的 `copy_object` 中为普通文件与目录应用元数据, 顺序固定为扩展属性 -> 权限位 -> mtime; 符号链接只复制链接文本, 不进入元数据路径; 验证 `src/backup.rs` 中文件与目录的既有备份测试仍通过.
- [x] 2.2 确认目录元数据在全部子项复制完成后应用, 且目录 mtime 最后设置; 验证新增单元测试: 含子项的目录备份后 mtime 等于原目录 mtime.
- [x] 2.3 让 `restore_backup` 通过同一 `copy_object` 路径恢复元数据; 验证新增单元测试: 备份 -> 恢复后目标 mtime 与用户扩展属性等于原对象.
- [x] 2.4 覆盖旧版本备份: 手工创建一份 mtime 为已知历史值的备份文件并执行恢复, 验证目标 mtime 等于该备份的 mtime, 而不是恢复时刻.
- [x] 2.5 验证失败语义未被破坏: 元数据阶段发生致命错误时不发布最终备份名且原对象保留; 复用并扩展 `src/backup.rs` 的失败路径测试, 断言只可能留下 `.partial` 残留.

## 3. 端到端与文档

- [x] 3.1 新增 `tests/metadata.rs`: 在临时仓库中给目标文件设置 mtime 与用户扩展属性, 执行 `apply` 后检查备份, 再执行 `undo` 后检查恢复结果; 扩展属性不受支持时跳过该断言并说明原因.
- [x] 3.2 更新 `README.md`: 备份说明写明保留内容, 权限, mtime 与可设置的扩展属性, 并说明尽力而为策略, 不保留所有权/ACL, 旧备份按自身元数据恢复.
- [x] 3.3 更新 `specs/001-dotfile-link-management/data-model.md` 第 4 节的备份内容表, 使其与新的保真语义一致; 验证文档中的描述不再暗示备份只含内容与权限.

## 4. 全量验证

- [x] 4.1 在 macOS 上手工执行一次真实场景: 给文件设置旧 mtime 与 `user.test` 扩展属性, 运行 `apply` 后对备份执行 `stat` 与 `xattr -l`, 再运行 `undo` 后检查恢复结果; 保存输出作为验证证据.
- [x] 4.2 运行 `./scripts/check.sh`, 验证格式检查, 全套测试, Clippy (警告视为错误), 严格 rustdoc 与 release 构建全部通过.

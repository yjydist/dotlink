# Spec Delta

## ADDED Requirements

### Requirement: 同一清单上的并发调用串行化

同一清单上的 `apply` 与 `undo` MUST 取得排他锁, `preview` 与 `status` MUST 取得共享锁; 锁 MUST 加在 `dotlink.toml` 的文件描述符上, MUST NOT 新建状态文件或锁文件. 平台不支持 `flock` 时 MUST 降级为不加锁并继续, 其他锁错误 MUST 按清单读取失败处理 (退出码 2). 锁 MUST 在整条命令执行期间持有.

#### Scenario: 两个 apply 串行执行

- **WHEN** 两个 `dotlink apply` 在同一清单上同时启动
- **THEN** 两者按取得的排他锁先后执行, 后启动者等待前者结束, 不出现交错的部分替换或重复备份

#### Scenario: 只读命令共享锁

- **WHEN** 两个 `dotlink preview` 在同一清单上同时启动
- **THEN** 两者可以并行持有共享锁, 不互相阻塞

#### Scenario: 不支持 flock 的文件系统

- **WHEN** 清单所在文件系统对 `flock` 返回 `ENOTSUP`, `EOPNOTSUPP`, `ENOLCK` 或 `ENOSYS`
- **THEN** 命令降级为不加锁并继续执行, 退出码与逐条结果不受影响

### Requirement: 帮助文本是纯文本

`--help` 输出 MUST NOT 包含 markdown 强调符 `**`. 既有退出码语义, `set -e` 脚本示例与目标谓词说明 MUST 保持可读.

#### Scenario: 子命令帮助不含强调符

- **WHEN** 执行 `dotlink preview --help`
- **THEN** 输出不包含 `**`, 仍包含退出码 0 / 1 / 2 / 3 的说明与脚本示例

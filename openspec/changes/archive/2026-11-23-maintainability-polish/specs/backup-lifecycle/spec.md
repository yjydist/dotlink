# Spec Delta

## ADDED Requirements

### Requirement: 并发调用不产生重复备份

同一清单上的 `apply` 与 `undo` MUST 串行执行, 因此同一目标在同一时刻最多只有一个命令在写入备份或暂存名. 平台不支持 `flock` 时, 该保证降级为既有并发已知限制; 报告 MUST 仍按逐条结果给出, MUST NOT 因并发产生未报告的备份或覆盖.

#### Scenario: 串行写入不重复备份

- **WHEN** 两个 `apply` 在同一清单上竞争替换同一个目标
- **THEN** 两个命令串行执行, 不会同时为同一目标发布最终备份名

#### Scenario: 降级时仍如实报告

- **WHEN** 平台不支持 `flock`, 两个命令并发写入同一目标
- **THEN** 行为退回既有并发边界, 每个条目仍报告自己的 token 与原因, 不静默覆盖

### Requirement: 运行期 NAME_MAX 与静态预算共同约束

生成名长度 MUST 同时满足静态预算与目标最近已存在祖先的 `f_namemax`. 静态预算仍是常见 255 字节文件系统上的保守上限; 运行期检查只在实际探测成功且值大于 0 时收紧. 探测失败 MUST NOT 改变静态预算的接受面.

#### Scenario: 运行期上限更严格时拒绝

- **WHEN** 最近祖先的 `f_namemax` 小于静态预算加生成名后缀
- **THEN** 清单在解析阶段整体拒绝, 不推迟到备份或暂存时返回 `ENAMETOOLONG`

#### Scenario: 运行期上限不可用

- **WHEN** 无法取得最近祖先或 `statvfs` 失败
- **THEN** 仅按静态预算判定, 清单接受面不变

# Spec Delta

## ADDED Requirements

### Requirement: 清单校验 V-14: 运行期 NAME_MAX 预算

清单校验 MUST 在静态 basename 与完整路径预算之后, 对每个 `target` 从解析后的路径向上寻找最近的已存在祖先. 若该祖先可读且 `statvfs` 成功, 系统 MUST 读取 `f_namemax`; 当 `target` 末段字节数加最长生成名后缀超过 `f_namemax` 时 MUST 整体拒绝, 说明给出条目下标, 实际字节数, 生成名预算与文件系统上限. 找不到已存在祖先或 `statvfs` 失败时 MUST 跳过该检查, 仍使用静态预算; MUST NOT 因此拒绝清单. PATH_MAX 继续使用平台常量, 本要求不增加运行期 PATH_MAX 探测.

#### Scenario: 常见文件系统接受 200 字节末段

- **WHEN** `f_namemax` 为 255, `target` 末段为 200 字节, 最长生成名后缀为 50 字节
- **THEN** 清单被接受, 四条命令可继续

#### Scenario: 较小 NAME_MAX 在清单层拒绝

- **WHEN** 最近已存在祖先的 `f_namemax` 为 143, `target` 末段为 200 字节
- **THEN** 整体拒绝, 退出码 2, 说明包含条目下标与 143, 不创建任何链接或备份

#### Scenario: 探测失败时不误拒

- **WHEN** 最近的已存在祖先不可读, 或 `statvfs` 调用失败
- **THEN** 跳过运行期 NAME_MAX 检查, 清单仍按静态预算判定

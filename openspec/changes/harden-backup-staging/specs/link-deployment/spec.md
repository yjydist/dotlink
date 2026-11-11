# Spec Delta

## ADDED Requirements

### Requirement: 替换暂存名有界且形状一致

`apply` 替换路径的暂存名 MUST 以原子不覆盖的 rename 取得; 候选被占用时 MUST 追加递增序号重试, 但 MUST 有明确上限 (最多 16 个候选名). 达到上限时该条目 MUST 失败, 说明 MUST 给出已尝试次数与最后一个候选名, MUST NOT 继续尝试. 消歧序号 MUST 位于 `.partial` 之前, 形如 `<原名>.<pid>.<序号>.dotlink-replace.partial`; 首次候选不带序号. 暂存名只用于写入, MUST NOT 参与任何备份查找.

#### Scenario: 替换暂存名首次即可取得

- **WHEN** 目标同目录没有同 pid 的替换暂存名
- **THEN** 使用 `<原名>.<pid>.dotlink-replace.partial`, 序号不出现

#### Scenario: 替换暂存名冲突时换序号

- **WHEN** 首次候选已被用户文件占用
- **THEN** 使用 `<原名>.<pid>.2.dotlink-replace.partial`, 用户文件保持不变

#### Scenario: 替换暂存名连续被占用

- **WHEN** 从第 1 个候选起的 16 个替换暂存名都已被占用
- **THEN** 该条目失败, 说明包含 16 与最后一个候选名, 不创建第 17 个候选

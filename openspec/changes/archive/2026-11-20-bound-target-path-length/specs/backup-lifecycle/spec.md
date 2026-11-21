# Spec Delta

## ADDED Requirements

### Requirement: 生成名长后缀计入清单路径预算

备份最终名, 备份暂存名, 恢复暂存名与替换/撤销暂存名都在原路径后追加固定长后缀; 这些后缀 MUST 计入 `manifest-schema` V-13 的完整路径预算. 清单接受的路径 MUST 在追加最长生成后缀后仍不超过平台路径预算; 超限时 MUST 在清单解析阶段整体拒绝 (退出码 2, 零改动), MUST NOT 推迟到备份或暂存时由文件系统返回 `ENAMETOOLONG`. 本要求只说明预算归属与拒绝时机, 不改变任何生成名的形态, 查找或发布语义.

#### Scenario: 超长路径在清单层被拒

- **WHEN** 某条目的 `target` 规范化后加上最长生成后缀超过平台路径预算
- **THEN** 四条命令都在任何文件系统改动之前退出 2, 不产生备份或暂存名, 也不会出现运行期 `ENAMETOOLONG`

#### Scenario: 预算内的生成名正常发布与清理

- **WHEN** `target` 加上最长生成后缀恰好等于平台路径预算
- **THEN** `apply` 能生成备份最终名与暂存名, `undo` 能生成恢复暂存名并成功移除链接, 全程不返回 `ENAMETOOLONG`

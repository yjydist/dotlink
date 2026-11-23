# Proposal

## Why

`README.md` 的"退出码 2 的两重含义"一节把"主目录无法确定"写成清单校验失败的无条件原因, 但规格与实现只在需要展开 `~` 时要求 `HOME`. 这不影响程序行为, 但会让读者误以为不含 `~` 的清单也会因 `HOME` 缺失而失败.

## What Changes

- 把 `README.md:222` 的"清单校验失败 (含清单不存在, 当前工作目录不可用, 主目录无法确定)"改为明确限定: 只有需要展开 `~` 时主目录不可用才是校验失败.
- 在同一段补一句: 不含 `~` 的清单不因 `HOME` 缺失被拒, 与 `manifest-schema` 的"主目录解析"一致.
- 不改变任何命令行为, 清单格式, token 或退出码.

## Capabilities

### New Capabilities

- 无.

### Modified Capabilities

- 无. 本变更只修正文档措辞, `.openspec.yaml` 设置 `skip_specs: true`.

## Impact

- 仅 `README.md`; 不涉及源码, 测试, 依赖或规格行为.

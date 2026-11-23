# Design

## Context

`README.md:222` 当前把"主目录无法确定"与"清单不存在", "当前工作目录不可用"并列, 读起来是清单校验失败的无条件原因. 规格 `openspec/specs/manifest-schema/spec.md` 的"主目录解析"只在展开 `~` 时要求 `HOME`; 不含 `~` 的清单不因 `HOME` 缺失被拒. 测试 `tests/manifest_errors.rs` 已锁定绝对 target 在 `HOME` 缺失时仍可 `apply`.

## Goals / Non-Goals

**Goals:**

- 让 README 的退出码 2 说明与规格和实现一致.

**Non-Goals:**

- 不改变清单解析, 路径解析, 退出码或任何命令行为.
- 不重写 README 的其他章节.

## Decisions

### D1. 只改措辞, 不改行为

最小改动是把无条件列表改为限定语, 并在同段补一句反例. 备选方案是删除"主目录无法确定"这一项; 否决理由是它在需要 `~` 时确实是失败原因, 删除会让读者漏掉这个真实分支.

### D2. 不新增规格 delta

这是文档措辞修正, 没有行为变化, 因此 `.openspec.yaml` 设置 `skip_specs: true`, 规格本身不改.

## Risks / Trade-offs

- [文档再次漂移] -> 用 `grep -n "主目录无法确定" README.md` 锁定唯一出现点, 并运行标点检查与全量门禁.

# Design

## Context

`src/render.rs` 的 `push_escaped` 目前用 `char::is_control()` 与 `is_layout_control` 判断是否转义. `is_layout_control` 只覆盖 U+2028, U+2029, 常见零宽 / bidi 控制符与 U+FEFF; U+00AD, U+061C, 标签字符等在 `Cf` 或 Default_Ignorable 中的码点仍原样输出. 现有报告测试已经锁定"每条目恰好一行"与"控制字符转义"; 本变更只扩大转义集合.

## Goals / Non-Goals

**Goals:**

- 让报告层覆盖 Unicode 17.0 的 Default_Ignorable_Code_Point, 使剩余的不可见格式字符不能隐藏文本或改变视觉顺序.
- 保持现有转义形式, 单行输出与列宽规则不变.

**Non-Goals:**

- 不改变清单解析, 路径解析, 文件系统操作或退出码.
- 不引入 Unicode 属性依赖, 不按字素簇分段, 不改写宽度表.

## Decisions

### D1. 使用静态排序区间表

新增 `DEFAULT_IGNORABLE_RANGES: &[(u32, u32)]`, 数据取自 Unicode 17.0 的 `DerivedCoreProperties.txt` 中 Default_Ignorable_Code_Point 的区间; 用与 `WIDE_RANGES` 相同的二分查找. 备选方案是引入 `unicode-properties` 或 `unicode-general-category`; 否决理由是本项目当前只靠静态表即可, 新依赖会扩大供应链与 MSRV 风险.

### D2. 保留布局控制符并追加 Default_Ignorable

`push_escaped` 的判断从 `character.is_control() || is_layout_control(character)` 改为 `character.is_control() || is_layout_control(character) || is_default_ignorable(character)`. 其中 `is_layout_control` 收窄为 U+2028 / U+2029, 因为行分隔与段分隔不属于 Default_Ignorable; 若把条件直接写成两谓词形式, 这两个码点会退回原样显示, 与主规格及现有测试冲突. 其余常见零宽 / bidi 控制符本身具有 Default_Ignorable 属性, 由 `is_default_ignorable` 统一覆盖, 不再在 `is_layout_control` 中重复列出. 三谓词形式因此是必需的, 而非冗余.

被否决的备选方案: 把条件写成 `character.is_control() || is_default_ignorable(character)`. 否决理由是它会丢掉 U+2028 / U+2029 的转义. 另一备选方案是保留 `is_layout_control` 的完整旧集合再追加第三个谓词; 否决理由是旧集合大部分与 Default_Ignorable 重叠, 留下重复分支会让后续新增码点容易漏掉一处.

### D3. 只改显示层

转义只发生在 `push_escaped` 与 `escape_controls`, 不触碰 `classify`, `apply`, `undo` 或备份查找. 选择仍使用原始路径字节与文件系统 API.

## Risks / Trade-offs

- [区间表与 Unicode 版本漂移] -> 表头注明 Unicode 17.0, 测试锁定代表性码点, 升级时同步 README.
- [误伤正常可见字符] -> 只覆盖 Default_Ignorable 属性, 正常路径字符不受影响.
- [宽度计算差异] -> 转义后按 ASCII 字面计宽, 与现有控制字符一致.

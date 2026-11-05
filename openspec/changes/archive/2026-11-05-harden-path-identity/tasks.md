# Tasks

## 1. 身份判等原语

- [x] 1.1 在 `src/os/mod.rs` 增加文件身份类型与两个原语: 按 lstat 语义取"落点对象"身份, 按跟随语义取基准对象身份; 用硬链接 (同身份) 与符号链接 (落点不同) 的单测断言两者语义, 运行 `cargo test --locked os` 确认通过.
- [x] 1.2 为原语补充失败路径断言: 不存在的路径返回错误, 调用方据此回退字符串比较; 运行对应单测确认通过.

## 2. 基准点集合与 V-9

- [x] 2.1 在 `src/manifest.rs` 解析入口构造基准点身份集合 (文件系统根, 仓库根与其祖先, HOME 两种形态, 清单文件且跟随符号链接), 取不到身份时只保留字符串形态; 单测覆盖"仓库根为普通目录"与"HOME 为符号链接"两种形态, 运行 `cargo test --locked manifest` 确认通过.
- [x] 2.2 让 V-9 在目标落点存在时比较基准身份, 命中即整体拒绝并复用既有说明措辞; 单测断言别名命中与不命中两种情况, 运行 `cargo test --locked manifest` 确认通过.
- [x] 2.3 在 `tests/manifest_errors.rs` 增加端到端用例: 大小写不敏感卷上把 `target` 写成仓库根别名与清单文件别名, 断言退出码 2, 说明指出对应落点, 且仓库与清单文件保持原样; 卷大小写敏感时打印原因并跳过; 运行 `cargo test --locked --test manifest_errors` 确认通过.

## 3. V-5 与 V-10 的身份判等

- [x] 3.1 让 V-5 在两侧目标都存在时按身份判重, 字符串判重保持不变; 单测断言"不同写法同一对象"被拒与"不同对象"仍接受, 运行 `cargo test --locked manifest` 确认通过.
- [x] 3.2 让 V-10 在目标与源都存在时按身份判自指; 单测断言 `source = "a"`, `target = "A"` 形态被拒, 运行 `cargo test --locked manifest` 确认通过.
- [x] 3.3 在 `tests/manifest_errors.rs` 增加大小写别名自指的端到端用例, 断言退出码 2, 说明指出自指, 且源文件内容未被备份或改动; 运行 `cargo test --locked --test manifest_errors` 确认通过.

## 4. V-11 的祖先身份判等

- [x] 4.1 为已存在源与已存在目标分别构建身份映射, 在判断包含关系时沿祖先逐级比较身份; 单测覆盖别名目录嵌套与"不同目录不误报"两种情况, 运行 `cargo test --locked manifest` 确认通过.
- [x] 4.2 在 `tests/manifest_errors.rs` 增加别名目录嵌套的端到端用例, 断言退出码 2, 说明指出相关条目与重叠路径, 且文件系统无改动; 运行 `cargo test --locked --test manifest_errors` 确认通过.
- [x] 4.3 补充 firmlink 门控用例: 仅在 macOS 且 firmlink 别名与主目录同身份时, 断言别名形式的主目录落点被拒; 不满足条件时打印原因并跳过; 运行 `cargo test --locked --test manifest_errors` 确认通过.

## 5. 文档与集成验证

- [x] 5.1 更新 `README.md` 的 V-9 段落 (说明别名写法同样被拒) 与"已知限制" (原先"清单文件为符号链接时只保护路径自身"的残余风险已被身份判等关闭; 两侧都不存在时的别名漏判仍属已知边界); 人工核对与 `manifest-schema` delta 一致.
- [x] 5.2 运行 `./scripts/check.sh`, 确认 fmt, 全量测试, clippy, rustdoc 与 release 构建全部通过.
- [x] 5.3 运行 `cargo check --locked --target x86_64-unknown-linux-gnu` 与 `cargo check --locked --target x86_64-pc-windows-gnu`, 确认身份原语不破坏交叉编译.

## Workflow follow-up

- 归档前复核 `manifest-schema` 四个 MODIFIED requirement 合并到主规格后的文本与 Scenario 完整性.
- 归档后人工复核 README 的别名说明与主规格一致, 并确认测试在大小写敏感卷上的跳过行为符合预期.

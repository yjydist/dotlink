# Tasks

## 1. 比较形态 helper

- [x] 1.1 在 `src/os/mod.rs` 增加只读的比较形态 helper, 复用 `Resolver` 的深度上限与词法回退; 验证: 新增单测覆盖普通路径, 悬空链接, 自指环与两节点互指, 运行 `cargo test --locked os::` 通过.
- [x] 1.2 明确 helper 的注释与可见性: 只供 `manifest` 校验使用, 不改变 `normalize` 的运行时语义; 验证: `cargo doc --locked --no-deps --document-private-items` 无警告.

## 2. 清单双形态与双身份

- [x] 2.1 在 `src/manifest.rs` 为每个 target/source 建立词法形态与比较形态的索引; V-5 目标去重同时使用两套字符串与身份; 验证: 新增"符号链接 target 与 referent 同时出现"用例, 退出码 2.
- [x] 2.2 扩展 V-9b 的基准身份, 同时保存 lstat 与 follow 形态; 验证: 新增 HOME 或 manifest 为符号链接的别名落点用例, 退出码 2, 零改动.
- [x] 2.3 扩展 V-10: source 为符号链接时, 使用 follow 身份与链接文本解析目标参与自指判定; 验证: 新增 `source = "link"`, `target = "~/.foo"`, `link -> ~/.foo` 的用例, 退出码 2.
- [x] 2.4 扩展 V-11: 目标祖先/后代按双形态执行; 目标与源的比较采用非对称规则 (目标词法形态对源词法+比较形态, source follow 身份/解析目标对目标词法形态; 目标比较形态只参与目标间嵌套与 V-5, 不与源比较, 否则会拒绝已部署的 `target -> source` 链接); 验证: "符号链接父目标 + 子目标" 退 2 且零改动, 已部署的正确链接仍被接受.
- [x] 2.5 保持合法清单行为: 单一目标位于符号链接父目录下, 多个独立目标共享同一源仍被接受; 验证: 既有 `manifest_errors` 与 `conflict` 套件全部通过.

## 3. 集成测试

- [x] 3.1 增加"符号链接父目标 + 嵌套子目标"端到端用例: 四个命令都退出 2, 目标, 父目录与备份目录保持原样; 验证: 运行该用例并断言 `snapshot_all` 前后一致.
- [x] 3.2 增加"source 符号链接指向 target"端到端用例: `apply` 不建立链接, 退出 2; 验证: `fs::symlink_metadata(target)` 不存在, 清单文件不变.
- [x] 3.3 增加"合法的符号链接父目录 + 单一子目标"用例: 清单被接受, `apply` 正常建立链接; 验证: `status` 退出 0, 链接指向源.
- [x] 3.4 增加"target 是符号链接, 比较形态指向另一条目 target"的 V-5 用例; 验证: 退出 2, 说明指出两个条目.

## 4. 文档与规格

- [x] 4.1 更新 `README.md` 的"源与目标必须相互隔离"与"落点本身也不得退化": 说明比较形态与运行时末段不解析的区别, 以及新增拒绝面和替代写法; 验证: `rg -n "比较形态|符号链接父目标" README.md` 能定位.
- [x] 4.2 运行 `openspec validate harden-symlink-isolation --strict` 通过; 验证: 无 error 与 warning.

## 5. 全量验证

- [x] 5.1 运行 `cargo test --locked` 与 `cargo clippy --locked --all-targets -- -D warnings`; 验证: 全部通过.
- [x] 5.2 运行 `./scripts/check.sh`; 验证: 标点检查, fmt, 测试, clippy, rustdoc 与 release 构建全部通过.

## Workflow follow-up

- 归档前确认 README 的新拒绝面与 `manifest-schema` / `path-normalization` 的 Scenario 措辞一致.

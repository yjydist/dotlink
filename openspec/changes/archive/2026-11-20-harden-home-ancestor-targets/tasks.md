# Tasks

## 1. 清单校验实现 (`src/manifest.rs`)

- [x] 1.1 在 `reject_degenerate_target` 的主目录相等判定之后增加祖先判定: `home.starts_with(landing)` 与 `home_physical.starts_with(landing)` 任一成立即拒绝; 验证 `cargo build --locked` 成功. 可观察结果: `target = "~/.."` 在仓库位于 `HOME` 之外时退 2.
- [x] 1.2 增加 `~` 逃逸判定: 书写形态以 `~` 或 `~/` 开头, 对 `HOME` 与剩余段按 `Path::components` 做纯词法折叠 (不解析祖先符号链接), 折叠后不再是 `HOME` 或其子路径时拒绝; `HOME` 不可用时跳过. 验证 `cargo build --locked` 成功. 可观察结果: `target = "~//../x"` 退 2, `target = "~/sub"` 仍被接受, `~/symlinked-dir/file` (父目录是指向 HOME 外部的符号链接) 也仍被接受.
- [x] 1.3 在 `BaselineIdentities::collect` 中收集 `home` 与 `home_physical` 全部 `ancestors()` 的 `os::basis_identity`, 去重后并入主目录身份集合; 在 `reject_degenerate_target` 的落点身份分支中命中该集合即拒绝. 验证 `cargo build --locked` 成功. 可观察结果: `HOME` 祖先目录的大小写别名写法退 2.
- [x] 1.4 为祖先命中新增独立错误说明, 经 `invalid_entry` 输出并包含条目下标与书写形态; 验证 `cargo build --locked` 成功且现有 `cargo test --locked --lib manifest::tests` 不回归.

## 2. 单元测试 (`src/manifest.rs` 的 `mod tests`)

- [x] 2.1 增加 `~/..` 与绝对形式 `HOME` 父目录的拒绝用例, 断言错误含 `entry[0]` 与书写形态; 验证 `cargo test --locked --lib manifest::tests::v9c_` 通过.
- [x] 2.2 增加 `~//../x` 的拒绝用例与 `~/sub`, `/etc/hosts` 的接受用例; 验证同一命令通过, 且合法用例的 `parse` 返回 `Ok`.
- [x] 2.3 增加 `HOME` 是符号链接时物理形态祖先的拒绝用例, 以及大小写不敏感卷上祖先目录别名用例 (卷不支持时打印跳过); 验证同一命令通过.
- [x] 2.4 增加 `HOME` 不可用且 target 为绝对路径时不被拒绝的用例; 验证同一命令通过.
- [x] 2.5 增加 `~/symlinked-dir/file` 的接受用例: `symlinked-dir` 是指向 `HOME` 外部且不是 `HOME` 祖先的目录; 断言词法 `~` 判定不误伤该合法结构; 验证同一命令通过. 组验证: `cargo test --locked --lib manifest::tests` 全绿.

## 3. 端到端测试 (`tests/manifest_errors.rs`)

- [x] 3.1 新增一个仓库位于 `HOME` 之外的夹具 (仓库与 `HOME` 分别为互不包含的临时目录, 必要时在 `tests/support/mod.rs` 增加辅助函数), 避免 `TempEnv` 的 `repo/home` 布局让仓库祖先检查提前命中; 验证夹具下 `target = "~/.."` 能走到新判定.
- [x] 3.2 用该夹具断言 `target = "~/.."`, `target = "~//../x"` 与 `HOME` 父目录绝对路径: 四条命令 (`preview` / `apply` / `status` / `undo`) 都退 2, 标准输出为空, 说明指出 `entry[0]` 与书写形态, 且 `snapshot_all` 前后一致; 验证 `cargo test --locked --test manifest_errors v9c_` 通过.
- [x] 3.3 用该夹具断言 `target = "~/sub"` 与仓库外的合法绝对路径仍成功, 以及 `HOME` 缺失 (含 `env_remove("HOME")`) 时绝对 target 不被误拒; 验证同一命令通过.
- [x] 3.4 若可构造, 增加祖先目录大小写别名的端到端用例 (不可构造时打印跳过); 验证同一命令通过. 组验证: `cargo test --locked --test manifest_errors` 全绿.

## 4. 文档 (`README.md`)

- [x] 4.1 更新 `README.md` 的 V-9 段落: 把受保护落点写为"主目录本身及其祖先", 补充 `~/..` 与 `~//../x` 示例, 并标注这是兼容性收紧 (旧版接受, 现在退 2, 零改动); 验证 `grep -n "主目录的祖先" README.md` 有命中且语义与 V-9c 一致.
- [x] 4.2 确认 README 仍写明 `~/sub` 与 `/etc/hosts` 不受影响, 且"身份判等不可用时退回字符串比较"的表述与本条一致; 验证 `grep -n "~/sub" README.md` 与 `grep -n "文件系统身份" README.md` 均有命中.

## 5. 全量验证

- [x] 5.1 运行 `openspec validate harden-home-ancestor-targets --strict`, 期望无错误与警告.
- [x] 5.2 运行 `cargo test --locked`, 期望全部测试通过 (含新增单测与端到端用例).
- [x] 5.3 运行 `cargo clippy --locked --all-targets -- -D warnings`, 期望零警告.

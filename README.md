# dotlink

声明式 dotfile 软链接管理: 用一份 `dotlink.toml` 描述"哪个源文件应该出现在哪个目标路径", 由 dotlink 负责建立软链接, 并在需要覆盖已有内容时先备份.

无配置面, 无状态文件, 无交互提示. 重复执行不会产生累积副作用: `apply` 第二次运行为全部条目报 `ok` 且不改动文件系统, `undo` 在撤销后目标为空的清单上同样如此 (若上一次恢复了备份, 目标被恢复出的原内容占用, 第二次 `undo` 报 `skip` 且同样不改动文件系统).

## 安装

获取源码后, 在项目源码目录运行:

```sh
cargo install --path . --locked  # 安装到 ~/.cargo/bin
```

要求 Rust 1.97 或更高; 受支持平台为 macOS 与 Linux. 安装完成后, dotlink 在你的 dotfiles 仓库中运行 (下节的 `~/dotfiles` 即该仓库).

## 快速上手

以仓库 `~/dotfiles` 为例, 四步即可完成部署:

> **命令在哪里运行**: 所有命令都在**存放 `dotlink.toml` 的那个目录**中运行, dotlink 不向上搜索父目录.

```sh
cd ~/dotfiles                        # 仓库根目录, 也是 dotlink.toml 所在处
mkdir -p shell nvim
printf 'export EDITOR=vim\n' > shell/bashrc
printf 'vim.opt.number = true\n' > nvim/init.lua
```

**1. 编写清单** `~/dotfiles/dotlink.toml`:

```toml
[[entry]]
source = "shell/bashrc"
target = "~/.bashrc"

[[entry]]
source = "nvim"
target = "~/.config/nvim"
```

`source` 是仓库内的相对路径, `target` 是期望链接出现的位置.

**2. 预览** 将要发生什么 (只读, 不改动任何文件):

```console
$ dotlink preview
1/2 create    ~/.bashrc           未部署 -> 将创建链接
2/2 create    ~/.config/nvim      未部署 -> 将创建链接
2 个条目: 2 成功, 0 已是最新, 0 未处理, 0 失败
$ echo $?
1
```

这里退出 1 表示有链接待创建; 其他情况下也可能表示冲突、源缺失或失败, 需结合逐条结果判断.

**3. 部署**:

```console
$ dotlink apply
1/2 create    ~/.bashrc           未部署 -> 已创建链接
2/2 create    ~/.config/nvim      未部署 -> 已创建链接
2 个条目: 2 成功, 0 已是最新, 0 未处理, 0 失败
$ echo $?
0
```

**4. 验证链接生效**:

```console
$ dotlink status
1/2 ok        ~/.bashrc           已是最新
2/2 ok        ~/.config/nvim      已是最新
2 个条目: 0 成功, 2 已是最新, 0 未处理, 0 失败
$ echo $?
0
$ readlink ~/.bashrc
/Users/you/dotfiles/shell/bashrc
$ cat ~/.bashrc
export EDITOR=vim
```

`~/.config` 这类缺失的父目录会被自动创建. 想回到原状时执行 `dotlink undo`.

## 清单格式

清单固定命名为 `dotlink.toml`. 顶层只允许 `entry` 键, 条目以 `[[entry]]` 数组表给出, 至少需要一条:

```toml
[[entry]]
source = "shell/bashrc"
target = "~/.bashrc"

[[entry]]
source = "shell/zshrc"
target = "~/.zshrc"

[[entry]]
source = "nvim"
target = "~/.config/nvim"

[[entry]]
source = "git/gitconfig"
target = "~/.gitconfig"
on_conflict = "replace_and_backup"
```

### 字段

| 键 | 必填 | 取值 | 默认 | 语义 |
|----|------|------|------|------|
| `source` | 是 | 非空字符串 | 无 | 仓库内的相对路径, 可以是文件或目录; 不接受 `~` 前缀、绝对路径, 以及解析后落在仓库之外的路径 |
| `target` | 是 | 非空字符串 | 无 | 期望链接出现的位置, 规则见下 |
| `on_conflict` | 否 | `"skip"` 或 `"replace_and_backup"` | `"skip"` | 目标被非 dotlink 内容占用时的策略 |

`on_conflict = "skip"` (默认) 会保留占用物并把该条目标记为 `conflict`, 本次不动它; `on_conflict = "replace_and_backup"` 会先把占用物备份到同目录的 `<原名>.dotlink-backup.<时间戳>`, 再建立链接. 备份文件的完整路径会打印在 `replace` 行里.

备份保留原对象的内容、对象类型、权限位、修改时间 (mtime) 以及全部可设置的扩展属性. 扩展属性采用**尽力而为**策略: 文件系统不支持、权限不足或内核保护导致某个属性无法设置时跳过该属性, 其余内容与元数据仍照常备份; 磁盘满等 I/O 错误仍使该条目失败. 不保留所有权与 ACL (macOS 的 ACL 不经 `listxattr` 暴露), 也不保留符号链接自身的元数据. 目录的 mtime 在其子项全部复制完成后设置.

`undo` 恢复时应用该备份自身的权限、mtime 与扩展属性. 旧版本创建的备份没有保真元数据, 恢复后 mtime 是**备份时刻**而不是更早的原文件时间, 内容与权限不受影响. 恢复成功后备份文件仍留在原地, 不会被删除; 不再需要的备份需自行清理.

### `target` 的三种形态

| 形态 | 写法 | 解析为 |
|------|------|--------|
| 主目录 | `~/.bashrc` | 当前用户主目录下的 `.bashrc` |
| 绝对路径 | `/etc/hosts` | 原样使用 |
| 相对路径 | `link/bashrc` | 相对于清单文件所在目录, 即仓库根 |

三种形态可以混用. 报告中显示的是你在清单里写的原始字符串, 不是解析后的绝对路径, 便于逐行对照. 目标路径重复的清单会被整体拒绝.

`~` 简写只认 `~/...` 与裸 `~` (裸 `~` 的落点是主目录本身, 会被下一段拒绝). 写成 `~.bashrc` (漏斜杠) 或 `~foo/bar` 会被整体拒绝 (退出码 2), 而不会静默落到仓库里建出一个名为 `~.bashrc` 的路径 -- 落点错误的清单必须报错, 不能假装成功.

落点本身也不得退化: 归一化后是文件系统根 `/`、当前用户主目录 (裸 `~` 或等价的绝对路径)、仓库根本身或其祖先 (如 `.` / `..`)、或该条目自己的 `source` 时, 整份清单都会被拒绝并指出条目 (退出码 2, 不做任何改动). 这些落点一旦被替换备份, 毁掉的是整个目录、整个主目录乃至源仓库. `~/sub`、`/etc/hosts` 这类绝对路径与仓库内的相对 `target` 都不受影响.

### 源与目标必须相互隔离

同一清单的目标之间不能互为祖先或后代; 任一目标也不能与任一源路径重合、包含或被包含. 比较以规范化后的路径组件为准, 多个独立目标仍可共享同一源. 违反时四条命令都会在改动前退出 2, 指出相关条目和路径.

这是一次兼容性收紧: 旧版接受的跨条目源目标重合及嵌套目标清单现在会被拒绝. 例如同时部署 `nvim -> ~/.config/nvim` 和 `extra -> ~/.config/nvim/extra.lua` 会通过父链接修改源目录. 可以把额外文件直接整理到仓库的 `nvim` 目录中, 仅部署整个目录; 或取消目录条目, 改为逐文件声明互不嵌套的目标:

```toml
[[entry]]
source = "nvim/init.lua"
target = "~/.config/nvim/init.lua"

[[entry]]
source = "extra"
target = "~/.config/nvim/extra.lua"
```

源目标重合的条目则需把目标移到独立位置, 不再让部署目标兼任源文件.

### 未知键会被拒绝

顶层出现 `entry` 之外的键, 或条目里出现 `source` / `target` / `on_conflict` 之外的键 (例如把 `source` 拼成 `sorce`), 整份清单都会被拒绝并指出键名. 这是为了避免拼错的键静默退化成默认值.

## 命令与退出码

四条命令都不接受参数, 也没有 `--repo` / `--manifest` / `--force` 之类的开关: 仓库根与覆盖许可都写在清单里.

| 命令 | 作用 | 目标谓词 (达成才算成功) |
|------|------|--------------------------|
| `preview` | 只读: 报告每个条目将会发生什么 | 没有任何条目需要执行动作 |
| `apply` | 执行: 按清单建立链接, 必要时先备份 | 每个条目的目标路径都是指向其源条目的正确链接 |
| `status` | 只读: 报告清单与实际状态的不一致 | 清单与实际状态完全一致, 没有任何偏离 |
| `undo` | 撤销: 移除 dotlink 创建的链接, 并恢复备份 | 每个条目的目标路径上都不存在 dotlink 创建的链接 |

退出码的通用语义是: **退出 0 当且仅当该命令的目标已达成, 且没有遗留待办**. 两个条件必须同时成立.

| 码 | 含义 (四条命令共用) |
|----|---------------------|
| 0 | 该命令的目标已完全达成, 且无遗留待办 |
| 1 | 存在未达成目标的条目, 或有被跳过 / 被拒绝 / 失败 / 源缺失的条目 |
| 2 | 清单校验失败 **或** 调用方式错误 (未做任何改动) |
| 3 | 平台不受支持 (仅支持 macOS 与 Linux) |

同一份"已部署好"的清单上, `apply` 与 `status` 退 0, 而 `undo` 退 1 -- 二者的目标刻意相反, 这是预期而非缺陷. `preview` 与 `apply` 共用同一套判断, 区别只在求值时刻: `preview` 在动手**之前**求值, 所以未部署时退 1; `apply` 在动手**之后**求值, 所以同一份清单上退 0.

`-h` / `-V` 是正常路径, 退出 0, 不属于上面四类结局.

### 退出码 2 的两重含义

"清单校验失败"与"子命令缺失或拼错"共用码值 2, 二者靠**标准错误的内容**区分:

清单校验失败 (含清单不存在, 当前工作目录不可用, 主目录无法确定):

```console
$ dotlink status
dotlink: 无法读取清单 ./dotlink.toml: No such file or directory (os error 2)
$ echo $?
2
```

调用方式错误 (由 `clap` 在读取清单之前拒绝):

```console
$ dotlink statu
error: unrecognized subcommand 'statu'

  tip: a similar subcommand exists: 'status'

Usage: dotlink <COMMAND>

For more information, try '--help'.
$ echo $?
2
```

判据是"标准错误是否为用法说明": 带 `Usage:` 与 `unrecognized subcommand` 的是敲错了命令, 单行的 `dotlink: 无法读取清单 ...` 是清单本身有问题. 这一点同样写在每个子命令的 `--help` 里.

## 在 `set -e` 脚本中使用

退出 1 既可能表示有待办, 也可能包含跳过或失败的条目. 保留逐条输出, 并区分 1 与清单错误 2、平台错误 3. 以下示例在 `preview` 退出 1 时尝试部署, 将最终结果交给 `apply` 判断:

```sh
set -e
if dotlink preview; then
  echo "已完全符合期望, 无需部署"
else
  dotlink_code=$?
  case "$dotlink_code" in
    1) dotlink apply ;;
    *) exit "$dotlink_code" ;;
  esac
fi
```

`undo` 会直接执行撤销, 成功移除链接也可能退出 1. 只调用一次, 查看逐条结果, 并保留原退出码供调用者判断:

```sh
set -e
dotlink_code=0
dotlink undo || dotlink_code=$?
case "$dotlink_code" in
  0) echo "无需撤销" ;;
  1) echo "请检查上方结果: 已执行撤销, 或有跳过、源缺失或失败的条目" ;;
  2) echo "清单校验或调用方式错误" ;;
  3) echo "平台不受支持" ;;
esac
exit "$dotlink_code"
```

## 平台

仅支持 macOS 与 Linux. 在其他平台上, 任何参数解析与清单读取**之前**就会拒绝: 标准输出为空, 标准错误输出一行 `dotlink: 不支持当前平台 (仅支持 macOS 与 Linux)`, 退出码 3, 不触碰文件系统.

Windows 上的运行时拒绝**未在任何环境验证** (`cargo check --target x86_64-pc-windows-gnu` 只能证明源码可编译, 不能证明运行时行为).

## 开发

要求 Rust 1.97 或更高 (edition 2024), 并安装当前工具链的 `rustfmt` 与 `clippy` 组件. 运行本地完整检查:

```sh
./scripts/check.sh
```

脚本自动定位项目根目录, 依次执行格式检查、全套测试、Clippy、将警告视为错误的 rustdoc 构建和 release 构建, 任一步失败即停止. 检查不修改源码; 构建产物位于 `target/`, 二进制为 `target/release/dotlink`. 脚本使用锁文件, 不安装工具链或交叉编译目标.

按需验证最低 Rust 版本及跨平台编译 (需事先通过 `rustup` 安装对应工具链和目标):

```sh
cargo +1.97.0 test --locked
cargo check --locked --target x86_64-unknown-linux-gnu
cargo check --locked --target x86_64-pc-windows-gnu
```

设计文档 (规格, 契约, 数据模型, 验证场景) 在 `specs/001-dotfile-link-management/`. 交叉编译检查不能代替目标平台的运行时验证; Windows 拒绝行为仍需在 Windows 环境确认 (见"平台").

## 已知限制

- **并发**: 不保证并发调用串行化. 单次链接创建与不覆盖移动是原子操作, 但整个 `apply` / `undo` 不是事务; 替换期间目标可能暂时不存在, 多步操作可能部分失败. 并发修改源或父目录没有完整隔离, 并发调用可能产生重复备份.
- **备份不自动回收**: `undo` 恢复后备份文件仍留在原地, 更早的备份不会被自动使用或删除, 需自行清理; 复制中断留下的 `.partial` 残留也不会被 `undo` 消费.
- **`undo` 只恢复最新备份**: 同目录下按"先比时间戳, 同一秒内再比消歧序号的数值"取最新的一份; 更早的备份保留但不被自动使用.
- **元数据保真为尽力而为**: 不保留所有权与 ACL; 无法设置的扩展属性按属性跳过; 平台不支持扩展属性或 mtime 设置时静默降级, 不产生额外警告. 旧版本创建的备份按自身元数据恢复, 其 mtime 是备份时刻.

更多可执行验证场景见 [端到端验证指南](specs/001-dotfile-link-management/quickstart.md).

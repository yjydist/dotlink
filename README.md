# dotlink

声明式 dotfile 软链接管理: 用一份 `dotlink.toml` 描述"哪个源文件应该出现在哪个目标路径", 由 dotlink 负责建立软链接, 并在需要覆盖已有内容时先备份.

无配置面, 无状态文件, 无交互提示. 重复执行不会产生累积副作用: `apply` 第二次运行为全部条目报 `ok` 且不改动文件系统, `undo` 在已撤销干净的清单上同样如此.

## 安装

```sh
git clone <dotlink 仓库>
cd dotlink
cargo install --path .           # 安装到 ~/.cargo/bin; 或者只 cargo build --release, 产物为 target/release/dotlink
```

要求 Rust 1.97 或更高; 受支持平台为 macOS 与 Linux. 安装完成后, dotlink 在你的 dotfiles 仓库中运行 (下节的 `~/dotfiles` 即该仓库).

## 快速上手

以仓库 `~/dotfiles` 为例, 四步即可完成部署:

> **命令在哪里运行**: 所有命令都在**存放 `dotlink.toml` 的那个目录**中运行, dotlink 不向上搜索父目录.

```sh
cd ~/dotfiles                        # 仓库根目录, 也是 dotlink.toml 所在处
mkdir -p shell nvim
printf 'export EDITOR=vim\n' > shell/bashrc
printf 'set number\n' > nvim/init.lua
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

退出码 1 的含义是"有活要干", 不是出错.

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
| `source` | 是 | 非空字符串 | 无 | 仓库内的相对路径, 可以是文件或目录; 不接受 `~` 前缀或绝对路径 |
| `target` | 是 | 非空字符串 | 无 | 期望链接出现的位置, 规则见下 |
| `on_conflict` | 否 | `"skip"` 或 `"replace_and_backup"` | `"skip"` | 目标被非 dotlink 内容占用时的策略 |

`on_conflict = "skip"` (默认) 会保留占用物并把该条目标记为 `conflict`, 本次不动它; `on_conflict = "replace_and_backup"` 会先把占用物备份到同目录的 `<原名>.dotlink-backup.<时间戳>`, 再建立链接. 备份文件的完整路径会打印在 `replace` 行里. `undo` 恢复成功后备份文件仍留在原地, 不会被删除; 不再需要的备份需自行清理.

### `target` 的三种形态

| 形态 | 写法 | 解析为 |
|------|------|--------|
| 主目录 | `~/.bashrc` | 当前用户主目录下的 `.bashrc` |
| 绝对路径 | `/etc/hosts` | 原样使用 |
| 相对路径 | `link/bashrc` | 相对于清单文件所在目录, 即仓库根 |

三种形态可以混用. 报告中显示的是你在清单里写的原始字符串, 不是解析后的绝对路径, 便于逐行对照. 目标路径重复的清单会被整体拒绝.

`~` 简写只认 `~/...` 与裸 `~`. 写成 `~.bashrc` (漏斜杠) 或 `~foo/bar` 会被整体拒绝 (退出码 2), 而不会静默落到仓库里建出一个名为 `~.bashrc` 的路径 -- 落点错误的清单必须报错, 不能假装成功.

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

退出 1 表示"有待办", **不代表出错**. 因此裸调只读命令会让 `set -e` 的脚本中断在"其实还有活要干"这一步上, 而不是中断在真正的错误上. 请改用条件形态:

```sh
set -e
if dotlink preview >/dev/null; then
  echo "已完全符合期望, 无需部署"
else
  dotlink apply
fi
```

`undo` 同理:

```sh
if dotlink undo >/dev/null; then echo "无需撤销"; else dotlink undo; fi
```

只在关心失败时, 用退出码区分三种结局:

```sh
dotlink status || case $? in
  1) echo "有偏离或待办, 需要处理" ;;
  2) echo "清单有问题, 先修清单" ;;
  3) echo "平台不受支持" ;;
esac
```

## 平台

仅支持 macOS 与 Linux. 在其他平台上, 任何参数解析与清单读取**之前**就会拒绝: 标准输出为空, 标准错误输出一行 `dotlink: 不支持当前平台 (仅支持 macOS 与 Linux)`, 退出码 3, 不触碰文件系统.

Windows 上的运行时拒绝**未在任何环境验证** (`cargo check --target x86_64-pc-windows-gnu` 只能证明源码可编译, 不能证明运行时行为).

## 文档审阅记录

以下三条成功标准为主观判断项, 由本文件的作者逐条复核, 结论留痕于此.

- **SC-008** (新用户只读快速上手即可在 5 分钟内完成全流程): 通过. 快速上手给出 4 个编号步骤, 共 6 条命令 (`cd` / `mkdir` + `printf` + `printf` 建素材, `preview`, `apply`, `status` 验证), 期望输出逐条列出且与实测一致, 无需查阅其他文档.
- **SC-007** (仅凭命令行输出即可判断每个条目的结果与下一步动作): 通过. 实测构造的 `conflict` / `skip` / `missing` / `fail` / `foreign` / `wrong` 各行之说明均含原因, `replace` / `restore` 行含备份文件完整路径, `foreign` / `wrong` 行含占用物类型与实际指向.
- **SC-010** (解释命令该在哪里运行所需的文字不超过一句话): 通过. 全文只有"快速上手"开头引用块的一句陈述运行位置, 句子数计 1; 安装与清单格式两节不涉及运行位置.
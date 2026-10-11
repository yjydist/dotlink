# Quickstart: 端到端验证指南

本文件用可复制的命令证明规格中的每条成功标准与关键需求.契约细节见 `contracts/cli.md` 与 `contracts/manifest.md`, 判定规则见 `data-model.md`.

## 前置

- macOS 或 Linux; Rust 工具链 1.97 或更高.
- 构建: `cargo build` (产物 `target/debug/dotlink`).
- **所有场景都必须在临时目录中执行**, 且用临时目录作为 `HOME`, 以免触及真实主目录:

  ```sh
  export D=/tmp/dotlink-demo && rm -rf "$D" && mkdir -p "$D/repo/shell" "$D/home/.config"
  export HOME="$D/home"          # 关键: 隔离主目录
  export PATH="$PWD/target/debug:$PATH"
  printf 'export EDITOR=vim\n' > "$D/repo/shell/bashrc"
  mkdir -p "$D/repo/nvim" && printf 'set number\n' > "$D/repo/nvim/init.lua"
  cd -P "$D/repo"
  ```

  后续所有清单内容都写入 `$D/repo/dotlink.toml`, 命令都在 `$D/repo` 下执行.

  **`$D` 的路径形态是固定的, 不要改变**: 在 macOS 上 `/tmp` 是指向 `/private/tmp` 的符号链接, `cd -P "$D/repo"` 会进入物理路径 `/private/tmp/dotlink-demo/repo`. 本文件里展示的输出与 `readlink` 期望值都按这个形态书写 (例如 `readlink "$HOME/.config/nvim"` 期望 `/private/tmp/dotlink-demo/nvim`). 若你的 shell 把 `cd` 实现为逻辑切换 (保留 `/tmp` 形态, 如 zsh 的默认设置), 则场景 1 的 `readlink` 输出会是 `/tmp/dotlink-demo/nvim`, 这是同一个位置, 属正常差异. 由于 `$HOME` 与仓库同被 `/tmp` 前缀覆盖, 二者的规范化结果一致, 归属判定不受影响 (这正是 `research.md` R2 的实测情形).

  **状态是累积的**: 各场景依次在前一个场景结束时留下的状态上继续, 因此每个场景的"期望"只在**上一个场景确实按文档执行完**时才成立. 下表是各场景的起始状态, 供手工分步执行时对照:

  | 场景 | 起始状态 |
  |------|----------|
  | 1 | 全新: 无链接, 清单不存在 |
  | 2 | 场景 1 已部署两条链接 |
  | 3 | 同上; 本场景把清单换成 50 条性能清单 |
  | 4 | **需重置**: 清单复原为 2 条, `$HOME/.bashrc` 已删掉并换成含 `my precious notes` 的普通文件 |
  | 5 | 场景 4 结束时两条链接均已建立, 清单仍带 `on_conflict = "replace_and_backup"`, 且有一个备份残留; 本场景**后半段需重置清单** (见该场景的起始状态) |
  | 6 | 同场景 5 结束 (两条链接均已建立, `$HOME/.bashrc` 为正确链接); 本场景把 `$HOME/.config/nvim` 换成含 `keep` 的真实目录 |
  | 7 | **需重置**: 清单复原为 2 条, 两条链接重建 |
  | 8 | 场景 7 情形 C 修好了 `$HOME/.bashrc`; 清单被换成含重复目标路径的非法清单 |
  | 9 | **需重置**: 清单复原为 2 条, 并删掉 `$HOME/.config/nvim` 后重建 (见该场景的起始状态) |
  | 10 | **需重置**: 清单复原为 2 条, 删掉 `$HOME/.bashrc` 后重建 |
  | 11 | 场景 10 结束时两条链接均为本工具创建 (其中 `$HOME/.bashrc` 的链接文本含 `..`) |
  | 12 | **需重置**: 清单复原为 2 条, 两条路径删除后重新部署 |
  | 13 | 与场景 12 结束时相同 (本场景只跑 `cargo check`, 不碰仓库) |
  | 14 | 场景 12 末尾的 `preview || apply` 已把两条链接重新部署好; 本场景换成只含一个源缺失条目的清单 |

  累积有三类, 都必须重置:

  1. **文件系统状态**: 场景 6 留下两个偏离 (真实目录 `$HOME/.config/nvim` 与普通文件 `$HOME/.bashrc`); 场景 12 的两次 `undo` 会把两条链接都移除, 并留下 `$HOME/.zshrc` 从未存在.场景 1 与场景 2 建立的 `$HOME/.bashrc` 链接在场景 3 之后依然存在, 因此场景 4 必须先删除它才能模拟"已有手工配置".
  2. **清单内容**: 场景 3, 8, 9 与 14 会**整体改写** `dotlink.toml`.它们的残留会直接改变后续场景的实际行为与退出码: 50 条清单使 `~/.bashrc` 不再是被管理的条目 (场景 4 的 `conflict` 断言不成立), 非法清单使其退 2, 源缺失条目使**四条命令一律退 1** (FR-020 条件 2), 因而任何"期望 0"的断言都不可能成立.
  3. **备份文件**: 场景 4 的 `replace` 会留下 `$HOME/.bashrc.dotlink-backup.*`.它本身不改变任何判定, 但会混进后续场景的 `find "$HOME" | sort` 比对与目录列举, 也会使"备份是否存在"这类判据失去意义 (场景 5 后半段因此改为只检测"前后是否新增"备份); 场景 12 的起始状态已显式清掉它.

  因此**场景 4, 5 (后半段), 7, 9, 10 与 12 在其开头显式重置清单与文件系统**; 它们都已在场景开头写出重置命令, 直接整段复制执行即可, 手工分步执行时务必不要跳过.

---

## 场景 1 (SC-001, US1): 一条命令完成部署

```sh
cat > dotlink.toml <<'EOF'
[[entry]]
source = "shell/bashrc"
target = "~/.bashrc"

[[entry]]
source = "nvim"
target = "~/.config/nvim"
EOF

dotlink apply; echo "exit=$?"        # 期望 exit=0, 2 个条目: 2 成功, 0 已是最新, 0 未处理, 0 失败
ls -l "$HOME/.bashrc"                # 期望: 符号链接 -> /private/tmp/dotlink-demo/repo/shell/bashrc
ls -l "$HOME/.config/nvim"           # 期望: 符号链接 -> /private/tmp/dotlink-demo/repo/nvim
cat "$HOME/.bashrc"                  # 期望: export EDITOR=vim
```

**证明**: 未出现任何手工 `ln -s`; 主目录下缺失的 `.config` 已被自动创建 (FR-005).

## 场景 2 (SC-002): 幂等

```sh
dotlink apply; echo "exit=$?"
```

**期望**: 两行均为 `ok`; 退出码 0; `$HOME` 下无内容变更. 验证方式:

```sh
before=$(find "$HOME" | sort)
dotlink apply >/dev/null; echo "exit=$? (期望 0: 2 个条目: 0 成功, 2 已是最新, 0 未处理, 0 失败)"
after=$(find "$HOME" | sort)
[ "$before" = "$after" ] && echo "无新增文件"
find "$HOME" -cnewer dotlink.toml -type f | grep -q . && echo "有变更 (不应出现)" || echo "无变更"
```

注意这里必须用 `find -cnewer` 而非 `-newer`: `-newer` 比较的是 mtime, 而 `apply` 首次运行时创建 `.bashrc` 等条目会更新 `$HOME` 目录自身的 mtime, 使 `-newer` 恒有输出. `-cnewer` 比较 inode 变更时间, 且只检查 `-type f` 的真实文件 (排除目录本身), 才能正确验证"没有新文件产生".

## 场景 3 (SC-003): 性能

```sh
mkdir -p "$D/repo/bulk"; for i in $(seq 1 50); do echo x > "$D/repo/bulk/f$i"; done
{ for i in $(seq 1 50); do printf '[[entry]]\nsource = "bulk/f%s"\ntarget = "~/.bulk/f%s"\n\n' "$i" "$i"; done; } > dotlink.toml
time dotlink preview    # 期望 < 2s, 50 行逐条目结果 + 1 行末尾汇总
time dotlink apply      # 期望 < 2s, 同样 50 行逐条目结果 + 1 行末尾汇总

# 收尾: 卸掉这 50 条链接并删除源文件与空的 ~/.bulk, 使后续场景看到的 $HOME 不被污染
dotlink undo >/dev/null
rm -rf "$HOME/.bulk" "$D/repo/bulk"
```

## 场景 4 (SC-004, US3): 不静默丢弃已有内容

**起始状态**: 场景 3 把清单换成了 50 条性能清单 (已在场景 3 末尾 `undo` 并复原), 且 `$HOME/.bashrc` 目前仍是场景 1 建立的正确链接.必须先复原清单并清掉该链接, 否则下面的 `conflict` 断言不可能出现:

1. `printf > "$HOME/.bashrc"` 会**穿过符号链接**写进仓库源文件 `shell/bashrc`, 而链接本身保持正确, 该条目仍是 `ok` (甚至污染仓库);
2. 50 条清单中根本没有 `~/.bashrc` 这一条目.

```sh
cat > dotlink.toml <<'EOF'
[[entry]]
source = "shell/bashrc"
target = "~/.bashrc"

[[entry]]
source = "nvim"
target = "~/.config/nvim"
EOF

rm -f "$HOME/.bashrc"
printf 'my precious notes\n' > "$HOME/.bashrc"     # 模拟已有手工配置

dotlink apply; echo "exit=$?"
```

**期望**: `1/2` 行为 `conflict` (说明含 "on_conflict = skip"), `2/2` 行为 `ok` (上一场景的链接仍在), 汇总为 `2 个条目: 0 成功, 1 已是最新, 1 未处理, 0 失败`, 退出码 1; `$HOME/.bashrc` **内容未变**.
随后允许备份:

```sh
# 重写清单, 给第一个 entry 加上 on_conflict = "replace_and_backup"
cat > dotlink.toml <<'EOF'
[[entry]]
source = "shell/bashrc"
target = "~/.bashrc"
on_conflict = "replace_and_backup"

[[entry]]
source = "nvim"
target = "~/.config/nvim"
EOF

dotlink apply; echo "exit=$?"
```

注意不要用 `sed -i.bak 's|...|<内容含 \\n>|'` 来做这件事: `sed` 的替换串里 `\n` 的展开行为依实现而异, 且 `-i.bak` 在 GNU sed 下会被解析为"`-i` 的脚本是 `.bak`"而非"备份后缀为 `.bak`", 结果不是预期的改文件. 需要一份新清单时直接重写整个文件最稳.

**期望**: `1/2` 行为 `replace` 且说明中给出备份文件的完整路径; 该路径内容为 `my precious notes`; `$HOME/.bashrc` 已成为指向 `/private/tmp/dotlink-demo/repo/shell/bashrc` 的链接; 汇总为 `2 个条目: 1 成功, 1 已是最新, 0 未处理, 0 失败`, 退出码 0.

## 场景 5 (SC-006, US2): 预览与实际一致, 且只读

```sh
before=$(find "$HOME" | sort)
dotlink preview; echo "exit=$?"
after=$(find "$HOME" | sort)
[ "$before" = "$after" ] && echo "文件系统未被改动"
```

**期望**: 两行均为 `ok` (与场景 4 结束时"两条都已部署"这一实际状态逐条对应), 退出码 0; 文件系统未被改动.

上面这一半是平凡情形. SC-006 的实质断言是"未部署状态下 `preview` 预测的动作 == 随后 `apply` 实际执行的动作", 按下面三步做.

**起始状态**: 场景 4 末尾把清单换成了带 `on_conflict = "replace_and_backup"` 的版本, 且那次 `apply` 留下了一个备份文件 `$HOME/.bashrc.dotlink-backup.*`. 两部分残留都必须在动手前处理:

1. **清单残留**: 若沿用场景 4 末尾那份清单, 下面的"制造冲突"会变成 `replace` (该条目声明了 `replace_and_backup`) 而非本段注释所称的 `conflict`, 期望与实况不符. 因此先整份重写清单为不含 `on_conflict` 的版本 (与场景 4 开头同一手法).
2. **备份残留**: 上面那次 `replace` 留下的旧备份会让"preview 未产生备份"的检查失效 -- 该检查不能再以"备份是否存在"为判据 (旧备份本来就存在). 改为只检测 `preview` **前后是否新增**了备份文件.

```sh
# 0. 重置清单: 去掉 on_conflict, 使 bashrc 条目回到默认的 skip
cat > dotlink.toml <<'EOF'
[[entry]]
source = "shell/bashrc"
target = "~/.bashrc"

[[entry]]
source = "nvim"
target = "~/.config/nvim"
EOF

# 1. 把 nvim 条目退回未部署, 并给 bashrc 条目制造一个冲突 (普通文件占位)
rm -rf "$HOME/.config/nvim"
mv "$HOME/.bashrc" "$HOME/.bashrc.moved"      # 先移开, 避免穿过链接写入仓库源
printf 'preview probe\n' > "$HOME/.bashrc"

# 2. preview 预测: nvim -> create, bashrc -> conflict
before=$(find "$HOME" | sort)
backups_before=$(find "$HOME" -maxdepth 1 -name '.bashrc.dotlink-backup.*' | sort)
preview_out=$(dotlink preview); echo "exit=$?"   # 期望: create + conflict, 退出码 1
printf '%s\n' "$preview_out"
[ "$before" = "$(find "$HOME" | sort)" ] && echo "文件系统未被改动"
backups_after=$(find "$HOME" -maxdepth 1 -name '.bashrc.dotlink-backup.*' | sort)
[ "$backups_before" = "$backups_after" ] && echo "preview 未产生新备份" || echo "preview 产生了备份 (不应出现)"

# 3. apply 的实际结果必须与上面逐条一致: nvim 被 create, bashrc 仍 conflict (skip)
apply_out=$(dotlink apply); echo "exit=$?"       # 期望: create + conflict, 退出码 1
printf '%s\n' "$apply_out"

# 逐条比对: 取两边的 token 列 (第 1 列是 <序号>/<总数>, 第 2 列是 token), 必须完全一致
pt=$(printf '%s\n' "$preview_out" | grep -E '^[0-9]+/[0-9]+ ' | awk '{print $1, $2}')
at=$(printf '%s\n' "$apply_out"   | grep -E '^[0-9]+/[0-9]+ ' | awk '{print $1, $2}')
[ "$pt" = "$at" ] && echo "SC-006 通过: preview 与 apply 逐条一致" || { echo "SC-006 失败"; printf 'preview:\n%s\napply:\n%s\n' "$pt" "$at"; }

# 复原 `$HOME/.bashrc` 为正确链接 (清单已在第 0 步重置为不含 on_conflict 的版本, 保持该重置版)
rm -f "$HOME/.bashrc"
mv "$HOME/.bashrc.moved" "$HOME/.bashrc"
```

**注意**: 上面第 0 步的重置清单正是为了让第 1 步的"冲突"名副其实. 由于 `$HOME/.bashrc` 被换成普通文件且该条目为默认的 `skip`, `1/2` 在 `preview` 与 `apply` 下都应为 `conflict` (而 `2/2` 都应为 `create`), 二者退出码均为 1, 且 `apply` 不会产生任何新备份 (这也顺带证明 `conflict` 路径确实不碰文件系统). 场景 4 留下的一份旧备份 `$HOME/.bashrc.dotlink-backup.*` 仍在原处, 它不参与本次比对 (比对只看"有没有新增"), 但仍是"备份文件"这类累积残留的一部分.

## 场景 6 (SC-005, US4): 撤销不触碰外部路径

```sh
dotlink undo; echo "exit=$?"            # 期望: 第一行 restore 并给出恢复来源
cat "$HOME/.bashrc"                     # 期望: 回到 my precious notes (来自场景 4 留下的那份备份)

# 反例: 把目标换成非本工具创建的真实目录
# 注意: 此处不能用 `rm "$HOME/.config/nvim"` -- 该路径是 dotlink 创建的, 指向目录的符号链接.
# 用 `-f` 是为了容忍"路径不存在" (本场景不依赖它一定存在), 不是因为它会被拒绝 --
# 实测 (Darwin 27.0.0, /bin/rm 即 BSD rm) 不带 `-f` 也能删除目录链接本身 (只删链接, 不跟随), `-f` 因此只是更稳妥.
rm -f "$HOME/.config/nvim"
mkdir -p "$HOME/.config/nvim" && echo keep > "$HOME/.config/nvim/keep"
dotlink undo; echo "exit=$?"
cat "$HOME/.config/nvim/keep"           # 期望: 内容仍在, 目录未被删除
ls "$HOME/.config/nvim"                 # 期望: 只有 keep, 目录未被删除
```

**期望**: 第一次 `undo` 的 `1/2` 为 `restore` (附备份来源), `2/2` 为 `undo`; 第二次 `undo` 的 `2/2` 该条目标记为 `skip` 并说明原因 (占用物是目录), 汇总为 `2 个条目: 0 成功, 0 已是最新, 2 未处理, 0 失败`, 退出码 1 -- **条件 2 单独生效**: 目标谓词 (该路径上没有本工具的链接) 已成立, 但 `skip` 使退出码为 1.

第二次 `undo` 的 `1/2` 此时是 `skip` 而不是 `ok`: 第一次 `undo` 已把 `$HOME/.bashrc` 从本工具的链接**恢复成普通文件** (内容来自备份, 见上一行的 `cat`), 它是 `ForeignFile` 而非 `Absent`, 因此 `undo` 只 `skip` 并说明占用物是普通文件, 绝不删除. 这与 `contracts/cli.md` 的 "`undo` 分支" 表逐字一致: 只有 `Absent` 才归 `ok`. 于是本场景的汇总两个条目都在"未处理"列 (`0 成功, 0 已是最新, 2 未处理, 0 失败`), 而 `2/2` 的 `skip` (占用物是**目录**) 依然是"条件 2 单独生效"的证明 -- 该条目的目标谓词已成立 (该路径上确实没有本工具的链接), 却因 `skip` 使退出码为 1.

## 场景 7 (US5): 发现不一致, 且区分"我的链接指错了"与"这是别人的链接"

**起始状态**: 场景 6 留下了两个偏离 -- `$HOME/.config/nvim` 被换成含 `keep` 的真实目录 (用 `rm -rf` 清), `$HOME/.bashrc` 被 `undo` 从本工具的链接恢复成含 `my precious notes` 的普通文件 (用 `rm -f` 清). 二者都必须重置, 然后由 `apply` 重建两条链接:

注意 `$HOME/.bashrc` 此刻是**普通文件**, 对它用 `-rf` 与用 `-f` 效果相同; 但**不可省略 `rm`** 而直接 `apply` -- 该条目会按默认的 `skip` 报 `conflict` 并退 1, 使下面的"期望 0"不成立.

```sh
rm -rf "$HOME/.config/nvim"
rm -f  "$HOME/.bashrc"
dotlink apply >/dev/null
dotlink status; echo "exit=$?"           # 期望 exit=0 (起始状态已完全符合期望)
```

情形 A -- 链接被手工改成**仓库之外**的目标 (`foreign`, 非本工具创建):

```sh
ln -sfn /etc/hosts "$HOME/.bashrc"
dotlink status; echo "exit=$?"
```

**期望**: `1/2` 为 `foreign`, 说明中给出实际指向 `/etc/hosts` 并指明占用物是符号链接; `2/2` 为 `ok`; 退出码 1.

情形 B -- 链接仍落在**仓库内**但指向的源不对 (`wrong`, 本工具创建的链接配置有误):

```sh
ln -sfn "$D/repo/shell/other" "$HOME/.bashrc"
dotlink status; echo "exit=$?"
```

**期望**: `1/2` 为 `wrong` 而非 `foreign`, 说明中给出实际指向 `/private/tmp/dotlink-demo/repo/shell/other`; `2/2` 为 `ok`; 退出码 1.

情形 C -- 恢复正确链接后回到一致:

```sh
ln -sfn "$D/repo/shell/bashrc" "$HOME/.bashrc"
dotlink status; echo "exit=$?"           # 期望 exit=0: 2 个条目: 0 成功, 2 已是最新, 0 未处理, 0 失败
```

**期望**: 情形 A 与 B 必须给出**不同的** token (`foreign` vs `wrong`), 且只有 B 会被 `undo` 视为可移除. 若实现把两者都报成同一个 token, 则本场景失败.

## 场景 8 (FR-013): 校验失败时零改动

```sh
cat > dotlink.toml <<'EOF'
[[entry]]
source = "shell/bashrc"
target = "~/.bashrc"

[[entry]]
source = "shell/zshrc"
target = "~/.bashrc"
EOF

before=$(find "$HOME" | sort)
dotlink apply; echo "exit=$?"           # 期望 exit=2, 指出重复目标路径的两个条目
[ "$before" = "$(find "$HOME" | sort)" ] && echo "零改动"
```

另需验证: 缺少 `target` 字段,`on_conflict = "yes"`,未知键名,TOML 语法错误 (各期望退出码 2).

## 场景 9 (Edge Cases): 源缺失与整目录链接

**起始状态**: 场景 8 把清单换成了含重复目标路径的非法清单, 且 `$HOME/.config/nvim` 目前**仍是**场景 7 起始状态建立的正确链接. 本场景要证明"目录条目被**新建**为整目录软链接", 因此必须先把该链接删掉, 使它确为 `Absent`; 否则它会以 `ok` 出现, 下面的 `create` 断言不成立, FR-018 也就没被证明:

```sh
cat > dotlink.toml <<'EOF'
[[entry]]
source = "shell/nope"
target = "~/.nope"

[[entry]]
source = "nvim"
target = "~/.config/nvim"
EOF

rm -rf "$HOME/.config/nvim"          # 退回未部署, 使下面的 create 断言成立
dotlink apply; echo "exit=$?"
```

**期望**: `1/2` 为 `missing` (不创建链接, 且 `$HOME/.nope` 不存在), `2/2` 为 `create` (目录条目被**整目录**软链接), 汇总为 `2 个条目: 1 成功, 0 已是最新, 0 未处理, 1 失败`, 退出码 1 —— 单条目失败不阻塞其余 (FR-014). 证明 FR-018 的是 `2/2` 行的 `create` 与紧随的断言:

```sh
[ -L "$HOME/.config/nvim" ] && echo "是符号链接 (整目录链接)"
ls "$HOME/.config/nvim/init.lua"     # 期望: 能读到 (链接穿透到仓库目录)
[ ! -e "$HOME/.nope" ] && echo "源缺失条目未创建链接"
```

若实现逐文件建链而非整目录链接, `[ -L "$HOME/.config/nvim" ]` 与 `ls "$HOME/.config/nvim/init.lua"` 必有一条失败.

## 场景 10 (R2 陷阱): 归属判定在规范化路径下仍正确

macOS 上 `/tmp` 是指向 `/private/tmp` 的符号链接, 因此上面所有场景的 `$D` 已经落在规范化差异之下.额外验证显式的 `..` 形态:

**起始状态**: 场景 9 把清单改写成了含 `source = "shell/nope"` 的一份 (故意源缺失), 而 `SourceMissing` 恒使退出码为 1 (FR-020 条件 2). 因此必须先复原清单, 且**确认两条链接都在位**, 否则下面的"期望 0"不可能成立:

```sh
cat > dotlink.toml <<'EOF'
[[entry]]
source = "shell/bashrc"
target = "~/.bashrc"

[[entry]]
source = "nvim"
target = "~/.config/nvim"
EOF

dotlink apply >/dev/null              # 期望 0 (前提见下方说明: 两个条目均已处于期望状态)
dotlink status; echo "exit=$?"        # 期望 0 (起始状态已完全符合期望)
```

上面的 `apply` 之所以能退 0, 是因为**场景 9 结尾**留下了这样两个事实: (1) `$HOME/.bashrc` 仍是**场景 7 情形 C** 建立的正确链接 (`ln -sfn "$D/repo/shell/bashrc"`, 未受场景 8 / 9 影响 -- 场景 8 退 2 零改动, 场景 9 只 `rm -rf` 了 `.config/nvim`); (2) `$HOME/.config/nvim` 是场景 9 的 `rm -rf` 后由那次 `apply` **重建**的整目录链接. 二者都已处于期望状态, 故该条目的 `Action` 为 `Noop` 而非 `conflict`.

若**漏掉**场景 9 的起始状态 (即没有 `rm -rf "$HOME/.config/nvim"`), `2/2` 在场景 9 会是 `ok` 而不是 `create`, 那一次 `apply` 照旧退 0; 后果只是 FR-018 未被证明, 不影响本场景的退出码. 场景 7 起始状态中被删掉的 `$HOME/.bashrc` 之所以能回到正确链接, 是因为场景 7 起始状态的 `apply` 把它建了起来, 而情形 C 又用 `ln -sfn` 覆盖过它 -- 这是上一版文档把成因归给场景 7 `apply` 的错误之处.

```sh
# 用含 ".." 的链接文本替换正确链接. 必须带 -f/-n:
# 目标路径已存在, 不带 -f 的 `ln -s` 会以 "File exists" 失败且不改动链接 (实测), 使下面的断言变成假阳性.
mkdir -p "$D/repo/alias"
ln -sfn "$D/repo/alias/../shell/bashrc" "$HOME/.bashrc"
readlink "$HOME/.bashrc"          # 期望: 路径下含 "/alias/../shell/bashrc"
# 先断言"链接文本确实含 ..", 否则本场景会假阳性通过 (若上一步的 ln 失败, 链接仍是正确的那条, status 报 ok 但什么都没验证):
link=$(readlink "$HOME/.bashrc")
case "$link" in *'/..'*) echo "链接文本确含 ..";; *) echo "本场景无效: 链接未被替换";; esac
dotlink status; echo "exit=$?"    # 期望: 1/2 为 ok, 退出码 0 (含 ".." 的链接文本仍被识别为本工具创建)
```

若实现只做字面前缀比较 (比较 `.../repo/alias/../shell/bashrc` 与 `.../repo/shell/bashrc`), 此条必然误判为 `wrong` 并退 1. 本场景同时覆盖 `research.md` R2 的第二类陷阱: `link_is_inside_repo` 的归属判定也必须先规范化再比较, 否则该链接会被判为"落在仓库外".

`rm -f "$HOME/.bashrc"` 后再用 `dotlink apply` 也能回到一致状态; 但本场景结束后**不要**这样做, 因为场景 11 会以 `$HOME/.bashrc` 的链接文本含 `..` 为前提.

## 场景 11 (FR-015): 清单只在当前目录, 不向上搜索

```sh
mkdir -p "$D/repo/sub" && cd "$D/repo/sub"
before=$(find "$HOME" | sort)
dotlink apply; echo "exit=$?"       # 期望 exit=2, 说明当前目录下找不到 dotlink.toml
dotlink status; echo "exit=$?"      # 期望同样是 2, 且未触碰文件系统
[ "$before" = "$(find "$HOME" | sort)" ] && echo "零改动"

# 当前工作目录本身不可用 (FR-015 的另一条边界): 在子目录中先删掉自己, 再运行
mkdir -p "$D/gone" && cd "$D/gone" && rm -rf "$D/gone"
dotlink status; echo "exit=$?"      # 期望 2 (校验失败), 说明指出"当前工作目录不可用"而非"找不到清单"
cd "$D/repo"
dotlink status; echo "exit=$?"      # 期望 0
```

**证明**: 即使父目录存在 `dotlink.toml`, 也不会被使用.用户的心智模型只包含"存放 `dotlink.toml` 的那个目录 + 该目录下的内容 + 清单声明的目标路径".

注意本场景的 `cd "$D/repo/sub"` 之后不要用 `cd -P`, 因为要验证的正是"用户站在哪"这一状态; 另外上面"当前工作目录已被删除"这一段在 macOS 上的效果依赖 shell: `cd` 到已删除目录后, 某些 shell 会在下一次提示符或命令时自动回到有效目录, 使命令意外地照常运行. 若你的 shell 如此, 改用 `( cd "$D/gone" && exec dotlink status )` 或直接跳到断言标准错误的内容, 不必强求复现.

## 场景 12 (FR-020, SC-009): 退出码语义

**起始状态**: 需重置为"清单完全符合期望"的状态. 场景 9 与场景 11 在 `$HOME` 下留下的东西有: `$HOME/.config/nvim` 为正确链接; `$HOME/.bashrc` 的链接文本含 `..` (场景 10 留下), 且场景 11 的 `cd "$D/repo/sub"` 未改动它; 另有一个由场景 9 的 `missing` 条目声明, 从未被创建的 `$HOME/.nope`. 下面的 `undo` 断言要求状态可预测, 因此先复原清单, 删掉这两条路径, 再重新部署:

```sh
cd "$D/repo"
cat > dotlink.toml <<'EOF'
[[entry]]
source = "shell/bashrc"
target = "~/.bashrc"

[[entry]]
source = "nvim"
target = "~/.config/nvim"
EOF

rm -rf "$HOME/.config/nvim"
rm -f "$HOME/.bashrc"
rm -f "$HOME"/.bashrc.dotlink-backup.* "$HOME"/.config/nvim.dotlink-backup.* "$HOME"/.nope.dotlink-backup.*
dotlink apply  >/dev/null
dotlink status;  echo "status=$?"    # 期望 0: 清单与实际完全一致
dotlink preview; echo "preview=$?"   # 期望 0: 无需执行任何动作

# undo 的目标与 apply 相反: 它是"不再有本工具创建的链接"
dotlink undo;    echo "undo=$?"      # 期望 1: 尚有链接待撤 (两行均为 undo 或 restore)
dotlink undo;    echo "undo=$?"      # 期望 0: 已撤销干净 (两行均为 ok, 幂等)

# 上面的两次 undo 已移除全部链接 ($HOME/.bashrc 与 $HOME/.config/nvim 均不存在),
# 因此这里无需再手工删除任何路径: 两个条目此时都是 Absent -> 有活要干.
dotlink preview; echo "preview=$?"   # 期望 1: 有活要干
dotlink status;  echo "status=$?"    # 期望 1: 存在偏离
dotlink preview || dotlink apply     # 期望: preview 退出 1, 故 apply 被执行
```

**关键**: 同一份"已部署好"的清单上 `apply`/`preview` 退 0, 而 `undo` 退 1 -- 二者目标相反, 这是预期而非缺陷.

在 `set -e` 脚本中的推荐写法 (避免裸调只读命令导致中断):

```sh
if dotlink preview >/dev/null; then echo "无需部署"; else dotlink apply; fi
if dotlink undo    >/dev/null; then echo "无需撤销"; else dotlink undo; fi
```

## 场景 13 (FR-019): 平台门禁

```sh
rustup target add x86_64-pc-windows-gnu      # 若该 target 尚未安装
cargo check --target x86_64-pc-windows-gnu   # 期望: 通过 (证明非 unix 分支可编译)
```

**必须用 `cargo check` 而非 `cargo build`**: 链接 Windows 二进制需要 mingw-w64 工具链 (`x86_64-w64-mingw32-dlltool` 等), 本机 (实测) 未安装, `cargo build` 会以 `error calling dlltool` 失败 -- 那是环境缺工具, 不是代码问题. `cargo check` 只做类型与借用检查, 已足够证明"非 unix 分支的源码在 Windows 上可编译", 即 `os/unix.rs` 的 `#[cfg(unix)]` 与空实现分支划分正确.

Windows 上的**运行时**行为无法在本机验证, 需在 Windows 环境或 CI 中确认: 退出码 3,标准输出为空,标准错误含平台说明,文件系统未改动.

---

## 场景 14 (FR-020 条件 2): 源缺失让四条命令一律退出 1

`contracts/cli.md` 的退出码规则由两个条件合成: **目标谓词成立** 且 **无遗留待办**. 本场景专门验证第二个条件单独生效 -- 源缺失条目计入汇总的"失败"列, 因此即使某条命令的目标谓词在它身上已成立, 也必须退 1 而非 0.

**起始状态**: 场景 12 末尾的 `dotlink preview || dotlink apply` 已把两条链接重新部署好 (场景 13 只跑 `cargo check`, 不改仓库). 本场景把清单整体改写为只含一个源缺失条目; 由于 `SourceMissing` 的判定优先于目标侧的一切 (源不存在时无从谈起"已是最新"或"冲突"), 该条目在四条命令下一律为 `missing`:

```sh
cat > dotlink.toml <<'EOF'
[[entry]]
source = "shell/zshrc"
target = "~/.zshrc"
EOF

dotlink preview; echo "preview=$?"   # 期望 1: 该条目不入"成功"也不入"已是最新"
dotlink status;  echo "status=$?"    # 期望 1: 输出 missing
dotlink apply;   echo "apply=$?"     # 期望 1: 输出 missing, 且不创建任何链接
dotlink undo;    echo "undo=$?"      # 期望 1: 目标谓词已成立 (无我的链接), 但仍有遗留待办
[ ! -e "$HOME/.zshrc" ] && echo "未创建链接"
```

**关键**: 本例的判别点是**第一条命令** `preview`. `SourceMissing` 是**终态** (`data-model.md` 3.4), 它无法被任何命令的动作消除 -- `preview` 无从 dispatch, `apply` 无从修复, `undo` 也没有链接可撤. 因此若实现把"目标谓词"算成"所有条目都无需动作"(把 `missing` 也算作"无需动作"), `preview` 会给 0, 与本行期望的 1 不符. 正确实现按 `data-model.md` 3.2a: `SourceMissing` **不参与目标谓词计数**, 其残留待办只由条件 2 表达, 于是四条命令一律退 1.

同理, 场景 6 第二次 `undo` 的 `skip` 情形 (目标谓词成立但有条目未处理) 也必须退 1.

---

## 完成后清理

```sh
rm -rf "$D"; cd /; unset D HOME
```

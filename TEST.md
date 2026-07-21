# dotlink 测试环境方案

## 目标

在不污染主机真实 `$HOME` 与配置文件的前提下，对 `dotlink` 的 `apply`、`status`、`remove` 进行端到端验证。

核心原则：

- 所有测试都在临时沙箱目录内进行。
- 通过环境变量与相对路径让 `dotlink` 把沙箱当作“伪 home”。
- 测试结束后删除沙箱，不留痕迹。

## 沙箱结构设计

```
/tmp/dotlink-test-XXXXXX/
├── repo/                 # 模拟 dotfiles 仓库
│   ├── dotlink.toml      # 测试用配置文件
│   ├── zsh/
│   │   └── .zshrc
│   ├── nvim/
│   │   └── init.lua
│   └── git/
│       └── .gitconfig
└── home/                 # 模拟目标 home 目录
    └── .config/          # 模拟 $XDG_CONFIG_HOME
```

测试运行时：

- `dotlink` 在 `repo/` 目录下执行，因此 `source` 的相对路径基于 `repo/`。
- `target` 指向 `home/` 下的子路径，不会触及真实文件系统。
- 若需要测试 `~` 或 `$HOME` 展开，可在执行前临时导出 `HOME=$SANDBOX/home`。

## 测试脚本方案

使用 Bash 脚本驱动，流程如下：

1. 创建临时沙箱目录。
2. 在沙箱内生成测试用的 `dotlink.toml` 与源文件。
3. 运行 `dotlink status` 验证初始状态为 `missing`。
4. 运行 `dotlink apply` 创建符号链接。
5. 再次运行 `dotlink status` 验证状态为 `linked-correct`。
6. 运行 `dotlink remove` 删除符号链接。
7. 再次验证状态回到 `missing`。
8. 测试 `--dry-run` 与 `--force` 分支。
9. 清理沙箱目录。

脚本应提供以下辅助函数：

- `setup_sandbox`：创建目录结构并写入配置。
- `assert_status <target> <expected>`：读取 `dotlink status` 输出并断言指定 target 的状态。
- `cleanup`：删除沙箱。
- `run_dotlink <args...>`：在 `repo/` 目录下执行 `dotlink`。

## 关键测试用例

| 用例 | 验证点 |
| ---- | ------ |
| 文件链接 | `.zshrc` 这类普通文件能正确创建 symlink |
| 目录链接 | `nvim/` 目录整体链接，不递归内部文件 |
| `~` 展开 | `target = "~/.zshrc"` 能解析到沙箱 home |
| `$VAR` / `${VAR}` 展开 | 如 `$XDG_CONFIG_HOME/nvim` |
| source 相对路径 | 相对于配置文件所在目录解析 |
| 自动创建父目录 | target 上级目录不存在时自动创建 |
| 默认冲突处理 | target 已存在时报错退出，退出码为 3 |
| `--force` 覆盖 | 强制替换已存在的 target |
| `--dry-run` | 只输出不修改文件系统 |
| `remove` 安全删除 | 只删除指向 source 的 symlink，跳过其他文件/链接 |

## 清理策略

- 正常路径：脚本 `trap` 捕获 `EXIT`，自动删除沙箱。
- 调试路径：若设置 `DEBUG=1`，保留沙箱目录并打印路径，便于人工查看。

## 扩展建议

- 可在 GitHub Actions 中运行相同脚本，增加 macOS 与 Ubuntu 两个 runner 验证跨平台行为。
- 对 Rust 项目，推荐把核心逻辑拆为可单元测试的库函数，沙箱脚本作为集成测试补充。

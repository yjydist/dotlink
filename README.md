# dotlink

基于符号链接的 dotfiles 管理工具。通过 TOML 配置文件显式声明每条链接，将 dotfiles 仓库中的配置文件链接到目标位置。

## 特性

- 显式声明，禁止自动推断
- 支持 `~`、`$VAR`、`${VAR}` 路径展开
- 相对 source 路径以配置文件所在目录为基准
- 自动创建目标父目录
- 支持 `--dry-run` 预览和 `--force` 强制覆盖
- 仅支持 Linux / macOS

## 安装

```bash
go install github.com/yjydist/dotlink/cmd/dotlink@latest
```

或从源码构建：

```bash
git clone https://github.com/yjydist/dotlink.git
cd dotlink
go build -o bin/dotlink ./cmd/dotlink
```

## 配置

在 dotfiles 仓库根目录创建 `dotlink.toml`：

```toml
[[link]]
source = "zsh/.zshrc"
target = "~/.zshrc"

[[link]]
source = "nvim"
target = "$XDG_CONFIG_HOME/nvim"

[[link]]
source = "git/.gitconfig"
target = "~/.gitconfig"
```

| 字段 | 类型 | 说明 |
|------|------|------|
| source | string | 源文件/目录路径（相对路径基于配置文件目录） |
| target | string | 目标路径（支持 `~` 和环境变量展开） |

## 命令

### `dotlink apply`

根据配置创建符号链接。

```bash
dotlink apply [--config <path>] [--force] [--dry-run]
```

- `--config <path>`：指定配置文件，默认 `./dotlink.toml`
- `--force`：目标已存在时强制覆盖（不备份）
- `--dry-run`：只打印操作，不修改文件系统

### `dotlink status`

显示每条链接的当前状态。

```bash
dotlink status [--config <path>]
```

### `dotlink remove`

删除由 dotlink 创建的符号链接（不删除源文件）。

```bash
dotlink remove [--config <path>]
```

## 退出码

| 退出码 | 场景 |
|--------|------|
| 0 | 成功 |
| 1 | 通用错误 |
| 2 | 配置解析失败 |
| 3 | 目标冲突（未使用 `--force`） |
| 4 | source 缺失 |

## 开发

```bash
task build    # 编译到 bin/dotlink
task test     # 运行单元测试
task lint     # 运行 golangci-lint
```

## 许可证

MIT

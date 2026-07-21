# dotlink 产品需求文档（PRD）

## 1. 目标与范围

`dotlink` 是一个**自用的 dotfiles 管理工具**，通过**符号链接（symlink）**把分散在 dotfiles 仓库里的配置文件挂到目标位置。

- **平台**：Linux / macOS，不支持 Windows。
- **语言**：Go（标准项目布局，cobra CLI 框架）。
- **链接方式**：只支持软链接，不支持硬链接、复制 fallback。
- **核心原则**：通过配置文件显式声明每条链接，禁止自动推断。

## 2. 配置文件

默认读取当前目录下的 `dotlink.toml`，可通过 `--config <path>` 指定。

### 2.1 文件格式

使用标准 TOML 数组表：

```toml
[[link]]
source = "zsh/.zshrc"
target = "$HOME/.zshrc"

[[link]]
source = "nvim"
target = "$XDG_CONFIG_HOME/nvim"

[[link]]
source = "git/.gitconfig"
target = "$HOME/.gitconfig"
```

### 2.2 字段说明

| 字段   | 类型   | 必填 | 说明                       |
| ------ | ------ | ---- | -------------------------- |
| source | string | 是   | 源文件或源目录路径         |
| target | string | 是   | 目标文件或目标目录路径     |

## 3. 路径解析规则

### 3.1 展开规则

`source` 和 `target` 都支持以下展开：

- `~`：展开为当前用户 home 目录。
- `$VAR`：展开为对应环境变量值。
- `${VAR}`：展开为对应环境变量值。

不支持 `${VAR:-default}` 等默认值语法。

### 3.2 相对路径基准

- 若 `source` 为相对路径，以**配置文件所在目录**为基准解析。
- 若 `source` 展开后为绝对路径，则直接使用。
- `target` 若为相对路径，以**当前工作目录**为基准解析（推荐始终使用 `~` 或绝对路径）。

### 3.3 目标父目录

应用链接时，若 `target` 的上级目录不存在，**自动创建缺失的父目录**。

## 4. CLI 命令

### 4.1 `dotlink apply`

根据配置文件创建符号链接。

```bash
dotlink apply [--config <path>] [--force] [--dry-run]
```

- `--config <path>`：指定配置文件路径，默认 `./dotlink.toml`。
- `--force`：目标已存在时强制覆盖，**不备份**。
- `--dry-run`：只打印将要执行的操作，不真正创建或删除任何文件。

#### 冲突默认行为

当 `target` 已存在且不是指向 `source` 的符号链接时，**默认直接报错退出**，不会修改任何文件。

#### 目录处理

若 `source` 是目录，则在 `target` 处创建指向该目录的符号链接（整体链接），不会递归链接目录内部文件。

### 4.2 `dotlink status`

显示每条链接的当前状态。

```bash
dotlink status [--config <path>]
```

#### 状态定义

| 状态                  | 含义                                           |
| --------------------- | ---------------------------------------------- |
| `linked-correct`      | `target` 是指向 `source` 的符号链接            |
| `linked-elsewhere`    | `target` 是符号链接，但指向别处                |
| `exists-not-link`     | `target` 存在，但不是符号链接                  |
| `missing`             | `target` 不存在                                |
| `source-missing`      | `source` 不存在，无法链接                      |

### 4.3 `dotlink remove`

删除由 `dotlink` 创建的符号链接，只删除 `target` 处的 symlink，**不删除 source 文件**。

```bash
dotlink remove [--config <path>]
```

- 只删除指向 `source` 的符号链接；若 `target` 不是链接或指向别处，跳过并提示。

## 5. 错误与退出码

| 退出码 | 场景                          |
| ------ | ----------------------------- |
| 0      | 成功                          |
| 1      | 通用错误                      |
| 2      | 配置解析失败                  |
| 3      | 冲突导致退出（未使用 `--force`）|
| 4      | source 缺失导致退出           |

## 6. 非目标

以下功能不在 MVP 范围内：

- Windows 支持
- 硬链接、复制 fallback
- 冲突时自动备份
- 模板渲染、加密、条件配置
- `dotlink init`、`dotlink add` 等辅助命令
- 配置文件热重载

## 7. MVP 验收标准

- [x] `dotlink apply` 能根据 `dotlink.toml` 正确创建文件和目录的符号链接。
- [x] 目标已存在时默认报错退出，加 `--force` 后覆盖且不备份。
- [x] `--dry-run` 只输出操作不修改文件系统。
- [x] `dotlink status` 正确显示每条链接的状态。
- [x] `dotlink remove` 只删除指向 source 的符号链接。
- [x] 支持 `~`、`$VAR`、`${VAR}` 展开。
- [x] 相对 source 路径以配置文件目录为基准。
- [x] 在 Linux 和 macOS 上可编译运行。

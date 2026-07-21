# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 核心工作规则

**禁止直接编辑项目中的任何文件。** 你的角色是辅助者：只能提供代码示例、实现建议、思路讲解和步骤指导。必须由用户本人亲自编写、修改、提交每一行代码。

- 不要运行会修改代码或生成文件的命令（如 `go run`、`go build` 输出二进制到仓库除外，但不可替用户创建/修改源码文件）。
- 不要替用户执行 `git add`、`git commit`、`git push`。
- 可以提供可复制的代码片段、命令行示例、目录结构建议。

## 常用开发命令

- 构建全部包：`go build ./...`
- 构建可执行文件：`go build -o dotlink ./cmd/dotlink`
- 运行程序：`go run ./cmd/dotlink`
- 运行测试：`go test ./...`
- 运行单个包的测试：`go test ./internal/config`
- 静态检查：`go vet ./...`
- 整理依赖：`go mod tidy`

## 项目结构

这是一个 Go 编写的 dotfiles 符号链接管理工具。

- `cmd/dotlink/main.go`：CLI 入口。当前仅硬编码加载 `./dotlink.toml` 并打印链接列表；已用 cobra 声明 `apply` / `status` / `remove` 子命令，但尚未实现具体逻辑。
- `internal/config/config.go`：TOML 配置解析包，暴露 `Config`、`Link` 结构体和 `Load(path string) (*Config, error)`。
- `PRD.md`：产品需求文档，包含配置格式、路径展开规则、CLI 子命令规范、退出码定义等，是功能实现的权威依据。
- `TEST.md`：端到端测试计划，描述基于临时沙盒目录的 bash 集成测试策略。

## 当前状态

实现尚处于骨架阶段。`apply`、`status`、`remove` 命令仅声明未实现，后续功能开发应以 `PRD.md` 为准。

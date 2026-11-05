//! 命令行接口: 四个子命令, 无参数, 仅 `-h/--help` 与 `-V/--version`.
//!
//! 帮助文本必须自带退出码语义 (见 `openspec/specs/cli-contract/spec.md` 的"`--help`
//! 文本自带退出码语义"): `clap` 只生成命令与参数, 若不在 `long_about` 里写明退出码
//! 语义, 用户就无法仅凭 `--help` 决定下一步. `about` 只供 `-h` 显示短描述;
//! 退出码语义只承诺在 `--help` (规格也只要求 `--help`), 不在 `-h` 中重复.

use clap::{Parser, Subcommand};

/// 内部命令枚举, 由 `clap` 的解析结果映射而来.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Preview,
    Apply,
    Status,
    Undo,
}

/// `clap` 的 `about` / `long_about` 要求编译期常量, `format!` 不可用; 用 `concat!`
/// 把"本命令的目标谓词"与"四条命令共用的退出码说明"接起来, 使后者只写一次 --
/// 退出码语义是**同一套**, 若为每条命令各写一份措辞就会出现第二个来源.
macro_rules! long_about {
    ($head:literal) => {
        // 每行独立成字面量: 用 `\` 续行会剥掉下一行的前导空白, 使示例失去缩进.
        concat!(
            $head,
            "\n",
            "\n",
            "退出 1 可能表示有待办, 也可能包含跳过或失败. set -e 脚本应保留输出并按退出码分支\n",
            "(见 README); undo 会直接执行撤销, 不要为了检查结果而重复调用:\n",
            "\n",
            "  if dotlink preview; then echo '无需部署'; else\n",
            "    code=$?; case \"$code\" in 1) dotlink apply ;; *) exit \"$code\" ;; esac\n",
            "  fi\n",
            "\n",
            "退出码 0 当且仅当本命令的目标已达成**且**没有遗留待办; 有待办时 (含只读命令) 退 1.\n",
            "退出码 2 有两重含义: '清单校验失败' 或 '调用方式错误 (子命令缺失, 拼错或未知选项)',\n",
            "前者报单行的 'dotlink: ...' 说明, 后者是同时含 'error:' 与 'Usage:' 的 clap 用法说明\n",
            "(具体错误短语可以是 'unrecognized subcommand' 或 'unexpected argument'),\n",
            "二者靠标准错误的内容区分. 退出码 3 表示平台不受支持."
        )
    };
}

#[derive(Debug, Parser)]
#[command(
    name = "dotlink",
    version,
    about = "声明式 dotfile 软链接管理",
    long_about = concat!(
        "声明式 dotfile 软链接管理\n",
        "\n",
        "命令必须在存放 dotlink.toml 的那个目录中运行; 不向上搜索父目录.\n",
        "\n",
        "退出码 (四条命令共用):\n",
        "  0  该命令的目标已完全达成, 且无遗留待办\n",
        "  1  存在未达成目标的条目, 或有被跳过/被拒绝/失败/源缺失的条目\n",
        "  2  清单校验失败, 或调用方式错误 (子命令缺失, 拼错或未知选项), 二者均未做任何改动\n",
        "  3  平台不受支持 (仅支持 macOS 与 Linux)\n",
        "\n",
        "退出码 2 的两重含义靠标准错误的内容区分: 清单校验失败报单行的 'dotlink: ...'\n",
        "说明; 调用方式错误由 clap 给出同时含 'error:' 与 'Usage:' 的用法说明\n",
        "(具体错误短语可以是 'unrecognized subcommand' 或 'unexpected argument').\n",
        "各子命令的 --help 有同样的说明.",
    ),
    disable_help_subcommand = true
)]
struct Cli {
    #[command(subcommand)]
    command: SubCommand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Subcommand)]
enum SubCommand {
    #[command(
        about = "只读: 报告每个条目将会发生什么, 不修改文件系统",
        long_about = long_about!(
            "只读: 报告每个条目将会发生什么, 不修改文件系统\n\
\n\
目标谓词: 没有任何条目需要执行动作 (每个条目的动作均为 Noop). 谓词在动手**之前**\n\
求值, 因此在一份尚未部署的清单上退出 1 -- 语义是 '有活要干', 可直接用作部署的\n\
前置检查."
        )
    )]
    Preview,
    #[command(
        about = "执行: 按清单建立链接 (必要时先备份)",
        long_about = long_about!(
            "执行: 按清单建立链接 (必要时先备份)\n\
\n\
目标谓词: 每个条目的目标路径都已成为指向其源条目的正确链接. 谓词在动手**之后**\n\
求值, 因此与 preview 共用同一套判断却能在尚未部署的清单上退出 0 (同一份清单上\n\
preview 退 1, 二者刻意不同). 单条目失败不中断其余条目."
        )
    )]
    Apply,
    #[command(
        about = "只读: 报告清单与实际状态的不一致",
        long_about = long_about!(
            "只读: 报告清单与实际状态的不一致\n\
\n\
目标谓词: 清单与实际状态完全一致, 没有任何偏离. 不报告动作, 只报告现状."
        )
    )]
    Status,
    #[command(
        about = "撤销: 移除本工具创建的链接, 并恢复备份",
        long_about = long_about!(
            "撤销: 移除本工具创建的链接, 并恢复备份\n\
\n\
目标谓词: 每个条目的目标路径上都不存在由本工具创建的链接. 谓词在动手**之前**\n\
求值, 因此它与 apply 刻意相反 -- 一份刚部署好的清单上 apply 退 0 而本命令退 1;\n\
撤销后目标为空 (无备份可恢复) 时再运行则退 0. 曾恢复备份时, 目标被恢复出的原内容\n\
占用: 普通文件, 目录或仓库外链接报 skip 并退 1; 恢复出的内容本身是仓库内链接时,\n\
按归属规则再次报 restore 并退 1."
        )
    )]
    Undo,
}

/// 解析命令行参数.
///
/// 子命令缺失或拼错由 `clap` 直接拒绝: 它在读取清单之前向标准错误打印用法说明并
/// 退出 2, 且不提供自定义出口, 因此该路径**不**包进 `Error` 枚举.
pub fn parse() -> Command {
    match Cli::parse().command {
        SubCommand::Preview => Command::Preview,
        SubCommand::Apply => Command::Apply,
        SubCommand::Status => Command::Status,
        SubCommand::Undo => Command::Undo,
    }
}

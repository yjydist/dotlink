//! FR-020: 调用方式错误 (与清单校验失败同码 2, 靠标准错误区分).
//!
//! 该文件是 `openspec/specs/cli-contract/spec.md` 的 "退出码 2 的两重含义靠标准错误
//! 区分" 这一要求的可回归断言.
//! 它约束的是命令面 (四个子命令固定), 因此在本阶段即可全绿.

mod support;

use std::io::Read;
use std::process::Stdio;
use std::time::{Duration, Instant};

use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use support::TempEnv;

#[test]
fn missing_subcommand_exits_two() {
    let env = TempEnv::new();
    env.write_manifest("[[entry]]\nsource = \"a\"\ntarget = \"b\"\n");

    env.cmd()
        .assert()
        .code(2)
        .stdout("")
        .stderr(contains("Usage"));
}

#[test]
fn misspelled_subcommand_exits_two() {
    let env = TempEnv::new();
    env.write_manifest("[[entry]]\nsource = \"a\"\ntarget = \"b\"\n");

    env.cmd().arg("statu").assert().code(2).stdout("").stderr(
        contains("error:")
            .and(contains("Usage:"))
            .and(contains("unrecognized subcommand")),
    );
}

#[test]
fn unknown_option_exits_two() {
    let env = TempEnv::new();
    env.write_manifest("[[entry]]\nsource = \"a\"\ntarget = \"b\"\n");

    for args in [
        vec!["preview", "--bogus"],
        vec!["status", "--json"],
        vec!["preview", "--manifest", "dotlink.toml"],
        vec!["status", "--repo", "."],
        vec!["apply", "--home", "/tmp"],
    ] {
        env.cmd().args(&args).assert().code(2).stdout("").stderr(
            contains("error:")
                .and(contains("Usage:"))
                .and(contains("unexpected argument")),
        );
    }
}

/// 调用方式错误必须在读取清单**之前**被拒绝 (`cli-contract` 的 MUST).
///
/// 已有用法错误用例都在合法清单目录运行, 即使实现顺序颠倒也会通过; 这里用"没有
/// 清单"与"非法清单"两种目录, 并断言标准错误里没有清单错误, 才能区分顺序.
#[test]
fn usage_errors_are_rejected_before_the_manifest_is_read() {
    let env = TempEnv::new();

    // 没有 dotlink.toml: 若先读清单, 错误会是"无法读取清单"而不是用法说明.
    let observed = env.cmd().arg("statu").assert().code(2);
    let stderr = String::from_utf8_lossy(&observed.get_output().stderr).into_owned();
    assert!(
        stderr.contains("unrecognized subcommand"),
        "拼错的子命令必须是用法说明: {stderr}"
    );
    assert!(
        !stderr.contains("清单"),
        "用法错误必须先于清单读取: {stderr}"
    );

    // 操作性子命令参数同理: `status --json` 在读取清单之前就被拒绝.
    let observed = env.cmd().args(["status", "--json"]).assert().code(2);
    let stderr = String::from_utf8_lossy(&observed.get_output().stderr).into_owned();
    assert!(
        stderr.contains("unexpected argument"),
        "操作性参数必须是用法说明: {stderr}"
    );
    assert!(
        !stderr.contains("清单"),
        "用法错误必须先于清单读取: {stderr}"
    );

    // 合法清单同理: `status --json` 的用法说明不读取清单, 标准错误只含 clap 用法.
    env.write_manifest("[[entry]]\nsource = \"a\"\ntarget = \"b\"\n");
    let observed = env.cmd().args(["status", "--json"]).assert().code(2);
    let stderr = String::from_utf8_lossy(&observed.get_output().stderr).into_owned();
    assert!(
        stderr.contains("unexpected argument") && stderr.contains("Usage:"),
        "未知选项必须是用法说明: {stderr}"
    );
    assert!(
        !stderr.contains("清单"),
        "未知选项必须先于清单读取: {stderr}"
    );

    // 非法清单同理: 操作性参数在解析清单之前就被拒绝.
    env.write_manifest("this is not valid toml = = =\n");
    let observed = env.cmd().args(["preview", "--json"]).assert().code(2);
    let stderr = String::from_utf8_lossy(&observed.get_output().stderr).into_owned();
    assert!(
        stderr.contains("unexpected argument"),
        "多余参数必须是用法说明: {stderr}"
    );
    assert!(
        !stderr.contains("清单"),
        "用法错误必须先于清单读取: {stderr}"
    );
}

/// `link-deployment` "无全局开关": 覆盖许可只能来自条目级 `on_conflict`, 四条命令都
/// 不接受任何全局强制覆盖参数.
#[test]
fn no_command_accepts_a_global_force_override() {
    let env = TempEnv::new();
    env.write_manifest("[[entry]]\nsource = \"a\"\ntarget = \"b\"\n");

    for command in ["preview", "apply", "status", "undo"] {
        env.cmd()
            .args([command, "--force"])
            .assert()
            .code(2)
            .stdout("")
            .stderr(contains("unexpected argument"));
    }
}

/// `cli-contract` "命令面固定为四个子命令": 不提供指定仓库根或清单位置的选项与
/// 环境变量, 主目录只取 `HOME`. 这些名字出现在环境里也不得改变清单来源与主目录.
#[test]
fn repo_and_manifest_environment_variables_do_not_exist() {
    for (name, value) in [
        ("DOTLINK_MANIFEST", "/nonexistent/dotlink.toml"),
        ("DOTLINK_REPO", "/nonexistent"),
        ("DOTLINK_ROOT", "/nonexistent"),
        ("DOTLINK_HOME", "/nonexistent"),
    ] {
        let env = TempEnv::new();
        env.write_source("a", "content\n");
        env.write_manifest("[[entry]]\nsource = \"a\"\ntarget = \"~/.a\"\n");

        let observed = env.cmd().env(name, value).arg("apply").assert().code(0);
        let stdout = String::from_utf8_lossy(&observed.get_output().stdout).into_owned();
        assert!(
            stdout.contains("create"),
            "{name}={value} 不得改变清单来源或主目录: {stdout}"
        );
        env.assert_symlink_to(&env.home().join(".a"), &env.repo().join("a"));
    }
}

/// `backup-lifecycle` "备份不被自动回收": 命令面不存在清理/回收备份的子命令或参数.
#[test]
fn there_is_no_backup_cleanup_subcommand_or_option() {
    let env = TempEnv::new();
    env.write_manifest("[[entry]]\nsource = \"a\"\ntarget = \"b\"\n");

    for subcommand in ["gc", "clean", "cleanup", "prune"] {
        env.cmd()
            .arg(subcommand)
            .assert()
            .code(2)
            .stdout("")
            .stderr(contains("unrecognized subcommand"));
    }
    for args in [["undo", "--gc"], ["apply", "--prune"]] {
        env.cmd()
            .args(args)
            .assert()
            .code(2)
            .stdout("")
            .stderr(contains("unexpected argument"));
    }

    // 根帮助的子命令集合由 `root_help_lists_exactly_the_four_subcommands` 精确锁定,
    // 这里不再用"若干禁用词不存在"的弱断言重复同一件事.
}

/// 解析根帮助的 `Commands:` 段, 返回其中列出的子命令名.
fn root_help_commands(text: &str) -> Vec<&str> {
    let mut commands = Vec::new();
    let mut in_section = false;
    for line in text.lines() {
        if line.trim_end() == "Commands:" {
            in_section = true;
            continue;
        }
        if !in_section {
            continue;
        }
        // 段落到空行为止; 子命令条目以两个空格加名字开头.
        if line.trim().is_empty() || !line.starts_with("  ") {
            break;
        }
        let name = line.split_whitespace().next().unwrap_or_default();
        commands.push(name);
    }
    commands
}

/// `cli-contract` "命令面固定为四个子命令": 根帮助的 `Commands:` 段必须恰好列出
/// `preview`, `apply`, `status`, `undo`, 不能多也不能少.
#[test]
fn root_help_lists_exactly_the_four_subcommands() {
    let env = TempEnv::new();
    let observed = env.cmd().arg("--help").assert().code(0);
    let text = String::from_utf8_lossy(&observed.get_output().stdout).into_owned();

    assert_eq!(
        root_help_commands(&text),
        vec!["preview", "apply", "status", "undo"],
        "根帮助 Commands 段的子命令集合不符:\n{text}"
    );

    // 解析器本身必须把第五个条目也算进来: 若产品新增子命令, 上面的精确相等断言
    // 就会失败, 而不是因为禁用词列表没有更新而静默通过.
    let with_extra =
        "Commands:\n  preview  x\n  apply  x\n  status  x\n  undo  x\n  gc  x\n\nOptions:\n";
    assert_ne!(
        root_help_commands(with_extra),
        vec!["preview", "apply", "status", "undo"],
        "解析器必须识别出第五个子命令"
    );
}

/// `cli-contract` "命令面固定为四个子命令": clap 的 `help` 子命令被关闭,
/// 不能作为第五个入口; 它必须与拼错的子命令同样以 2 被拒.
#[test]
fn help_subcommand_is_rejected() {
    let env = TempEnv::new();
    env.write_manifest("[[entry]]\nsource = \"a\"\ntarget = \"b\"\n");

    env.cmd()
        .arg("help")
        .assert()
        .code(2)
        .stdout("")
        .stderr(contains("unrecognized subcommand"));
}

#[test]
fn help_and_version_exit_zero_with_output() {
    let env = TempEnv::new();

    env.cmd()
        .arg("-h")
        .assert()
        .code(0)
        .stdout(contains("Usage"));
    env.cmd()
        .arg("-V")
        .assert()
        .code(0)
        .stdout(contains("dotlink"));
}

/// 帮助文本必须是纯文本: 不得含 markdown 强调符, 同时保留退出码语义.
#[test]
fn help_output_contains_no_markdown_emphasis() {
    let env = TempEnv::new();
    for args in [
        vec!["--help"],
        vec!["preview", "--help"],
        vec!["apply", "--help"],
        vec!["status", "--help"],
        vec!["undo", "--help"],
    ] {
        let observed = env.cmd().args(&args).assert().code(0);
        let text = String::from_utf8_lossy(&observed.get_output().stdout).into_owned();
        assert!(
            !text.contains("**"),
            "`{args:?}` 的帮助不得含 markdown 强调符:\n{text}"
        );
        assert!(
            text.contains("退出码"),
            "`{args:?}` 的帮助必须保留退出码语义:\n{text}"
        );
    }
}

#[test]
fn usage_errors_do_not_touch_the_filesystem() {
    let env = TempEnv::new();
    env.write_source("a", "content\n");
    env.write_manifest("[[entry]]\nsource = \"a\"\ntarget = \"b\"\n");

    let before = env.snapshot(env.repo());
    for args in [vec![], vec!["statu"], vec!["preview", "--bogus"]] {
        env.cmd().args(&args).assert().code(2);
    }
    assert_eq!(
        before,
        env.snapshot(env.repo()),
        "调用方式错误不得触碰文件系统"
    );
}

/// openspec/specs/cli-contract/spec.md 的 `--help` 文本自带退出码语义: 退出码 2 的双重
/// 含义必须写进帮助文本.
///
/// 子命令帮助必须同时给出 0 / 1 / 2 / 3 的含义与 `set -e` 脚本建议; 根帮助只需给出
/// 共用标题与两重含义的区分判据. 断言锁定稳定标记, 不钉全文措辞.
#[track_caller]
fn assert_help_spells_out_exit_code_semantics(args: &[&str], full: bool) {
    let env = TempEnv::new();
    let observed = env.cmd().args(args).assert().code(0);
    let text = String::from_utf8_lossy(&observed.get_output().stdout);
    assert!(
        text.contains("退出码"),
        "`{args:?}` 的帮助文本必须含退出码语义说明:\n{text}"
    );
    assert!(
        text.contains("error:") && text.contains("Usage:"),
        "`{args:?}` 的帮助文本必须给出退出码 2 两重含义的区分判据:\n{text}"
    );
    assert!(
        text.contains("dotlink:"),
        "`{args:?}` 的帮助文本必须给出单行清单说明的形态:\n{text}"
    );
    // 缩进必须保留: `\` 续行会吞掉下一行的前导空白.
    if full {
        for needle in ["  if dotlink preview", "    code=$?", "  fi"] {
            assert!(
                text.contains(needle),
                "`{args:?}` 的帮助文本必须保留示例缩进 {needle:?}:\n{text}"
            );
        }
    } else {
        assert!(
            text.contains("  0  "),
            "`{args:?}` 的帮助文本必须保留退出码条目的缩进:\n{text}"
        );
    }
    let needles: &[&str] = if full {
        // 子命令帮助还必须给出按退出码分支的可复制示例 (`case` + `dotlink apply`),
        // 这是"仅凭命令行输出即可判断下一步"的实质部分.
        &[
            "退出码 0",
            "退出 1",
            "退出码 2",
            "退出码 3",
            "set -e",
            "case ",
            "dotlink apply",
        ]
    } else {
        &["退出码 (四条命令共用)", "平台不受支持", "清单校验失败"]
    };
    for needle in needles {
        assert!(
            text.contains(needle),
            "`{args:?}` 的帮助文本必须含 {needle:?}:\n{text}"
        );
    }
}

#[test]
fn root_help_documents_exit_code_semantics() {
    assert_help_spells_out_exit_code_semantics(&["--help"], false);
}

#[test]
fn preview_help_documents_exit_code_semantics() {
    assert_help_spells_out_exit_code_semantics(&["preview", "--help"], true);
}

#[test]
fn apply_help_documents_exit_code_semantics() {
    assert_help_spells_out_exit_code_semantics(&["apply", "--help"], true);
}

#[test]
fn status_help_documents_exit_code_semantics() {
    assert_help_spells_out_exit_code_semantics(&["status", "--help"], true);
}

#[test]
fn undo_help_documents_exit_code_semantics() {
    assert_help_spells_out_exit_code_semantics(&["undo", "--help"], true);
}

/// `link-teardown` "撤销幂等且退出码随备份情形而异": 帮助文本必须区分两种恢复后
/// 情形, 不得再声称"曾恢复备份时一律报 skip". 旧措辞漏掉了"恢复出的内容本身是
/// 仓库内链接"这一类 (实现按归属规则再次报 `restore`).
#[test]
fn undo_help_mentions_the_restore_again_case() {
    let env = TempEnv::new();
    let observed = env.cmd().args(["undo", "--help"]).assert().code(0);
    let text = String::from_utf8_lossy(&observed.get_output().stdout).into_owned();
    assert!(
        text.contains("仓库内链接"),
        "undo 帮助必须区分'恢复出仓库内链接'的情形:\n{text}"
    );
    assert!(
        text.contains("restore"),
        "undo 帮助必须给出再次报 `restore` 的情形:\n{text}"
    );
}

/// `cli-contract` "undo 帮助写明完整目标谓词": `--help` 必须同时给出"不存在本工具
/// 创建的链接"与"没有从备份恢复", 不得只写前半句而让 `Absent + 有备份` 暗示退出 0.
#[test]
fn undo_help_spells_out_the_full_target_predicate() {
    let env = TempEnv::new();
    let observed = env.cmd().args(["undo", "--help"]).assert().code(0);
    let text = String::from_utf8_lossy(&observed.get_output().stdout).into_owned();
    assert!(
        text.contains("不存在本工具创建的链接"),
        "undo 帮助必须写明目标路径上不存在本工具创建的链接:\n{text}"
    );
    assert!(
        text.contains("没有从备份恢复"),
        "undo 帮助必须写明本次没有从备份恢复任何内容:\n{text}"
    );
}

/// 四条命令都不得读取标准输入: 保持 stdin 打开且不提供内容, 命令仍须自行结束.
#[test]
fn commands_do_not_read_standard_input() {
    let env = TempEnv::new();
    env.write_manifest("[[entry]]\nsource = \"a\"\ntarget = \"b\"\n");

    for command in ["preview", "apply", "status", "undo"] {
        let mut child = std::process::Command::new(assert_cmd::cargo::cargo_bin("dotlink"))
            .current_dir(env.repo())
            .env("HOME", env.home())
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("USERPROFILE")
            .arg(command)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("启动 dotlink");
        let mut stderr_pipe = child.stderr.take().expect("取得子进程标准错误");

        // 关键: 既不写入也不关闭 stdin. 等待输入的实现会一直阻塞到这里超时.
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match child.try_wait().expect("等待子进程") {
                Some(status) => {
                    assert_eq!(
                        status.code(),
                        Some(1),
                        "`{command}` 在源缺失的清单上应退 1; 实际 {status:?}"
                    );
                    break;
                }
                None if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                None => panic!("`{command}` 在标准输入保持打开且无内容时阻塞"),
            }
        }

        let mut stderr = String::new();
        stderr_pipe
            .read_to_string(&mut stderr)
            .expect("读取子进程标准错误");
        assert!(
            stderr.is_empty(),
            "`{command}` 不得输出任何交互提示: {stderr}"
        );
    }
}

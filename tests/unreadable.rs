//! FR-011 / `contracts/cli.md` 的 `fail` 行: `Unreadable` 的跨命令一致性 (tasks.md 的 T057).
//!
//! 用 chmod `000` 的父目录构造 `lstat` 的 `EACCES`, 断言**四条命令**下该条目的 token
//! 均为 `fail`, 汇总的"失败"列为 1, 退出码为 **1** (非 2 或 3), 且文件系统零改动.
//!
//! 与 `src/classify.rs` 的单元测试分工: 那里只覆盖 `classify` 的**内部判定** (两次
//! `lstat` 的故障裁决与两侧独立); 这里补的是**跨命令**的一致性: 判定阶段的故障在
//! 四条命令下都必须是同一个 token, 且退出码不得退化成"清单校验失败".
//!
//! 以 root 运行时权限位无效 (chmod 000 不阻止读取), 该断言必须跳过 -- 见
//! `running_as_root`. 跳过时会向标准输出打印提示, 使"跳过"这件事不会被误读为"通过".
//!
//! 本文件经真实二进制断言, 因此依赖 `src/lib.rs` 的命令接入. 在那些接入完成之前,
//! 这里的断言必然失败 -- 这是预期的 TDD 红阶段, 不得为了让本文件变绿而改动 `src/`.

mod support;

use std::fs;
use std::path::{Path, PathBuf};

use support::TempEnv;

/// 四条命令.
const COMMANDS: [&str; 4] = ["preview", "apply", "status", "undo"];

/// 一次命令调用的观测结果.
struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

/// 运行 `dotlink <args...>` (cwd = 仓库根, HOME = 临时主目录).
fn run(env: &TempEnv, args: &[&str]) -> Run {
    let assert = env.cmd().args(args).assert();
    let output = assert.get_output();
    Run {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// 仓库 + HOME 的递归快照 (路径 + 文件内容 / 链接文本).
///
/// `TempEnv` 的 HOME 是仓库**之内**的一个子目录, 因此 `repo()` 与 `home()` 的两次
/// 递归会重叠出一份重复项; 这里按路径去重, 使"前后比对"不受重复影响.
fn snapshot_all(env: &TempEnv) -> Vec<(PathBuf, String)> {
    let mut rows = env.snapshot(env.repo());
    rows.extend(env.snapshot(env.home()));
    rows.sort();
    rows.dedup();
    rows
}

/// 该行是否是逐条目行 (`<序号>/<总数> ...`).
fn is_entry_line(line: &str) -> bool {
    match line
        .split_whitespace()
        .next()
        .and_then(|word| word.split_once('/'))
    {
        Some((index, total)) => index.parse::<usize>().is_ok() && total.parse::<usize>().is_ok(),
        None => false,
    }
}

/// 含 `needle` 的那一行.
#[track_caller]
fn line_for<'a>(stdout: &'a str, needle: &str) -> &'a str {
    stdout
        .lines()
        .find(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("输出中没有含 {needle} 的行:\n{stdout}"))
}

/// 行内的 token (第 2 个字段).
#[track_caller]
fn token_of(line: &str) -> &str {
    line.split_whitespace()
        .nth(1)
        .unwrap_or_else(|| panic!("行内没有 token: {line}"))
}

/// 逐条目行数.
fn entry_line_count(stdout: &str) -> usize {
    stdout.lines().filter(|line| is_entry_line(line)).count()
}

/// 以 root 运行时权限位无效 (chmod 000 不阻止读取), 依赖 `EACCES` 的断言必须跳过.
fn running_as_root() -> bool {
    let probe = std::env::temp_dir().join(format!("dotlink-root-probe-{}", std::process::id()));
    let _ = fs::remove_dir_all(&probe);
    if fs::create_dir_all(&probe).is_err() {
        return false;
    }
    let locked = probe.join("locked");
    let outcome = fs::create_dir_all(&locked)
        .and_then(|()| {
            fs::set_permissions(&locked, std::os::unix::fs::PermissionsExt::from_mode(0o000))
        })
        .and_then(|()| fs::read_dir(&locked).map(|_| ()));
    let as_root = outcome.is_ok();
    let _ = fs::remove_dir_all(&probe);
    as_root
}

/// 就**本次实际用到的那个目录**再确认一次: chmod 000 之后是否真的读不到.
///
/// 比只按 uid 判断更可靠 (容器里 `uid != 0` 而能力集仍可能绕过权限位), 且它验证的
/// 是断言真正依赖的那个目录.
fn permissions_are_ignored(locked: &Path) -> bool {
    fs::read_dir(locked).is_ok()
}

/// 一份"目标父目录不可进入"的清单与被打上 chmod 000 的目录.
///
/// 第一条目标是正常的 (用于断言"其余条目正常处理"), 第二条落在不可进入的目录里.
fn locked_fixture() -> (TempEnv, PathBuf) {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_source("shell/zshrc", "managed-zsh\n");
    env.write_manifest(
        "[[entry]]\nsource = \"shell/bashrc\"\ntarget = \"~/.bashrc\"\n\n\
         [[entry]]\nsource = \"shell/zshrc\"\ntarget = \"~/.locked/rc\"\n",
    );

    let locked = env.home().join(".locked");
    fs::create_dir_all(&locked).expect("创建目标父目录");
    fs::set_permissions(&locked, std::os::unix::fs::PermissionsExt::from_mode(0o000))
        .expect("设置权限位");
    (env, locked)
}

/// 恢复权限位, 使临时目录能被正常清理.
fn unlock(dir: &Path) {
    let _ = fs::set_permissions(dir, std::os::unix::fs::PermissionsExt::from_mode(0o755));
}

// ---------------------------------------------------------------------------
// 四条命令: 该条目 token 均为 fail, 汇总"失败"列为 1, 退出码 1, 零改动
// ---------------------------------------------------------------------------

#[test]
fn an_unreadable_entry_is_fail_under_every_command() {
    if running_as_root() {
        println!(
            "跳过 an_unreadable_entry_is_fail_under_every_command: 当前以 root 运行, chmod 000 无效"
        );
        return;
    }

    for command in COMMANDS {
        let (env, locked) = locked_fixture();

        // 若 chmod 000 在本环境里被绕过, 该用例会退化成"正常处理", 断言无意义.
        if permissions_are_ignored(&locked) {
            unlock(&locked);
            println!(
                "跳过 an_unreadable_entry_is_fail_under_every_command ({command}): 权限位在本环境无效"
            );
            return;
        }

        let before = snapshot_all(&env);
        let run = run(&env, &[command]);
        let after = snapshot_all(&env);

        assert_eq!(
            run.code, 1,
            "`Unreadable` 是判定阶段的环境故障: 退出码必须是 1 (非 2/3);\
             `{command}` 实际退 {};\nstdout:\n{}\nstderr:\n{}",
            run.code, run.stdout, run.stderr
        );
        assert_eq!(
            run.stderr, "",
            "`Unreadable` 不是清单校验失败, 不得走标准错误的拒绝路径 (`{command}`)"
        );

        let line = line_for(&run.stdout, "~/.locked/rc");
        assert_eq!(
            token_of(line),
            "fail",
            "`Unreadable` 的 token 不随命令变化, 四条命令下都是 fail (`{command}`): {line}"
        );
        assert!(
            line.split_whitespace().count() > 3,
            "fail 行必须说明原因 (FR-016) (`{command}`): {line}"
        );

        let summary = run.stdout.lines().last().unwrap_or_default();
        assert!(
            summary.contains("1 失败"),
            "该条目必须计入汇总的 `失败` 列 (`{command}`): {summary}"
        );
        assert_eq!(
            entry_line_count(&run.stdout),
            2,
            "其余条目仍必须被处理并各占一行 (`{command}`):\n{}",
            run.stdout
        );

        // 其余条目按各自命令的正常语义处理 (单条目失败不阻塞其余条目, FR-014).
        let expected_other = match command {
            "preview" | "apply" => "create",
            "status" => "absent",
            _ => "ok",
        };
        assert_eq!(
            token_of(line_for(&run.stdout, "~/.bashrc")),
            expected_other,
            "`{command}` 下正常条目必须照常处理:\n{}",
            run.stdout
        );

        // 逐条目结果必须是"该条目失败, 其余条目成功", 因此 `apply` 与 `undo` 会按
        // 各自语义在**正常条目**上产生改动 -- 本断言的范围因此限定在: 不可进入的
        // 目标路径上不得出现任何东西 (FR-014 的"单条目失败不阻塞其余"的对偶).
        // 按**路径**判定"新出现", 不含内容 (目录的内容是 "dir", 用空串比较会把
        // 所有既有目录都误报成新增).
        let before_paths: Vec<&PathBuf> = before.iter().map(|(path, _)| path).collect();
        let blocked: Vec<&PathBuf> = after
            .iter()
            .map(|(path, _)| path)
            .filter(|path| !before_paths.contains(path))
            .filter(|path| path.starts_with(&locked))
            .collect();
        assert!(
            blocked.is_empty(),
            "`{command}` 不得在不可进入的目标路径上留下任何东西: {blocked:?}"
        );
        assert!(
            fs::read_dir(&locked).is_err(),
            "`{command}` 之后该目录仍必须是不可进入的 (未被改动)"
        );

        unlock(&locked);
    }
}

// ---------------------------------------------------------------------------
// 汇总的"失败"列恰好为 1: 单条目清单
// ---------------------------------------------------------------------------

#[test]
fn the_failure_column_counts_exactly_one_for_a_single_unreadable_entry() {
    if running_as_root() {
        println!(
            "跳过 the_failure_column_counts_exactly_one_for_a_single_unreadable_entry: 当前以 root 运行, chmod 000 无效"
        );
        return;
    }

    let env = TempEnv::new();
    env.write_source("shell/zshrc", "managed-zsh\n");
    env.write_manifest("[[entry]]\nsource = \"shell/zshrc\"\ntarget = \"~/.locked/rc\"\n");

    let locked = env.home().join(".locked");
    fs::create_dir_all(&locked).expect("创建目标父目录");
    fs::set_permissions(&locked, std::os::unix::fs::PermissionsExt::from_mode(0o000))
        .expect("设置权限位");

    if permissions_are_ignored(&locked) {
        unlock(&locked);
        println!(
            "跳过 the_failure_column_counts_exactly_one_for_a_single_unreadable_entry: 权限位在本环境无效"
        );
        return;
    }

    let before = snapshot_all(&env);
    let run = run(&env, &["status"]);
    let after = snapshot_all(&env);
    unlock(&locked);

    assert_eq!(
        run.code, 1,
        "stdout:\n{}\nstderr:\n{}",
        run.stdout, run.stderr
    );
    assert_eq!(token_of(line_for(&run.stdout, "~/.locked/rc")), "fail");
    assert_eq!(
        run.stdout.lines().last().unwrap_or_default(),
        "1 个条目: 0 成功, 0 已是最新, 0 未处理, 1 失败",
        "汇总的 `失败` 列必须恰好为 1"
    );
    assert_eq!(before, after, "零改动");
}

// ---------------------------------------------------------------------------
// NotFound 与 Unreadable 不得混淆 (端到端对照)
// ---------------------------------------------------------------------------

#[test]
fn a_nonexistent_path_is_reported_as_a_deviation_not_as_a_failure() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_manifest(&manifest_of(&[
        ("shell/bashrc", "~/.bashrc"),
        ("shell/gone", "~/.gone"),
    ]));

    let run = run(&env, &["status"]);

    assert_eq!(run.code, 1, "stdout:\n{}", run.stdout);
    // 路径不存在是**确定的文件系统事实**: 目标侧 -> absent, 源侧 -> missing.
    assert_eq!(token_of(line_for(&run.stdout, "~/.bashrc")), "absent");
    assert_eq!(token_of(line_for(&run.stdout, "~/.gone")), "missing");
    assert_eq!(
        run.stdout.lines().last().unwrap_or_default(),
        "2 个条目: 0 成功, 0 已是最新, 1 未处理, 1 失败",
        "缺失的源计入 `失败`, 未部署的目标计入 `未处理`"
    );
}

/// 构造清单: 每个条目为 `(source, target)`.
fn manifest_of(entries: &[(&str, &str)]) -> String {
    entries
        .iter()
        .map(|(source, target)| {
            format!("[[entry]]\nsource = \"{source}\"\ntarget = \"{target}\"\n")
        })
        .collect()
}

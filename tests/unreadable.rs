//! FR-011 / `openspec/specs/cli-contract/spec.md` 的 `fail` 行: `Unreadable` 的跨命令
//! 一致性.
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
//! 末尾另有一条"仅源侧不可读"的用例: 源路径穿过一个普通文件 (lstat 得到 ENOTDIR,
//! 非 NotFound), 目标侧正常. 它不依赖权限位, 因此不需要 root 跳过, 并且覆盖
//! "只有一侧故障"时说明只透出该侧原因 (与"双侧故障"用例互为对偶).
//!
//! 本文件经真实二进制断言, 因此依赖 `src/lib.rs` 的命令接入. 在那些接入完成之前,
//! 这里的断言必然失败 -- 这是预期的 TDD 红阶段, 不得为了让本文件变绿而改动 `src/`.

mod support;

use std::fs;
use std::path::{Path, PathBuf};

use support::{
    TempEnv, entry_line_count, env_with_locked_target, line_for, manifest_of, run, running_as_root,
    snapshot_all, token_of, unlock,
};

/// 四条命令.
const COMMANDS: [&str; 4] = ["preview", "apply", "status", "undo"];

/// 就**本次实际用到的那个目录**再确认一次: chmod 000 之后是否真的读不到.
///
/// 比只按 uid 判断更可靠 (容器里 `uid != 0` 而能力集仍可能绕过权限位), 且它验证的
/// 是断言真正依赖的那个目录.
fn permissions_are_ignored(locked: &Path) -> bool {
    fs::read_dir(locked).is_ok()
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
        let (env, locked) = env_with_locked_target();

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
        ("shell/bashrc", "~/.bashrc", "-"),
        ("shell/gone", "~/.gone", "-"),
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

// ---------------------------------------------------------------------------
// 双侧都读不到: fail 行必须同时给出两侧原因 (不得只透出源侧)
// ---------------------------------------------------------------------------

#[test]
fn a_doubly_unreadable_entry_reports_the_reason_of_both_sides() {
    if running_as_root() {
        println!(
            "跳过 a_doubly_unreadable_entry_reports_the_reason_of_both_sides: 当前以 root 运行, chmod 000 无效"
        );
        return;
    }

    let env = TempEnv::new();
    // 源侧故障: 中间段是普通文件, lstat 报 "Not a directory" (非 NotFound 的硬失败).
    env.write_source("shell/file", "not a directory\n");
    // 目标侧故障: 父目录 chmod 000, lstat 报 "Permission denied".
    env.write_manifest("[[entry]]\nsource = \"shell/file/rc\"\ntarget = \"~/.locked/rc\"\n");

    let locked = env.home().join(".locked");
    fs::create_dir_all(&locked).expect("创建目标父目录");
    fs::set_permissions(&locked, std::os::unix::fs::PermissionsExt::from_mode(0o000))
        .expect("设置权限位");

    if permissions_are_ignored(&locked) {
        unlock(&locked);
        println!(
            "跳过 a_doubly_unreadable_entry_reports_the_reason_of_both_sides: 权限位在本环境无效"
        );
        return;
    }

    let run = run(&env, &["status"]);
    unlock(&locked);

    let line = line_for(&run.stdout, "~/.locked/rc");
    assert_eq!(token_of(line), "fail", "两侧都读不到必须是 fail: {line}");
    let source = env.repo().join("shell/file/rc");
    let target = env.home().join(".locked/rc");
    assert!(
        line.contains(&format!("源侧路径 {}: Not a directory", source.display())),
        "fail 行必须给出源侧路径与原因: {line}"
    );
    assert!(
        line.contains(&format!(
            "目标侧路径 {}: Permission denied",
            target.display()
        )),
        "fail 行必须给出目标侧路径与原因: {line}"
    );
}

// ---------------------------------------------------------------------------
// 仅源侧不可读: 源侧 ENOTDIR, 目标侧正常
//
// 与上一个用例互为对偶: 双侧故障时要求两侧原因都透出; 只有源侧故障时, 说明必须
// 给出源侧原因, 且不得捏造目标侧原因. 故障由"中间段是普通文件"构造 (lstat 得到
// ENOTDIR, 不是 NotFound), 因此不依赖权限位, 也不需要 root 跳过.
// ---------------------------------------------------------------------------

#[test]
fn a_source_side_only_failure_is_fail_under_every_command() {
    for command in COMMANDS {
        let env = TempEnv::new();
        // 源侧故障: 中间段是普通文件, lstat 报 "Not a directory" (非 NotFound 的硬失败).
        env.write_source("shell/file", "not a directory\n");
        // 目标侧正常: 目标路径不存在 (Absent), 其父目录 (HOME) 存在.
        env.write_manifest("[[entry]]\nsource = \"shell/file/rc\"\ntarget = \"~/.bashrc\"\n");

        let target = env.home().join(".bashrc");
        assert!(
            fs::symlink_metadata(&target).is_err(),
            "前置条件: 目标路径必须不存在"
        );

        let before = snapshot_all(&env);
        let run = run(&env, &[command]);
        let after = snapshot_all(&env);

        assert_eq!(
            run.code, 1,
            "源侧环境故障是判定阶段的失败: 退出码必须是 1 (非 2/3);\
             `{command}` 实际退 {};\nstdout:\n{}\nstderr:\n{}",
            run.code, run.stdout, run.stderr
        );
        assert_eq!(
            run.stderr, "",
            "`Unreadable` 不是清单校验失败, 不得走标准错误的拒绝路径 (`{command}`)"
        );

        let line = line_for(&run.stdout, "~/.bashrc");
        assert_eq!(
            token_of(line),
            "fail",
            "仅源侧不可读时 token 不随命令变化, 四条命令下都是 fail (`{command}`): {line}"
        );
        // 说明必须给出源侧路径与原因. 实际拼写为
        // "源侧读取失败: 路径 <source>: Not a directory (...)".
        let source = env.repo().join("shell/file/rc");
        assert!(
            line.contains(&format!(
                "源侧读取失败: 路径 {}: Not a directory",
                source.display()
            )),
            "fail 行必须给出源侧路径与原因 (`{command}`): {line}"
        );
        assert!(
            !line.contains("目标侧"),
            "目标侧正常时不得捏造目标侧的原因 (`{command}`): {line}"
        );

        assert_eq!(
            entry_line_count(&run.stdout),
            1,
            "单条目清单只有一行条目行 (`{command}`):\n{}",
            run.stdout
        );
        assert_eq!(
            run.stdout.lines().last().unwrap_or_default(),
            "1 个条目: 0 成功, 0 已是最新, 0 未处理, 1 失败",
            "`fail` 只能计入汇总的 `失败` 列 (`{command}`)"
        );

        // 目标路径, 其父目录 (HOME) 与备份目录都不得出现任何改动.
        assert!(
            fs::symlink_metadata(&target).is_err(),
            "`{command}` 不得在目标路径上留下任何东西"
        );
        assert_eq!(
            before, after,
            "`{command}` 必须零改动 (含目标路径, 父目录与备份目录)"
        );
    }
}

// ---------------------------------------------------------------------------
// 双侧都读不到 (不依赖权限位): 源侧与目标侧都穿过普通文件, lstat 得到 ENOTDIR
//
// 与上一条双侧故障用例互补: 那条用目标侧 chmod 000 构造 EACCES, root 下必须跳过;
// 这条两侧都用"中间段是普通文件"触发 ENOTDIR, 在 root 下同样可执行, 使"双侧原因
// 都透出"不只在非 root 环境成立.
// ---------------------------------------------------------------------------

#[test]
fn a_doubly_unreadable_entry_is_reported_without_permission_bits() {
    let env = TempEnv::new();
    // 源侧故障: `shell/file` 是普通文件, `shell/file/rc` 的 lstat 报 ENOTDIR.
    env.write_source("shell/file", "not a directory\n");
    // 目标侧故障: `~/.occupied` 是普通文件, `~/.occupied/rc` 的 lstat 同样报 ENOTDIR.
    fs::write(env.home().join(".occupied"), "not a directory\n").expect("写入目标侧占位文件");
    env.write_manifest("[[entry]]\nsource = \"shell/file/rc\"\ntarget = \"~/.occupied/rc\"\n");

    let run = run(&env, &["status"]);

    assert_eq!(
        run.code, 1,
        "双侧环境故障是判定阶段的失败, 退出码必须是 1 (非 2/3);\nstdout:\n{}\nstderr:\n{}",
        run.stdout, run.stderr
    );
    let line = line_for(&run.stdout, "~/.occupied/rc");
    assert_eq!(token_of(line), "fail", "双侧都读不到必须是 fail: {line}");
    let source = env.repo().join("shell/file/rc");
    let target = env.home().join(".occupied/rc");
    assert!(
        line.contains(&format!("源侧路径 {}: Not a directory", source.display())),
        "fail 行必须给出源侧路径与原因: {line}"
    );
    assert!(
        line.contains(&format!("目标侧路径 {}: Not a directory", target.display())),
        "fail 行必须给出目标侧路径与原因: {line}"
    );
}

// ---------------------------------------------------------------------------
// 仅目标侧不可读: 目标路径穿过普通文件, 源侧正常
//
// 与"仅源侧不可读"互为对偶: 说明只给出目标侧路径与原因, 不得以源路径代替.
// ---------------------------------------------------------------------------

#[test]
fn a_target_side_only_failure_reports_the_target_path() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    fs::write(env.home().join(".occupied"), "not a directory\n").expect("写入目标侧占位文件");
    env.write_manifest("[[entry]]\nsource = \"shell/bashrc\"\ntarget = \"~/.occupied/rc\"\n");

    let run = run(&env, &["status"]);

    assert_eq!(
        run.code, 1,
        "stdout:\n{}\nstderr:\n{}",
        run.stdout, run.stderr
    );
    let line = line_for(&run.stdout, "~/.occupied/rc");
    assert_eq!(token_of(line), "fail", "{line}");
    let target = env.home().join(".occupied/rc");
    assert!(
        line.contains(&format!(
            "目标侧读取失败: 路径 {}: Not a directory",
            target.display()
        )),
        "fail 行必须给出目标侧路径与原因: {line}"
    );
    assert!(!line.contains("源侧"), "源侧正常时不得捏造源侧原因: {line}");
}

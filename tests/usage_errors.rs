//! FR-020: 调用方式错误 (与清单校验失败同码 2, 靠标准错误区分).
//!
//! 该文件是 SC-009 "退出码 2 的两重含义靠标准错误区分" 这一依据的可回归断言.
//! 它约束的是 T005 的命令面, 因此在本阶段即可全绿.

mod support;

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

    env.cmd()
        .arg("statu")
        .assert()
        .code(2)
        .stdout("")
        .stderr(contains("unrecognized subcommand"));
}

#[test]
fn unknown_option_exits_two() {
    let env = TempEnv::new();
    env.write_manifest("[[entry]]\nsource = \"a\"\ntarget = \"b\"\n");

    env.cmd()
        .args(["preview", "--bogus"])
        .assert()
        .code(2)
        .stdout("")
        .stderr(contains("unexpected argument"));
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

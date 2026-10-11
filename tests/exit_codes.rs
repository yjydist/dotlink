//! FR-020: 退出码契约 (`contracts/cli.md` 的"退出码"小节).
//!
//! 两条链:
//! - 链 1 直接对 `exit_code_for` 做断言 (喂入构造的逐条结果), 不依赖任何二进制;
//! - 链 2 经真实二进制断言四条命令的退出码.
//!
//! 本文件在 T004 之前即为红 (链 1 与链 2 都断言失败), 链 1 在 T004 后立即转绿,
//! 链 2 随各命令的接入逐步转绿. 该红是预期的.

mod support;

use std::fs;

use dotlink::{Command, EntryOutcome, exit_code_for};

use support::TempEnv;

/// 断言某命令在给定逐条结果下的退出码.
#[track_caller]
fn assert_exit_code_for(command: Command, outcomes: &[EntryOutcome], expected: i32) {
    let actual = exit_code_for(command, outcomes);
    assert_eq!(
        actual, expected,
        "exit_code_for({command:?}, {outcomes:?}) 期望 {expected}, 实际 {actual}"
    );
}

/// 断言 `exit_code_for` 绝不返回 2 或 3.
#[track_caller]
fn assert_never_two_or_three(command: Command, outcomes: &[EntryOutcome]) {
    let actual = exit_code_for(command, outcomes);
    assert!(
        actual == 0 || actual == 1,
        "exit_code_for({command:?}, {outcomes:?}) 返回了 {actual}, 只允许 0 或 1"
    );
}

// ---------------------------------------------------------------------------
// 链 1: 单元链, 直接调用 dotlink::exit_code_for
// ---------------------------------------------------------------------------

#[test]
fn preview_reports_pending_work_while_apply_reports_success() {
    let created = [EntryOutcome::Create, EntryOutcome::Create];

    assert_exit_code_for(Command::Preview, &created, 1);
    assert_exit_code_for(Command::Apply, &created, 0);
}

#[test]
fn both_preview_and_apply_are_ok_when_already_up_to_date() {
    let all_ok = [EntryOutcome::Ok, EntryOutcome::Ok, EntryOutcome::Ok];

    assert_exit_code_for(Command::Preview, &all_ok, 0);
    assert_exit_code_for(Command::Apply, &all_ok, 0);
}

#[test]
fn apply_is_ok_while_undo_reports_pending_work_on_a_deployed_manifest() {
    let undone = [EntryOutcome::Undo, EntryOutcome::Restore];

    assert_exit_code_for(Command::Apply, &undone, 0);
    assert_exit_code_for(Command::Undo, &undone, 1);
}

#[test]
fn undo_is_idempotent_when_everything_was_already_removed() {
    // undo 语境下的 `Ok` 代表 `Absent -> ok`.
    let all_ok = [EntryOutcome::Ok, EntryOutcome::Ok];

    assert_exit_code_for(Command::Undo, &all_ok, 0);
}

#[test]
fn missing_source_forces_exit_code_one_for_every_command() {
    // SourceMissing 是终态: 即使 `undo` 的目标谓词在它身上已成立, 也不能退 0.
    let outcomes = [EntryOutcome::Missing];

    assert_exit_code_for(Command::Preview, &outcomes, 1);
    assert_exit_code_for(Command::Apply, &outcomes, 1);
    assert_exit_code_for(Command::Status, &outcomes, 1);
    assert_exit_code_for(Command::Undo, &outcomes, 1);
}

#[test]
fn skipped_entry_forces_undo_to_exit_code_one() {
    // 目标谓词成立 (没有本工具的链接待撤), 但条件 2 因 `skip` 不成立.
    let outcomes = [EntryOutcome::Skip];

    assert_exit_code_for(Command::Undo, &outcomes, 1);
}

#[test]
fn conflict_forces_apply_and_status_to_exit_code_one() {
    let outcomes = [EntryOutcome::Conflict];

    assert_exit_code_for(Command::Apply, &outcomes, 1);
    assert_exit_code_for(Command::Status, &outcomes, 1);
}

#[test]
fn status_reflects_deviation() {
    let all_ok = [EntryOutcome::Ok, EntryOutcome::Ok];
    assert_exit_code_for(Command::Status, &all_ok, 0);

    assert_exit_code_for(Command::Status, &[EntryOutcome::Absent], 1);
    assert_exit_code_for(Command::Status, &[EntryOutcome::Foreign], 1);
    assert_exit_code_for(Command::Status, &[EntryOutcome::Wrong], 1);
}

#[test]
fn exit_code_for_never_returns_two_or_three() {
    let every_outcome = [
        EntryOutcome::Create,
        EntryOutcome::Ok,
        EntryOutcome::Replace,
        EntryOutcome::Conflict,
        EntryOutcome::Absent,
        EntryOutcome::Missing,
        EntryOutcome::Foreign,
        EntryOutcome::Wrong,
        EntryOutcome::Undo,
        EntryOutcome::Restore,
        EntryOutcome::Skip,
        EntryOutcome::Fail,
    ];
    let commands = [
        Command::Preview,
        Command::Apply,
        Command::Status,
        Command::Undo,
    ];

    for command in commands {
        assert_never_two_or_three(command, &[]);
        for outcome in every_outcome {
            assert_never_two_or_three(command, &[outcome]);
            assert_never_two_or_three(command, &every_outcome);
        }
    }
}

// ---------------------------------------------------------------------------
// 链 2: 端到端链, 经真实二进制
// ---------------------------------------------------------------------------

/// 链 2 的夹具: 一份含单条目清单的临时环境 (源在仓库中, 目标在 HOME 下).
fn deployable_env() -> TempEnv {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "export A=1\n");
    env.write_manifest("[[entry]]\nsource = \"shell/bashrc\"\ntarget = \"~/.bashrc\"\n");
    env
}

#[test]
fn end_to_end_exit_codes_on_a_fresh_manifest() {
    let env = deployable_env();

    // 未部署: preview 报告"有活要干".
    env.cmd().arg("preview").assert().code(1);
    // apply 把活干完.
    env.cmd().arg("apply").assert().code(0);
    // 部署后目标已成为指向源的链接.
    let link = env.home().join(".bashrc");
    assert!(
        fs::symlink_metadata(&link).is_ok_and(|meta| meta.file_type().is_symlink()),
        "apply 之后 {} 应当是一个符号链接",
        link.display()
    );
    // 部署后清单已完全符合期望.
    env.cmd().arg("status").assert().code(0);
    env.cmd().arg("preview").assert().code(0);
    // 有链接待撤: undo 退 1.
    env.cmd().arg("undo").assert().code(1);
    // 撤销干净后幂等地退 0.
    env.cmd().arg("undo").assert().code(0);
}

#[test]
fn end_to_end_missing_source_forces_exit_code_one() {
    let env = deployable_env();
    env.write_manifest("[[entry]]\nsource = \"shell/gone\"\ntarget = \"~/.bashrc\"\n");

    for subcommand in ["preview", "apply", "status", "undo"] {
        env.cmd().arg(subcommand).assert().code(1);
    }
}

#[test]
fn end_to_end_help_and_version_exit_zero() {
    let env = deployable_env();

    env.cmd().arg("-h").assert().code(0);
    env.cmd().arg("-V").assert().code(0);
}

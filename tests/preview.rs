//! US2 / FR-017 / SC-006: 部署前预览 (tasks.md 的 T018).
//!
//! 覆盖 (a)-(f). 本文件经**真实二进制**断言, 因此依赖 `src/lib.rs` 的命令接入
//! (T019/T020). 在那些接入完成之前, 这里的断言必然失败 -- 这是预期的 TDD 红阶段
//! (测试先于实现存在), 不得为了让本文件变绿而改动 `src/` 下的任何文件.

mod support;

use std::fs;
use std::path::PathBuf;

use predicates::str::contains;
use support::{
    TempEnv, entry_lines, line_for, manifest_of, physical_repo, run, snapshot_all, target_of,
    token_of,
};

// ---------------------------------------------------------------------------
// T018 (a): 只读 (SC-006) -- 三种状态并存时执行前后完全一致
// ---------------------------------------------------------------------------

#[test]
fn preview_is_read_only_across_the_three_states() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_source("shell/zshrc", "managed-zsh\n");
    env.write_source("git/gitconfig", "managed-git\n");
    env.write_manifest(&manifest_of(&[
        ("shell/bashrc", "~/.bashrc", "-"),
        ("shell/zshrc", "~/.zshrc", "-"),
        ("git/gitconfig", "~/.gitconfig", "-"),
    ]));

    // 未部署: ~/.bashrc 不存在.
    // 已是最新: ~/.zshrc 已是指向源的链接.
    let source = physical_repo(&env).join("shell/zshrc");
    std::os::unix::fs::symlink(&source, env.home().join(".zshrc")).expect("建立已是最新的链接");
    // 冲突: ~/.gitconfig 被普通文件占用.
    fs::write(env.home().join(".gitconfig"), "pre-existing\n").expect("写入占用物");

    let before = snapshot_all(&env);

    let run = run(&env, &["preview"]);

    let after = snapshot_all(&env);
    assert_eq!(
        before, after,
        "SC-006: preview 必须完全只读; 输出:\n{}\nstderr:\n{}",
        run.stdout, run.stderr
    );
    assert_eq!(
        run.code, 1,
        "三种状态下 preview 报告有活要干; stdout:\n{}",
        run.stdout
    );

    // 三种状态各得其所, 使这条只读断言确实覆盖了三种状态.
    assert_eq!(token_of(line_for(&run.stdout, "~/.bashrc")), "create");
    assert_eq!(token_of(line_for(&run.stdout, "~/.zshrc")), "ok");
    assert_eq!(token_of(line_for(&run.stdout, "~/.gitconfig")), "conflict");
}

// ---------------------------------------------------------------------------
// T018 (b): 目标被普通文件占用 -> conflict 并说明原因
// ---------------------------------------------------------------------------

#[test]
fn a_plain_file_occupying_the_target_is_reported_as_conflict_with_a_reason() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_manifest(&manifest_of(&[("shell/bashrc", "~/.bashrc", "-")]));

    fs::write(env.home().join(".bashrc"), "pre-existing\n").expect("写入占用物");

    let run = run(&env, &["preview"]);

    assert_eq!(run.code, 1, "存在未处理的条目; stdout:\n{}", run.stdout);
    let line = line_for(&run.stdout, "~/.bashrc");
    assert_eq!(token_of(line), "conflict", "{line}");
    // 冲突行必须说明原因 (FR-016): 占用物类型与"未覆盖"这一处置.
    assert!(line.contains("未覆盖"), "conflict 行必须说明原因: {line}");
    assert!(
        line.contains("普通文件"),
        "conflict 行必须说明占用物类型: {line}"
    );

    // 预览不得动它分毫.
    env.assert_content(&env.home().join(".bashrc"), "pre-existing\n");
}

// ---------------------------------------------------------------------------
// T018 (c): 目标已是正确链接 -> ok
// ---------------------------------------------------------------------------

#[test]
fn a_correct_link_is_reported_as_ok() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_manifest(&manifest_of(&[("shell/bashrc", "~/.bashrc", "-")]));

    let link = env.home().join(".bashrc");
    std::os::unix::fs::symlink(physical_repo(&env).join("shell/bashrc"), &link)
        .expect("建立正确的链接");

    let run = run(&env, &["preview"]);

    assert_eq!(
        run.code, 0,
        "已完全符合期望时 preview 退 0; stdout:\n{}",
        run.stdout
    );
    assert_eq!(token_of(line_for(&run.stdout, "~/.bashrc")), "ok");
    let summary = run.stdout.lines().last().unwrap_or_default();
    assert_eq!(summary, "1 个条目: 0 成功, 1 已是最新, 0 未处理, 0 失败");
}

// ---------------------------------------------------------------------------
// T018 (d): 一致性 (FR-017 / SC-006) -- preview 的 token 与随后 apply 的结果 100% 对应
// ---------------------------------------------------------------------------

#[test]
fn preview_tokens_match_what_apply_actually_does_entry_by_entry() {
    let env = TempEnv::new();

    // 五种状态: 未部署 / 已是最新 / 目标被普通文件占用 (skip) / 源缺失 / 目标被目录占用 (replace).
    env.write_source("shell/bashrc", "managed\n");
    env.write_source("shell/zshrc", "managed-zsh\n");
    env.write_source("git/gitconfig", "managed-git\n");
    env.write_source("nvim/init.lua", "managed-nvim\n");
    env.write_manifest(&manifest_of(&[
        ("shell/bashrc", "~/.bashrc", "-"),
        ("shell/zshrc", "~/.zshrc", "-"),
        ("git/gitconfig", "~/.gitconfig", "-"),
        ("shell/gone", "~/.gone", "-"),
        ("nvim/init.lua", "~/.config/nvim", "replace_and_backup"),
    ]));

    // 已是最新.
    std::os::unix::fs::symlink(
        physical_repo(&env).join("shell/zshrc"),
        env.home().join(".zshrc"),
    )
    .expect("建立已是最新的链接");
    // 冲突 (默认 skip).
    fs::write(env.home().join(".gitconfig"), "pre-existing\n").expect("写入占用物");
    // 冲突 (允许备份替换), 占用物是目录.
    fs::create_dir_all(env.home().join(".config/nvim")).expect("创建占用目录");
    fs::write(env.home().join(".config/nvim/old.lua"), "old\n").expect("写入占用目录内的文件");

    let preview = run(&env, &["preview"]);
    let expected_rows = entry_lines(&preview.stdout).len();
    assert_eq!(
        expected_rows, 5,
        "应给出 5 行逐条目结果:\n{}",
        preview.stdout
    );

    let apply = run(&env, &["apply"]);
    assert_eq!(
        entry_lines(&apply.stdout).len(),
        expected_rows,
        "apply 的行数必须与 preview 相同:\n{}",
        apply.stdout
    );

    // 逐条比对: 同一状态, 同一 target, 两条命令的 token 必须完全一致.
    for line in entry_lines(&preview.stdout) {
        let target = target_of(line);
        let during = line_for(&apply.stdout, target);
        assert_eq!(
            token_of(line),
            token_of(during),
            "FR-017: {target} 的 preview token 与 apply 实际结果不符\npreview: {line}\napply:   {during}"
        );
    }

    // 逐条覆盖: preview 看到的待办确实被执行 (create -> 链接建立).
    assert_eq!(token_of(line_for(&preview.stdout, "~/.bashrc")), "create");
    env.assert_symlink_to(
        &env.home().join(".bashrc"),
        &physical_repo(&env).join("shell/bashrc"),
    );
    assert_eq!(
        token_of(line_for(&preview.stdout, "~/.config/nvim")),
        "replace"
    );
    env.assert_symlink_to(
        &env.home().join(".config/nvim"),
        &physical_repo(&env).join("nvim/init.lua"),
    );
    // 源缺失与冲突跳过不因 apply 而改变.
    assert_eq!(token_of(line_for(&preview.stdout, "~/.gone")), "missing");
    assert_eq!(token_of(line_for(&apply.stdout, "~/.gone")), "missing");
    env.assert_content(&env.home().join(".gitconfig"), "pre-existing\n");
}

// ---------------------------------------------------------------------------
// T018 (e): preview 之后不得出现备份文件
// ---------------------------------------------------------------------------

#[test]
fn preview_never_leaves_a_backup_file_behind() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_source("shell/zshrc", "managed-zsh\n");
    env.write_manifest(&manifest_of(&[
        ("shell/bashrc", "~/.bashrc", "replace_and_backup"),
        ("shell/zshrc", "~/.zshrc", "-"),
    ]));

    fs::write(env.home().join(".bashrc"), "pre-existing\n").expect("写入占用物");
    fs::write(env.home().join(".zshrc"), "pre-existing-zsh\n").expect("写入占用物");

    let run = run(&env, &["preview"]);
    assert_eq!(run.code, 1, "stdout:\n{}", run.stdout);

    let rows = snapshot_all(&env);
    let backups: Vec<&PathBuf> = rows
        .iter()
        .filter(|(path, _)| path.to_string_lossy().contains(".dotlink-backup."))
        .map(|(path, _)| path)
        .collect();
    assert!(backups.is_empty(), "preview 不得产生备份文件: {backups:?}");

    // 两个占用物都逐字未变.
    env.assert_content(&env.home().join(".bashrc"), "pre-existing\n");
    env.assert_content(&env.home().join(".zshrc"), "pre-existing-zsh\n");
}

// ---------------------------------------------------------------------------
// T018 (f): 空清单退 2 并说明 (V-3)
// ---------------------------------------------------------------------------

#[test]
fn an_empty_manifest_is_rejected_with_exit_code_two() {
    let env = TempEnv::new();
    // `entry` 为空数组: V-3 的整体拒绝.
    env.write_manifest("entry = []\n");

    env.cmd()
        .arg("preview")
        .assert()
        .code(2)
        .stdout("")
        .stderr(contains("entry"));
}

#[test]
fn a_manifest_without_any_entry_key_is_rejected_with_exit_code_two() {
    let env = TempEnv::new();
    env.write_manifest("# 只有注释, 没有任何条目\n");

    env.cmd()
        .arg("preview")
        .assert()
        .code(2)
        .stdout("")
        .stderr(contains("entry"));
}

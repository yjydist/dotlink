//! US2 / FR-017 / FR-006: 部署前预览 (openspec/specs/link-deployment/spec.md 的
//! "预览与执行使用同一判定" 与 "预览不建目录" 场景).
//!
//! 覆盖 (a)-(f). 本文件经**真实二进制**断言, 因此依赖 `src/lib.rs` 的命令接入.
//! 在那些接入完成之前, 这里的断言必然失败 -- 这是预期的 TDD 红阶段 (测试先于实现
//! 存在), 不得为了让本文件变绿而改动 `src/` 下的任何文件.

mod support;

use std::fs;
use std::path::PathBuf;

use predicates::str::contains;
use support::{
    TempEnv, backup_candidate_from_output, backups_in, entry_lines, heal_backup_from_output,
    heal_backups_in, line_for, manifest_of, physical_repo, run, snapshot_all, target_of, token_of,
};

// ---------------------------------------------------------------------------
// 预览用例 (a): 只读 (FR-006 的预览要求) -- 三种状态并存时执行前后完全一致
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
        "preview 必须完全只读; 输出:\n{}\nstderr:\n{}",
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
// 预览用例 (b): 目标被普通文件占用 -> conflict 并说明原因
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
// 预览用例 (c): 目标已是正确链接 -> ok
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
// 预览用例 (d): 一致性 (FR-017 / FR-006) -- preview 的 token 与随后 apply 的结果 100% 对应
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
// 预览用例 (e): preview 之后不得出现备份文件
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
// 预览用例 (f): 空清单退 2 并说明 (openspec/specs/manifest-schema/spec.md 的 V-3)
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

// ---------------------------------------------------------------------------
// 预览行必须同时给出当前状态与将执行的动作 (link-deployment)
// ---------------------------------------------------------------------------

#[test]
fn preview_reports_the_current_state_and_the_planned_action() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_source("shell/zshrc", "managed-zsh\n");
    env.write_manifest(&manifest_of(&[
        ("shell/bashrc", "~/.bashrc", "-"),
        ("shell/zshrc", "~/.zshrc", "-"),
    ]));

    // ~/.bashrc 未部署; ~/.zshrc 已是最新.
    std::os::unix::fs::symlink(
        physical_repo(&env).join("shell/zshrc"),
        env.home().join(".zshrc"),
    )
    .expect("建立已是最新的链接");

    let run = run(&env, &["preview"]);
    assert_eq!(run.code, 1, "{}", run.stdout);

    let create = line_for(&run.stdout, "~/.bashrc");
    assert!(create.contains("未部署"), "必须给出当前状态: {create}");
    assert!(
        create.contains("将创建链接"),
        "必须给出将执行的动作: {create}"
    );

    let ok = line_for(&run.stdout, "~/.zshrc");
    assert!(ok.contains("已是最新"), "必须给出当前状态: {ok}");
    assert!(!ok.contains("将"), "已是最新的条目不得报告动作: {ok}");
}

// ---------------------------------------------------------------------------
// 预览不创建缺失的父目录 (link-deployment 的 "预览不建目录")
// ---------------------------------------------------------------------------

#[test]
fn preview_does_not_create_missing_parent_directories() {
    let env = TempEnv::new();
    env.write_source("nvim/init.lua", "managed\n");
    env.write_manifest(&manifest_of(&[(
        "nvim/init.lua",
        "~/.config/nvim/init.lua",
        "-",
    )]));

    let run = run(&env, &["preview"]);
    assert_eq!(run.code, 1, "{}", run.stdout);
    let line = line_for(&run.stdout, "~/.config/nvim/init.lua");
    assert_eq!(token_of(line), "create", "{line}");
    assert!(
        fs::symlink_metadata(env.home().join(".config")).is_err(),
        "preview 不得创建任何父目录"
    );
}

/// `YYYYMMDDThhmmss`: 15 个字符, 第 9 个是 `T`, 其余是数字.
fn is_timestamp(stamp: &str) -> bool {
    let bytes = stamp.as_bytes();
    bytes.len() == 15
        && bytes[8] == b'T'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 8 || byte.is_ascii_digit())
}

#[test]
fn preview_reports_the_backup_path_it_would_use() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_manifest(&manifest_of(&[(
        "shell/bashrc",
        "~/.bashrc",
        "replace_and_backup",
    )]));
    fs::write(env.home().join(".bashrc"), "pre-existing\n").expect("写入占用物");

    let preview = run(&env, &["preview"]);
    assert_eq!(preview.code, 1, "stdout:\n{}", preview.stdout);
    let line = line_for(&preview.stdout, "~/.bashrc");
    assert_eq!(token_of(line), "replace");

    // preview 给出的必须是完整路径, 且落在被覆盖对象所在目录.
    let predicted = backup_candidate_from_output(line);
    assert_eq!(
        predicted.parent(),
        Some(env.home()),
        "备份必须与被覆盖对象同目录: {}",
        predicted.display()
    );
    let predicted_name = predicted
        .file_name()
        .expect("预测路径必须有文件名")
        .to_string_lossy()
        .into_owned();
    let predicted_stamp = predicted_name
        .strip_prefix(".bashrc.dotlink-backup.")
        .unwrap_or_else(|| panic!("预测的备份名形态不对: {predicted_name}"));
    assert!(
        is_timestamp(predicted_stamp),
        "预测的备份名必须带 15 位时间戳: {predicted_name}"
    );
    assert!(
        fs::symlink_metadata(&predicted).is_err(),
        "preview 不得创建备份: {}",
        predicted.display()
    );

    // apply 实际生成的备份必须遵循同一规则 (同目录, 同一前缀, 同一时间戳形态).
    let apply = run(&env, &["apply"]);
    assert_eq!(apply.code, 0, "stdout:\n{}", apply.stdout);
    let backups = backups_in(env.home());
    assert_eq!(backups.len(), 1, "应只有一份备份: {backups:?}");
    let actual = &backups[0];
    let actual_stamp = actual
        .strip_prefix(".bashrc.dotlink-backup.")
        .unwrap_or_else(|| panic!("实际备份名形态不对: {actual}"));
    assert!(
        is_timestamp(actual_stamp),
        "实际备份名必须带 15 位时间戳: {actual}"
    );
}

// ---------------------------------------------------------------------------
// source 改名: preview 预测自愈 (默认 skip)
// ---------------------------------------------------------------------------

#[test]
fn preview_predicts_healing_a_renamed_source_under_the_default_policy() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_manifest(&manifest_of(&[("shell/bashrc", "~/.bashrc", "-")]));
    assert_eq!(run(&env, &["apply"]).code, 0, "先按旧 source 部署");

    // source 改名后, 目标上的旧链接指向仓库内旧路径; 条目保持缺省 `on_conflict`.
    let repo = physical_repo(&env);
    fs::rename(repo.join("shell/bashrc"), repo.join("shell/bashrc2")).expect("改名源文件");
    env.write_manifest(&manifest_of(&[("shell/bashrc2", "~/.bashrc", "-")]));

    let before = snapshot_all(&env);
    let preview = run(&env, &["preview"]);
    assert_eq!(preview.code, 1, "存在待办; stdout:\n{}", preview.stdout);
    assert_eq!(
        snapshot_all(&env),
        before,
        "preview 必须只读; stdout:\n{}",
        preview.stdout
    );

    let line = line_for(&preview.stdout, "~/.bashrc");
    assert_eq!(token_of(line), "replace", "{line}");
    assert!(
        line.contains("本工具的链接指向错误"),
        "预览必须说明这是本工具的错误链接: {line}"
    );

    let predicted = heal_backup_from_output(line);
    assert_eq!(
        predicted.parent(),
        Some(env.home()),
        "备份必须与被替换的链接同目录: {}",
        predicted.display()
    );
    let predicted_name = predicted
        .file_name()
        .expect("预测路径必须有文件名")
        .to_string_lossy()
        .into_owned();
    let predicted_stamp = predicted_name
        .strip_prefix(".bashrc.dotlink-heal.")
        .unwrap_or_else(|| panic!("预测的备份名形态不对: {predicted_name}"));
    assert!(
        is_timestamp(predicted_stamp),
        "预测的备份名必须带 15 位时间戳: {predicted_name}"
    );
    assert!(
        fs::symlink_metadata(&predicted).is_err(),
        "preview 不得创建备份: {}",
        predicted.display()
    );
    assert!(
        backups_in(env.home()).is_empty(),
        "preview 不得创建用户备份: {:?}",
        backups_in(env.home())
    );

    // apply 实际写入的命名空间必须与预测一致 (跨秒时只有字面时间戳可以不同).
    let applied = run(&env, &["apply"]);
    assert_eq!(applied.code, 0, "stdout:\n{}", applied.stdout);
    let actual = heal_backup_from_output(&applied.stdout);
    assert!(
        actual
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains(".dotlink-heal."),
        "apply 实际备份必须使用与 preview 相同的自愈命名空间: {}",
        actual.display()
    );
    assert_eq!(heal_backups_in(env.home()).len(), 1);
    assert!(backups_in(env.home()).is_empty());
}

// ---------------------------------------------------------------------------
// 汇总四列: 四个条目分别落入 成功 / 已是最新 / 未处理 / 失败, 各行计数为 1
// ---------------------------------------------------------------------------

#[test]
fn the_summary_puts_one_entry_in_each_of_the_four_columns() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_source("shell/zshrc", "managed-zsh\n");
    env.write_source("git/gitconfig", "managed-git\n");
    env.write_manifest(&manifest_of(&[
        // 未部署 -> create -> 成功.
        ("shell/bashrc", "~/.bashrc", "-"),
        // 已是最新 -> ok.
        ("shell/zshrc", "~/.zshrc", "-"),
        // 被普通文件占用 + 默认策略 -> conflict -> 未处理.
        ("git/gitconfig", "~/.gitconfig", "-"),
        // 源缺失 -> missing -> 失败.
        ("shell/gone", "~/.gone", "-"),
    ]));

    let source = physical_repo(&env).join("shell/zshrc");
    std::os::unix::fs::symlink(&source, env.home().join(".zshrc")).expect("建立已是最新的链接");
    fs::write(env.home().join(".gitconfig"), "pre-existing\n").expect("写入占用物");

    let run = run(&env, &["preview"]);

    assert_eq!(
        run.stdout.lines().last().unwrap_or_default(),
        "4 个条目: 1 成功, 1 已是最新, 1 未处理, 1 失败",
        "四列必须各计一个且和等于总数:\n{}",
        run.stdout
    );
    assert_eq!(run.code, 1, "存在待办与失败, preview 必须退 1");
}

// ---------------------------------------------------------------------------
// 输出契约: 报告中的控制字符必须转义 (cli-contract 的"报告中的控制字符必须转义")
// ---------------------------------------------------------------------------

#[test]
fn preview_escapes_control_characters_in_target_and_source() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    // TOML 基本字符串里的转义: 未转义的换行会让一个条目折行, ESC/BEL 可向终端
    // 注入 OSC 序列. 第一个条目覆盖 target, 第二个条目让 source 出现在缺失说明里.
    env.write_manifest(&manifest_of(&[
        (
            "shell/bashrc",
            r"~/.bashrc\u001b]0;pwned\u0007\nsecond",
            "-",
        ),
        (r"shell/\u001b\u0007missing", "~/.missing", "-"),
    ]));

    let run = run(&env, &["preview"]);

    assert!(
        !run.stdout.as_bytes().contains(&0x1b),
        "输出含未转义 ESC: {:?}",
        run.stdout
    );
    assert!(
        !run.stdout.as_bytes().contains(&0x07),
        "输出含未转义 BEL: {:?}",
        run.stdout
    );
    // 两个条目加末尾汇总: 换行 target 不得使条目折行.
    assert_eq!(
        run.stdout.lines().count(),
        3,
        "每个条目必须恰占一行, 汇总在最后: {:?}",
        run.stdout
    );

    let lines: Vec<&str> = run.stdout.lines().collect();
    assert!(
        lines[0].contains(r"~/.bashrc\u{1b}]0;pwned\u{7}\nsecond"),
        "target 列必须显示转义后的原始写法: {}",
        lines[0]
    );
    assert!(
        lines[1].contains(r"shell/\u{1b}\u{7}missing"),
        "缺失说明中的 source 必须被转义: {}",
        lines[1]
    );
    assert_eq!(
        lines[2], "2 个条目: 1 成功, 0 已是最新, 0 未处理, 1 失败",
        "汇总行必须是最后一行:\n{}",
        run.stdout
    );
    assert_eq!(run.code, 1, "存在待办与失败; stdout:\n{}", run.stdout);
}

//! US4 / FR-009 / FR-010 / SC-005: 撤销部署 (tasks.md 的 T028 全部条目).
//!
//! 覆盖 (a)-(h). 本文件经真实二进制断言, 因此**依赖 `src/lib.rs` 的命令接入**
//! (T015 / T026 / T031). 在那些接入完成之前, 这里的断言必然失败 -- 这是预期的
//! TDD 红阶段 (测试先于实现存在), 不得为了让本文件变绿而改动 `src/lib.rs`.

mod support;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use support::{
    TempEnv, assert_token, backup_path_from_output, backups_in, line_for, manifest_of, run, unlock,
};

/// 已部署的环境: 源在仓库中, 目标是指向源的链接.
fn deployed(env: &TempEnv, source: &str, target: &str) -> PathBuf {
    env.write_source(source, "managed\n");
    let deployed = run(env, &["apply"]);
    assert_eq!(deployed.code, 0, "前置部署必须成功: {}", deployed.stdout);
    let path = env.home().join(target);
    assert!(
        fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_symlink()),
        "前置条件失败: apply 之后 {} 应当是指向源的符号链接 (命令接入由 T015/T031 负责, 尚未完成)",
        path.display()
    );
    env.assert_symlink_to(&path, &env.repo().join(source));
    path
}

// ---------------------------------------------------------------------------
// (a) 已部署且曾有备份: 链接被移除, 原内容恢复到原路径, 行为 restore 且给出备份来源路径
// ---------------------------------------------------------------------------

#[test]
fn undo_restores_the_previous_content_and_reports_the_backup_source() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_manifest(&manifest_of(&[(
        "shell/bashrc",
        "~/.bashrc",
        "replace_and_backup",
    )]));

    let target = env.home().join(".bashrc");
    fs::write(&target, "original\n").unwrap();
    let applied = run(&env, &["apply"]);
    assert_eq!(applied.code, 0, "{}", applied.stdout);
    let backup = backup_path_from_output(&applied.stdout);
    env.assert_content(&backup, "original\n");

    // restore 行必须给出备份来源路径 (FR-016).
    let undone = run(&env, &["undo"]);
    assert_token(&undone, "restore");
    assert_eq!(
        undone.code, 1,
        "undo 的目标谓词在动手之前求值: 撤销了东西 (restore) 故退 1; stdout:\n{}",
        undone.stdout
    );
    let line = line_for(&undone.stdout, "restore");
    assert!(
        line.contains(&backup.display().to_string()),
        "restore 行必须给出备份文件的完整路径: {line}"
    );

    assert!(
        fs::symlink_metadata(&target).is_ok_and(|meta| !meta.file_type().is_symlink()),
        "恢复后 {} 不应再是链接",
        target.display()
    );
    env.assert_content(&target, "original\n");

    // 恢复成功后不删除备份文件本身.
    assert!(
        fs::symlink_metadata(&backup).is_ok(),
        "备份文件必须保留在磁盘上"
    );
    env.assert_content(&backup, "original\n");
}

// ---------------------------------------------------------------------------
// (b) 已部署但无备份: 链接被移除, 行为 undo, 原路径不再存在
// ---------------------------------------------------------------------------

#[test]
fn undo_removes_a_link_without_a_backup() {
    let env = TempEnv::new();
    env.write_manifest(&manifest_of(&[("shell/bashrc", "~/.bashrc", "-")]));
    let target = deployed(&env, "shell/bashrc", ".bashrc");

    let undone = run(&env, &["undo"]);
    assert_token(&undone, "undo");
    assert_eq!(
        undone.code, 1,
        "撤销了东西 (undo) 故退 1; stdout:\n{}",
        undone.stdout
    );

    assert!(
        fs::symlink_metadata(&target).is_err(),
        "{} 应已被移除",
        target.display()
    );
    assert!(
        backups_in(env.home()).is_empty(),
        "无备份时不得凭空产生备份文件: {:?}",
        backups_in(env.home())
    );
    assert!(
        env.repo().join("shell/bashrc").is_file(),
        "源条目必须原样保留"
    );
}

// ---------------------------------------------------------------------------
// (c) 安全反例 (SC-005): 手工换成真实目录 -> skip 且目录完整保留
// ---------------------------------------------------------------------------

#[test]
fn undo_skips_a_real_directory_and_keeps_its_content() {
    let env = TempEnv::new();
    env.write_manifest(&manifest_of(&[("shell/bashrc", "~/.bashrc", "-")]));
    let target = deployed(&env, "shell/bashrc", ".bashrc");

    fs::remove_file(&target).unwrap();
    fs::create_dir_all(target.join("nested")).unwrap();
    fs::write(target.join("nested/keep"), "keep me\n").unwrap();

    let undone = run(&env, &["undo"]);
    assert_token(&undone, "skip");
    assert_eq!(undone.code, 1, "skip 使条件 2 不成立");

    assert!(
        fs::symlink_metadata(&target).unwrap().is_dir(),
        "非本工具创建的目录绝不允许被删除"
    );
    env.assert_content(&target.join("nested/keep"), "keep me\n");
    assert!(
        line_for(&undone.stdout, "skip").contains("目录"),
        "skip 行必须说明原因与占用物类型"
    );
}

// ---------------------------------------------------------------------------
// (d) 目标被替换成指向仓库外的链接 -> skip, 不删除该链接
// ---------------------------------------------------------------------------

#[test]
fn undo_skips_a_link_pointing_outside_the_repo() {
    let env = TempEnv::new();
    env.write_manifest(&manifest_of(&[("shell/bashrc", "~/.bashrc", "-")]));
    let target = deployed(&env, "shell/bashrc", ".bashrc");

    // 必须落在仓库**之外**: 夹具的 HOME 位于仓库之内, 因此"仓库内的别处"仍会被
    // 正确判为本工具创建的链接 (`LinkedWrong`), 报 `undo` 而非 `skip`.
    let elsewhere = std::env::temp_dir()
        .join(format!("dotlink-undo-outside-{}", std::process::id()))
        .join("elsewhere");
    fs::create_dir_all(elsewhere.parent().unwrap()).unwrap();
    fs::write(&elsewhere, "foreign\n").unwrap();
    assert!(
        !elsewhere.starts_with(env.repo()),
        "构造 foreign 的路径必须落在仓库之外: {}",
        elsewhere.display()
    );
    fs::remove_file(&target).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &target).unwrap();

    let undone = run(&env, &["undo"]);
    assert_token(&undone, "skip");
    assert_eq!(undone.code, 1);

    let _ = fs::remove_file(&elsewhere);

    assert!(
        fs::symlink_metadata(&target)
            .unwrap()
            .file_type()
            .is_symlink(),
        "外部链接可能是用户自己的配置, 绝不删除"
    );
    assert_eq!(fs::read_link(&target).unwrap(), elsewhere);
}

// ---------------------------------------------------------------------------
// (e) 目标被替换成普通文件 -> 只 skip, 且不消费备份 (并存语义)
// ---------------------------------------------------------------------------

#[test]
fn undo_skips_a_plain_file_and_does_not_consume_the_backup() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_manifest(&manifest_of(&[(
        "shell/bashrc",
        "~/.bashrc",
        "replace_and_backup",
    )]));

    let target = env.home().join(".bashrc");
    fs::write(&target, "original\n").unwrap();
    let applied = run(&env, &["apply"]);
    assert_eq!(applied.code, 0, "{}", applied.stdout);
    let backup = backup_path_from_output(&applied.stdout);

    // 手工把链接换成普通文件: 恢复备份的前提是"我先移除了自己创建的链接".
    fs::remove_file(&target).unwrap();
    fs::write(&target, "hand written\n").unwrap();

    let undone = run(&env, &["undo"]);
    assert_token(&undone, "skip");
    assert_eq!(undone.code, 1);

    env.assert_content(&target, "hand written\n");
    assert!(fs::symlink_metadata(&backup).is_ok(), "备份不得被消费/删除");
    env.assert_content(&backup, "original\n");
}

// ---------------------------------------------------------------------------
// (f) 幂等: 第二次 undo 每行 token 为 ok, 退出码 0
// ---------------------------------------------------------------------------

#[test]
fn undo_is_idempotent_and_the_second_run_reports_ok() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_source("shell/zshrc", "managed\n");
    env.write_manifest(&manifest_of(&[
        ("shell/bashrc", "~/.bashrc", "-"),
        ("shell/zshrc", "~/.zshrc", "-"),
    ]));
    let applied = run(&env, &["apply"]);
    assert_eq!(applied.code, 0, "前置部署必须成功: {}", applied.stdout);

    let first = run(&env, &["undo"]);
    assert_eq!(
        first.code, 1,
        "撤销了东西 (两条 undo) 故退 1; stdout:\n{}",
        first.stdout
    );
    assert!(
        first
            .stdout
            .lines()
            .filter(|line| line.contains("undo") || line.contains("restore"))
            .count()
            >= 2,
        "两条都应被撤销: {}",
        first.stdout
    );

    // 第二次: `Current` 为 `Absent` -> token `ok`, 归入"已是最新"列 -> 退 0.
    let second = run(&env, &["undo"]);
    assert_eq!(second.code, 0, "撤销必须幂等: {}", second.stdout);
    let entry_lines: Vec<&str> = second
        .stdout
        .lines()
        .filter(|line| line.trim_start().starts_with("1/") || line.trim_start().starts_with("2/"))
        .collect();
    assert_eq!(entry_lines.len(), 2, "应有两条逐条目行: {}", second.stdout);
    for line in entry_lines {
        assert!(
            line.split_whitespace().nth(1) == Some("ok"),
            "第二次 undo 的每一行 token 必须是 ok (若实现把 Absent 归为 skip, 本断言必失败): {line}"
        );
    }
    assert!(
        fs::symlink_metadata(env.home().join(".bashrc")).is_err(),
        "第二次 undo 不得产生多余删除"
    );
}

// ---------------------------------------------------------------------------
// (g) 混合场景: 可撤销的被处理, 须跳过的不受影响, 退出码 1
// ---------------------------------------------------------------------------

#[test]
fn undo_handles_a_mixed_manifest() {
    let env = TempEnv::new();
    // 两个源都必须在部署**之前**写入: `deployed()` 会对整份两条目的清单执行一次
    // `apply`, 若此时第二个源还缺失, 它会以 `missing` 使该次 apply 退 1, 从而让
    // `deployed()` 的前置断言失败 (那条断言本身是对的).
    env.write_source("shell/bashrc", "managed\n");
    env.write_source("shell/zshrc", "managed-zsh\n");
    env.write_manifest(&manifest_of(&[
        ("shell/bashrc", "~/.bashrc", "-"),
        ("shell/zshrc", "~/.zshrc", "-"),
    ]));
    let removable = deployed(&env, "shell/bashrc", ".bashrc");
    let blocked = deployed(&env, "shell/zshrc", ".zshrc");

    // 把第二条手工换成真实目录.
    fs::remove_file(&blocked).unwrap();
    fs::create_dir_all(blocked.join("nested")).unwrap();
    fs::write(blocked.join("nested/keep"), "keep me\n").unwrap();

    let undone = run(&env, &["undo"]);
    assert_token(&undone, "undo");
    assert_token(&undone, "skip");
    assert_eq!(undone.code, 1, "存在 skip 条目");

    assert!(
        fs::symlink_metadata(&removable).is_err(),
        "可撤销的条目必须被处理"
    );
    assert!(fs::symlink_metadata(&blocked).unwrap().is_dir());
    env.assert_content(&blocked.join("nested/keep"), "keep me\n");
}

// ---------------------------------------------------------------------------
// (h) 归属判定不依赖状态文件
// ---------------------------------------------------------------------------

#[test]
fn undo_never_creates_a_state_or_index_file() {
    let env = TempEnv::new();
    env.write_manifest(&manifest_of(&[("shell/bashrc", "~/.bashrc", "-")]));
    // 前置部署**之后**才取基准: 该快照是"undo 之前"的状态, 因此它必须包含 `apply`
    // 建立的链接. 若在部署之前取, 基准里就没有那条链接, 断言会把 `apply` 的产物
    // 误报成"undo 往仓库里写了东西".
    deployed(&env, "shell/bashrc", ".bashrc");

    let home_before = env.snapshot(env.home());
    let repo_before = env.snapshot(env.repo());

    let undone = run(&env, &["undo"]);
    assert_eq!(
        undone.code, 1,
        "撤销了东西 (undo) 故退 1; stdout:\n{}",
        undone.stdout
    );

    // 仓库侧只允许"目标路径被移除"这一处变化; HOME 侧除目标路径外必须逐字节不变.
    let home_after = env.snapshot(env.home());
    let removed: Vec<PathBuf> = home_before
        .iter()
        .filter(|row| !home_after.contains(row))
        .map(|row| row.0.clone())
        .collect();
    assert_eq!(
        removed,
        vec![env.home().join(".bashrc")],
        "除待撤销的链接之外, HOME 不得有任何改动 (也不得出现状态/索引文件)"
    );
    for row in &home_after {
        assert!(
            home_before.contains(row) || row.0.starts_with(env.home().join(".bashrc")),
            "HOME 中出现了新路径: {}",
            row.0.display()
        );
    }

    // 仓库侧: HOME 位于仓库之内, 因此"被撤销的链接被移除"会在仓库快照里体现为
    // 少了一行. 这里逐行核对: 消失的只允许是那条链接, 且不得出现任何**新**路径
    // (新路径才是"写入了状态/索引文件"的证据).
    let repo_after = env.snapshot(env.repo());
    let disappeared: Vec<PathBuf> = repo_before
        .iter()
        .filter(|row| !repo_after.contains(row))
        .map(|row| row.0.clone())
        .collect();
    assert_eq!(
        disappeared,
        vec![env.home().join(".bashrc")],
        "仓库中只允许少掉那条被撤销的链接"
    );
    for row in &repo_after {
        assert!(
            repo_before.contains(row),
            "undo 往仓库里写入了新路径 (状态/索引文件?): {}",
            row.0.display()
        );
    }
}

// ---------------------------------------------------------------------------
// 同一秒内 10 个并存备份: 恢复序号数值最大的 `.10`, 不是字典序最大的 `.9`
// ---------------------------------------------------------------------------

/// 同一秒同路径产生 bare / `.2` ... `.10` 共 10 个备份后, `undo` 必须恢复**真正最新**
/// 的 `.10` 并报告 `restore` (FR-016 给出备份来源路径).
///
/// "最新"是 (时间戳, 序号) 的序: 同一秒内 bare 是第 1 个, 序号逐一递增到 10. 若按
/// 整名字典序取最大, `.9` 会压过 `.10`, 恢复出来的就不是最新的备份.
#[test]
fn undo_restores_the_newest_of_ten_same_second_backups() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_manifest(&manifest_of(&[("shell/bashrc", "~/.bashrc", "-")]));

    let applied = run(&env, &["apply"]);
    assert_eq!(applied.code, 0, "{}", applied.stdout);

    // 手工摆出"同一秒内产生 10 个备份"之后的名字集合 (bare, .2 ... .10), 时间戳取
    // 定值使本用例不依赖墙钟: 名字集合与工具在同一秒内替换 10 次的结果逐一相同,
    // 第 N 个备份的内容记为 backup-{N-1}.
    let home = env.home();
    let mut backups = Vec::new();
    for round in 0..10usize {
        let name = if round == 0 {
            ".bashrc.dotlink-backup.20260101T000000".to_string()
        } else {
            format!(".bashrc.dotlink-backup.20260101T000000.{}", round + 1)
        };
        let path = home.join(&name);
        fs::write(&path, format!("backup-{round}")).unwrap();
        backups.push(path);
    }
    let newest = backups.last().unwrap().clone();

    let undone = run(&env, &["undo"]);
    assert_token(&undone, "restore");
    assert_eq!(
        undone.code, 1,
        "撤销了东西 (restore) 故退 1; stdout:\n{}",
        undone.stdout
    );

    env.assert_content(&home.join(".bashrc"), "backup-9");
    let line = line_for(&undone.stdout, "restore");
    assert!(
        line.contains(&newest.display().to_string()),
        "restore 行必须给出真正最新的备份 (.10) 的完整路径: {line}"
    );
    for path in &backups {
        assert!(
            fs::symlink_metadata(path).is_ok(),
            "备份文件本身必须保留: {}",
            path.display()
        );
    }
}

// ---------------------------------------------------------------------------
// 备份查找的目录读取故障: 报 fail 且给出底层原因, 不是成功列的 "无备份可恢复"
// ---------------------------------------------------------------------------

/// 父目录可进入 (lstat 得到链接) 但不可列举 (read_dir 失败): 判定照常得出
/// `LinkedCorrect`, 而备份查找是环境故障. 该条目必须报 `fail` 并给出底层原因, 且与
/// `Unreadable` 同类处置 -- 不移除链接. 若把列举故障吞成"无备份", 该条目会被报成
/// 成功列的 `undo`, 链接也会被移除.
#[test]
fn undo_reports_fail_with_the_cause_when_backups_cannot_be_listed() {
    let env = TempEnv::new();
    env.write_source("shell/zshrc", "managed-zsh\n");
    env.write_manifest(&manifest_of(&[("shell/zshrc", "~/.locked/rc", "-")]));

    let applied = run(&env, &["apply"]);
    assert_eq!(applied.code, 0, "{}", applied.stdout);

    let locked = env.home().join(".locked");
    let target = locked.join("rc");
    assert!(
        fs::symlink_metadata(&target)
            .unwrap()
            .file_type()
            .is_symlink(),
        "前置部署必须留下链接"
    );

    // 可进入 (x) 不可列举 (r): 判定与备份查找所需权限正交, 这正是"判定成功而备份
    // 查找失败"的最小夹具. 以 root 运行时权限位无效, 该断言必须跳过.
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o333)).unwrap();
    if fs::read_dir(&locked).is_ok() {
        unlock(&locked);
        println!(
            "跳过 undo_reports_fail_with_the_cause_when_backups_cannot_be_listed: 权限位在本环境无效"
        );
        return;
    }

    let undone = run(&env, &["undo"]);
    unlock(&locked);

    assert_eq!(undone.code, 1, "stdout:\n{}", undone.stdout);
    let line = line_for(&undone.stdout, "~/.locked/rc");
    assert_eq!(
        line.split_whitespace().nth(1),
        Some("fail"),
        "备份查找故障必须报 fail, 不得落进成功列的 undo: {line}"
    );
    assert!(
        line.contains("Permission denied"),
        "fail 行必须给出底层原因: {line}"
    );
    assert!(
        undone
            .stdout
            .lines()
            .last()
            .unwrap_or_default()
            .contains("1 失败"),
        "该条目必须计入汇总的 `失败` 列: {}",
        undone.stdout
    );
    assert!(
        fs::symlink_metadata(&target)
            .unwrap()
            .file_type()
            .is_symlink(),
        "环境故障下不得移除链接 (与 Unreadable 同类处置)"
    );
}

//! US4 / FR-009 / FR-010: 撤销部署 (openspec/specs/link-teardown/spec.md 的 FR-009 与
//! FR-010).
//!
//! 覆盖 (a)-(h), 以及备份生命周期的补充用例 (不可读备份 / 同目录多目标 / 同秒多备份).
//! 本文件经真实二进制断言, 因此**依赖 `src/lib.rs` 的命令接入**. 在那些接入完成之前,
//! 这里的断言必然失败 -- 这是预期的 TDD 红阶段 (测试先于实现存在), 不得为了让本文件
//! 变绿而改动 `src/lib.rs`.

mod support;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use support::{
    TempEnv, assert_token, backup_path_from_output, backups_in, heal_backup_from_output,
    heal_backups_in, line_for, manifest_of, physical_repo, run, running_as_root, token_of, unlock,
};

/// 已部署的环境: 源在仓库中, 目标是指向源的链接.
fn deployed(env: &TempEnv, source: &str, target: &str) -> PathBuf {
    env.write_source(source, "managed\n");
    let deployed = run(env, &["apply"]);
    assert_eq!(deployed.code, 0, "前置部署必须成功: {}", deployed.stdout);
    let path = env.home().join(target);
    assert!(
        fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_symlink()),
        "前置条件失败: apply 之后 {} 应当是指向源的符号链接 (命令接入尚未完成)",
        path.display()
    );
    env.assert_symlink_to(&path, &env.repo().join(source));
    path
}

#[test]
fn undo_restores_each_targets_latest_backup_from_shared_and_separate_directories() {
    let env = TempEnv::new();
    env.write_source("rc", "managed");
    env.write_manifest(&manifest_of(&[
        ("rc", "~/a", "-"),
        ("rc", "~/b", "-"),
        ("rc", "~/nested/a", "-"),
    ]));
    assert_eq!(run(&env, &["apply"]).code, 0);
    for name in ["a", "b", "nested/a"] {
        for serial in [9, 10] {
            fs::write(
                env.home()
                    .join(format!("{name}.dotlink-backup.20260101T000000.{serial}")),
                format!("{name}/{serial}"),
            )
            .unwrap();
        }
        fs::write(
            env.home()
                .join(format!("{name}.dotlink-backup.20990101T000000.partial")),
            "incomplete",
        )
        .unwrap();
    }
    let result = run(&env, &["undo"]);
    assert_eq!(result.code, 1, "{}", result.stdout);
    for name in ["a", "b", "nested/a"] {
        env.assert_content(&env.home().join(name), &format!("{name}/10"));
        let backup = env
            .home()
            .join(format!("{name}.dotlink-backup.20260101T000000.10"));
        assert!(backup.exists());
        assert!(
            line_for(&result.stdout, &format!("~/{name}")).contains(&backup.display().to_string())
        );
    }
    // `.partial` 残留不参与查找, 也不得在 undo 中被删除或改写.
    for name in ["a", "b", "nested/a"] {
        env.assert_content(
            &env.home()
                .join(format!("{name}.dotlink-backup.20990101T000000.partial")),
            "incomplete",
        );
    }
}

// ---------------------------------------------------------------------------
// 同一目录下三个目标: 各自取自己的最新备份, 互不串用
// ---------------------------------------------------------------------------

/// 三个条目的 target 同在一个目录 (`~/a` / `~/b` / `~/c`): 备份查找对目录做一次
/// 快照, 因此必须为每个**原始文件名**各自选出最新备份; 任何"取全目录最新一份"的
/// 实现都会把 `.10` 错配给别的目标. 同秒两份备份 (`.9` / `.10`) 使序号比较一并被
/// 覆盖: 字典序上 `.9` 大于 `.10`, 只有按数值比才能取对.
#[test]
fn three_targets_in_one_directory_each_restore_their_own_latest_backup() {
    let env = TempEnv::new();
    env.write_source("rc", "managed\n");
    env.write_manifest(&manifest_of(&[
        ("rc", "~/a", "-"),
        ("rc", "~/b", "-"),
        ("rc", "~/c", "-"),
    ]));
    assert_eq!(run(&env, &["apply"]).code, 0);

    let home = env.home();
    for name in ["a", "b", "c"] {
        for serial in [9, 10] {
            fs::write(
                home.join(format!("{name}.dotlink-backup.20260101T000000.{serial}")),
                format!("{name}/{serial}"),
            )
            .unwrap();
        }
    }

    let undone = run(&env, &["undo"]);
    assert_eq!(undone.code, 1, "{}", undone.stdout);
    for name in ["a", "b", "c"] {
        let latest = home.join(format!("{name}.dotlink-backup.20260101T000000.10"));
        env.assert_content(&home.join(name), &format!("{name}/10"));
        let line = line_for(&undone.stdout, &format!("~/{name}"));
        assert_eq!(token_of(line), "restore", "{line}");
        assert!(
            line.contains(&latest.display().to_string()),
            "restore 行必须给出该目标自己的最新备份 (.10) 的完整路径: {line}"
        );
        // 备份只被复制, 不被消费或改写: 同秒的两份都原样保留.
        for serial in [9, 10] {
            let backup = home.join(format!("{name}.dotlink-backup.20260101T000000.{serial}"));
            env.assert_content(&backup, &format!("{name}/{serial}"));
        }
    }
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
// (a2) 恢复过备份后再次撤销: 恢复出的普通文件被判为占用, 报 skip 且不被吞掉
// ---------------------------------------------------------------------------

#[test]
fn a_second_undo_after_a_restore_reports_skip_and_keeps_the_file() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_manifest(&manifest_of(&[(
        "shell/bashrc",
        "~/.bashrc",
        "replace_and_backup",
    )]));

    let target = env.home().join(".bashrc");
    fs::write(&target, "original\n").unwrap();
    assert_eq!(run(&env, &["apply"]).code, 0);

    let first = run(&env, &["undo"]);
    assert_token(&first, "restore");
    assert_eq!(first.code, 1, "{}", first.stdout);
    let backup = backup_path_from_output(&first.stdout);
    env.assert_content(&target, "original\n");

    let second = run(&env, &["undo"]);
    assert_eq!(second.code, 1, "{}", second.stdout);
    let line = line_for(&second.stdout, "~/.bashrc");
    assert_eq!(token_of(line), "skip", "{line}");

    // 恢复出的文件不被再次消费或删除, 备份也仍在原地.
    env.assert_content(&target, "original\n");
    env.assert_content(&backup, "original\n");
}

// ---------------------------------------------------------------------------
// (a3) 恢复出的内容本身是仓库内的链接: 第二次撤销按同一归属规则再次 restore
// ---------------------------------------------------------------------------

#[test]
fn a_second_undo_of_a_restored_in_repo_link_reports_restore_again() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_source("shell/stale", "stale\n");
    env.write_manifest(&manifest_of(&[(
        "shell/bashrc",
        "~/.bashrc",
        "replace_and_backup",
    )]));

    // 前置: 目标是指向仓库内另一个路径的链接 (LinkedWrong), 同目录另有一份用户
    // 备份, 其内容也是指向仓库内的链接. 该备份可来自旧版本或人工放置; 本变更后
    // apply 的自愈备份不再进入用户命名空间, 因此不能再由 apply 构造这一前置.
    let repo = physical_repo(&env);
    let stale = repo.join("shell/stale");
    let target = env.home().join(".bashrc");
    std::os::unix::fs::symlink(repo.join("shell/elsewhere"), &target)
        .expect("建立指向仓库内其他路径的链接");
    let backup = env.home().join(".bashrc.dotlink-backup.20260101T000000");
    std::os::unix::fs::symlink(&stale, &backup).expect("建立用户备份链接");

    let first = run(&env, &["undo"]);
    assert_token(&first, "restore");
    assert_eq!(
        fs::read_link(&target).unwrap(),
        stale,
        "恢复出的内容必须仍是指向仓库内的链接"
    );

    let second = run(&env, &["undo"]);
    assert_eq!(second.code, 1, "{}", second.stdout);
    let line = line_for(&second.stdout, "~/.bashrc");
    assert_eq!(token_of(line), "restore", "{line}");

    // 目标内容不变, 备份仍在原地 (只消费最新一份, 不删除).
    assert_eq!(fs::read_link(&target).unwrap(), stale);
    assert!(
        fs::symlink_metadata(&backup).is_ok(),
        "恢复后备份必须留在磁盘上"
    );
}

// ---------------------------------------------------------------------------
// 自愈备份不参与撤销: 情形 A (目标原本不存在) 与情形 B (用户备份优先)
// ---------------------------------------------------------------------------

/// 情形 A: 目标原本不存在. 自愈后撤销只移除链接, 回到未部署态; 自愈备份保留,
/// 第二次撤销无待办并以 0 退出.
#[test]
fn undo_after_self_heal_returns_to_the_undeployed_state() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_manifest(&manifest_of(&[("shell/bashrc", "~/.bashrc", "-")]));
    assert_eq!(run(&env, &["apply"]).code, 0);

    let repo = physical_repo(&env);
    fs::rename(repo.join("shell/bashrc"), repo.join("shell/bashrc2")).expect("改名源文件");
    env.write_manifest(&manifest_of(&[("shell/bashrc2", "~/.bashrc", "-")]));

    let healed = run(&env, &["apply"]);
    assert_eq!(healed.code, 0, "{}", healed.stdout);
    let heal = heal_backup_from_output(&healed.stdout);
    let heal_text = fs::read_link(&heal).expect("自愈备份必须是链接");
    assert_eq!(heal_text, repo.join("shell/bashrc"));
    assert!(
        backups_in(env.home()).is_empty(),
        "自愈不得产生用户备份: {:?}",
        backups_in(env.home())
    );

    let target = env.home().join(".bashrc");
    let undone = run(&env, &["undo"]);
    assert_eq!(undone.code, 1, "{}", undone.stdout);
    let line = line_for(&undone.stdout, "~/.bashrc");
    assert_eq!(token_of(line), "undo", "{line}");
    assert!(
        fs::symlink_metadata(&target).is_err(),
        "撤销后目标必须回到不存在"
    );
    assert_eq!(
        fs::read_link(&heal).unwrap(),
        heal_text,
        "自愈备份不得被消费"
    );
    assert!(backups_in(env.home()).is_empty());

    let second = run(&env, &["undo"]);
    assert_eq!(second.code, 0, "{}", second.stdout);
    let line = line_for(&second.stdout, "~/.bashrc");
    assert_eq!(token_of(line), "ok", "{line}");
    assert!(fs::symlink_metadata(&target).is_err());
    assert_eq!(heal_backups_in(env.home()).len(), 1);
}

/// 情形 B: 目标原本是用户文件, 用户备份 B1 在自愈备份 B2 之前. 撤销恢复 B1,
/// token 为 restore, B2 保持原样.
#[test]
fn undo_after_self_heal_restores_the_user_backup_not_the_heal_backup() {
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
    let user_backup = backup_path_from_output(&applied.stdout);
    env.assert_content(&user_backup, "original\n");

    let repo = physical_repo(&env);
    fs::rename(repo.join("shell/bashrc"), repo.join("shell/bashrc2")).expect("改名源文件");
    env.write_manifest(&manifest_of(&[(
        "shell/bashrc2",
        "~/.bashrc",
        "replace_and_backup",
    )]));

    let healed = run(&env, &["apply"]);
    assert_eq!(healed.code, 0, "{}", healed.stdout);
    let heal = heal_backup_from_output(&healed.stdout);
    let heal_text = fs::read_link(&heal).expect("自愈备份必须是链接");
    assert_eq!(heal_text, repo.join("shell/bashrc"));
    assert_eq!(
        backups_in(env.home()),
        vec![
            user_backup
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        ],
        "自愈后用户备份集合不得变化"
    );

    let undone = run(&env, &["undo"]);
    assert_eq!(undone.code, 1, "{}", undone.stdout);
    let line = line_for(&undone.stdout, "~/.bashrc");
    assert_eq!(token_of(line), "restore", "{line}");
    assert!(
        line.contains(&user_backup.display().to_string()),
        "restore 行必须给出用户备份路径: {line}"
    );
    assert!(
        !line.contains(&heal.display().to_string()),
        "restore 行不得使用自愈备份: {line}"
    );
    assert!(
        fs::symlink_metadata(&target).is_ok_and(|meta| !meta.file_type().is_symlink()),
        "恢复出的内容必须是用户原始文件"
    );
    env.assert_content(&target, "original\n");
    env.assert_content(&user_backup, "original\n");
    assert_eq!(
        fs::read_link(&heal).unwrap(),
        heal_text,
        "自愈备份不得被删除或改写"
    );
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
// (b2) 只有 `.partial` 残留: 视为没有可用备份, 只移除链接而不恢复
// ---------------------------------------------------------------------------

#[test]
fn undo_ignores_a_partial_only_backup_and_still_removes_the_link() {
    let env = TempEnv::new();
    env.write_manifest(&manifest_of(&[("shell/bashrc", "~/.bashrc", "-")]));
    let target = deployed(&env, "shell/bashrc", ".bashrc");
    let partial = env
        .home()
        .join(".bashrc.dotlink-backup.20990101T000000.partial");
    fs::write(&partial, "half\n").unwrap();

    let undone = run(&env, &["undo"]);
    assert_eq!(undone.code, 1, "{}", undone.stdout);
    let line = line_for(&undone.stdout, "~/.bashrc");
    assert_eq!(token_of(line), "undo", "{line}");
    assert!(
        fs::symlink_metadata(&target).is_err(),
        "只有残留时必须只移除链接"
    );
    // 残留不参与查找, 也不得被删除或当成"没有备份"的理由清理.
    env.assert_content(&partial, "half\n");
}

// ---------------------------------------------------------------------------
// (b3) 源缺失: undo 报 missing 并计入失败列 (不因目标不存在而归为 ok)
// ---------------------------------------------------------------------------

#[test]
fn undo_reports_missing_for_a_source_missing_entry() {
    let env = TempEnv::new();
    env.write_manifest(&manifest_of(&[("shell/gone", "~/.gone", "-")]));

    let undone = run(&env, &["undo"]);
    assert_eq!(undone.code, 1, "{}", undone.stdout);
    let line = line_for(&undone.stdout, "~/.gone");
    assert_eq!(token_of(line), "missing", "{line}");
    assert!(
        undone
            .stdout
            .lines()
            .last()
            .unwrap_or_default()
            .contains("1 失败"),
        "源缺失必须计入失败列: {}",
        undone.stdout
    );
}

// ---------------------------------------------------------------------------
// (c) 安全反例 (真实文件不被删除): 手工换成真实目录 -> skip 且目录完整保留
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
// (d2) 悬空链接的归属: 仓库内仍属本工具 (可移除), 仓库外不是 (跳过)
// ---------------------------------------------------------------------------

/// 仓库内已失效的链接 (`path-normalization` 的 "指向仓库内但已失效"): 即使目标不
/// 存在, 仍按链接文本落在仓库内判为 `LinkedWrong`, 撤销必须移除它而不是跳过.
#[test]
fn undo_removes_a_dangling_link_that_points_inside_the_repo() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_manifest(&manifest_of(&[("shell/bashrc", "~/.bashrc", "-")]));
    let target = env.home().join(".bashrc");

    // 链接文本落在仓库内, 但指向的源文件从未存在 -> 悬空的仓库内链接.
    let gone = env.repo().join("shell/deleted");
    std::os::unix::fs::symlink(&gone, &target).expect("建立悬空链接");
    assert!(
        fs::symlink_metadata(&gone).is_err(),
        "夹具要求链接文本指向不存在的路径"
    );

    let undone = run(&env, &["undo"]);

    assert_token(&undone, "undo");
    assert_eq!(undone.code, 1, "撤销了本工具的链接, 目标谓词在动手前为假");
    assert!(
        fs::symlink_metadata(&target).is_err(),
        "仓库内的悬空链接是本工具产物, undo 必须移除它"
    );
    assert!(
        env.repo().join("shell/bashrc").exists(),
        "移除链接不得触碰仓库中的源"
    );
}

/// 仓库外已失效的链接: 链接文本落在仓库外, 仍判为"别人的链接", 撤销必须跳过且
/// 保持链接文本原样 (即使它指向的目标不存在).
#[test]
fn undo_skips_a_dangling_link_that_points_outside_the_repo() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_manifest(&manifest_of(&[("shell/bashrc", "~/.bashrc", "-")]));
    let target = env.home().join(".bashrc");

    let outside = tempfile::tempdir().expect("创建仓库之外的目录");
    let gone = outside.path().join("gone");
    std::os::unix::fs::symlink(&gone, &target).expect("建立悬空链接");
    assert!(
        !gone.starts_with(env.repo()),
        "构造 foreign 的路径必须在仓库之外"
    );
    assert!(fs::symlink_metadata(&gone).is_err(), "夹具要求链接悬空");

    let undone = run(&env, &["undo"]);

    assert_token(&undone, "skip");
    assert_eq!(undone.code, 1, "跳过的条目使退出码非零");
    assert_eq!(
        fs::read_link(&target).expect("外部悬空链接必须原样保留"),
        gone,
        "别人的链接即使悬空也不得被删除或改写"
    );
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
    for (round, path) in backups.iter().enumerate() {
        assert!(
            fs::symlink_metadata(path).is_ok(),
            "备份文件本身必须保留: {}",
            path.display()
        );
        // 备份只被复制, 不得被消费或改写: 第 round 份的内容仍是 backup-{round}.
        env.assert_content(path, &format!("backup-{round}"));
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
    env.write_source("shell/bashrc", "managed-bash\n");
    env.write_manifest(&manifest_of(&[
        ("shell/zshrc", "~/.locked/rc", "-"),
        ("shell/bashrc", "~/.free/rc", "-"),
    ]));

    let applied = run(&env, &["apply"]);
    assert_eq!(applied.code, 0, "{}", applied.stdout);

    let locked = env.home().join(".locked");
    let target = locked.join("rc");
    let free_target = env.home().join(".free/rc");
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

    // 故障只作用于那一个目录; 另一目录的条目照常撤销.
    let free_line = line_for(&undone.stdout, "~/.free/rc");
    assert_eq!(
        token_of(free_line),
        "undo",
        "其余条目必须照常处理: {free_line}"
    );
    assert!(
        fs::symlink_metadata(&free_target).is_err(),
        "其余目录的链接应被正常移除"
    );

    let summary = undone.stdout.lines().last().unwrap_or_default();
    assert_eq!(
        summary, "2 个条目: 1 成功, 0 已是最新, 0 未处理, 1 失败",
        "故障条目计入失败列, 其余条目照常完成: {}",
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

// ---------------------------------------------------------------------------
// 恢复阶段的备份读取故障: 报 fail, 链接回滚, 恢复权限后可重试
// ---------------------------------------------------------------------------

/// 备份文件已完整命名但内容不可读 (chmod 000): 恢复阶段的复制必然失败. 该条目必须
/// 报 `fail` 并给出底层原因; 链接被回滚到原路径, 备份不被消费, 因此恢复权限后再次
/// `undo` 仍能成功恢复 -- 这是"撤销失败可重试"的端到端证据.
#[test]
fn undo_reports_fail_and_can_be_retried_when_the_backup_is_unreadable() {
    if running_as_root() {
        eprintln!("以 root 运行: chmod 000 不生效, 跳过备份不可读的 undo 断言");
        return;
    }

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
    assert_eq!(applied.code, 0, "前置部署必须成功: {}", applied.stdout);
    let backup = backup_path_from_output(&applied.stdout);
    env.assert_content(&backup, "original\n");

    fs::set_permissions(&backup, fs::Permissions::from_mode(0o000)).unwrap();
    let first = run(&env, &["undo"]);
    // 立即放宽备份权限: 后续断言与重试都要求可读.
    fs::set_permissions(&backup, fs::Permissions::from_mode(0o644)).unwrap();

    let line = line_for(&first.stdout, "~/.bashrc");
    assert_eq!(token_of(line), "fail", "{line}");
    assert_eq!(first.code, 1, "stdout:\n{}", first.stdout);
    assert!(
        line.contains("Permission denied"),
        "fail 行必须给出底层原因: {line}"
    );

    // 失败不得消费链接与备份: 目标仍是本工具的链接, 备份原样留在磁盘上.
    assert!(
        fs::symlink_metadata(&target)
            .unwrap()
            .file_type()
            .is_symlink(),
        "恢复失败时链接必须回滚到原路径, 以便重试"
    );
    env.assert_symlink_to(&target, &env.repo().join("shell/bashrc"));
    env.assert_content(&backup, "original\n");

    // 权限恢复后重试: 链接还在, 因此走通恢复; 退出码 1 表示本次确实撤销了东西.
    let second = run(&env, &["undo"]);
    let second_line = line_for(&second.stdout, "~/.bashrc");
    assert_eq!(token_of(second_line), "restore", "{second_line}");
    assert_eq!(
        second.code, 1,
        "撤销了东西 (restore) 故退 1; stdout:\n{}",
        second.stdout
    );
    env.assert_content(&target, "original\n");
    assert!(
        fs::symlink_metadata(&target).unwrap().file_type().is_file(),
        "恢复后目标必须是普通文件, 不再是链接"
    );
    env.assert_content(&backup, "original\n");
}

// ---------------------------------------------------------------------------
// 输出契约: 备份路径中的控制字符同样转义 (cli-contract 的"报告中的控制字符必须转义")
// ---------------------------------------------------------------------------

/// target 的文件名含 ESC/BEL 时, apply 生成的备份文件名也带着它们. `undo` 的
/// `restore` 说明会给出备份完整路径, 该路径属于"备份路径"这一类动态字段, 必须在
/// 输出中转义, 不能把原始控制字节送进终端.
#[test]
fn undo_escapes_control_characters_in_the_restored_backup_path() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    // TOML 基本字符串用 `\u001b` / `\u0007` 表达 ESC / BEL; 原始控制字节不能直接写进清单.
    let target_name = ".bashrc\u{1b}\u{7}";
    env.write_manifest(&manifest_of(&[(
        "shell/bashrc",
        r"~/.bashrc\u001b\u0007",
        "replace_and_backup",
    )]));

    let target = env.home().join(target_name);
    fs::write(&target, "original\n").unwrap();

    let applied = run(&env, &["apply"]);
    assert_eq!(applied.code, 0, "前置部署必须成功: {}", applied.stdout);
    assert!(
        !applied.stdout.as_bytes().contains(&0x1b) && !applied.stdout.as_bytes().contains(&0x07),
        "apply 输出含未转义 ESC/BEL: {:?}",
        applied.stdout
    );
    let applied_line = line_for(&applied.stdout, r"~/.bashrc\u{1b}\u{7}");
    assert_eq!(token_of(applied_line), "replace", "{applied_line}");
    assert!(
        applied_line.contains(r".bashrc\u{1b}\u{7}.dotlink-backup."),
        "备份路径必须显示转义形式: {applied_line}"
    );
    assert_eq!(backups_in(env.home()).len(), 1, "apply 应留下恰好一份备份");

    let undone = run(&env, &["undo"]);
    assert_eq!(
        undone.code, 1,
        "撤销了东西 (restore) 故退 1; stdout:\n{}",
        undone.stdout
    );
    assert!(
        !undone.stdout.as_bytes().contains(&0x1b) && !undone.stdout.as_bytes().contains(&0x07),
        "undo 输出含未转义 ESC/BEL: {:?}",
        undone.stdout
    );
    assert_eq!(
        undone.stdout.lines().count(),
        2,
        "条目与汇总各占一行: {:?}",
        undone.stdout
    );
    let line = line_for(&undone.stdout, r"~/.bashrc\u{1b}\u{7}");
    assert_eq!(token_of(line), "restore", "{line}");
    assert!(
        line.contains(r".bashrc\u{1b}\u{7}.dotlink-backup."),
        "恢复说明必须显示转义后的备份路径: {line}"
    );
    env.assert_content(&target, "original\n");
}

//! US3 / FR-007 / FR-008: 冲突的安全处理 (openspec/specs/link-deployment/spec.md 的
//! "未获许可的冲突保持原样" 与 "获许可的替换先完成备份再删除原对象").
//!
//! 覆盖 (a)-(j). 本文件经真实二进制断言, 因此**依赖 `src/lib.rs` 的命令接入**.
//! 在那些接入完成之前, 这里的断言必然失败 -- 这是预期的 TDD 红阶段 (测试先于实现
//! 存在), 不得为了让本文件变绿而改动 `src/lib.rs`.

mod support;

use std::fs;
use std::path::Path;

use support::{
    TempEnv, assert_token, backup_path_from_output, backups_in, heal_backup_from_output,
    heal_backups_in, line_for, manifest_of, run, running_as_root, token_of, unlock,
};

// ---------------------------------------------------------------------------
// (a) 默认 skip: 原文件逐字未变, 且无备份文件产生
// ---------------------------------------------------------------------------

#[test]
fn plain_file_occupied_target_is_refused_by_default() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_manifest(&manifest_of(&[("shell/bashrc", "~/.bashrc", "-")]));

    let target = env.home().join(".bashrc");
    fs::write(&target, "pre-existing\n").unwrap();

    let run = run(&env, &["apply"]);
    assert_token(&run, "conflict");
    assert_eq!(run.code, 1, "被拒绝的条目使退出码为 1");

    // apply 侧同样必须给出冲突原因与占用物类型 (不只是 preview 侧).
    let line = line_for(&run.stdout, "~/.bashrc");
    assert!(
        line.contains("未覆盖") && line.contains("普通文件"),
        "apply 的 conflict 行必须给出未覆盖与占用物类型: {line}"
    );

    env.assert_content(&target, "pre-existing\n");
    assert!(masks_as_regular_file(&target), "该路径必须仍是普通文件");
    assert!(
        backups_in(env.home()).is_empty(),
        "拒绝覆盖时不得产生任何备份: {:?}",
        backups_in(env.home())
    );
}

/// 判断路径是否是"非符号链接"的普通文件 (lstat 语义).
fn masks_as_regular_file(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_file())
}

// ---------------------------------------------------------------------------
// (b) replace_and_backup: 输出给出备份完整路径, 备份内容等于原内容, 目标成为正确链接
// ---------------------------------------------------------------------------

#[test]
fn replace_and_backup_replaces_a_file_and_reports_the_backup_path() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_manifest(&manifest_of(&[(
        "shell/bashrc",
        "~/.bashrc",
        "replace_and_backup",
    )]));

    let target = env.home().join(".bashrc");
    fs::write(&target, "pre-existing\n").unwrap();

    let run = run(&env, &["apply"]);
    assert_token(&run, "replace");
    assert_eq!(run.code, 0, "全部条目成功且无遗留待办");

    let backup = backup_path_from_output(&run.stdout);
    assert_eq!(backup.parent(), Some(env.home()));
    assert!(
        backup
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(".bashrc.dotlink-backup."),
        "备份名必须基于原文件名: {}",
        backup.display()
    );
    assert!(
        heal_backups_in(env.home()).is_empty(),
        "外部占用的备份不得使用自愈命名空间: {:?}",
        heal_backups_in(env.home())
    );
    env.assert_content(&backup, "pre-existing\n");

    env.assert_symlink_to(&target, &env.repo().join("shell/bashrc"));
    env.assert_content(&target, "managed\n");
}

// ---------------------------------------------------------------------------
// (c) 目标为目录 (非链接) 时默认为 conflict, 目录及其内容未被删除
// ---------------------------------------------------------------------------

#[test]
fn directory_occupied_target_is_refused_and_left_intact() {
    let env = TempEnv::new();
    env.write_source("nvim/init.lua", "managed\n");
    env.write_manifest(&manifest_of(&[("nvim/init.lua", "~/.config/nvim", "-")]));

    let target = env.home().join(".config/nvim");
    fs::create_dir_all(target.join("lua")).unwrap();
    fs::write(target.join("init.lua"), "pre-existing\n").unwrap();
    fs::write(target.join("lua/a.lua"), "nested\n").unwrap();

    let run = run(&env, &["apply"]);
    assert_token(&run, "conflict");
    assert_eq!(run.code, 1);

    assert!(fs::symlink_metadata(&target).unwrap().is_dir());
    env.assert_content(&target.join("init.lua"), "pre-existing\n");
    env.assert_content(&target.join("lua/a.lua"), "nested\n");
    assert!(backups_in(env.home()).is_empty());
}

// ---------------------------------------------------------------------------
// (d) 指向仓库之外的符号链接默认为 conflict
// ---------------------------------------------------------------------------

#[test]
fn foreign_link_is_refused_by_default() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_manifest(&manifest_of(&[("shell/bashrc", "~/.bashrc", "-")]));

    // 链接必须真的落在仓库之外, 否则它只是"本工具的失效链接" (`wrong`), 走不到
    // `foreign` 这一分类. 仓库根与 HOME 各自是临时目录, 仓库外另取一个临时目录.
    let outside = tempfile::tempdir().expect("创建仓库之外的目录");
    let elsewhere = outside.path().join("elsewhere");
    fs::write(&elsewhere, "foreign\n").unwrap();
    let target = env.home().join(".bashrc");
    std::os::unix::fs::symlink(&elsewhere, &target).unwrap();

    let run = run(&env, &["apply"]);
    assert_token(&run, "conflict");
    assert_eq!(run.code, 1);

    assert!(
        fs::symlink_metadata(&target)
            .unwrap()
            .file_type()
            .is_symlink(),
        "外部链接必须原样保留"
    );
    assert_eq!(fs::read_link(&target).unwrap(), elsewhere);
    assert_eq!(fs::read_to_string(&elsewhere).unwrap(), "foreign\n");
    assert!(backups_in(env.home()).is_empty());
}

// ---------------------------------------------------------------------------
// (e) 指向错误位置的链接: 备份的是链接本身 (不跟随)
// ---------------------------------------------------------------------------

#[test]
fn replace_and_backup_copies_the_link_itself() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_manifest(&manifest_of(&[(
        "shell/bashrc",
        "~/.bashrc",
        "replace_and_backup",
    )]));

    // 链接落在仓库内 (因此是可备份的"本工具产物"), 但指向的不是清单声明的源.
    let stale = env.repo().join("shell/stale");
    fs::write(&stale, "stale\n").unwrap();
    let target = env.home().join(".bashrc");
    std::os::unix::fs::symlink(&stale, &target).unwrap();

    let run = run(&env, &["apply"]);
    assert_token(&run, "replace");
    assert_eq!(run.code, 0);

    let backup = heal_backup_from_output(&run.stdout);
    assert!(
        fs::symlink_metadata(&backup)
            .unwrap()
            .file_type()
            .is_symlink(),
        "备份的必须是链接本身, 而非其目标的内容: {}",
        backup.display()
    );
    assert_eq!(
        fs::read_link(&backup).unwrap(),
        stale,
        "备份应保留链接的原始文本"
    );
    assert!(
        backup
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains(".dotlink-heal."),
        "本工具错误链接的备份必须使用自愈命名空间: {}",
        backup.display()
    );
    assert!(backups_in(env.home()).is_empty(), "不得新增用户备份");
    assert!(stale.is_file(), "原链接的目标不得被删除");
    env.assert_symlink_to(&target, &env.repo().join("shell/bashrc"));
}

// ---------------------------------------------------------------------------
// (e2) 指向仓库之外的链接在允许覆盖时同样只备份链接本身 (link-deployment 的
//      "替换别人的符号链接")
// ---------------------------------------------------------------------------

#[test]
fn replace_and_backup_copies_a_foreign_link_without_following_it() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_manifest(&manifest_of(&[(
        "shell/bashrc",
        "~/.bashrc",
        "replace_and_backup",
    )]));

    // 链接落在仓库之外: 状态是 `ForeignLink` (别人的链接), 与 (e) 的
    // `LinkedWrong` 走同一条 `ReplaceAndBackup` 动作, 但分类路径不同.
    let outside = tempfile::tempdir().expect("创建仓库之外的目录");
    let elsewhere = outside.path().join("elsewhere");
    fs::write(&elsewhere, "foreign\n").unwrap();
    let target = env.home().join(".bashrc");
    std::os::unix::fs::symlink(&elsewhere, &target).unwrap();

    let run = run(&env, &["apply"]);
    assert_token(&run, "replace");
    assert_eq!(run.code, 0, "{}", run.stdout);

    let backup = backup_path_from_output(&run.stdout);
    assert!(
        fs::symlink_metadata(&backup)
            .unwrap()
            .file_type()
            .is_symlink(),
        "备份的必须是链接本身, 而非其目标的内容: {}",
        backup.display()
    );
    assert_eq!(
        fs::read_link(&backup).unwrap(),
        elsewhere,
        "备份应保留链接的原始文本"
    );
    assert_eq!(
        fs::read_to_string(&elsewhere).unwrap(),
        "foreign\n",
        "原链接指向的内容不得被删除或改动"
    );
    env.assert_symlink_to(&target, &env.repo().join("shell/bashrc"));
}

// ---------------------------------------------------------------------------
// (f) 目录递归备份, 且不残留 .partial
// ---------------------------------------------------------------------------
#[test]
fn replace_and_backup_copies_a_directory_recursively_without_partial_residue() {
    let env = TempEnv::new();
    env.write_source("nvim/init.lua", "managed\n");
    env.write_manifest(&manifest_of(&[(
        "nvim/init.lua",
        "~/.config/nvim",
        "replace_and_backup",
    )]));

    let target = env.home().join(".config/nvim");
    fs::create_dir_all(target.join("lua/plugins")).unwrap();
    fs::write(target.join("init.lua"), "old-init\n").unwrap();
    fs::write(target.join("lua/plugins/a.lua"), "old-a\n").unwrap();

    let run = run(&env, &["apply"]);
    assert_token(&run, "replace");
    assert_eq!(run.code, 0);

    let backup = backup_path_from_output(&run.stdout);
    assert!(fs::symlink_metadata(&backup).unwrap().is_dir());
    env.assert_content(&backup.join("init.lua"), "old-init\n");
    env.assert_content(&backup.join("lua/plugins/a.lua"), "old-a\n");

    // 备份一步到位: 目录下只有最终名, 不得残留 `.partial`.
    let leftovers = env.list_dir(backup.parent().unwrap());
    assert!(
        !leftovers.iter().any(|name| name.contains(".partial")),
        "备份成功后不得残留 .partial: {leftovers:?}"
    );
    assert_eq!(
        leftovers
            .iter()
            .filter(|n| n.contains(".dotlink-backup."))
            .count(),
        1,
        "只应有一个最终名的备份: {leftovers:?}"
    );

    env.assert_symlink_to(&target, &env.repo().join("nvim/init.lua"));
}

// ---------------------------------------------------------------------------
// (g) 冲突跳过的条目不影响其余条目 (FR-014)
// ---------------------------------------------------------------------------

#[test]
fn a_refused_entry_does_not_block_the_others() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_source("shell/zshrc", "managed-zsh\n");
    env.write_manifest(&manifest_of(&[
        ("shell/bashrc", "~/.bashrc", "-"),
        ("shell/zshrc", "~/.zshrc", "-"),
    ]));

    fs::write(env.home().join(".bashrc"), "pre-existing\n").unwrap();

    let run = run(&env, &["apply"]);
    assert_eq!(run.code, 1, "存在被拒绝的条目");

    assert_token(&run, "conflict");
    assert_token(&run, "create");

    let refused = env.home().join(".bashrc");
    env.assert_content(&refused, "pre-existing\n");
    let created = env.home().join(".zshrc");
    env.assert_symlink_to(&created, &env.repo().join("shell/zshrc"));
}

// ---------------------------------------------------------------------------
// (h) 同一秒内的两次替换: 命名消歧, 先前的备份内容不被覆盖
// ---------------------------------------------------------------------------

#[test]
fn two_replacements_in_the_same_second_keep_both_backups() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_manifest(&manifest_of(&[(
        "shell/bashrc",
        "~/.bashrc",
        "replace_and_backup",
    )]));

    let target = env.home().join(".bashrc");
    fs::write(&target, "first\n").unwrap();

    let first = run(&env, &["apply"]);
    assert_token(&first, "replace");
    assert_eq!(first.code, 0);
    let first_backup = backup_path_from_output(&first.stdout);
    env.assert_content(&first_backup, "first\n");

    // 立刻再替换一次 (通常落在同一秒内): 撤销后目标路径空出, 重新布置另一份内容.
    let undone = run(&env, &["undo"]);
    assert_eq!(
        undone.code, 1,
        "undo 撤销了已部署的链接 (目标谓词在动手之前求值), 故退 1: {}",
        undone.stdout
    );
    env.assert_content(&target, "first\n");

    fs::remove_file(&target).unwrap();
    fs::write(&target, "second\n").unwrap();

    let second = run(&env, &["apply"]);
    assert_token(&second, "replace");
    assert_eq!(second.code, 0);
    let second_backup = backup_path_from_output(&second.stdout);

    assert_ne!(
        first_backup, second_backup,
        "两次替换必须产生两个并存的备份文件"
    );
    env.assert_content(&first_backup, "first\n");
    env.assert_content(&second_backup, "second\n");

    let names = backups_in(env.home());
    assert_eq!(names.len(), 2, "两个备份必须并存: {names:?}");
    let first_name = first_backup
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let second_name = second_backup
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();

    // 时间戳 (含 'T') 之后的多余段就是消歧序号.
    let serial_of = |name: &str| {
        name.split('T')
            .nth(1)
            .and_then(|rest| rest.split_once('.').map(|(_, serial)| serial.to_string()))
    };
    // 备份名中 `<原名>.dotlink-backup.` 之后的部分再取时间戳段 (`<15 位时间戳>[.<序号>]`).
    let stamp_of = |name: &str| {
        name.split_once(".dotlink-backup.")
            .map(|(_, rest)| rest.split('.').next().unwrap_or_default().to_string())
            .unwrap_or_else(|| panic!("{name} 不是备份名"))
    };
    assert_eq!(
        serial_of(&first_name),
        None,
        "第一个备份不应带序号: {first_name}"
    );

    // 两次替换跨过秒边界时, 第二次拿到的是新时间戳的裸名而非序号 -- 这同样满足
    // "两个并存且互不覆盖". 序号断言仅在确属同一秒时做出, 使本测试不因墙钟走动而
    // 抖动 (与 `src/backup.rs` 的同名单测同一处置).
    if stamp_of(&second_name) == stamp_of(&first_name) {
        assert_eq!(
            serial_of(&second_name).as_deref(),
            Some("2"),
            "同一秒内的第二个备份必须以序号消歧: {second_name}"
        );
    } else {
        assert_eq!(
            serial_of(&second_name),
            None,
            "跨秒的第二次替换应得到新时间戳的裸名: {second_name}"
        );
        assert!(
            stamp_of(&second_name) > stamp_of(&first_name),
            "跨秒后的新时间戳必须更晚: {first_name} -> {second_name}"
        );
        eprintln!("两次替换跨过秒边界, 序号消歧未被触发: {first_name} -> {second_name}");
    }
}

// ---------------------------------------------------------------------------
// (i) 备份失败的原子性: 原对象保留, 且磁盘上不存在最终名的备份
// ---------------------------------------------------------------------------

#[test]
fn a_failed_backup_keeps_the_original_and_leaves_no_final_name() {
    if running_as_root() {
        eprintln!("以 root 运行: chmod 000 不生效, 跳过备份失败原子性断言");
        return;
    }

    let env = TempEnv::new();
    env.write_source("nvim/init.lua", "managed\n");
    env.write_manifest(&manifest_of(&[(
        "nvim/init.lua",
        "~/.config/nvim",
        "replace_and_backup",
    )]));

    let target = env.home().join(".config/nvim");
    let locked = target.join("locked");
    fs::create_dir_all(&locked).unwrap();
    fs::write(locked.join("secret"), "secret\n").unwrap();
    fs::write(target.join("init.lua"), "old\n").unwrap();
    fs::set_permissions(&locked, std::os::unix::fs::PermissionsExt::from_mode(0o000)).unwrap();

    let run = run(&env, &["apply"]);
    assert_token(&run, "fail");
    assert_eq!(run.code, 1, "条目失败使退出码为 1");

    // 原对象必须完整保留在原路径 (顺序颠倒即数据丢失).
    assert!(fs::symlink_metadata(&target).unwrap().is_dir());
    env.assert_content(&target.join("init.lua"), "old\n");
    fs::set_permissions(&locked, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    env.assert_content(&locked.join("secret"), "secret\n");

    // 磁盘上不得存在最终名的备份: 中断残留只能是 `.partial`.
    for name in backups_in(env.home()) {
        assert!(
            name.ends_with(".partial"),
            "{name} 不是 .partial 残留, 会被误认为完整备份"
        );
    }
}

// ---------------------------------------------------------------------------
// (j) 备份成功但替换失败: 失败行给出备份路径与暂存路径
// ---------------------------------------------------------------------------

/// 构造: 目标目录内含一个只读可进的子目录 (`0o555`), 备份能读取它, 但删除暂存副本时
/// 无法清空它 -- 于是备份已完整落定而替换失败, 正是"原内容已备份但目标未就位"的
/// 情形. 这条失败行必须给出备份路径 (FR-016) 与暂存路径; 目标路径已原子移入暂存名,
/// 不得留下半删除的原路径.
#[test]
fn a_failed_replacement_reports_the_backup_path_and_stages_the_original() {
    if running_as_root() {
        eprintln!("以 root 运行: chmod 555 不生效, 跳过替换失败的备份路径断言");
        return;
    }

    let env = TempEnv::new();
    env.write_source("nvim/init.lua", "managed\n");
    env.write_manifest(&manifest_of(&[(
        "nvim/init.lua",
        "~/.config/nvim",
        "replace_and_backup",
    )]));

    let target = env.home().join(".config/nvim");
    let locked = target.join("locked");
    fs::create_dir_all(&locked).unwrap();
    fs::write(locked.join("secret"), "secret\n").unwrap();
    fs::write(target.join("init.lua"), "old\n").unwrap();
    fs::set_permissions(&locked, std::os::unix::fs::PermissionsExt::from_mode(0o555)).unwrap();

    let run = run(&env, &["apply"]);

    // 删除失败时原对象已原子移入暂存名; 暂存树与备份都保留只读子目录的权限位, 两者
    // 都要在断言之前恢复, 否则临时目录清理会因无法清空 0o555 子目录而失败.
    let backup = backup_path_from_output(&run.stdout);
    let staged = fs::read_dir(target.parent().unwrap())
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().contains(".dotlink-replace.partial"))
        })
        .expect("替换失败时应留下暂存目录");
    unlock(&staged.join("locked"));
    unlock(&backup.join("locked"));

    let line = line_for(&run.stdout, "~/.config/nvim");
    assert_eq!(token_of(line), "fail", "{line}");
    assert_eq!(run.code, 1, "条目失败使退出码为 1");

    // 失败行必须给出备份路径与暂存路径, 且两者真实存在.
    assert!(
        line.contains("原内容已备份到"),
        "替换失败的 fail 行必须说明原内容已备份: {line}"
    );
    assert!(
        line.contains(&backup.display().to_string()),
        "fail 行必须给出备份文件的完整路径: {line}"
    );
    assert!(
        line.contains(&staged.display().to_string()),
        "fail 行必须给出暂存目录的完整路径: {line}"
    );
    assert!(
        fs::symlink_metadata(&backup).unwrap().is_dir(),
        "备份的必须是原目录: {}",
        backup.display()
    );
    env.assert_content(&backup.join("init.lua"), "old\n");
    env.assert_content(&backup.join("locked/secret"), "secret\n");

    // 目标已原子移入暂存名, 不得留下半删除的原路径; 原内容仍可从暂存树找回.
    assert!(
        fs::symlink_metadata(&target).is_err(),
        "目标应已移入暂存名: {}",
        target.display()
    );
    env.assert_content(&staged.join("locked/secret"), "secret\n");
}

//! US3 / FR-007 / FR-008 / SC-004: 冲突的安全处理 (tasks.md 的 T022 全部条目).
//!
//! 覆盖 (a)-(i). 本文件经真实二进制断言, 因此**依赖 `src/lib.rs` 的命令接入**
//! (T015 / T026 / T031). 在那些接入完成之前, 这里的断言必然失败 -- 这是预期的
//! TDD 红阶段 (测试先于实现存在), 不得为了让本文件变绿而改动 `src/lib.rs`.

mod support;

use std::fs;
use std::path::{Path, PathBuf};

use support::TempEnv;

/// 一次命令调用的观测结果.
struct Run {
    code: i32,
    stdout: String,
    #[allow(dead_code)]
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

/// 构造清单: 每个条目为 `(source, target, on_conflict)`, 策略写 `"-"` 表示缺省.
fn manifest_of(entries: &[(&str, &str, &str)]) -> String {
    let mut text = String::new();
    for (source, target, policy) in entries {
        text.push_str("[[entry]]\n");
        text.push_str(&format!("source = \"{source}\"\n"));
        text.push_str(&format!("target = \"{target}\"\n"));
        if *policy != "-" {
            text.push_str(&format!("on_conflict = \"{policy}\"\n"));
        }
    }
    text
}

/// 某目录下所有备份文件名 (含最终名与 `.partial` 残留).
fn backups_in(dir: &Path) -> Vec<String> {
    let names = fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("列举 {} 失败: {error}", dir.display()))
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".dotlink-backup."))
        .collect::<Vec<_>>();
    let mut names = names;
    names.sort();
    names
}

/// 从命令输出中取出备份文件的完整路径 (FR-016 的"必现信息").
#[track_caller]
fn backup_path_from_output(stdout: &str) -> PathBuf {
    for word in stdout.split_whitespace() {
        if !word.contains(".dotlink-backup.") {
            continue;
        }
        for candidate in [
            word.to_string(),
            word.trim_end_matches([',', ';', '.', ')']).to_string(),
        ] {
            let path = PathBuf::from(&candidate);
            if fs::symlink_metadata(&path).is_ok() {
                return path;
            }
        }
    }
    panic!("输出中未给出可定位的备份文件完整路径: {stdout}");
}

#[track_caller]
fn assert_token(run: &Run, token: &str) {
    assert!(
        run.stdout.split_whitespace().any(|word| word == token),
        "输出中缺少 token {token}, 实际输出: {}",
        run.stdout
    );
}

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

    let elsewhere = env.repo().join("elsewhere");
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
        stale,
        "备份应保留链接的原始文本"
    );
    assert!(stale.is_file(), "原链接的目标不得被删除");
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
    assert_eq!(
        serial_of(&first_name),
        None,
        "第一个备份不应带序号: {first_name}"
    );
    assert_eq!(
        serial_of(&second_name).as_deref(),
        Some("2"),
        "第二个备份必须带序号 2: {second_name}"
    );
}

// ---------------------------------------------------------------------------
// (i) 备份失败的原子性: 原对象保留, 且磁盘上不存在最终名的备份
// ---------------------------------------------------------------------------

/// 以 root 运行时权限位无效 (chmod 000 不阻止读取), 该断言只能跳过.
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

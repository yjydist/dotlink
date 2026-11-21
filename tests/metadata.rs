//! 备份保真: 端到端验证 `apply` -> 备份 -> `undo` 恢复保留 mtime 与用户扩展属性.
//!
//! 依赖 macOS/Linux 的扩展属性与 `utimensat`; 扩展属性不受支持时跳过该断言并
//! 说明原因 (mtime 断言仍必须成立).

#![cfg(any(target_os = "macos", target_os = "linux"))]

mod support;

use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use rustix::fs::Timespec;
use rustix::fs::{AtFlags, CWD, Timestamps, UTIME_OMIT, XattrFlags, getxattr, setxattr, utimensat};

use support::{TempEnv, backups_in, manifest_of, run};

const MTIME_SECONDS: i64 = 1_600_000_000;
const MTIME_NANOSECONDS: i64 = 123_456_789;
const XATTR_NAME: &str = "user.test";
const XATTR_VALUE: &[u8] = b"dotlink-fidelity";

fn set_known_mtime(path: &Path) {
    let times = Timestamps {
        last_access: Timespec {
            tv_sec: 0,
            tv_nsec: UTIME_OMIT,
        },
        last_modification: Timespec {
            tv_sec: MTIME_SECONDS,
            tv_nsec: MTIME_NANOSECONDS,
        },
    };
    utimensat(CWD, path, &times, AtFlags::empty()).expect("设置测试 mtime");
}

fn try_set_user_xattr(path: &Path) -> bool {
    match setxattr(path, XATTR_NAME, XATTR_VALUE, XattrFlags::empty()) {
        Ok(()) => true,
        Err(error) => {
            eprintln!("跳过扩展属性断言: 当前环境无法设置 {XATTR_NAME} ({error})");
            false
        }
    }
}

fn read_user_xattr(path: &Path) -> Option<Vec<u8>> {
    let mut buffer = vec![0_u8; 1024];
    match getxattr(path, XATTR_NAME, &mut buffer[..]) {
        Ok(length) => Some(buffer[..length].to_vec()),
        Err(error) if is_missing_xattr(error) => None,
        Err(error) => panic!("读取 {XATTR_NAME} 失败: {error}"),
    }
}

#[cfg(target_os = "macos")]
fn is_missing_xattr(error: rustix::io::Errno) -> bool {
    error == rustix::io::Errno::NODATA || error == rustix::io::Errno::NOATTR
}

#[cfg(not(target_os = "macos"))]
fn is_missing_xattr(error: rustix::io::Errno) -> bool {
    error == rustix::io::Errno::NODATA
}

#[track_caller]
fn assert_mtime(path: &Path) {
    let metadata = fs::metadata(path)
        .unwrap_or_else(|error| panic!("读取 {} 的元数据失败: {error}", path.display()));
    assert_eq!(
        (metadata.mtime(), metadata.mtime_nsec()),
        (MTIME_SECONDS, MTIME_NANOSECONDS),
        "{} 的 mtime 未被保留",
        path.display()
    );
}

/// 断言权限位 (`mode & 0o777`) 逐字等于 `expected`.
#[track_caller]
fn assert_mode(path: &Path, expected: u32) {
    let metadata = fs::symlink_metadata(path)
        .unwrap_or_else(|error| panic!("读取 {} 的元数据失败: {error}", path.display()));
    assert_eq!(
        metadata.permissions().mode() & 0o777,
        expected,
        "{} 的权限位未被保留",
        path.display()
    );
}

/// 在 `dir` 中找出原名 `original` 的备份路径.
#[track_caller]
fn backup_for(dir: &Path, original: &str) -> PathBuf {
    let prefix = format!("{original}.dotlink-backup.");
    let names = backups_in(dir);
    let name = names
        .iter()
        .find(|name| name.starts_with(&prefix))
        .unwrap_or_else(|| panic!("{dir:?} 中没有 {original} 的备份: {names:?}"));
    dir.join(name)
}

/// 备份与恢复两侧都必须保留文件 0640, 目录 0750 及嵌套子项的权限位.
#[test]
fn apply_and_undo_preserve_permission_bits_for_files_and_directories() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_source("nvim/init.lua", "managed-lua\n");
    env.write_manifest(&manifest_of(&[
        ("shell/bashrc", "~/.bashrc", "replace_and_backup"),
        ("nvim", "~/.config/nvim", "replace_and_backup"),
    ]));

    // 文件占用物: 0640.
    let file_target = env.home().join(".bashrc");
    fs::write(&file_target, "original\n").unwrap();
    fs::set_permissions(&file_target, fs::Permissions::from_mode(0o640)).unwrap();

    // 目录占用物: 根 0750, 嵌套目录 0750, 嵌套文件 0640.
    let dir_target = env.home().join(".config/nvim");
    fs::create_dir_all(dir_target.join("lua")).unwrap();
    fs::write(dir_target.join("init.lua"), "user\n").unwrap();
    fs::write(dir_target.join("lua/plugins.lua"), "user-plugins\n").unwrap();
    fs::set_permissions(&dir_target, fs::Permissions::from_mode(0o750)).unwrap();
    fs::set_permissions(dir_target.join("lua"), fs::Permissions::from_mode(0o750)).unwrap();
    fs::set_permissions(
        dir_target.join("init.lua"),
        fs::Permissions::from_mode(0o640),
    )
    .unwrap();
    fs::set_permissions(
        dir_target.join("lua/plugins.lua"),
        fs::Permissions::from_mode(0o640),
    )
    .unwrap();

    let applied = run(&env, &["apply"]);
    assert_eq!(applied.code, 0, "apply 失败:\n{}", applied.stdout);

    // 备份侧: 文件备份保留 0640; 目录备份保留 0750 与嵌套子项的权限位.
    let file_backup = backup_for(env.home(), ".bashrc");
    assert_mode(&file_backup, 0o640);
    let config = env.home().join(".config");
    let dir_backup = backup_for(&config, "nvim");
    assert_mode(&dir_backup, 0o750);
    assert_mode(&dir_backup.join("lua"), 0o750);
    assert_mode(&dir_backup.join("init.lua"), 0o640);
    assert_mode(&dir_backup.join("lua/plugins.lua"), 0o640);

    // 恢复侧: undo 从同一份备份恢复, 权限位必须与备份侧一致.
    let undone = run(&env, &["undo"]);
    assert_eq!(undone.code, 1, "undo 结果不符预期:\n{}", undone.stdout);
    assert_mode(&file_target, 0o640);
    assert_mode(&dir_target, 0o750);
    assert_mode(&dir_target.join("lua"), 0o750);
    assert_mode(&dir_target.join("init.lua"), 0o640);
    assert_mode(&dir_target.join("lua/plugins.lua"), 0o640);
    env.assert_content(&file_target, "original\n");
    env.assert_content(&dir_target.join("init.lua"), "user\n");
    env.assert_content(&dir_target.join("lua/plugins.lua"), "user-plugins\n");
}

/// 快照必须能发现 chmod 与 touch: 否则只读命令的"零改动"断言会静默放行.
///
/// 用例先在真实仓副本上制造变化并断言快照不同, 再把权限位改回; 默认套件中它始终
/// 通过, 不依赖"故意失败"的临时状态.
#[test]
fn snapshot_detects_permission_and_mtime_changes() {
    let env = TempEnv::new();
    env.write_source("probe", "content\n");
    let probe = env.repo().join("probe");
    let original_mode = fs::metadata(&probe).unwrap().permissions().mode() & 0o777;
    let baseline = env.snapshot(env.repo());

    let changed_mode = if original_mode == 0o600 { 0o640 } else { 0o600 };
    fs::set_permissions(&probe, fs::Permissions::from_mode(changed_mode)).unwrap();
    assert_ne!(
        baseline,
        env.snapshot(env.repo()),
        "快照必须能发现 chmod 造成的权限位变化"
    );

    fs::set_permissions(&probe, fs::Permissions::from_mode(original_mode)).unwrap();
    let restored = env.snapshot(env.repo());
    assert_eq!(baseline, restored, "恢复原权限位后快照必须回到原值");

    set_known_mtime(&probe);
    assert_ne!(
        restored,
        env.snapshot(env.repo()),
        "快照必须能发现 mtime 变化"
    );
}

#[test]
fn apply_and_undo_preserve_mtime_and_user_xattrs() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_manifest(&manifest_of(&[(
        "shell/bashrc",
        "~/.bashrc",
        "replace_and_backup",
    )]));

    let target = env.home().join(".bashrc");
    fs::write(&target, "original\n").unwrap();
    set_known_mtime(&target);
    let xattr_supported = try_set_user_xattr(&target);

    let applied = run(&env, &["apply"]);
    assert_eq!(applied.code, 0, "apply 失败:\n{}", applied.stdout);

    let backups = backups_in(env.home());
    assert_eq!(backups.len(), 1, "应恰好产生一份备份: {backups:?}");
    let backup = env.home().join(&backups[0]);
    assert_mtime(&backup);
    if xattr_supported {
        assert_eq!(
            read_user_xattr(&backup).as_deref(),
            Some(XATTR_VALUE),
            "备份未保留用户扩展属性"
        );
    }

    // undo 的目标谓词在动手之前求值: 确实恢复了一份备份, 因此退 1.
    let undone = run(&env, &["undo"]);
    assert_eq!(undone.code, 1, "undo 结果不符预期:\n{}", undone.stdout);
    assert!(undone.stdout.contains(" restore "), "{}", undone.stdout);
    assert_eq!(fs::read_to_string(&target).unwrap(), "original\n");
    assert_mtime(&target);
    if xattr_supported {
        assert_eq!(
            read_user_xattr(&target).as_deref(),
            Some(XATTR_VALUE),
            "恢复目标未保留用户扩展属性"
        );
    }
}

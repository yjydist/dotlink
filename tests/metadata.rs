//! 备份保真: 端到端验证 `apply` -> 备份 -> `undo` 恢复保留 mtime 与用户扩展属性.
//!
//! 依赖 macOS/Linux 的扩展属性与 `utimensat`; 扩展属性不受支持时跳过该断言并
//! 说明原因 (mtime 断言仍必须成立).

#![cfg(any(target_os = "macos", target_os = "linux"))]

mod support;

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

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

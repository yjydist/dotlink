//! `backup` 模块的单元测试.
//!
//! 从原 `backup.rs` 的内联测试原样移出; 断言与测试数量保持不变.

use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use time::{OffsetDateTime, UtcOffset};

use super::copy::*;
use super::name::*;
use super::test_support::*;
use super::*;
use crate::error::Error;
use crate::os;
use crate::os::MAX_STAGING_ATTEMPTS;

/// 用系统 `mkfifo` 创建 FIFO; macOS 与 Linux 都自带该工具.
fn make_fifo(path: &Path) {
    let status = std::process::Command::new("mkfifo")
        .arg(path)
        .status()
        .expect("执行 mkfifo");
    assert!(status.success(), "mkfifo 退出码非零: {status}");
}

/// 取出备份名中 `<原名>.dotlink-backup.` 之后的部分.
fn suffix_of(target: &Path, backup: &Path) -> String {
    let prefix = format!(
        "{}{}",
        target.file_name().unwrap().to_string_lossy(),
        USER_MARKER
    );
    let name = backup.file_name().unwrap().to_string_lossy().into_owned();
    name.strip_prefix(&prefix)
        .unwrap_or_else(|| panic!("{name} 不以 {prefix} 开头"))
        .to_string()
}

#[test]
fn backup_name_carries_a_second_precision_timestamp() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("bashrc");

    let backup = backup_path_for(&target, BackupKind::User).unwrap();
    assert_eq!(backup.parent(), Some(dir.path()));

    let suffix = suffix_of(&target, &backup);
    assert!(
        is_timestamp(&suffix),
        "备份后缀 {suffix} 不是 YYYYMMDDThhmmss 形态"
    );
    assert!(
        suffix[..4].parse::<u32>().unwrap() >= 2020,
        "备份时间戳的年份看起来不对: {suffix}"
    );
    assert_eq!(suffix.len(), 15);
}

/// 形态与字段都合法的名字才被接受; 闰日与月末边界取该月真实天数.
#[test]
fn is_timestamp_accepts_valid_calendar_values() {
    for text in [
        "20260101T000000",
        "20260228T235959",
        "20240229T000000",
        "20261231T235959",
        // 年份范围以 `time` 能否构成合法日期为准: 4 位数字的上下端都被接受.
        // 本工具生成的年份来自 `now_utc()`, 不会走到这两端, 这里只为固定住当前范围.
        "00000101T000000",
        "99991231T235959",
    ] {
        assert!(is_timestamp(text), "{text} 是合法日历时刻");
    }
}

/// 形态合法但字段非法 (月, 月末, 闰日, 时分秒越界) 与形态非法都必须被拒绝.
#[test]
fn is_timestamp_rejects_invalid_fields_and_shapes() {
    for text in [
        // 字段非法: 13 月, 2 月 30 日, 平年闰日, 4 月 31 日, 24 时, 60 分, 60 秒.
        "20261301T000000",
        "20260230T000000",
        "20230229T000000",
        "20260431T000000",
        "20260101T240000",
        "20260101T236000",
        "20260101T005960",
        "20260101T000060",
        // 0 月不在 01 至 12 之间.
        "20260001T000000",
        // 形态非法: 长度不足, 缺分隔符, 非数字, `.partial` 后缀.
        "2026010T000000",
        "20260101T00000",
        "20260101-000000",
        "20260101T00000a",
        "20260101T000000.partial",
    ] {
        assert!(!is_timestamp(text), "{text} 不是完整的合法时间戳");
    }
}

/// 后缀拆分: 合法时间戳仍按 `.` 拆出序号; 字段非法时无论带不带序号都没有备份.
#[test]
fn complete_suffix_of_requires_valid_fields_but_keeps_the_serial() {
    assert_eq!(
        complete_suffix_of("20260101T000000.10"),
        Some(("20260101T000000", "10"))
    );
    assert_eq!(
        complete_suffix_of("20240229T000000"),
        Some(("20240229T000000", ""))
    );
    for suffix in [
        "20261301T000000",
        "20261301T000000.10",
        "20230229T000000.2",
        "20260101T240000",
        "20260101T000000.partial",
    ] {
        assert_eq!(complete_suffix_of(suffix), None, "{suffix}");
    }
}

/// 字段非法的名字不参与"取最新": 既不会被选中, 也不会盖过合法备份.
#[test]
fn find_latest_backup_ignores_names_with_invalid_timestamp_fields() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("bashrc");
    let valid = dir.path().join("bashrc.dotlink-backup.20260101T000000");
    fs::write(&valid, "real").unwrap();
    // 字典序更大, 但字段非法 (13 月与平年闰日).
    for name in [
        "bashrc.dotlink-backup.99999999T999999",
        "bashrc.dotlink-backup.20261301T000000",
        "bashrc.dotlink-backup.20230229T000000.9",
    ] {
        fs::write(dir.path().join(name), "bogus").unwrap();
    }

    assert_eq!(find_latest_backup(&target).unwrap(), Some(valid));

    // 同目录并列合法闰日与平年闰日: 前者参与查找, 后者被忽略 (spec 的"闰日与月末").
    let leap_target = dir.path().join("profile");
    let leap = dir.path().join("profile.dotlink-backup.20240229T000000");
    fs::write(&leap, "leap").unwrap();
    fs::write(
        dir.path().join("profile.dotlink-backup.20230229T000000"),
        "bogus",
    )
    .unwrap();
    assert_eq!(find_latest_backup(&leap_target).unwrap(), Some(leap));
}

#[test]
fn backup_name_uses_the_original_file_name_verbatim() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("nvim");

    let backup = backup_path_for(&target, BackupKind::User).unwrap();
    let name = backup.file_name().unwrap().to_string_lossy().into_owned();
    assert!(name.starts_with("nvim.dotlink-backup."), "实际为 {name}");
}

/// 审计最长生成名后缀: 200 字节 basename 加上备份最终名, 备份暂存名与恢复
/// 暂存名, 都必须落在 NAME_MAX 255 内.
#[test]
fn the_longest_generated_backup_names_fit_name_max() {
    let dir = tempfile::tempdir().unwrap();
    let basename = "a".repeat(crate::entry::MAX_TARGET_BASENAME_BYTES);
    let target = dir.path().join(&basename);
    let base = backup_candidates_at(&target, "20260101T000000".to_string(), BackupKind::User)
        .next()
        .expect("备份候选名非空");

    // 备份暂存名 (2 位消歧序号) 是最长形态: `.dotlink-backup.` 16 + 时间戳 15
    // + `.` + 7 位 pid + `.` + 2 位序号 + `.partial` 8 = 50.
    let staging = staging_candidate(&base, 16, PARTIAL_SUFFIX);
    let staging_suffix =
        staging.file_name().unwrap().len() - crate::entry::MAX_TARGET_BASENAME_BYTES;
    assert!(
        staging_suffix <= crate::entry::MAX_GENERATED_SUFFIX_BYTES,
        "备份暂存名后缀 {staging_suffix} 超过审计值 {}",
        crate::entry::MAX_GENERATED_SUFFIX_BYTES
    );
    assert!(
        staging.file_name().unwrap().len() <= 255,
        "备份暂存名超过 NAME_MAX: {}",
        staging.display()
    );

    // 备份最终名与恢复暂存名都更短, 同样必须落在预算与 NAME_MAX 内.
    for name in [
        with_suffix(&base, ".16"),
        staging_candidate(&target, 16, RESTORE_SUFFIX),
    ] {
        let bytes = name.file_name().unwrap().len();
        assert!(
            bytes <= 255,
            "{} 字节超过 NAME_MAX: {}",
            bytes,
            name.display()
        );
        assert!(
            bytes - crate::entry::MAX_TARGET_BASENAME_BYTES
                <= crate::entry::MAX_GENERATED_SUFFIX_BYTES
        );
    }
}

/// 裸文件名 (`bashrc`) 的 `parent()` 是 `Some("")` 而非 `None`: 备份名必须落在
/// 当前目录 `.`, 而不是退化成相对名.
#[test]
fn a_bare_file_name_backs_up_into_the_current_directory() {
    let target = Path::new("bashrc");
    assert_eq!(parent_dir(target), Path::new("."));

    let backup = backup_path_for(target, BackupKind::User).unwrap();
    assert_eq!(backup.parent(), Some(Path::new(".")));
    assert!(
        backup
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("bashrc.dotlink-backup."),
        "实际为 {}",
        backup.display()
    );

    let staged = with_suffix(target, PARTIAL_SUFFIX);
    assert_eq!(staged.parent(), Some(Path::new(".")));
}

/// `parent_dir` 对无父目录与普通父目录都必须给出确定结果, 不返回空目录.
#[test]
fn parent_dir_never_returns_an_empty_directory() {
    assert_eq!(parent_dir(Path::new("/")), Path::new("."));
    assert_eq!(parent_dir(Path::new("dir/bashrc")), Path::new("dir"));
    assert_eq!(parent_dir(Path::new("/tmp/bashrc")), Path::new("/tmp"));
}

#[test]
fn disambiguation_keeps_two_backups_of_the_same_second() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("bashrc");

    let first = backup_path_for(&target, BackupKind::User).unwrap();
    fs::write(&first, "first").unwrap();

    let second = backup_path_for(&target, BackupKind::User).unwrap();
    assert_ne!(first, second, "已占用的名字不得被再次返回");

    // 两次调用跨过秒边界时, 第二次拿到的是新时间戳而非序号 -- 这同样满足
    // "不覆盖既有备份"的不变量, 只是序号消歧未被触发. 序号路径仅在确属同一秒
    // 时断言, 使本测试不因墙钟走动而抖动.
    let first_suffix = suffix_of(&target, &first);
    let second_suffix = suffix_of(&target, &second);
    if second_suffix.starts_with(&first_suffix[..15]) {
        assert_eq!(
            second_suffix.split('.').nth(1),
            Some("2"),
            "同一秒内的第二个备份必须以序号消歧"
        );
    } else {
        eprintln!("两次调用跨过秒边界, 序号消歧未被触发: {first_suffix} -> {second_suffix}");
    }

    fs::write(&second, "second").unwrap();
    assert_eq!(fs::read_to_string(&first).unwrap(), "first");
    assert_eq!(fs::read_to_string(&second).unwrap(), "second");
}

/// 同一秒内的两次替换: 序号消歧必须确定发生, 不依赖墙钟是否跨秒.
///
/// 固定时间戳经 [`write_backup_at`] 注入, 断言两条: 后者取 `.2`, 前者内容不变.
#[test]
fn two_replacements_in_one_second_get_serial_two_deterministically() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("bashrc");
    let stamp = "20260101T000000";

    fs::write(&target, "first").unwrap();
    let first = write_backup_at(&target, stamp, BackupKind::User)
        .unwrap()
        .path;
    fs::write(&target, "second").unwrap();
    let second = write_backup_at(&target, stamp, BackupKind::User)
        .unwrap()
        .path;

    assert_eq!(suffix_of(&target, &first), stamp);
    assert_eq!(suffix_of(&target, &second), format!("{stamp}.2"));
    assert_eq!(fs::read_to_string(&first).unwrap(), "first");
    assert_eq!(fs::read_to_string(&second).unwrap(), "second");
}

/// 两个命名空间的裸名与同一秒内的 `.2` 消歧名都由同一个候选生成器给出.
#[test]
fn both_namespaces_generate_marked_names_and_serial_two() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("bashrc");
    let stamp = "20260101T000000";

    for (kind, marker) in [
        (BackupKind::User, USER_MARKER),
        (BackupKind::SelfHeal, SELF_HEAL_MARKER),
    ] {
        let mut candidates = backup_candidates_at(&target, stamp.to_string(), kind);
        let first = candidates.next().expect("第一个候选名");
        let second = candidates.next().expect("第二个候选名");
        assert_eq!(
            first.file_name().unwrap().to_string_lossy(),
            format!("bashrc{marker}{stamp}")
        );
        assert_eq!(
            second.file_name().unwrap().to_string_lossy(),
            format!("bashrc{marker}{stamp}.2")
        );
    }
}

/// 自愈暂存名沿用同一命名空间标记, 并且仍带进程号与 `.partial`.
#[test]
fn self_heal_staging_names_keep_the_namespace() {
    let dir = tempfile::tempdir().unwrap();
    let stamp = "20260101T000000";
    let base = dir.path().join(format!("bashrc{SELF_HEAL_MARKER}{stamp}"));

    let staging = staging_candidate(&base, 1, PARTIAL_SUFFIX);

    assert_eq!(staging.parent(), Some(dir.path()));
    assert_eq!(
        staging.file_name().unwrap().to_string_lossy(),
        format!(
            "bashrc{SELF_HEAL_MARKER}{stamp}.{}{PARTIAL_SUFFIX}",
            std::process::id()
        )
    );
}

/// 查找器只扫描用户命名空间: 更晚的自愈备份不参与"取最新", 只有自愈备份时
/// 视为没有可用备份 (backup-lifecycle 的两个 Scenario).
#[test]
fn find_latest_backup_ignores_the_self_heal_namespace() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("bashrc");
    let user = dir
        .path()
        .join(format!("bashrc{USER_MARKER}20260101T000000"));
    let heal = dir
        .path()
        .join(format!("bashrc{SELF_HEAL_MARKER}20990101T000000"));
    fs::write(&user, "user").unwrap();
    fs::write(&heal, "heal").unwrap();

    assert_eq!(
        find_latest_backup(&target).unwrap(),
        Some(user),
        "更晚的自愈备份不得压过用户备份"
    );

    fs::remove_file(
        dir.path()
            .join(format!("bashrc{USER_MARKER}20260101T000000")),
    )
    .unwrap();
    assert_eq!(
        find_latest_backup(&target).unwrap(),
        None,
        "只有自愈备份时必须视为没有可用备份"
    );
    assert_eq!(fs::read_to_string(&heal).unwrap(), "heal");
}

/// 原文件名本身含 `.dotlink-backup.` 时, 自愈名仍不构成用户备份的完整形态.
#[test]
fn a_self_heal_name_containing_the_user_marker_is_not_a_user_backup() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("rc.dotlink-backup.orig");
    let heal = dir.path().join(format!(
        "rc.dotlink-backup.orig{SELF_HEAL_MARKER}20260101T000000"
    ));
    fs::write(&heal, "heal").unwrap();

    assert_eq!(find_latest_backup(&target).unwrap(), None);
    assert_eq!(
        find_latest_backup(&dir.path().join("rc")).unwrap(),
        None,
        "最后一个用户标记之后必须是完整后缀, 含自愈段的名字不满足"
    );
}

/// 旧版本写出的 `.dotlink-backup.` 自愈残留: 新代码不识别, 不迁移, 不删除,
/// 仍按既有查找规则被选中. 这是 README "已知限制" 固定的可观察行为.
#[test]
fn old_self_heal_residue_is_kept_and_still_found_as_a_user_backup() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("bashrc");
    let old = dir
        .path()
        .join(format!("bashrc{USER_MARKER}20250101T000000"));
    let old_text = Path::new("/repo/old-source");
    os::create_symlink(old_text, &old).unwrap();

    assert_eq!(find_latest_backup(&target).unwrap(), Some(old.clone()));

    fs::write(&target, "current").unwrap();
    let heal = write_backup_at(&target, "20260101T000000", BackupKind::SelfHeal)
        .unwrap()
        .path;

    assert_eq!(
        heal.file_name().unwrap().to_string_lossy(),
        format!("bashrc{SELF_HEAL_MARKER}20260101T000000")
    );
    assert!(
        fs::symlink_metadata(&old).unwrap().file_type().is_symlink(),
        "旧残留不得被改名或迁移"
    );
    assert_eq!(fs::read_link(&old).unwrap(), old_text);
    assert_eq!(
        find_latest_backup(&target).unwrap(),
        Some(old),
        "旧残留仍按用户备份查找规则被选中"
    );
}

/// 同步失败必须使该条目失败: 不发布最终名, 不删除原对象, 也不留暂存残留.
#[test]
fn a_failed_file_sync_publishes_no_backup_and_keeps_the_original() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("bashrc");
    let stamp = "20260101T000000";
    fs::write(&target, "original").unwrap();

    let guard = os::fail_syncs();
    let error = write_backup_at(&target, stamp, BackupKind::User).unwrap_err();
    drop(guard);

    assert!(error.to_string().contains("同步备份文件"), "{error}");
    assert_eq!(fs::read_to_string(&target).unwrap(), "original");
    assert!(
        fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .all(|entry| !entry
                .file_name()
                .to_string_lossy()
                .starts_with("bashrc.dotlink-backup.")),
        "同步失败不得发布最终名或留下暂存残留"
    );
}

/// 目录自身的同步失败同样不得发布最终名, 原目录保持原样.
#[test]
fn a_failed_directory_sync_publishes_no_backup() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("nvim");
    let stamp = "20260101T000000";
    // 空目录: 复制过程只涉及目录自身, 因此失败点必然落在目录同步上.
    fs::create_dir(&target).unwrap();

    let guard = os::fail_syncs();
    let error = write_backup_at(&target, stamp, BackupKind::User).unwrap_err();
    drop(guard);

    assert!(error.to_string().contains("同步备份目录"), "{error}");
    assert!(
        fs::symlink_metadata(&target).unwrap().is_dir(),
        "原目录必须保留"
    );
    assert_eq!(fs::read_dir(&target).unwrap().count(), 0);
    assert!(
        fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .all(|entry| !entry
                .file_name()
                .to_string_lossy()
                .starts_with("nvim.dotlink-backup.")),
        "同步失败不得发布最终名或留下暂存残留"
    );
}

/// 同步失败且源目录只读: 清理前恢复 owner rwx, 不留下 `.partial` 暂存树.
///
/// 复制完成后暂存树已应用源的只读权限 (0o555); 若清理不先改权限, 空目录可以用
/// 父目录写权限删除, 但树内的只读子目录无法清空, 会留下本可避免的残留.
#[test]
fn a_failed_sync_cleans_a_read_only_staging_tree() {
    let dir = tempfile::tempdir().unwrap();
    // 只含普通文件: 唯一一次目录同步发生在外层暂存根上, 且已在应用只读源权限
    // (0o555) 之后. 若清理不先恢复 owner rwx, 无法从只读目录中删除子项.
    let source = dir.path().join("nvim");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("a"), "a").unwrap();
    fs::set_permissions(&source, fs::Permissions::from_mode(0o555)).unwrap();
    let before = fs::read_dir(dir.path()).unwrap().count();

    let guard = os::fail_directory_syncs();
    let error = write_backup_at(&source, "20260101T000000", BackupKind::User).unwrap_err();
    drop(guard);
    fs::set_permissions(&source, fs::Permissions::from_mode(0o755)).unwrap();

    assert!(error.to_string().contains("同步备份目录"), "{error}");
    assert_eq!(
        fs::read_dir(dir.path()).unwrap().count(),
        before,
        "失败清理不得留下 .partial 暂存树"
    );
    assert!(
        fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .all(|entry| !entry
                .file_name()
                .to_string_lossy()
                .ends_with(PARTIAL_SUFFIX)),
        "失败不得残留暂存对象"
    );
    assert_eq!(
        fs::read_to_string(source.join("a")).unwrap(),
        "a",
        "只读源目录必须保持原样"
    );
}

/// rename 之后的父目录同步失败: 备份已经发布, 失败说明必须给出最终路径, 原对象保留.
#[test]
fn a_failed_parent_sync_reports_the_published_backup() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("bashrc");
    let stamp = "20260101T000000";
    fs::write(&target, "original").unwrap();

    // 只让目录同步失败: 普通文件的内容同步会先通过, 失败点因此落在 rename 之后
    // 的父目录同步上.
    let guard = os::fail_directory_syncs();
    let error = write_backup_at(&target, stamp, BackupKind::User).unwrap_err();
    drop(guard);

    let message = error.to_string();
    assert!(message.contains("同步父目录"), "{message}");
    assert!(message.contains("备份已发布到"), "{message}");
    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        "original",
        "原对象不得被删除"
    );

    let backups: Vec<PathBuf> = fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("bashrc.dotlink-backup."))
        })
        .collect();
    assert_eq!(backups.len(), 1, "rename 已发布最终名: {backups:?}");
    assert!(
        message.contains(&backups[0].display().to_string()),
        "说明必须给出已发布的备份路径: {message}"
    );
    assert_eq!(fs::read_to_string(&backups[0]).unwrap(), "original");
}

/// 时间戳以 UTC 输出: 带 `+08:00` 偏移的固定时刻必须按 UTC 字段格式化, 否则
/// 同一目标的备份会落在不同时间轴上, 字符串序不再等于时间序.
#[test]
fn timestamp_formats_a_fixed_instant_in_utc() {
    let offset = UtcOffset::from_hms(8, 0, 0).unwrap();
    let fixed = time::Date::from_calendar_date(2026, time::Month::November, 4)
        .unwrap()
        .with_hms(23, 17, 19)
        .unwrap()
        .assume_offset(offset);

    assert_eq!(
        format_timestamp(fixed),
        "20261104T151719",
        "2026-11-04T23:17:19+08:00 的 UTC 形态必须是 20261104T151719"
    );
    assert_eq!(
        format_timestamp(OffsetDateTime::from_unix_timestamp(0).unwrap()),
        "19700101T000000",
        "epoch 的 UTC 形态必须是 19700101T000000"
    );
}

#[test]
fn disambiguation_also_handles_a_dangling_symlink_at_the_backup_name() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("bashrc");

    let first = backup_path_for(&target, BackupKind::User).unwrap();
    // 悬空链接: `Path::exists` 对它返回 false, 必须用 lstat 检测才算"已占用".
    os::create_symlink(Path::new("/definitely/gone"), &first).unwrap();

    let second = backup_path_for(&target, BackupKind::User).unwrap();
    assert_ne!(first, second);
    assert!(
        fs::symlink_metadata(&first)
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

/// 发布路径 (rename) 遇到悬空链接占用最终名时必须递增序号, 且不得覆盖该链接.
///
/// 预检测试只覆盖 `backup_path_for`; 这里直接走 [`write_backup_at`], 使 rename 的
/// `AlreadyExists -> 下一候选` 分支有真实断言 (而不是仅靠代码审阅).
#[test]
fn write_backup_retries_when_the_final_name_is_occupied_by_a_dangling_symlink() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("bashrc");
    let stamp = "20260101T000000";
    fs::write(&target, "payload").unwrap();

    let occupied = dir.path().join(format!("bashrc{USER_MARKER}{stamp}"));
    os::create_symlink(Path::new("/definitely/gone"), &occupied).unwrap();

    let written = write_backup_at(&target, stamp, BackupKind::User)
        .unwrap()
        .path;

    assert_eq!(suffix_of(&target, &written), format!("{stamp}.2"));
    assert_eq!(fs::read_to_string(&written).unwrap(), "payload");
    assert!(
        fs::symlink_metadata(&occupied)
            .unwrap()
            .file_type()
            .is_symlink(),
        "占用最终名的悬空链接必须原样保留"
    );
    assert_eq!(
        fs::read_link(&occupied).unwrap(),
        Path::new("/definitely/gone")
    );
}

#[test]
fn find_latest_backup_picks_the_newest_timestamp_then_the_largest_serial() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("bashrc");

    let older = dir.path().join("bashrc.dotlink-backup.20200101T000000");
    let newer = dir.path().join("bashrc.dotlink-backup.20260101T000000");
    let disambiguated = dir.path().join("bashrc.dotlink-backup.20260101T000000.2");
    for path in [&older, &newer, &disambiguated] {
        fs::write(path, "x").unwrap();
    }
    // 无关文件与别的目标的备份都必须被忽略.
    fs::write(dir.path().join("bashrc"), "orig").unwrap();
    fs::write(dir.path().join("zshrc.dotlink-backup.20990101T000000"), "x").unwrap();

    assert_eq!(find_latest_backup(&target).unwrap(), Some(disambiguated));
}

/// 序号按**数值**比较而非字典序: 同一秒内 bare 与 `.2` ... `.10` 共 10 个并存时,
/// 真正最新的是 `.10`, 而整名字典序最大是 `.9`.
#[test]
fn find_latest_backup_compares_serials_numerically() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("bashrc");

    let stamp = "20260101T000000";
    for serial in ["", ".2", ".3", ".4", ".5", ".6", ".7", ".8", ".9", ".10"] {
        fs::write(
            dir.path()
                .join(format!("bashrc{USER_MARKER}{stamp}{serial}")),
            "x",
        )
        .unwrap();
    }

    assert_eq!(
        find_latest_backup(&target).unwrap(),
        Some(dir.path().join(format!("bashrc{USER_MARKER}{stamp}.10"))),
        "序号 10 是数值上的最大者, 不得被字典序更大的 .9 压过"
    );
}

/// 跨时间戳取较晚者: 时间先后先于序号大小.
#[test]
fn find_latest_backup_prefers_the_later_timestamp_regardless_of_serial() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("bashrc");

    let earlier = dir.path().join("bashrc.dotlink-backup.20260101T000000.10");
    let later = dir.path().join("bashrc.dotlink-backup.20260102T000000");
    fs::write(&earlier, "x").unwrap();
    fs::write(&later, "x").unwrap();

    assert_eq!(find_latest_backup(&target).unwrap(), Some(later));
}

/// 取最新以 UTC 时间戳的字符串序为前提: 同一目录并存两个 UTC 时间戳时取较晚者,
/// 同一时间戳内 `.10` 按数值大于 `.9`.
#[test]
fn find_latest_backup_orders_utc_timestamps_then_numeric_serials() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("bashrc");

    let earlier = "20261104T151719";
    let later = "20261104T151720";
    for stamp in [earlier, later] {
        fs::write(dir.path().join(format!("bashrc{USER_MARKER}{stamp}")), "x").unwrap();
    }
    for serial in [".9", ".10"] {
        fs::write(
            dir.path()
                .join(format!("bashrc{USER_MARKER}{earlier}{serial}")),
            "x",
        )
        .unwrap();
    }

    assert_eq!(
        find_latest_backup(&target).unwrap(),
        Some(dir.path().join(format!("bashrc{USER_MARKER}{later}"))),
        "跨 UTC 时间戳必须取较晚者 {later}, 而不是 {earlier} 的任一序号"
    );

    fs::remove_file(dir.path().join(format!("bashrc{USER_MARKER}{later}"))).unwrap();
    assert_eq!(
        find_latest_backup(&target).unwrap(),
        Some(dir.path().join(format!("bashrc{USER_MARKER}{earlier}.10"))),
        "同一 UTC 时间戳内必须按序号数值取 .10, 而不是字典序更大的 .9"
    );
}

/// 时间戳相等且序号**数值**相等时 (`.02` 与 `.2`), 以文件名字典序兜底, 使结论
/// 与目录遍历顺序无关: 两种创建顺序下都必须取同一份 (字典序较大者).
#[test]
fn find_latest_backup_tiebreaks_equal_serials_by_file_name() {
    let stamp = "20260101T000000";
    for order in [[".02", ".2"], [".2", ".02"]] {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("bashrc");
        for serial in order {
            fs::write(
                dir.path()
                    .join(format!("bashrc{USER_MARKER}{stamp}{serial}")),
                serial,
            )
            .unwrap();
        }
        assert_eq!(
            find_latest_backup(&target).unwrap(),
            Some(dir.path().join(format!("bashrc{USER_MARKER}{stamp}.2"))),
            "创建顺序 {order:?} 下必须按文件名兜底取同一份, 不得依赖遍历顺序"
        );
    }
}

#[test]
fn find_latest_backup_ignores_partial_residue() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("bashrc");

    let complete = dir.path().join("bashrc.dotlink-backup.20260101T000000");
    fs::write(&complete, "complete").unwrap();
    // 字典序更大, 但都不是"已完整命名"的备份.
    fs::write(
        dir.path()
            .join("bashrc.dotlink-backup.20990101T000000.partial"),
        "half",
    )
    .unwrap();
    fs::write(
        dir.path()
            .join("bashrc.dotlink-backup.20990101T000001.partial.2"),
        "half",
    )
    .unwrap();

    assert_eq!(find_latest_backup(&target).unwrap(), Some(complete));
}

/// 进程级暂存名: 带进程号, 且不得被 `find_latest_backup` 认作"已完整命名"的备份.
#[test]
fn staging_names_are_process_unique_and_never_look_like_complete_backups() {
    let suffix = unique_staging_suffix(PARTIAL_SUFFIX);
    assert!(
        suffix.contains(&std::process::id().to_string()),
        "暂存后缀必须带进程号: {suffix}"
    );
    assert!(suffix.ends_with(PARTIAL_SUFFIX), "{suffix}");

    let name = format!("bashrc{USER_MARKER}20260101T000000{suffix}");
    let after_marker = name.strip_prefix(&format!("bashrc{USER_MARKER}")).unwrap();
    assert!(
        complete_suffix_of(after_marker).is_none(),
        "暂存名 {name} 不得被认作完整备份"
    );
}

#[test]
fn find_latest_backup_returns_none_without_a_backup() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("bashrc");
    fs::write(&target, "orig").unwrap();

    assert_eq!(find_latest_backup(&target).unwrap(), None);
    assert_eq!(
        find_latest_backup(&dir.path().join("absent")).unwrap(),
        None
    );
    // 父目录不存在是"无备份"而不是故障: `NotFound` 与其他 IO 故障必须分开处置.
    assert_eq!(
        find_latest_backup(&dir.path().join("gone/rc")).unwrap(),
        None
    );
}

/// 替换/撤销暂存名不参与备份查找, 也不会被当成可恢复备份.
#[test]
fn generated_staging_names_do_not_participate_in_backup_lookup() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("bashrc");
    let pid = std::process::id();
    for name in [
        format!("bashrc.{pid}.dotlink-replace.partial"),
        format!("bashrc.{pid}.2.dotlink-replace.partial"),
        format!("bashrc.{pid}.dotlink-undo.partial"),
    ] {
        fs::write(dir.path().join(name), "staged").unwrap();
    }

    assert_eq!(find_latest_backup(&target).unwrap(), None);
}

/// 非 UTF-8 目标名的备份仍能按原始字节命中并恢复 (设计 D4).
///
/// 清单本身必须是 UTF-8, 因此非 UTF-8 只可能来自目录项; 这里直接构造目标与备份
/// 名字. 文件系统不接受该名字时打印跳过 (macOS APFS 等).
#[test]
fn a_non_utf8_backup_name_is_found_and_restored() {
    use std::os::unix::ffi::OsStringExt;

    let dir = tempfile::tempdir().unwrap();
    let target_name = OsString::from_vec(vec![b'r', b'c', 0xff, b'!']);
    let target = dir.path().join(&target_name);
    let mut backup_name = target_name.clone();
    backup_name.push(USER_MARKER);
    backup_name.push("20260101T000000");
    let backup = dir.path().join(backup_name);
    if fs::write(&backup, "original").is_err() {
        eprintln!("跳过: 当前文件系统不接受非 UTF-8 文件名");
        return;
    }

    assert_eq!(find_latest_backup(&target).unwrap(), Some(backup.clone()));

    restore_backup(&target, &backup).unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap(), "original");
    assert!(fs::symlink_metadata(&backup).is_ok(), "恢复不消费备份");
}

/// 非 UTF-8 的无关目录项被跳过, 不影响其他目标的备份选择 (设计 D4).
#[test]
fn a_non_utf8_unrelated_directory_entry_does_not_disturb_lookup() {
    use std::os::unix::ffi::OsStringExt;

    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("rc");
    let correct = dir.path().join(format!("rc{USER_MARKER}20260101T000000"));
    fs::write(&correct, "correct").unwrap();
    let mut bogus_name = OsString::from_vec(vec![b'x', 0xff, b'y']);
    bogus_name.push(USER_MARKER);
    bogus_name.push("20990101T000000");
    let bogus = dir.path().join(bogus_name);
    if fs::write(&bogus, "bogus").is_err() {
        eprintln!("跳过: 当前文件系统不接受非 UTF-8 文件名");
        return;
    }

    assert_eq!(find_latest_backup(&target).unwrap(), Some(correct));
    assert_eq!(fs::read_to_string(&bogus).unwrap(), "bogus");
}

/// 候选名的父目录不可读: 预测失败而不是跳过错误去选后续候选.
#[test]
fn backup_path_for_reports_a_permission_error_instead_of_skipping() {
    let dir = tempfile::tempdir().unwrap();
    let locked = dir.path().join("locked");
    fs::create_dir(&locked).unwrap();
    let target = locked.join("bashrc");
    fs::write(&target, "orig").unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

    // 以 root 运行时权限位无效, 无法构造 EACCES, 只能跳过.
    if fs::symlink_metadata(&target).is_ok() {
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        eprintln!("以 root 运行: chmod 000 不生效, 跳过 EACCES 预测失败断言");
        return;
    }

    let error = backup_path_for(&target, BackupKind::User).unwrap_err();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

    assert!(matches!(error, Error::EntryFailed(_)));
    let message = error.to_string();
    assert!(
        message.contains("检查备份名"),
        "说明必须指出是候选名检查失败: {message}"
    );
    assert!(
        message.contains("Permission denied"),
        "说明必须给出底层权限原因: {message}"
    );
}

/// 候选路径过长: 预测失败而不是静默选择后续候选.
#[test]
fn backup_path_for_reports_an_overlong_candidate_instead_of_skipping() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("a".repeat(240));

    let error = backup_path_for(&target, BackupKind::User).unwrap_err();

    assert!(matches!(error, Error::EntryFailed(_)));
    let message = error.to_string();
    assert!(
        message.contains("检查备份名"),
        "说明必须指出是候选名检查失败: {message}"
    );
    assert!(
        error
            .to_string()
            .to_lowercase()
            .contains("file name too long"),
        "说明必须保留底层路径过长原因: {message}"
    );
}

/// 目录读取失败 (chmod 000) 是环境故障, 不得降级成"无备份".
#[test]
fn find_latest_backup_reports_a_listing_failure_instead_of_no_backup() {
    let dir = tempfile::tempdir().unwrap();
    let locked = dir.path().join("locked");
    fs::create_dir_all(&locked).unwrap();
    fs::write(locked.join("bashrc.dotlink-backup.20260101T000000"), "x").unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

    // 以 root 运行时权限位无效, 无法构造"列举必然失败"的场景, 只能跳过.
    if fs::read_dir(&locked).is_ok() {
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        eprintln!("以 root 运行: chmod 000 不生效, 跳过备份查找的故障传播断言");
        return;
    }

    let error = find_latest_backup(&locked.join("bashrc")).unwrap_err();
    assert!(matches!(error, Error::EntryFailed(_)));
    let message = error.to_string();
    assert!(
        message.contains("Permission denied"),
        "错误必须给出底层原因: {message}"
    );

    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn write_backup_copies_files_and_leaves_no_partial() {
    let dir = tempfile::tempdir().unwrap();
    let object = dir.path().join("bashrc");
    fs::write(&object, "original").unwrap();

    let backup = write_backup(&object, BackupKind::User).unwrap().path;

    assert_eq!(fs::read_to_string(&backup).unwrap(), "original");
    assert!(complete_suffix_of(&suffix_of(&object, &backup)).is_some());
    let names: Vec<String> = fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        !names.iter().any(|name| name.contains(PARTIAL_SUFFIX)),
        "成功路径上不得残留 .partial: {names:?}"
    );
}

/// 身份基准来自复制起点: 返回的元数据必须描述备份里保存的那个对象.
///
/// 有了它, `apply` 不必在备份之前另取快照, 复核基准与备份内容天然同源 (设计 D1).
#[test]
fn write_backup_returns_the_metadata_of_the_copied_object() {
    let dir = tempfile::tempdir().unwrap();
    let object = dir.path().join("bashrc");
    fs::write(&object, "original").unwrap();

    let published = write_backup(&object, BackupKind::User).unwrap();

    let source = fs::symlink_metadata(&object).unwrap();
    let copied = fs::symlink_metadata(&published.path).unwrap();
    assert_eq!(published.source_metadata.ino(), source.ino());
    assert_eq!(published.source_metadata.dev(), source.dev());
    assert_eq!(
        published.source_metadata.file_type().is_file(),
        copied.file_type().is_file()
    );

    // 符号链接备份的是链接本身, 返回的元数据也必须是链接的元数据.
    let link = dir.path().join("link");
    os::create_symlink(Path::new("bashrc"), &link).unwrap();
    let published = write_backup(&link, BackupKind::User).unwrap();
    assert!(published.source_metadata.file_type().is_symlink());
    assert_eq!(
        published.source_metadata.ino(),
        fs::symlink_metadata(&link).unwrap().ino()
    );
    assert!(
        fs::symlink_metadata(&published.path)
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
fn write_backup_copies_directories_recursively() {
    let dir = tempfile::tempdir().unwrap();
    let object = dir.path().join("nvim");
    fs::create_dir_all(object.join("lua/plugins")).unwrap();
    fs::write(object.join("init.lua"), "init").unwrap();
    fs::write(object.join("lua/plugins/a.lua"), "a").unwrap();

    let backup = write_backup(&object, BackupKind::User).unwrap().path;

    assert_eq!(fs::read_to_string(backup.join("init.lua")).unwrap(), "init");
    assert_eq!(
        fs::read_to_string(backup.join("lua/plugins/a.lua")).unwrap(),
        "a"
    );
    // 目录的权限位被保留 (默认 0755 或更严).
    let mode = fs::metadata(&backup).unwrap().permissions().mode() & 0o777;
    let source_mode = fs::metadata(&object).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, source_mode);
}

#[test]
fn write_backup_copies_the_link_itself() {
    let dir = tempfile::tempdir().unwrap();
    let pointed = dir.path().join("real");
    fs::write(&pointed, "content").unwrap();
    let object = dir.path().join("bashrc");
    os::create_symlink(Path::new("real"), &object).unwrap();

    let backup = write_backup(&object, BackupKind::User).unwrap().path;

    let metadata = fs::symlink_metadata(&backup).unwrap();
    assert!(
        metadata.file_type().is_symlink(),
        "备份必须是链接本身而非其目标的内容"
    );
    // 同目录复制, 因此相对链接文本原样保留且解析结果不变.
    assert_eq!(fs::read_link(&backup).unwrap(), PathBuf::from("real"));
    assert_eq!(fs::read_to_string(&backup).unwrap(), "content");
}

#[test]
fn write_backup_does_not_follow_nested_links_inside_a_directory() {
    let dir = tempfile::tempdir().unwrap();
    let object = dir.path().join("nvim");
    fs::create_dir_all(&object).unwrap();
    os::create_symlink(Path::new("/definitely/gone"), &object.join("dangling")).unwrap();

    let backup = write_backup(&object, BackupKind::User).unwrap().path;

    let nested = fs::symlink_metadata(backup.join("dangling")).unwrap();
    assert!(nested.file_type().is_symlink());
    assert_eq!(
        fs::read_link(backup.join("dangling")).unwrap(),
        PathBuf::from("/definitely/gone")
    );
}

#[test]
fn write_backup_reports_failure_without_leaving_a_final_name() {
    let dir = tempfile::tempdir().unwrap();
    let object = dir.path().join("nvim");
    let locked = object.join("locked");
    fs::create_dir_all(&locked).unwrap();
    fs::write(locked.join("secret"), "secret").unwrap();
    fs::write(object.join("init.lua"), "init").unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

    // 以 root 运行时权限位无效, 此时无法构造"复制必然失败"的场景, 只能跳过.
    let readable_even_when_locked = fs::read_dir(&locked).is_ok();
    if readable_even_when_locked {
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        eprintln!("以 root 运行: chmod 000 不生效, 跳过备份失败原子性断言");
        return;
    }

    let error = write_backup(&object, BackupKind::User).unwrap_err();
    assert!(matches!(error, Error::EntryFailed(_)));

    // 原对象必须完整保留.
    assert_eq!(fs::read_to_string(object.join("init.lua")).unwrap(), "init");
    // 恢复权限位后才能读回被锁住的子目录内容.
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(fs::read_to_string(locked.join("secret")).unwrap(), "secret");

    // 失败不得发布最终名: `undo` 的查找看不见任何完整备份.
    assert_eq!(find_latest_backup(&object).unwrap(), None);

    // 磁盘上不得存在最终名的备份: 中断残留只能是 `.partial`.
    let prefix = format!("nvim{}", USER_MARKER);
    let residue: Vec<String> = fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(&prefix))
        .collect();
    for name in &residue {
        assert!(
            name.ends_with(PARTIAL_SUFFIX),
            "{name} 不是 .partial 残留, 会被误认为完整备份"
        );
    }
}

#[test]
fn restore_backup_puts_the_content_back_and_keeps_the_backup() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("bashrc");
    let backup = dir.path().join("bashrc.dotlink-backup.20260101T000000");
    fs::write(&backup, "original").unwrap();

    restore_backup(&target, &backup).unwrap();

    assert_eq!(fs::read_to_string(&target).unwrap(), "original");
    assert!(
        fs::symlink_metadata(&backup).is_ok(),
        "恢复成功后不删除备份文件本身"
    );
}

#[test]
fn restore_preserves_occupied_targets() {
    use std::os::unix::fs::MetadataExt;

    for kind in ["file", "link", "dangling", "directory"] {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("bashrc");
        let backup = dir.path().join("bashrc.dotlink-backup.20260101T000000");
        fs::write(&backup, "original").unwrap();
        match kind {
            "file" => fs::write(&target, "new user content").unwrap(),
            "link" => os::create_symlink(Path::new("managed"), &target).unwrap(),
            "dangling" => os::create_symlink(Path::new("missing"), &target).unwrap(),
            "directory" => fs::create_dir(&target).unwrap(),
            _ => unreachable!(),
        }
        fs::write(dir.path().join("managed"), "managed").unwrap();
        let inode = fs::symlink_metadata(&target).unwrap().ino();

        let error = restore_backup(&target, &backup).unwrap_err();

        assert!(error.to_string().contains("恢复备份到"), "{error}");
        assert_eq!(
            fs::symlink_metadata(&target).unwrap().ino(),
            inode,
            "{kind}"
        );
        match kind {
            "file" => assert_eq!(fs::read_to_string(&target).unwrap(), "new user content"),
            "link" => assert_eq!(fs::read_link(&target).unwrap(), Path::new("managed")),
            "dangling" => assert_eq!(fs::read_link(&target).unwrap(), Path::new("missing")),
            "directory" => assert_eq!(fs::read_dir(&target).unwrap().count(), 0),
            _ => unreachable!(),
        }
        assert_eq!(fs::read_to_string(&backup).unwrap(), "original");
        assert!(fs::read_dir(dir.path()).unwrap().all(|item| {
            !item
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(RESTORE_SUFFIX)
        }));
    }
}

#[test]
fn restore_preserves_an_empty_directory() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("nvim");
    let backup = dir.path().join("nvim.dotlink-backup.20260101T000000");
    fs::create_dir(&target).unwrap();
    fs::create_dir(&backup).unwrap();
    fs::write(backup.join("init.lua"), "original").unwrap();

    restore_backup(&target, &backup).unwrap_err();

    assert_eq!(fs::read_dir(&target).unwrap().count(), 0);
    assert_eq!(
        fs::read_to_string(backup.join("init.lua")).unwrap(),
        "original"
    );
}

#[test]
fn restore_backup_rebuilds_a_directory_and_a_link() {
    let dir = tempfile::tempdir().unwrap();

    let backup_dir = dir.path().join("nvim.dotlink-backup.20260101T000000");
    fs::create_dir_all(backup_dir.join("lua")).unwrap();
    fs::write(backup_dir.join("lua/a.lua"), "a").unwrap();
    let target_dir = dir.path().join("nvim");
    restore_backup(&target_dir, &backup_dir).unwrap();
    assert_eq!(
        fs::read_to_string(target_dir.join("lua/a.lua")).unwrap(),
        "a"
    );

    let backup_link = dir.path().join("bashrc.dotlink-backup.20260101T000000");
    os::create_symlink(Path::new("/somewhere/else"), &backup_link).unwrap();
    let target_link = dir.path().join("bashrc");
    restore_backup(&target_link, &backup_link).unwrap();
    assert_eq!(
        fs::read_link(&target_link).unwrap(),
        PathBuf::from("/somewhere/else")
    );
    assert!(
        fs::symlink_metadata(&target_link)
            .unwrap()
            .file_type()
            .is_symlink()
    );

    let leftover: Vec<String> = fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(RESTORE_SUFFIX))
        .collect();
    assert!(
        leftover.is_empty(),
        "恢复的临时名必须已被改名: {leftover:?}"
    );
}

/// 失败清理对三种残渣形态都必须生效: `fs::remove_dir_all` 对普通文件与符号链接
/// 失败 (std 明确只处理目录), 文件复制中途失败留下的 `.partial` 因此清不掉.
#[test]
fn remove_any_clears_files_links_and_directories() {
    let dir = tempfile::tempdir().unwrap();

    let file = dir.path().join("file.partial");
    fs::write(&file, "half").unwrap();

    // 符号链接按链接本身移除: 指向的目录必须保留.
    let kept = dir.path().join("kept");
    fs::create_dir_all(&kept).unwrap();
    let link = dir.path().join("link.partial");
    os::create_symlink(&kept, &link).unwrap();

    let tree = dir.path().join("tree.partial");
    fs::create_dir_all(tree.join("nested")).unwrap();
    fs::write(tree.join("nested/a"), "a").unwrap();

    remove_any(&file);
    remove_any(&link);
    remove_any(&tree);
    // 不存在的路径: 不 panic 也不外抛.
    remove_any(&dir.path().join("absent.partial"));

    for residue in [&file, &link, &tree] {
        assert!(
            fs::symlink_metadata(residue).is_err(),
            "{} 必须已被清除",
            residue.display()
        );
    }
    assert!(
        fs::symlink_metadata(&kept).is_ok(),
        "清理符号链接不得跟随删除其指向"
    );
}

/// 恢复失败时不得残留 `.partial` -- 残渣是普通文件时必须按文件清除.
///
/// 注入点: 目标路径被非空目录占住, 使"复制到临时名"成功而"改名到目标"失败
/// (调用方契约要求目标不存在, 这里故意违约以稳定地走通失败清理路径).
#[test]
fn a_failed_restore_leaves_no_partial_residue() {
    let dir = tempfile::tempdir().unwrap();
    let backup = dir.path().join("bashrc.dotlink-backup.20260101T000000");
    fs::write(&backup, "original").unwrap();

    let target = dir.path().join("bashrc");
    fs::create_dir_all(target.join("nested")).unwrap();
    fs::write(target.join("nested/keep"), "keep").unwrap();

    let error = restore_backup(&target, &backup).unwrap_err();
    assert!(matches!(
        error,
        RestoreError::NotLanded(Error::EntryFailed(_))
    ));

    let residue: Vec<String> = fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(RESTORE_SUFFIX))
        .collect();
    assert!(residue.is_empty(), "恢复失败不得残留临时名: {residue:?}");
    assert_eq!(
        fs::read_to_string(target.join("nested/keep")).unwrap(),
        "keep",
        "目标上已有的内容不得被失败的恢复改动"
    );
}

#[test]
fn copying_never_changes_an_occupied_destination() {
    use std::os::unix::fs::MetadataExt;

    for source_kind in ["file", "directory", "link", "missing"] {
        for destination_kind in ["file", "directory", "link", "dangling"] {
            let dir = tempfile::tempdir().unwrap();
            let source = dir.path().join("source");
            let dest = dir.path().join("dest");
            let sentinel = dir.path().join("sentinel");
            fs::write(&sentinel, "unrelated content").unwrap();
            match source_kind {
                "file" => fs::write(&source, "original").unwrap(),
                "directory" => {
                    fs::create_dir(&source).unwrap();
                    fs::write(source.join("child"), "original").unwrap();
                }
                "link" => os::create_symlink(Path::new("sentinel"), &source).unwrap(),
                "missing" => {}
                _ => unreachable!(),
            }
            match destination_kind {
                "file" => fs::write(&dest, "keep").unwrap(),
                "directory" => {
                    fs::create_dir(&dest).unwrap();
                    fs::write(dest.join("keep"), "keep").unwrap();
                }
                "link" => os::create_symlink(Path::new("sentinel"), &dest).unwrap(),
                "dangling" => os::create_symlink(Path::new("missing"), &dest).unwrap(),
                _ => unreachable!(),
            }
            let inode = fs::symlink_metadata(&dest).unwrap().ino();
            assert!(
                copy_object(&source, &dest).is_err(),
                "{source_kind}/{destination_kind}"
            );
            assert_eq!(fs::symlink_metadata(&dest).unwrap().ino(), inode);
            assert_eq!(fs::read_to_string(&sentinel).unwrap(), "unrelated content");
            match destination_kind {
                "file" => assert_eq!(fs::read_to_string(&dest).unwrap(), "keep"),
                "directory" => {
                    assert_eq!(fs::read_to_string(dest.join("keep")).unwrap(), "keep");
                    assert_eq!(fs::read_dir(&dest).unwrap().count(), 1);
                }
                "link" => assert_eq!(fs::read_link(&dest).unwrap(), Path::new("sentinel")),
                "dangling" => assert_eq!(fs::read_link(&dest).unwrap(), Path::new("missing")),
                _ => unreachable!(),
            }
        }
    }
}

#[test]
fn restore_chooses_another_staging_name_without_following_or_merging_occupants() {
    use std::os::unix::fs::MetadataExt;

    for kind in ["file", "directory", "link", "dangling"] {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("rc");
        let backup = dir.path().join("backup");
        fs::write(&backup, "original").unwrap();
        let sentinel = dir.path().join("sentinel");
        fs::write(&sentinel, "unrelated content").unwrap();
        let occupied = with_suffix(&target, &unique_staging_suffix(RESTORE_SUFFIX));
        match kind {
            "file" => fs::write(&occupied, "keep").unwrap(),
            "directory" => {
                fs::create_dir(&occupied).unwrap();
                fs::write(occupied.join("keep"), "keep").unwrap();
            }
            "link" => os::create_symlink(Path::new("sentinel"), &occupied).unwrap(),
            "dangling" => os::create_symlink(Path::new("missing"), &occupied).unwrap(),
            _ => unreachable!(),
        }
        let inode = fs::symlink_metadata(&occupied).unwrap().ino();

        restore_backup(&target, &backup).unwrap();

        assert_eq!(fs::read_to_string(&sentinel).unwrap(), "unrelated content");
        assert_eq!(fs::symlink_metadata(&occupied).unwrap().ino(), inode);
        assert!(fs::symlink_metadata(&target).unwrap().is_file());
        assert_eq!(fs::read_to_string(&target).unwrap(), "original");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 4);
    }
}

/// 备份暂存名被占用时以排他方式取下一个序号, 既有对象不受影响.
#[test]
fn backup_staging_chooses_another_name_without_touching_occupants() {
    use std::os::unix::fs::MetadataExt;

    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    fs::write(&source, "original").unwrap();
    let base = dir.path().join("bashrc.dotlink-backup.20260101T000000");
    let occupied = with_suffix(&base, &unique_staging_suffix(PARTIAL_SUFFIX));
    fs::write(&occupied, "keep").unwrap();
    let inode = fs::symlink_metadata(&occupied).unwrap().ino();

    let (staged, _) = copy_to_staging(&source, &base, PARTIAL_SUFFIX).unwrap();

    assert_ne!(staged, occupied, "不得覆盖已占用的暂存名");
    assert_eq!(fs::symlink_metadata(&occupied).unwrap().ino(), inode);
    assert_eq!(fs::read_to_string(&occupied).unwrap(), "keep");
    assert_eq!(fs::read_to_string(&staged).unwrap(), "original");
    assert!(
        staged.to_string_lossy().ends_with(".2.partial"),
        "应在 .partial 之前追加递增序号: {}",
        staged.display()
    );
}

/// 暂存根名冲突的尝试有界: 占满 16 个候选后失败, 不创建第 17 个.
#[test]
fn staging_name_retries_are_bounded() {
    use std::os::unix::fs::MetadataExt;

    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    fs::write(&source, "original").unwrap();
    let base = dir.path().join("bashrc.dotlink-backup.20260101T000000");
    let occupants: Vec<_> = (1..=MAX_STAGING_ATTEMPTS)
        .map(|serial| {
            let path = staging_candidate(&base, serial, PARTIAL_SUFFIX);
            let content = format!("occupied-{serial}");
            fs::write(&path, &content).unwrap();
            let inode = fs::symlink_metadata(&path).unwrap().ino();
            (path, content, inode)
        })
        .collect();
    let before = fs::read_dir(dir.path()).unwrap().count();

    let error = copy_to_staging(&source, &base, PARTIAL_SUFFIX).unwrap_err();

    let text = error.to_string();
    assert!(
        text.contains(&MAX_STAGING_ATTEMPTS.to_string()),
        "超限说明必须给出尝试次数: {text}"
    );
    assert!(
        text.contains(
            &staging_candidate(&base, MAX_STAGING_ATTEMPTS, PARTIAL_SUFFIX)
                .display()
                .to_string()
        ),
        "超限说明必须给出最后一个候选名: {text}"
    );
    assert_eq!(
        fs::read_dir(dir.path()).unwrap().count(),
        before,
        "不得创建第 17 个候选"
    );
    for (path, content, inode) in &occupants {
        assert_eq!(fs::symlink_metadata(path).unwrap().ino(), *inode);
        assert_eq!(fs::read_to_string(path).unwrap(), *content);
    }
}

/// 暂存根已取得后填充失败: 只清理本次暂存根, 不产生最终名.
#[test]
fn a_fill_failure_cleans_only_the_staging_root_from_this_attempt() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("file"), "original").unwrap();
    let _socket = std::os::unix::net::UnixListener::bind(source.join("socket")).unwrap();
    let base = dir.path().join("bashrc.dotlink-backup.20260101T000000");
    let existing_backup = dir.path().join("bashrc.dotlink-backup.20250101T000000");
    fs::write(&existing_backup, "earlier").unwrap();
    let user_file = dir.path().join("user-file");
    fs::write(&user_file, "keep").unwrap();
    let before = fs::read_dir(dir.path()).unwrap().count();

    let error = copy_to_staging(&source, &base, PARTIAL_SUFFIX).unwrap_err();

    let text = error.to_string();
    assert!(
        text.contains(&source.join("socket").display().to_string()),
        "说明必须给出特殊文件的完整路径: {text}"
    );
    assert!(
        text.contains("请先手工移走该特殊文件再重试"),
        "说明必须给出下一步动作: {text}"
    );
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), before);
    assert!(!base.exists(), "失败不得发布最终名");
    assert!(
        fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .all(|entry| !entry
                .file_name()
                .to_string_lossy()
                .ends_with(PARTIAL_SUFFIX)),
        "失败不得残留本次暂存对象"
    );
    assert_eq!(fs::read_to_string(source.join("file")).unwrap(), "original");
    assert_eq!(fs::read_to_string(&existing_backup).unwrap(), "earlier");
    assert_eq!(fs::read_to_string(&user_file).unwrap(), "keep");
}

/// 目录含普通文件与 FIFO: 整体成败, 说明给出 FIFO 路径, 暂存清理且原对象不变.
#[test]
fn a_fifo_inside_a_directory_fails_the_whole_backup_with_a_hint() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("file"), "original").unwrap();
    let fifo = source.join("fifo");
    make_fifo(&fifo);
    let fifo_meta = fs::symlink_metadata(&fifo).unwrap();
    let fifo_inode = fifo_meta.ino();
    assert!(fifo_meta.file_type().is_fifo());

    let base = dir.path().join("source.dotlink-backup.20260101T000000");
    let before = fs::read_dir(dir.path()).unwrap().count();

    let error = copy_to_staging(&source, &base, PARTIAL_SUFFIX).unwrap_err();

    let text = error.to_string();
    assert!(
        text.contains(&fifo.display().to_string()),
        "说明必须给出 FIFO 的完整路径: {text}"
    );
    assert!(
        text.contains("请先手工移走该特殊文件再重试"),
        "说明必须给出下一步动作: {text}"
    );
    assert_eq!(
        fs::read_dir(dir.path()).unwrap().count(),
        before,
        "失败不得残留本次暂存对象"
    );
    assert!(!base.exists(), "失败不得发布最终名");
    assert!(
        fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .all(|entry| !entry
                .file_name()
                .to_string_lossy()
                .ends_with(PARTIAL_SUFFIX)),
        "暂存对象必须被清理"
    );

    // 原目录与 FIFO 保持原样: 内容不变, FIFO 仍是同一个 FIFO.
    assert_eq!(fs::read_to_string(source.join("file")).unwrap(), "original");
    let after = fs::symlink_metadata(&fifo).unwrap();
    assert!(after.file_type().is_fifo(), "FIFO 必须保留");
    assert_eq!(after.ino(), fifo_inode, "FIFO 不得被替换");
}

/// 元数据阶段失败: 不发布最终名, 只清理本次暂存, 原对象与既有备份不变.
#[test]
fn a_metadata_failure_publishes_no_backup_and_keeps_the_original() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("child"), "original").unwrap();
    fs::set_permissions(&source, fs::Permissions::from_mode(0o750)).unwrap();
    let base = dir.path().join("source.dotlink-backup.20260101T000000");
    let existing_backup = dir.path().join("bashrc.dotlink-backup.20250101T000000");
    fs::write(&existing_backup, "earlier").unwrap();
    let user_file = dir.path().join("user-file");
    fs::write(&user_file, "keep").unwrap();
    let before = fs::read_dir(dir.path()).unwrap().count();

    let guard = fail_metadata();
    let error = write_backup_at(&source, "20260101T000000", BackupKind::User).unwrap_err();
    assert_eq!(guard.injections(), 1, "元数据失败应立即结束复制");
    drop(guard);

    assert!(matches!(error, Error::EntryFailed(_)));
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), before);
    assert!(!base.exists(), "失败不得发布最终名");
    assert!(
        fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .all(|entry| !entry
                .file_name()
                .to_string_lossy()
                .ends_with(PARTIAL_SUFFIX)),
        "失败不得残留本次暂存对象"
    );
    assert_eq!(
        fs::read_to_string(source.join("child")).unwrap(),
        "original"
    );
    assert_eq!(
        fs::metadata(&source).unwrap().permissions().mode() & 0o777,
        0o750
    );
    assert_eq!(fs::read_to_string(&existing_backup).unwrap(), "earlier");
    assert_eq!(fs::read_to_string(&user_file).unwrap(), "keep");
}

/// 首个暂存候选被占用时仍能换名并成功发布, 不残留 `.partial`.
#[test]
fn backup_staging_retry_publishes_without_partial_residue() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("child"), "original").unwrap();
    let base = dir.path().join("source.dotlink-backup.20260101T000000");
    let occupied = staging_candidate(&base, 1, PARTIAL_SUFFIX);
    fs::write(&occupied, "keep").unwrap();

    let backup = write_backup_at(&source, "20260101T000000", BackupKind::User)
        .unwrap()
        .path;

    assert_eq!(backup, base);
    assert_eq!(
        fs::read_to_string(backup.join("child")).unwrap(),
        "original"
    );
    assert_eq!(fs::read_to_string(&occupied).unwrap(), "keep");
    let residues: Vec<String> = fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(PARTIAL_SUFFIX))
        .collect();
    assert_eq!(
        residues,
        vec![occupied.file_name().unwrap().to_string_lossy().into_owned()],
        "成功发布后除既有占用物外不得残留暂存对象"
    );
}

/// 嵌套复制冲突只尝试一次: 不换暂存名, 清理本次产物, 原对象与既有备份不变.
#[test]
fn nested_copy_conflicts_fail_the_entry_without_renaming() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("child"), "original").unwrap();
    fs::set_permissions(&source, fs::Permissions::from_mode(0o750)).unwrap();
    let base = dir.path().join("source.dotlink-backup.20260101T000000");
    let existing_backup = dir.path().join("bashrc.dotlink-backup.20250101T000000");
    fs::write(&existing_backup, "earlier").unwrap();
    let user_file = dir.path().join("user-file");
    fs::write(&user_file, "keep").unwrap();
    let before = fs::read_dir(dir.path()).unwrap().count();

    let guard = fail_nested_child_copy();
    let error = write_backup_at(&source, "20260101T000000", BackupKind::User).unwrap_err();
    assert_eq!(guard.injections(), 1, "嵌套冲突不得触发暂存名重试");
    drop(guard);

    let text = error.to_string();
    assert!(text.contains("child"), "说明必须指出失败子项: {text}");
    assert!(
        !text.contains("暂存名被占用"),
        "不得把嵌套冲突说成暂存名被占用: {text}"
    );
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), before);
    assert!(!base.exists(), "失败不得发布最终名");
    assert!(
        fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .all(|entry| !entry
                .file_name()
                .to_string_lossy()
                .ends_with(PARTIAL_SUFFIX)),
        "失败不得残留本次暂存对象"
    );
    assert_eq!(
        fs::read_to_string(source.join("child")).unwrap(),
        "original"
    );
    assert_eq!(
        fs::metadata(&source).unwrap().permissions().mode() & 0o777,
        0o750
    );
    assert_eq!(fs::read_to_string(&existing_backup).unwrap(), "earlier");
    assert_eq!(fs::read_to_string(&user_file).unwrap(), "keep");
}

/// 恢复路径复用相同的暂存上限与失败语义.
#[test]
fn restore_staging_retries_are_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let backup = dir.path().join("bashrc.dotlink-backup.20260101T000000");
    fs::write(&backup, "original").unwrap();
    let target = dir.path().join("bashrc");
    for serial in 1..=MAX_STAGING_ATTEMPTS {
        let occupied = staging_candidate(&target, serial, RESTORE_SUFFIX);
        fs::write(&occupied, "keep").unwrap();
    }
    let before = fs::read_dir(dir.path()).unwrap().count();

    let error = restore_backup(&target, &backup).unwrap_err();

    assert!(matches!(error, RestoreError::NotLanded(_)));
    let text = error.to_string();
    assert!(text.contains(&MAX_STAGING_ATTEMPTS.to_string()), "{text}");
    assert!(!target.exists(), "恢复失败不得创建目标");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), before);
    assert_eq!(fs::read_to_string(&backup).unwrap(), "original");
}

/// 恢复的嵌套复制冲突同样只尝试一次, 目标不落地, 备份不变.
#[test]
fn a_nested_conflict_during_restore_does_not_retry_or_leave_a_partial() {
    let dir = tempfile::tempdir().unwrap();
    let backup = dir.path().join("backup");
    fs::create_dir(&backup).unwrap();
    fs::write(backup.join("child"), "original").unwrap();
    let target = dir.path().join("rc");
    let before = fs::read_dir(dir.path()).unwrap().count();

    let guard = fail_nested_child_copy();
    let error = restore_backup(&target, &backup).unwrap_err();
    assert_eq!(guard.injections(), 1, "嵌套冲突不得触发暂存名重试");
    drop(guard);

    assert!(matches!(error, RestoreError::NotLanded(_)));
    let text = error.to_string();
    assert!(text.contains("child"), "说明必须指出失败子项: {text}");
    assert!(!text.contains("暂存名被占用"), "{text}");
    assert!(!target.exists(), "恢复失败不得创建目标");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), before);
    assert_eq!(
        fs::read_to_string(backup.join("child")).unwrap(),
        "original"
    );
}

#[test]
fn a_copy_failure_cleans_only_its_own_partial_directory() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    let dest = dir.path().join("partial");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("file"), "original").unwrap();
    let _socket = std::os::unix::net::UnixListener::bind(source.join("socket")).unwrap();
    // 失败清理只作用于本次创建的暂存对象: 预置一份既有备份与一个用户文件,
    // 二者都必须在失败后原样保留 (而不是只断言"本次产物已清理").
    let existing_backup = dir.path().join("bashrc.dotlink-backup.20250101T000000");
    fs::write(&existing_backup, "earlier").unwrap();
    let user_file = dir.path().join("user-file");
    fs::write(&user_file, "keep").unwrap();

    assert!(copy_object(&source, &dest).is_err());
    assert!(!dest.exists());
    assert_eq!(fs::read_to_string(source.join("file")).unwrap(), "original");
    assert_eq!(
        fs::read_to_string(&existing_backup).unwrap(),
        "earlier",
        "既有备份必须保持原样"
    );
    assert_eq!(
        fs::read_to_string(&user_file).unwrap(),
        "keep",
        "用户文件必须保持原样"
    );
}

#[test]
fn concurrent_backups_keep_every_complete_copy() {
    let dir = tempfile::tempdir().unwrap();
    let object = dir.path().join("rc");
    fs::write(&object, "original").unwrap();
    let barrier = std::sync::Barrier::new(8);
    let paths = std::thread::scope(|scope| {
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let object = &object;
                let barrier = &barrier;
                scope.spawn(move || {
                    barrier.wait();
                    (0..4)
                        .map(|_| write_backup(object, BackupKind::User).unwrap().path)
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        threads
            .into_iter()
            .flat_map(|thread| thread.join().unwrap())
            .collect::<Vec<_>>()
    });
    let distinct: std::collections::HashSet<_> = paths.iter().collect();
    assert_eq!(distinct.len(), 32);
    for path in paths {
        assert_eq!(fs::read_to_string(&path).unwrap(), "original");
        assert!(complete_suffix_of(&suffix_of(&object, &path)).is_some());
    }
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 33);
}

#[test]
fn backup_lookup_uses_one_snapshot_per_directory() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("a");
    let second = dir.path().join("b");
    let original = dir.path().join("b.dotlink-backup.20260101T000000.9");
    fs::write(&original, "original").unwrap();
    let mut finder = BackupFinder::default();
    assert_eq!(finder.find_latest(&first).unwrap(), None);

    let newer = dir.path().join("b.dotlink-backup.20260101T000000.10");
    fs::write(&newer, "newer").unwrap();
    assert_eq!(finder.find_latest(&second).unwrap(), Some(original));
    assert_eq!(
        BackupFinder::default().find_latest(&second).unwrap(),
        Some(newer)
    );

    let other_dir = dir.path().join("other");
    fs::create_dir(&other_dir).unwrap();
    let other = other_dir.join("b.dotlink-backup.20260102T000000");
    fs::write(&other, "other").unwrap();
    assert_eq!(
        finder.find_latest(&other_dir.join("b")).unwrap(),
        Some(other)
    );
}

#[test]
fn directory_backup_index_keeps_each_target_and_numeric_order_separate() {
    let dir = tempfile::tempdir().unwrap();
    let mut finder = BackupFinder::default();
    for target in [
        "rc",
        "rc.extra",
        "配置",
        "rc.dotlink-backup.20260101T000000",
    ] {
        for serial in ["9", "10", "00010", "partial"] {
            fs::write(
                dir.path()
                    .join(format!("{target}.dotlink-backup.20260102T000000.{serial}")),
                serial,
            )
            .unwrap();
        }
    }
    for target in [
        "rc",
        "rc.extra",
        "配置",
        "rc.dotlink-backup.20260101T000000",
    ] {
        assert_eq!(
            finder.find_latest(&dir.path().join(target)).unwrap(),
            Some(
                dir.path()
                    .join(format!("{target}.dotlink-backup.20260102T000000.10"))
            )
        );
    }
    assert_eq!(
        finder.find_latest(&dir.path().join("missing")).unwrap(),
        None
    );
}

// -----------------------------------------------------------------------
// 元数据保真: 目录 mtime, 恢复, 旧备份
// -----------------------------------------------------------------------

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn set_known_mtime(path: &Path, seconds: i64, nanoseconds: i64) {
    use rustix::fs::Timespec;
    use rustix::fs::{AtFlags, CWD, Timestamps, UTIME_OMIT, utimensat};

    let times = Timestamps {
        last_access: Timespec {
            tv_sec: 0,
            tv_nsec: UTIME_OMIT,
        },
        last_modification: Timespec {
            tv_sec: seconds,
            tv_nsec: nanoseconds,
        },
    };
    utimensat(CWD, path, &times, AtFlags::empty()).expect("设置测试 mtime");
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn user_xattr(path: &Path) -> Option<Vec<u8>> {
    use rustix::fs::getxattr;

    let mut buffer = vec![0_u8; 8 * 1024];
    match getxattr(path, "user.test", &mut buffer[..]) {
        Ok(length) => Some(buffer[..length].to_vec()),
        Err(error) if is_missing_xattr(error) => None,
        Err(error) => panic!("读取测试扩展属性失败: {error}"),
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

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn set_user_xattr(path: &Path, value: &[u8]) {
    use rustix::fs::{XattrFlags, setxattr};

    setxattr(path, "user.test", value, XattrFlags::empty()).expect("设置测试扩展属性");
}

#[test]
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn write_backup_preserves_a_directories_mtime_after_copying_children() {
    let dir = tempfile::tempdir().unwrap();
    let object = dir.path().join("nvim");
    fs::create_dir_all(object.join("lua/plugins")).unwrap();
    fs::write(object.join("lua/plugins/a.lua"), "a").unwrap();
    set_known_mtime(&object, 1_600_000_000, 123_456_789);

    let backup = write_backup(&object, BackupKind::User).unwrap().path;

    let copied = fs::metadata(&backup).unwrap();
    assert_eq!(copied.mtime(), 1_600_000_000);
    assert_eq!(copied.mtime_nsec(), 123_456_789);
    // 子项自身的时间戳同样保留.
    let child = fs::metadata(backup.join("lua/plugins/a.lua")).unwrap();
    assert_eq!(
        child.mtime(),
        fs::metadata(object.join("lua/plugins/a.lua"))
            .unwrap()
            .mtime()
    );
}

#[test]
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn restore_brings_back_the_mtime_and_user_xattrs_of_the_original() {
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("bashrc");
    fs::write(&original, "original").unwrap();
    set_known_mtime(&original, 1_577_836_800, 42);
    set_user_xattr(&original, b"fidelity");

    let backup = write_backup(&original, BackupKind::User).unwrap().path;
    fs::remove_file(&original).unwrap();
    restore_backup(&original, &backup).unwrap();

    let restored = fs::metadata(&original).unwrap();
    assert_eq!(restored.mtime(), 1_577_836_800);
    assert_eq!(restored.mtime_nsec(), 42);
    assert_eq!(user_xattr(&original), Some(b"fidelity".to_vec()));
}

#[test]
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn restoring_an_old_backup_uses_the_backups_own_mtime() {
    // 旧版本创建的备份不含保真元数据; 恢复应得到备份自身的 mtime, 而不是恢复时刻.
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("bashrc");
    let backup = dir.path().join("bashrc.dotlink-backup.20260101T000000");
    fs::write(&backup, "original").unwrap();
    set_known_mtime(&backup, 1_577_836_800, 999);

    restore_backup(&target, &backup).unwrap();

    let restored = fs::metadata(&target).unwrap();
    assert_eq!(restored.mtime(), 1_577_836_800);
    assert_eq!(restored.mtime_nsec(), 999);
    assert_eq!(fs::read_to_string(&target).unwrap(), "original");
}

//! 备份路径的构造, 备份的写入, 与"最近一次备份"的查找 (research.md R4).
//!
//! 备份不是需要维护的数据结构, 而是"文件系统上的命名约定": 位置固定在被覆盖路径的
//! **同一目录**, 名称携带本地时区的秒级时间戳, 因此"字典序最大"等价于"最新".
//!
//! 两处不变量由本模块独自承担:
//!
//! - **完整性**: 复制先落到 `<时间戳>.partial`, 全部复制成功后才原子改名为最终名,
//!   因此一个"名字看起来完整"的备份必然是完整内容 (中断残留只能是 `.partial`).
//! - **唯一性**: 最终名已被占用时追加序号 (`.2`, `.3` ...) 消歧, 绝不覆盖既有备份.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use time::OffsetDateTime;

use crate::error::{Error, Result};
use crate::os;

/// 备份名中的固定标记: `<原名>.dotlink-backup.<时间戳>`.
const MARKER: &str = ".dotlink-backup.";

/// 未完成备份的后缀, 加在**时间戳之后**: `x.dotlink-backup.20260101T000000.partial`.
const PARTIAL_SUFFIX: &str = ".partial";

/// 恢复过程中使用的临时名后缀: 目录恢复写到临时名再改名, 避免半成品.
const RESTORE_SUFFIX: &str = ".dotlink-restore.partial";

/// 构造 `<原名>.dotlink-backup.<YYYYMMDDThhmmss>` 形态的备份路径.
///
/// 若算出的名字已被占用 (用 `symlink_metadata` 检测 -- `Path::exists` 对悬空链接
/// 返回 false, 会误判为未占用), 追加序号 `.2`, `.3` ... 直到未占用. 同一秒内对
/// 同一路径的两次替换因此产生两个并存的备份, 而不是后者覆盖前者.
pub fn backup_path_for(target: &Path) -> PathBuf {
    let directory = target.parent().unwrap_or_else(|| Path::new("."));
    let base = backup_base(target);
    let mut candidate = base.clone();
    candidate.push(timestamp());

    if is_free(&candidate, directory) {
        return directory.join(candidate);
    }

    for serial in 2.. {
        let mut candidate = base.clone();
        candidate.push(timestamp());
        candidate.push(".");
        candidate.push(serial.to_string());
        if is_free(&candidate, directory) {
            return directory.join(candidate);
        }
    }

    unreachable!("序号消歧必然终止")
}

/// 取同目录下名称以 `<原名>.dotlink-backup.` 开头者中字典序最大的一个.
///
/// **只接受已完整命名的备份**: 后缀必须是 `<15 位时间戳>` 或 `<15 位时间戳>.<序号>`,
/// 因此复制中途中断留下的 `...<时间戳>.partial` 既不参与"取最新", 也不会被 `undo`
/// 消费 (data-model.md 第 4 节的完整性规则).
pub fn find_latest_backup(target: &Path) -> Option<PathBuf> {
    let directory = target.parent()?;
    let prefix = backup_base(target).to_str()?.to_string();

    let mut latest: Option<(String, PathBuf)> = None;
    for entry in fs::read_dir(directory).ok()?.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some(suffix) = name.strip_prefix(prefix.as_str()) else {
            continue;
        };
        if !is_complete_suffix(suffix) {
            continue;
        }
        if latest.as_ref().is_none_or(|(best, _)| name > best.as_str()) {
            latest = Some((name.to_string(), entry.path()));
        }
    }

    latest.map(|(_, path)| path)
}

/// 把 `object` 备份到它的同目录, 返回备份的最终路径.
///
/// `object` 可以是普通文件, 目录 (递归) 或符号链接 (**复制链接本身, 不跟随**).
/// 返回 `Ok` 意味着备份已完整落定并改名到最终名 -- 调用者此时才可以删除原对象
/// (data-model.md 第 4 节的"先复制后删除的硬顺序"). 失败时不留下最终名.
pub(crate) fn write_backup(object: &Path) -> Result<PathBuf> {
    let final_path = backup_path_for(object);
    let partial = with_suffix(&final_path, PARTIAL_SUFFIX);

    copy_object(object, &partial)?;

    // 复制期间最终名可能被并发覆盖占用; 改名前的这道检查使"绝不覆盖既有备份"在
    // 并发下也只是退化为"多留一个备份", 而不是"少一个备份".
    let mut final_path = final_path;
    for _ in 0..8 {
        if fs::symlink_metadata(&final_path).is_err() {
            return match fs::rename(&partial, &final_path) {
                Ok(()) => Ok(final_path),
                Err(error) => {
                    let _ = fs::remove_dir_all(&partial);
                    Err(failed("备份改名", &final_path, &error))
                }
            };
        }
        final_path = backup_path_for(object);
    }

    let _ = fs::remove_dir_all(&partial);
    Err(Error::EntryFailed(format!(
        "备份 {} 失败: 无法为备份选定未被占用的名称",
        object.display()
    )))
}

/// 把 `backup` 的内容恢复到 `target` (调用者已确保 `target` 当前不存在).
///
/// 先复制到临时名再改名: 目录恢复在中途失败时不会在原路径留下半成品. 恢复成功后
/// **不删除备份文件本身** (`contracts/cli.md` 的"幂等").
pub(crate) fn restore_backup(target: &Path, backup: &Path) -> Result<()> {
    let partial = with_suffix(target, RESTORE_SUFFIX);

    copy_object(backup, &partial)?;

    fs::rename(&partial, target).map_err(|error| {
        let _ = fs::remove_dir_all(&partial);
        failed("恢复备份到", target, &error)
    })
}

/// 把单个对象复制到 `dest`: 文件复制内容与权限位, 目录递归复制 (权限位在子项复制
/// 完成后设置), 符号链接重建**链接本身**.
///
/// 失败时尽最大努力清除 `dest`, 使调用者不会看到一个半成品. 备份与恢复都使用与
/// 原对象**同目录**的 `dest`, 因此链接文本 (可能是相对路径) 的解析基准不变.
pub(crate) fn copy_object(source: &Path, dest: &Path) -> Result<()> {
    match copy_inner(source, dest) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::remove_dir_all(dest);
            Err(error)
        }
    }
}

/// 递归复制的主体.
fn copy_inner(source: &Path, dest: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(source).map_err(|error| failed("读取", source, &error))?;
    let file_type = metadata.file_type();

    // 符号链接必须用 read_link + create_symlink 复制链接本身: `fs::copy` 会跟随链接,
    // 备份到的就是链接指向的内容, 与原对象不是同一个东西.
    if file_type.is_symlink() {
        let raw = os::read_link(source).map_err(|error| failed("读取链接", source, &error))?;
        return os::create_symlink(&raw, dest).map_err(|error| failed("备份链接", source, &error));
    }

    if file_type.is_dir() {
        fs::create_dir_all(dest).map_err(|error| failed("创建备份目录", dest, &error))?;
        let entries = fs::read_dir(source).map_err(|error| failed("读取目录", source, &error))?;
        for entry in entries {
            let entry = entry.map_err(|error| failed("读取目录项", source, &error))?;
            copy_inner(&entry.path(), &dest.join(entry.file_name()))?;
        }
        // 权限位必须最后设置: 一个不可写的目录若先设权限就无法再接收子项 (而它恰恰
        // 是"复制必然失败"这类用例的构造方式).
        fs::set_permissions(dest, metadata.permissions())
            .map_err(|error| failed("设置备份目录权限", dest, &error))?;
        return Ok(());
    }

    fs::copy(source, dest).map_err(|error| failed("复制", source, &error))?;
    Ok(())
}

/// `target` 的备份名前缀: `<原名>.dotlink-backup.`
fn backup_base(target: &Path) -> OsString {
    let mut base = target
        .file_name()
        .unwrap_or_else(|| OsStr::new(""))
        .to_os_string();
    base.push(MARKER);
    base
}

/// 本地时区的秒级时间戳 `YYYYMMDDThhmmss`.
///
/// `now_local()` 在容器等环境下会失败 (无本地时区信息), 此时退化到 UTC: 秒级精度与
/// 同秒消歧都不依赖时区, 因此退化只影响时间戳的字面值, 不影响任何不变量.
fn timestamp() -> String {
    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    format!(
        "{:04}{:02}{:02}T{:02}{:02}{:02}",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
}

/// 在 `directory` 中检测 `name` 是否未被占用 (lstat 语义).
fn is_free(name: &OsStr, directory: &Path) -> bool {
    fs::symlink_metadata(directory.join(name)).is_err()
}

/// 判断备份名的后缀是否为"已完整命名"形态.
fn is_complete_suffix(suffix: &str) -> bool {
    let (stamp, serial) = match suffix.split_once('.') {
        Some((stamp, rest)) => (stamp, Some(rest)),
        None => (suffix, None),
    };

    if !is_timestamp(stamp) {
        return false;
    }

    match serial {
        // 无后缀: `x.dotlink-backup.20260101T000000`
        None => true,
        // 消歧序号: `x.dotlink-backup.20260101T000000.2`; `.partial` 因含字母被排除
        Some(rest) => !rest.is_empty() && rest.bytes().all(|byte| byte.is_ascii_digit()),
    }
}

/// `YYYYMMDDThhmmss`, 15 个字符.
fn is_timestamp(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 15
        && bytes[..8].iter().all(u8::is_ascii_digit)
        && bytes[8] == b'T'
        && bytes[9..].iter().all(u8::is_ascii_digit)
}

/// 在同目录内给 `path` 追加一个后缀.
fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    let mut name = path
        .file_name()
        .unwrap_or_else(|| OsStr::new(""))
        .to_os_string();
    name.push(suffix);
    directory.join(name)
}

/// 统一的条目级错误措辞 (含底层 io 错误).
fn failed(context: &str, path: &Path, error: &io::Error) -> Error {
    Error::EntryFailed(format!("{context} {} 失败: {error}", path.display()))
}

#[cfg(all(test, unix))]
mod tests {

    use std::os::unix::fs::PermissionsExt;

    use super::*;

    /// 取出备份名中 `<原名>.dotlink-backup.` 之后的部分.
    fn suffix_of(target: &Path, backup: &Path) -> String {
        let prefix = format!(
            "{}{}",
            target.file_name().unwrap().to_string_lossy(),
            MARKER
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

        let backup = backup_path_for(&target);
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

    #[test]
    fn backup_name_uses_the_original_file_name_verbatim() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("nvim");

        let backup = backup_path_for(&target);
        let name = backup.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("nvim.dotlink-backup."), "实际为 {name}");
    }

    #[test]
    fn disambiguation_keeps_two_backups_of_the_same_second() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("bashrc");

        let first = backup_path_for(&target);
        fs::write(&first, "first").unwrap();

        let second = backup_path_for(&target);
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

    #[test]
    fn disambiguation_also_handles_a_dangling_symlink_at_the_backup_name() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("bashrc");

        let first = backup_path_for(&target);
        // 悬空链接: `Path::exists` 对它返回 false, 必须用 lstat 检测才算"已占用".
        os::create_symlink(Path::new("/definitely/gone"), &first).unwrap();

        let second = backup_path_for(&target);
        assert_ne!(first, second);
        assert!(
            fs::symlink_metadata(&first)
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    #[test]
    fn find_latest_backup_picks_the_lexicographically_greatest() {
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

        assert_eq!(find_latest_backup(&target), Some(disambiguated));
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

        assert_eq!(find_latest_backup(&target), Some(complete));
    }

    #[test]
    fn find_latest_backup_returns_none_without_a_backup() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("bashrc");
        fs::write(&target, "orig").unwrap();

        assert_eq!(find_latest_backup(&target), None);
        assert_eq!(find_latest_backup(&dir.path().join("absent")), None);
    }

    #[test]
    fn write_backup_copies_files_and_leaves_no_partial() {
        let dir = tempfile::tempdir().unwrap();
        let object = dir.path().join("bashrc");
        fs::write(&object, "original").unwrap();

        let backup = write_backup(&object).unwrap();

        assert_eq!(fs::read_to_string(&backup).unwrap(), "original");
        assert!(is_complete_suffix(&suffix_of(&object, &backup)));
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

    #[test]
    fn write_backup_copies_directories_recursively() {
        let dir = tempfile::tempdir().unwrap();
        let object = dir.path().join("nvim");
        fs::create_dir_all(object.join("lua/plugins")).unwrap();
        fs::write(object.join("init.lua"), "init").unwrap();
        fs::write(object.join("lua/plugins/a.lua"), "a").unwrap();

        let backup = write_backup(&object).unwrap();

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

        let backup = write_backup(&object).unwrap();

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

        let backup = write_backup(&object).unwrap();

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

        let error = write_backup(&object).unwrap_err();
        assert!(matches!(error, Error::EntryFailed(_)));

        // 原对象必须完整保留.
        assert_eq!(fs::read_to_string(object.join("init.lua")).unwrap(), "init");
        // 恢复权限位后才能读回被锁住的子目录内容.
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(fs::read_to_string(locked.join("secret")).unwrap(), "secret");

        // 磁盘上不得存在最终名的备份: 中断残留只能是 `.partial`.
        let prefix = format!("nvim{}", MARKER);
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
}

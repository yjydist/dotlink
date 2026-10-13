//! 备份路径的构造, 备份的写入, 与"最近一次备份"的查找 (research.md R4).
//!
//! 备份不是需要维护的数据结构, 而是"文件系统上的命名约定": 位置固定在被覆盖路径的
//! **同一目录**, 名称携带本地时区的秒级时间戳与消歧序号. "最新"因此可由名字直接
//! 判定: 先比时间戳, 同一秒内再比序号的**数值** (裸名视为 0) -- 而不是整名字典序,
//! 序号 `10` 在字典序上反而小于 `9`.
//!
//! 两处不变量由本模块独自承担:
//!
//! - **完整性**: 复制先落到**进程级唯一**的暂存名 (`<时间戳>.<pid>.partial`), 全部
//!   复制成功后才原子改名为最终名, 因此一个"名字看起来完整"的备份必然是完整内容
//!   (中断残留只能是暂存名, 失败清理会尽力移除它).
//! - **唯一性**: 暂存对象排他创建, 最终名以原子不覆盖移动落位. 两者占用时均追加
//!   序号消歧, 并发备份和遗留暂存对象都不会被覆盖或混入新备份.

use std::cmp::Ordering;
use std::collections::HashMap;
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

#[derive(Default)]
pub(crate) struct BackupFinder {
    directories: HashMap<PathBuf, Result<DirectoryBackups>>,
}

type DirectoryBackups = HashMap<OsString, LatestBackup>;

struct LatestBackup {
    key: (String, String, String),
    path: PathBuf,
}

/// 一次 undo 的目录快照, 在所有撤销动作开始前完成查询.
impl BackupFinder {
    pub(crate) fn find_latest(&mut self, target: &Path) -> Result<Option<PathBuf>> {
        let directory = parent_dir(target);
        let snapshot = self
            .directories
            .entry(directory.to_path_buf())
            .or_insert_with(|| scan_backups(directory));
        match snapshot {
            Ok(backups) => Ok(backups
                .get(target.file_name().unwrap_or_default())
                .map(|backup| backup.path.clone())),
            Err(error) => Err(Error::EntryFailed(error.to_string())),
        }
    }
}

/// 构造 `<原名>.dotlink-backup.<YYYYMMDDThhmmss>` 形态的备份路径.
///
/// 若算出的名字已被占用 (用 `symlink_metadata` 检测 -- `Path::exists` 对悬空链接
/// 返回 false, 会误判为未占用), 追加序号 `.2`, `.3` ... 直到未占用. 同一秒内对
/// 同一路径的两次替换因此产生两个并存的备份, 而不是后者覆盖前者.
pub fn backup_path_for(target: &Path) -> PathBuf {
    backup_candidates(target)
        .find(|path| fs::symlink_metadata(path).is_err())
        .expect("序号消歧必然终止")
}

fn backup_candidates(target: &Path) -> impl Iterator<Item = PathBuf> {
    let mut base = backup_base(target);
    base.push(timestamp());
    let base = parent_dir(target).join(base);
    (1_u64..).map(move |serial| {
        if serial == 1 {
            base.clone()
        } else {
            with_suffix(&base, &format!(".{serial}"))
        }
    })
}

/// 扫描目录, 为每个原始文件名选出最新完整备份.
///
/// **(时间戳, 序号) 最大者胜出**: 跨时间戳取较晚者 (时间戳定长, 字典序即时间序),
/// 同一时间戳内按序号的**数值**比较, 裸名视为序号 0. 不按整名字典序比较 -- 同一秒
/// 内的序号 `10` 在字典序上小于 `9`, 会取错备份 (undo 恢复出来的就不是最新的).
///
/// **只接受已完整命名的备份**: 后缀必须是 `<15 位时间戳>` 或 `<15 位时间戳>.<序号>`,
/// 因此复制中途中断留下的 `...<时间戳>.partial` 既不参与"取最新", 也不会被 `undo`
/// 消费 (data-model.md 第 4 节的完整性规则).
///
/// 目录读取失败不是"无备份"而是**环境故障**, 原样上抛 (调用者按 `Unreadable` 的
/// 先例报 `fail` 并给出底层原因); 只有目录不存在 (`NotFound`) 才算没有备份.
fn scan_backups(directory: &Path) -> Result<DirectoryBackups> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(error) => return Err(failed("读取备份目录", directory, &error)),
    };

    let mut latest = DirectoryBackups::new();
    for entry in entries {
        let entry = entry.map_err(|error| failed("读取备份目录项", directory, &error))?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some((target, suffix)) = name.rsplit_once(MARKER) else {
            continue;
        };
        let Some((stamp, serial)) = complete_suffix_of(suffix) else {
            continue;
        };
        let key = (stamp.to_string(), serial.to_string(), name.to_string());
        if latest
            .get(OsStr::new(target))
            .is_none_or(|best| cmp_backup_key(&key, &best.key) == Ordering::Greater)
        {
            latest.insert(
                OsString::from(target),
                LatestBackup {
                    key,
                    path: entry.path(),
                },
            );
        }
    }

    Ok(latest)
}

#[cfg(test)]
pub(crate) fn find_latest_backup(target: &Path) -> Result<Option<PathBuf>> {
    BackupFinder::default().find_latest(target)
}

/// 把 `object` 备份到它的同目录, 返回备份的最终路径.
///
/// `object` 可以是普通文件, 目录 (递归) 或符号链接 (**复制链接本身, 不跟随**).
/// 返回 `Ok` 意味着备份已完整落定并改名到最终名 -- 调用者此时才可以删除原对象
/// (data-model.md 第 4 节的"先复制后删除的硬顺序"). 失败时不留下最终名.
pub(crate) fn write_backup(object: &Path) -> Result<PathBuf> {
    let mut candidates = backup_candidates(object);
    let first = candidates.next().expect("备份候选名非空");
    let partial = copy_to_staging(object, &first, PARTIAL_SUFFIX)?;

    for final_path in std::iter::once(first).chain(candidates) {
        match os::rename_noreplace(&partial, &final_path) {
            Ok(()) => return Ok(final_path),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                remove_any(&partial);
                return Err(failed("备份改名", &final_path, &error));
            }
        }
    }
    unreachable!("序号消歧必然终止")
}

/// 把 `backup` 的内容恢复到已空出的 `target`, 拒绝覆盖期间新建的占用物.
///
/// 复制先落到**进程级唯一**的临时名, 全部复制成功后再改名到 `target`: 目录恢复中途
/// 失败不会在原路径留下半成品. 恢复成功后**不删除备份文件本身** (`contracts/cli.md`
/// 的"幂等").
pub(crate) fn restore_backup(target: &Path, backup: &Path) -> Result<()> {
    let partial = copy_to_staging(backup, target, RESTORE_SUFFIX)?;

    os::rename_noreplace(&partial, target).map_err(|error| {
        remove_any(&partial);
        failed("恢复备份到", target, &error)
    })
}

fn copy_to_staging(source: &Path, base: &Path, suffix: &str) -> Result<PathBuf> {
    for serial in 1_u64.. {
        let suffix = if serial == 1 {
            unique_staging_suffix(suffix)
        } else {
            format!(".{}.{serial}{suffix}", std::process::id())
        };
        let partial = with_suffix(base, &suffix);
        match copy_object(source, &partial) {
            Ok(()) => return Ok(partial),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(Error::EntryFailed(error.to_string())),
        }
    }
    unreachable!("序号消歧必然终止")
}

/// 排他创建副本. 只有取得目的地所有权之后的失败才允许清理它.
fn copy_object(source: &Path, dest: &Path) -> io::Result<()> {
    let metadata =
        fs::symlink_metadata(source).map_err(|error| copy_error("读取", source, error))?;
    let file_type = metadata.file_type();
    if file_type.is_symlink() {
        let raw = os::read_link(source).map_err(|error| copy_error("读取链接", source, error))?;
        return os::create_symlink(&raw, dest).map_err(|error| copy_error("备份链接", dest, error));
    }

    if file_type.is_dir() {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder
            .create(dest)
            .map_err(|error| copy_error("创建备份目录", dest, error))?;
        let copied = (|| {
            for entry in
                fs::read_dir(source).map_err(|error| copy_error("读取目录", source, error))?
            {
                let entry = entry.map_err(|error| copy_error("读取目录项", source, error))?;
                copy_object(&entry.path(), &dest.join(entry.file_name()))?;
            }
            // 子项复制完成后再收紧权限, 否则只读目录无法接收子项.
            fs::set_permissions(dest, metadata.permissions())
                .map_err(|error| copy_error("设置备份目录权限", dest, error))
        })();
        return finish_copy(dest, copied);
    }

    if !file_type.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!("无法复制特殊文件 {}", source.display()),
        ));
    }
    let mut input = fs::File::open(source).map_err(|error| copy_error("读取", source, error))?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut output = options
        .open(dest)
        .map_err(|error| copy_error("创建备份文件", dest, error))?;
    let copied = io::copy(&mut input, &mut output)
        .map_err(|error| copy_error("复制", source, error))
        .and_then(|_| {
            output
                .set_permissions(metadata.permissions())
                .map_err(|error| copy_error("设置备份文件权限", dest, error))
        });
    finish_copy(dest, copied)
}

fn finish_copy(dest: &Path, result: io::Result<()>) -> io::Result<()> {
    if result.is_err() {
        remove_any(dest);
    }
    result
}

fn copy_error(context: &str, path: &Path, error: io::Error) -> io::Error {
    io::Error::new(
        error.kind(),
        format!("{context} {} 失败: {error}", path.display()),
    )
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

/// `path` 的父目录; 无父目录 (`/`) 或父目录为空 (裸文件名) 时退回当前目录 `.`.
///
/// `Path::parent` 对 `bashrc` 这样的裸文件名返回 `Some("")` 而非 `None`, 因此
/// `unwrap_or_else(|| Path::new("."))` 这类兜底对空父目录不生效, 会让备份名落到
/// 相对路径上. 备份与恢复都要求与目标同目录, 这里统一把空父目录视作 `.`.
fn parent_dir(path: &Path) -> &Path {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    }
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

/// 尽最大努力清除 `path` (普通文件 / 符号链接 / 目录均可), 失败不外抛.
///
/// 失败清理的对象是 `.partial` 之类的残渣, 其类型取决于复制进行到哪一步 (文件复制
/// 失败留下普通文件, 目录复制失败留下半成品目录), 而 `fs::remove_dir_all` 对普通文件
/// 与符号链接**失败** (std 明确只处理目录), 因此按 `symlink_metadata` 分派; 符号链接
/// 按链接本身移除, 不跟随. 清理是尽力而为: 它的失败至多留下残渣, 不得盖过调用点
/// 正在处理的原始错误.
fn remove_any(path: &Path) {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() => {
            let _ = fs::remove_dir_all(path);
        }
        Ok(_) => {
            let _ = fs::remove_file(path);
        }
        Err(_) => {}
    }
}

/// 完整命名的后缀拆成 `(时间戳, 序号数字串)`; 裸名 (无序号) 的序号记为空串, 数值上是 0.
fn complete_suffix_of(suffix: &str) -> Option<(&str, &str)> {
    let (stamp, serial) = match suffix.split_once('.') {
        Some((stamp, rest)) => {
            // 消歧序号是纯数字; `.partial` 因含字母被排除, 空序号 (`<时间戳>.`) 同样排除.
            if rest.is_empty() || !rest.bytes().all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            (stamp, rest)
        }
        // 无后缀: `x.dotlink-backup.20260101T000000`
        None => (suffix, ""),
    };

    is_timestamp(stamp).then_some((stamp, serial))
}

/// 备份的"新旧"序: 先时间戳, 同时间戳内按序号的数值, 全同则按名字字典序兜底.
///
/// 名字兜底只在 `.02` 与 `.2` 这类同值异形上走到, 使结论与目录遍历顺序无关.
fn cmp_backup_key(a: &(String, String, String), b: &(String, String, String)) -> Ordering {
    a.0.cmp(&b.0)
        .then_with(|| cmp_serial(&a.1, &b.1))
        .then_with(|| a.2.cmp(&b.2))
}

/// 比较两个消歧序号的**数值**大小: 去掉前导零后按 (位数, 字典序) 比较, 空串 (裸名)
/// 与全零串都归一为 0. 不按整数解析: 序号是任意位数字串, 解析会溢出.
fn cmp_serial(a: &str, b: &str) -> Ordering {
    let a = a.trim_start_matches('0');
    let b = b.trim_start_matches('0');
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
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
    let directory = parent_dir(path);
    let mut name = path
        .file_name()
        .unwrap_or_else(|| OsStr::new(""))
        .to_os_string();
    name.push(suffix);
    directory.join(name)
}

/// PID 减少并发命名碰撞; 排他性由创建操作保证, 不能依赖 PID 未被复用.
fn unique_staging_suffix(suffix: &str) -> String {
    format!(".{}{suffix}", std::process::id())
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

    /// 裸文件名 (`bashrc`) 的 `parent()` 是 `Some("")` 而非 `None`: 备份名必须落在
    /// 当前目录 `.`, 而不是退化成相对名.
    #[test]
    fn a_bare_file_name_backs_up_into_the_current_directory() {
        let target = Path::new("bashrc");
        assert_eq!(parent_dir(target), Path::new("."));

        let backup = backup_path_for(target);
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
                dir.path().join(format!("bashrc{MARKER}{stamp}{serial}")),
                "x",
            )
            .unwrap();
        }

        assert_eq!(
            find_latest_backup(&target).unwrap(),
            Some(dir.path().join(format!("bashrc{MARKER}{stamp}.10"))),
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

        let name = format!("bashrc{MARKER}20260101T000000{suffix}");
        let after_marker = name.strip_prefix(&format!("bashrc{MARKER}")).unwrap();
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

        let backup = write_backup(&object).unwrap();

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
        assert!(matches!(error, Error::EntryFailed(_)));

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

    #[test]
    fn a_copy_failure_cleans_only_its_own_partial_directory() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        let dest = dir.path().join("partial");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("file"), "original").unwrap();
        let _socket = std::os::unix::net::UnixListener::bind(source.join("socket")).unwrap();

        assert!(copy_object(&source, &dest).is_err());
        assert!(!dest.exists());
        assert_eq!(fs::read_to_string(source.join("file")).unwrap(), "original");
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
                            .map(|_| write_backup(object).unwrap())
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
}

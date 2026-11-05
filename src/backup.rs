//! 备份路径的构造, 备份的写入, 与"最近一次备份"的查找
//! (openspec/specs/backup-lifecycle/spec.md).
//!
//! 备份不是需要维护的数据结构, 而是"文件系统上的命名约定": 位置固定在被覆盖路径的
//! **同一目录**, 名称携带 UTC 的秒级时间戳与消歧序号. "最新"因此可由名字直接
//! 判定: 先比时间戳, 同一秒内再比序号的**数值** (裸名视为 0) -- 而不是整名字典序,
//! 序号 `10` 在字典序上反而小于 `9`. 时间戳必须是 UTC: 定长的 15 字符形态只在同一
//! 时间轴上才能保证字符串序即时间序.
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

use time::{OffsetDateTime, UtcOffset};

use crate::error::{Error, Result};
use crate::os;

/// 用户备份命名空间的标记: `<原名>.dotlink-backup.<时间戳>`.
///
/// 只有这个命名空间参与"最新备份"查找, 因此外部占用触发的替换与撤销恢复共用它
/// (openspec/specs/link-teardown/spec.md 的"撤销只恢复用户原始备份").
const USER_MARKER: &str = ".dotlink-backup.";

/// 自愈备份命名空间的标记: `<原名>.dotlink-heal.<时间戳>`.
///
/// 本工具重建自己留下的错误链接时, 旧链接按这个命名空间备份. 它不是用户内容,
/// 因此刻意与 [`USER_MARKER`] 分开, 使撤销查找器天然看不到它.
const SELF_HEAL_MARKER: &str = ".dotlink-heal.";

/// 备份命名空间: 用户内容与工具自愈产生的旧链接分开存放.
///
/// 命名空间只影响名字中的标记, 不改变暂存, 复制, 发布与同步路径.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BackupKind {
    /// 被替换的外部占用物: 普通文件, 目录或外部符号链接.
    User,
    /// 本工具创建但指向错误的旧链接 (重建前先备份链接本身).
    SelfHeal,
}

impl BackupKind {
    fn marker(self) -> &'static str {
        match self {
            Self::User => USER_MARKER,
            Self::SelfHeal => SELF_HEAL_MARKER,
        }
    }
}

/// 未完成备份的后缀, 加在**时间戳之后**: `x.dotlink-backup.20260101T000000.partial`.
const PARTIAL_SUFFIX: &str = ".partial";

/// 恢复过程中使用的临时名后缀: 目录恢复写到临时名再改名, 避免半成品.
const RESTORE_SUFFIX: &str = ".dotlink-restore.partial";

/// 暂存根名被占用时最多尝试的候选名数量.
///
/// 只有暂存根的排他创建失败才消耗一次尝试; 正常路径第一次即成功. 上限防止外部进程
/// 持续占用名字时无界循环, 同时保留"换一个序号重试"的既有行为.
const MAX_STAGING_ATTEMPTS: u64 = 16;

/// 撤销用的"最新备份"查找器.
///
/// 只扫描用户备份命名空间: 自愈备份 (`.dotlink-heal.`) 不是可恢复的用户内容, 不参与
/// 快照, 也不会让撤销把"有备份"当作可以恢复的依据 (设计 D4).
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

/// 构造 `<原名>.<标记><YYYYMMDDThhmmss>` 形态的备份路径, 标记由 `kind` 决定.
///
/// 若算出的名字已被占用 (用 `symlink_metadata` 检测 -- `Path::exists` 对悬空链接
/// 返回 false, 会误判为未占用), 追加序号 `.2`, `.3` ... 直到未占用. 同一秒内对
/// 同一路径的两次替换因此产生两个并存的备份, 而不是后者覆盖前者.
pub fn backup_path_for(target: &Path, kind: BackupKind) -> PathBuf {
    backup_candidates(target, kind)
        .find(|path| fs::symlink_metadata(path).is_err())
        .expect("序号消歧必然终止")
}

fn backup_candidates(target: &Path, kind: BackupKind) -> impl Iterator<Item = PathBuf> {
    backup_candidates_at(target, timestamp(), kind)
}

/// 给定时间戳的候选名序列 (裸名, `.2`, `.3` ...).
///
/// 时间戳是参数而不是在闭包里取当前时刻: 这样"同一秒内的两次替换产生 `.2`"可以被
/// 确定性地测试, 不受墙钟是否跨秒的影响.
fn backup_candidates_at(
    target: &Path,
    stamp: String,
    kind: BackupKind,
) -> impl Iterator<Item = PathBuf> {
    let mut base = backup_base(target, kind);
    base.push(stamp);
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
/// **(时间戳, 序号) 最大者胜出**: 跨时间戳取较晚者 (时间戳来自 UTC 且定长 15 字符,
/// 字符串序即时间序),
/// 同一时间戳内按序号的**数值**比较, 裸名视为序号 0. 不按整名字典序比较 -- 同一秒
/// 内的序号 `10` 在字典序上小于 `9`, 会取错备份 (undo 恢复出来的就不是最新的).
///
/// **只接受已完整命名的备份**: 后缀必须是 `<15 位时间戳>` 或 `<15 位时间戳>.<序号>`,
/// 因此复制中途中断留下的 `...<时间戳>.partial` 既不参与"取最新", 也不会被 `undo`
/// 消费 (openspec/specs/backup-lifecycle/spec.md 的"只有完整命名的备份参与查找").
///
/// **只接受用户备份命名空间**: 扫描按 [`USER_MARKER`] 切分名字, 自愈备份使用
/// [`SELF_HEAL_MARKER`], 即使原文件名本身含 `.dotlink-backup.`, 最后一个用户标记
/// 之后也一定包含 `.dotlink-heal.` 段, 不可能是完整后缀. 自愈备份因此不会成为
/// "最新备份", 也不会被撤销恢复或消费.
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
        let Some((target, suffix)) = name.rsplit_once(USER_MARKER) else {
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

/// 已发布的备份: 最终路径与复制起点处顶层对象的元数据.
///
/// 元数据来自 [`copy_object`] 首次对源做的 `symlink_metadata`, 因此"备份的内容"与
/// "身份基准"同源: 调用者可以用它复核删除的对象是不是备份里保存的那一个, 而不必在
/// 备份之前另取一次快照 (设计 D1).
#[derive(Debug)]
pub(crate) struct PublishedBackup {
    pub(crate) path: PathBuf,
    pub(crate) source_metadata: fs::Metadata,
}

/// 把 `object` 按 `kind` 的命名空间备份到它的同目录, 返回最终路径与复制起点的元数据.
///
/// `object` 可以是普通文件, 目录 (递归) 或符号链接 (**复制链接本身, 不跟随**).
/// 返回 `Ok` 意味着备份已完整落定并改名到最终名 -- 调用者此时才可以删除原对象
/// (openspec/specs/backup-fidelity/spec.md 的"备份的发布仍以完整复制为前提"). 失败时不留下最终名.
pub(crate) fn write_backup(object: &Path, kind: BackupKind) -> Result<PublishedBackup> {
    write_backup_at(object, &timestamp(), kind)
}

/// 给定时间戳的备份写入 (见 [`backup_candidates_at`]): 测试用它绕过墙钟.
fn write_backup_at(object: &Path, stamp: &str, kind: BackupKind) -> Result<PublishedBackup> {
    let mut candidates = backup_candidates_at(object, stamp.to_string(), kind);
    let first = candidates.next().expect("备份候选名非空");
    let (partial, source_metadata) = copy_to_staging(object, &first, PARTIAL_SUFFIX)?;

    for final_path in std::iter::once(first).chain(candidates) {
        match os::rename_noreplace(&partial, &final_path) {
            Ok(()) => {
                // rename 已经把备份发布为最终名. 之后的父目录同步失败不能回滚已发布
                // 的备份, 但失败说明必须给出该路径, 用户才知道备份已经存在 (FR-016).
                sync_parent(&final_path).map_err(|error| {
                    Error::EntryFailed(format!("{error}; 备份已发布到 {}", final_path.display()))
                })?;
                return Ok(PublishedBackup {
                    path: final_path,
                    source_metadata,
                });
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                remove_any(&partial);
                return Err(failed("备份改名", &final_path, &error));
            }
        }
    }
    unreachable!("序号消歧必然终止")
}

/// `restore_backup` 的失败阶段.
///
/// rename 之前的失败意味着内容尚未落到目标, 调用方可以安全地把暂存链接移回原路径;
/// rename 之后的父目录同步失败意味着内容已经就位, 目标已被占用, 回滚既无意义也不会
/// 成功, 调用方 MUST 只清理旧链接并报告内容已恢复.
#[derive(Debug)]
pub(crate) enum RestoreError {
    /// 复制或 rename 失败, 目标路径仍为空.
    NotLanded(Error),
    /// rename 成功, 只有父目录同步失败.
    LandedButUnsynced(Error),
}

impl std::fmt::Display for RestoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotLanded(error) | Self::LandedButUnsynced(error) => write!(f, "{error}"),
        }
    }
}

/// 把 `backup` 的内容恢复到已空出的 `target`, 拒绝覆盖期间新建的占用物.
///
/// 复制先落到**进程级唯一**的临时名, 全部复制成功后再改名到 `target`: 目录恢复中途
/// 失败不会在原路径留下半成品. 恢复成功后**不删除备份文件本身**
/// (`openspec/specs/link-teardown/spec.md` 的"备份文件恢复后仍留在原地").
pub(crate) fn restore_backup(
    target: &Path,
    backup: &Path,
) -> std::result::Result<(), RestoreError> {
    let (partial, _) =
        copy_to_staging(backup, target, RESTORE_SUFFIX).map_err(RestoreError::NotLanded)?;

    os::rename_noreplace(&partial, target).map_err(|error| {
        remove_any(&partial);
        RestoreError::NotLanded(failed("恢复备份到", target, &error))
    })?;

    // rename 已把内容放到目标: 之后的同步失败不能触发回滚, 但仍要报告持久性未确认.
    sync_parent(target).map_err(|error| {
        RestoreError::LandedButUnsynced(Error::EntryFailed(format!(
            "{error}; 备份内容已恢复到 {}",
            target.display()
        )))
    })
}

/// 取得暂存根名失败的两类原因.
///
/// 只有 [`Self::StagingNameTaken`] 可以换名重试: 它只由本次调用**排他创建自己的
/// 目标根名**时产生. 一旦暂存根已取得, 填充期间的任何错误 (包括子项创建时的
/// `AlreadyExists`) 都是 [`Self::Fatal`], 必须直接成为条目失败.
#[derive(Debug)]
enum CopyFailure {
    StagingNameTaken,
    Fatal(io::Error),
}

impl CopyFailure {
    /// 把根创建错误归类: 仅 `AlreadyExists` 表示暂存根名已被占用.
    fn from_root_create(context: &str, dest: &Path, error: io::Error) -> Self {
        if error.kind() == io::ErrorKind::AlreadyExists {
            Self::StagingNameTaken
        } else {
            Self::Fatal(copy_error(context, dest, error))
        }
    }

    fn fatal(error: io::Error) -> Self {
        Self::Fatal(error)
    }
}

fn copy_to_staging(source: &Path, base: &Path, suffix: &str) -> Result<(PathBuf, fs::Metadata)> {
    let mut last = PathBuf::new();
    for serial in 1..=MAX_STAGING_ATTEMPTS {
        let partial = staging_candidate(base, serial, suffix);
        match copy_object(source, &partial) {
            Ok(metadata) => return Ok((partial, metadata)),
            Err(CopyFailure::StagingNameTaken) => last = partial,
            Err(CopyFailure::Fatal(error)) => {
                return Err(Error::EntryFailed(error.to_string()));
            }
        }
    }
    Err(Error::EntryFailed(format!(
        "暂存名 {} 连续 {MAX_STAGING_ATTEMPTS} 次尝试均被占用, 已尝试 {MAX_STAGING_ATTEMPTS} 次",
        last.display()
    )))
}

/// 第 `serial` 个暂存候选名: 首次带进程号, 之后在 `.partial` 前追加序号.
fn staging_candidate(base: &Path, serial: u64, suffix: &str) -> PathBuf {
    let suffix = if serial == 1 {
        unique_staging_suffix(suffix)
    } else {
        format!(".{}.{serial}{suffix}", std::process::id())
    };
    with_suffix(base, &suffix)
}

/// 排他创建副本, 返回复制起点处顶层对象的元数据. 只有取得目的地所有权之后的失败才
/// 允许清理它.
///
/// 返回值区分根名冲突与其余失败: 仅当**本函数自己创建目标根名**时收到
/// `AlreadyExists` 才是 [`CopyFailure::StagingNameTaken`]; 子项递归通过
/// [`copy_child`] 把同类失败包装成带子项路径的致命错误, 避免整树复制被误重试.
fn copy_object(source: &Path, dest: &Path) -> std::result::Result<fs::Metadata, CopyFailure> {
    let metadata = fs::symlink_metadata(source)
        .map_err(|error| CopyFailure::Fatal(copy_error("读取", source, error)))?;
    let file_type = metadata.file_type();
    if file_type.is_symlink() {
        let raw = os::read_link(source)
            .map_err(|error| CopyFailure::Fatal(copy_error("读取链接", source, error)))?;
        return os::create_symlink(&raw, dest)
            .map_err(|error| CopyFailure::from_root_create("备份链接", dest, error))
            .map(|()| metadata);
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
            .map_err(|error| CopyFailure::from_root_create("创建备份目录", dest, error))?;
        let copied = (|| {
            for entry in fs::read_dir(source)
                .map_err(|error| CopyFailure::Fatal(copy_error("读取目录", source, error)))?
            {
                let entry = entry
                    .map_err(|error| CopyFailure::Fatal(copy_error("读取目录项", source, error)))?;
                copy_child(&entry.path(), &dest.join(entry.file_name()))?;
            }
            // 子项复制完成后再应用元数据: 创建子项会改变目录 mtime, 只读权限也会
            // 阻止后续子项写入.
            apply_metadata(source, dest, &metadata).map_err(CopyFailure::fatal)?;
            // 全部子项与目录自身都同步完成后, 复制产物才算落盘 (设计 D4).
            skip_unsupported(os::sync_dir(dest))
                .map_err(|error| CopyFailure::Fatal(copy_error("同步备份目录", dest, error)))
        })();
        finish_copy(dest, copied)?;
        return Ok(metadata);
    }

    if !file_type.is_file() {
        return Err(CopyFailure::Fatal(io::Error::new(
            io::ErrorKind::Unsupported,
            format!(
                "无法复制特殊文件 {}; 请先手工移走该特殊文件再重试",
                source.display()
            ),
        )));
    }
    let mut input = fs::File::open(source)
        .map_err(|error| CopyFailure::Fatal(copy_error("读取", source, error)))?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut output = options
        .open(dest)
        .map_err(|error| CopyFailure::from_root_create("创建备份文件", dest, error))?;
    let copied = io::copy(&mut input, &mut output)
        .map_err(|error| CopyFailure::Fatal(copy_error("复制", source, error)))
        .and_then(|_| apply_metadata(source, dest, &metadata).map_err(CopyFailure::fatal))
        .and_then(|()| {
            // 数据与元数据在 rename 到最终名前同步到稳定存储 (设计 D4).
            skip_unsupported(os::sync_file(&output))
                .map_err(|error| CopyFailure::Fatal(copy_error("同步备份文件", dest, error)))
        });
    finish_copy(dest, copied)?;
    Ok(metadata)
}

/// 递归复制一个子项.
///
/// 子项的"目标名已被占用"不再具有重试意义: 暂存根已经取得, 换名只会从头复制整棵
/// 树并再次撞上同一个冲突. 因此把 [`CopyFailure::StagingNameTaken`] 包装成带子项
/// 路径的致命错误 (openspec/specs/backup-lifecycle/spec.md 的"嵌套复制冲突不换名重试").
fn copy_child(source: &Path, dest: &Path) -> std::result::Result<(), CopyFailure> {
    #[cfg(test)]
    let copied = if take_nested_copy_failure() {
        Err(CopyFailure::StagingNameTaken)
    } else {
        copy_object(source, dest)
    };
    #[cfg(not(test))]
    let copied = copy_object(source, dest);

    copied.map(|_| ()).map_err(|failure| match failure {
        CopyFailure::StagingNameTaken => nested_conflict(dest),
        fatal => fatal,
    })
}

fn nested_conflict(dest: &Path) -> CopyFailure {
    CopyFailure::Fatal(io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!("创建备份对象 {} 失败: 目标名已被占用", dest.display()),
    ))
}

/// 给普通文件或目录应用保真元数据, 顺序固定为**扩展属性 -> 权限位 -> mtime**.
///
/// 扩展属性必须最先设置: 目标此刻仍是创建时的 `0600` / `0700`, 最终权限若只读,
/// `setxattr` 会返回 `EACCES` (macOS 实测). 权限位只改变 ctime, mtime 最后设置即可
/// 覆盖前面所有写入 (目录还包括子项创建). 平台不支持某类元数据时返回 `Unsupported`,
/// 视为该平台的降级, 不使备份失败.
fn apply_metadata(source: &Path, dest: &Path, metadata: &fs::Metadata) -> io::Result<()> {
    #[cfg(test)]
    if take_metadata_failure() {
        return Err(io::Error::other("模拟应用备份元数据失败"));
    }
    skip_unsupported(os::copy_xattrs(source, dest))
        .map_err(|error| copy_error("设置备份扩展属性", dest, error))?;
    fs::set_permissions(dest, metadata.permissions())
        .map_err(|error| copy_error("设置备份权限", dest, error))?;
    skip_unsupported(os::set_mtime(metadata, dest))
        .map_err(|error| copy_error("设置备份修改时间", dest, error))
}

/// 平台级"不支持"是降级而非失败; 其他错误原样上抛.
fn skip_unsupported(result: io::Result<()>) -> io::Result<()> {
    match result {
        Err(error) if error.kind() == io::ErrorKind::Unsupported => Ok(()),
        other => other,
    }
}

fn finish_copy(
    dest: &Path,
    result: std::result::Result<(), CopyFailure>,
) -> std::result::Result<(), CopyFailure> {
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

/// `target` 的备份名前缀: `<原名>` 加命名空间标记.
fn backup_base(target: &Path, kind: BackupKind) -> OsString {
    let mut base = target
        .file_name()
        .unwrap_or_else(|| OsStr::new(""))
        .to_os_string();
    base.push(kind.marker());
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

/// UTC 的秒级时间戳 `YYYYMMDDThhmmss`.
///
/// 时间戳必须是 UTC, 因为"取最新"按 15 字符时间戳做字符串比较: 同一目标的全部备份
/// 必须共享同一时间轴, 字符串序才等于时间序. 读取本地时区会让同一目标在不同 UTC
/// 偏移下生成的备份无法比较, `undo` 可能恢复较旧的内容; 因此这里没有本地时区分支,
/// 也不依赖运行环境的时区数据.
fn timestamp() -> String {
    format_timestamp(OffsetDateTime::now_utc())
}

/// 把任意时刻归一到 UTC 后格式化为 15 字符 `YYYYMMDDThhmmss`.
///
/// 拆出纯函数是为了让带偏移的固定时刻可被单元测试断言: `+08:00` 的输入必须输出
/// UTC 化的字面值, 而不是原时刻的本地字段.
fn format_timestamp(at: OffsetDateTime) -> String {
    let utc = at.to_offset(UtcOffset::UTC);
    format!(
        "{:04}{:02}{:02}T{:02}{:02}{:02}",
        utc.year(),
        u8::from(utc.month()),
        utc.day(),
        utc.hour(),
        utc.minute(),
        utc.second()
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
/// 时间戳来自 UTC 且定长 15 字符, 因此第一步的字符串比较即时间比较; 不解析成日期,
/// 也不依据本地时区偏移重新解释. 名字兜底只在 `.02` 与 `.2` 这类同值异形上走到,
/// 使结论与目录遍历顺序无关.
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

/// rename 之后同步父目录, 让新名字在崩溃后仍然可见.
///
/// `Unsupported` 与其他元数据的降级语义一致; 其余错误使该条目失败 (设计 D4).
fn sync_parent(path: &Path) -> Result<()> {
    let parent = parent_dir(path);
    skip_unsupported(os::sync_dir(parent)).map_err(|error| failed("同步父目录", parent, &error))
}

/// 统一的条目级错误措辞 (含底层 io 错误).
fn failed(context: &str, path: &Path, error: &io::Error) -> Error {
    Error::EntryFailed(format!("{context} {} 失败: {error}", path.display()))
}

// 测试接缝: 让子项复制返回模拟的 `AlreadyExists` 分类 (`StagingNameTaken`).
//
// 真实的嵌套冲突无法在可移植的测试里稳定构造; 线程局部状态避免并行测试互相影响.
// 注入只发生在 `copy_child` (子项递归入口), 不覆盖暂存根的排他创建; 返回的
// `StagingNameTaken` 会经过 `copy_child` 的 `Fatal` 包装.
#[cfg(test)]
thread_local! {
    static FAIL_NESTED_COPY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static NESTED_COPY_FAILURES: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// 在当前线程启用子项冲突注入, 直到返回的守卫被丢弃.
#[cfg(test)]
fn fail_nested_child_copy() -> NestedCopyFailureGuard {
    let previous_active = FAIL_NESTED_COPY.with(|slot| slot.replace(true));
    let previous_count = NESTED_COPY_FAILURES.with(|slot| slot.replace(0));
    NestedCopyFailureGuard {
        previous_active,
        previous_count,
    }
}

#[cfg(test)]
struct NestedCopyFailureGuard {
    previous_active: bool,
    previous_count: u32,
}

#[cfg(test)]
impl NestedCopyFailureGuard {
    /// 已注入次数; 若实现仍把嵌套冲突当作暂存名冲突重试, 该值会随重试增长.
    fn injections(&self) -> u32 {
        NESTED_COPY_FAILURES.with(std::cell::Cell::get)
    }
}

#[cfg(test)]
impl Drop for NestedCopyFailureGuard {
    fn drop(&mut self) {
        FAIL_NESTED_COPY.with(|slot| slot.set(self.previous_active));
        NESTED_COPY_FAILURES.with(|slot| slot.set(self.previous_count));
    }
}

/// 测试注入点: 命中当前注入时记一次数, 让子项复制返回
/// [`CopyFailure::StagingNameTaken`], 由 [`copy_child`] 包装成带路径的 `Fatal`.
#[cfg(test)]
fn take_nested_copy_failure() -> bool {
    if !FAIL_NESTED_COPY.with(std::cell::Cell::get) {
        return false;
    }
    NESTED_COPY_FAILURES.with(|slot| slot.set(slot.get() + 1));
    true
}

// 测试接缝: 让元数据应用阶段返回模拟的元数据写入失败.
//
// 真实的扩展属性 / 权限 / mtime 失败无法在可移植的测试里稳定构造; 线程局部状态
// 避免并行测试互相影响. 注入命中后由调用点包装成 `CopyFailure::Fatal`, 因此不会
// 触发暂存名换名重试.
#[cfg(test)]
thread_local! {
    static FAIL_METADATA: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static METADATA_FAILURES: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// 在当前线程启用元数据失败注入, 直到返回的守卫被丢弃.
#[cfg(test)]
fn fail_metadata() -> MetadataFailureGuard {
    let previous_active = FAIL_METADATA.with(|slot| slot.replace(true));
    let previous_count = METADATA_FAILURES.with(|slot| slot.replace(0));
    MetadataFailureGuard {
        previous_active,
        previous_count,
    }
}

#[cfg(test)]
struct MetadataFailureGuard {
    previous_active: bool,
    previous_count: u32,
}

#[cfg(test)]
impl MetadataFailureGuard {
    /// 已注入次数; 元数据失败应立即结束复制, 因此用例断言恰为 1.
    fn injections(&self) -> u32 {
        METADATA_FAILURES.with(std::cell::Cell::get)
    }
}

#[cfg(test)]
impl Drop for MetadataFailureGuard {
    fn drop(&mut self) {
        FAIL_METADATA.with(|slot| slot.set(self.previous_active));
        METADATA_FAILURES.with(|slot| slot.set(self.previous_count));
    }
}

/// 测试注入点: 命中当前注入时记一次数, 让 `apply_metadata` 返回错误.
#[cfg(test)]
fn take_metadata_failure() -> bool {
    if !FAIL_METADATA.with(std::cell::Cell::get) {
        return false;
    }
    METADATA_FAILURES.with(|slot| slot.set(slot.get() + 1));
    true
}

#[cfg(all(test, unix))]
mod tests {

    use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};

    use super::*;

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

        let backup = backup_path_for(&target, BackupKind::User);
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

        let backup = backup_path_for(&target, BackupKind::User);
        let name = backup.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("nvim.dotlink-backup."), "实际为 {name}");
    }

    /// 裸文件名 (`bashrc`) 的 `parent()` 是 `Some("")` 而非 `None`: 备份名必须落在
    /// 当前目录 `.`, 而不是退化成相对名.
    #[test]
    fn a_bare_file_name_backs_up_into_the_current_directory() {
        let target = Path::new("bashrc");
        assert_eq!(parent_dir(target), Path::new("."));

        let backup = backup_path_for(target, BackupKind::User);
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

        let first = backup_path_for(&target, BackupKind::User);
        fs::write(&first, "first").unwrap();

        let second = backup_path_for(&target, BackupKind::User);
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
                path.file_name().is_some_and(|name| {
                    name.to_string_lossy().starts_with("bashrc.dotlink-backup.")
                })
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

        let first = backup_path_for(&target, BackupKind::User);
        // 悬空链接: `Path::exists` 对它返回 false, 必须用 lstat 检测才算"已占用".
        os::create_symlink(Path::new("/definitely/gone"), &first).unwrap();

        let second = backup_path_for(&target, BackupKind::User);
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
}

//! 备份的命名, 时间戳与"最近一次备份"的查找.
//!
//! 本模块只处理"名字"这一层: 命名空间标记, 候选名生成, 时间戳的格式化与校验,
//! 以及按目录扫描出每个目标的最新完整备份. 复制与发布在 `copy.rs`.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use time::{Date, Month, OffsetDateTime, Time, UtcOffset};

use crate::error::{Error, Result};

use super::{BackupKind, failed};

#[cfg(test)]
use super::test_support::take_backup_path_failure;

/// 用户备份命名空间的标记: `<原名>.dotlink-backup.<时间戳>`.
///
/// 只有这个命名空间参与"最新备份"查找, 因此外部占用触发的替换与撤销恢复共用它
/// (openspec/specs/link-teardown/spec.md 的"撤销只恢复用户原始备份").
pub(super) const USER_MARKER: &str = ".dotlink-backup.";

/// 自愈备份命名空间的标记: `<原名>.dotlink-heal.<时间戳>`.
///
/// 本工具重建自己留下的错误链接时, 旧链接按这个命名空间备份. 它不是用户内容,
/// 因此刻意与 [`USER_MARKER`] 分开, 使撤销查找器天然看不到它.
pub(super) const SELF_HEAL_MARKER: &str = ".dotlink-heal.";

/// 撤销用的"最新备份"查找器.
///
/// 只扫描用户备份命名空间: 自愈备份 (`.dotlink-heal.`) 不是可恢复的用户内容, 不参与
/// 快照, 也不会让撤销把"有备份"当作可以恢复的依据 (设计 D4).
#[derive(Default)]
pub(crate) struct BackupFinder {
    directories: HashMap<PathBuf, Result<DirectoryBackups>>,
}

type DirectoryBackups = HashMap<OsString, LatestBackup>;

/// 备份的排序键: `(时间戳, 序号数字串, 原始文件名)`.
///
/// 第三项保留原始字节, 因此非 UTF-8 的原文件名也能参与稳定兜底比较, 不经过有损
/// 字符串转换.
type BackupKey = (String, String, OsString);

struct LatestBackup {
    key: BackupKey,
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
///
/// **只有 `NotFound` 才算名字空闲**: `EACCES`, `ENAMETOOLONG` 等其他错误使本次预测
/// 直接失败, 不再跳过该错误去选后续候选名 (设计 D5). 否则会报告一个实际无法发布的
/// 预测路径, 让 preview 与 apply 的结论不一致.
pub fn backup_path_for(target: &Path, kind: BackupKind) -> Result<PathBuf> {
    #[cfg(test)]
    if take_backup_path_failure() {
        return Err(Error::EntryFailed("模拟预测备份名失败".to_string()));
    }
    for path in backup_candidates(target, kind) {
        match fs::symlink_metadata(&path) {
            Ok(_) => continue,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(path),
            Err(error) => {
                return Err(Error::EntryFailed(format!(
                    "检查备份名 {} 失败: {error}",
                    path.display()
                )));
            }
        }
    }
    unreachable!("序号消歧必然终止")
}

fn backup_candidates(target: &Path, kind: BackupKind) -> impl Iterator<Item = PathBuf> {
    backup_candidates_at(target, timestamp(), kind)
}

/// 给定时间戳的候选名序列 (裸名, `.2`, `.3` ...).
///
/// 时间戳是参数而不是在闭包里取当前时刻: 这样"同一秒内的两次替换产生 `.2`"可以被
/// 确定性地测试, 不受墙钟是否跨秒的影响.
pub(super) fn backup_candidates_at(
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
        // 按原始字节拆分 `<原名>` 与后缀, 不要求名字是 UTF-8 (设计 D4).
        let Some((target, suffix)) = split_backup_name(&name) else {
            continue;
        };
        // 时间戳与序号必须是 ASCII; 非 UTF-8 的后缀不可能是完整备份.
        let Some(suffix) = suffix.to_str() else {
            continue;
        };
        let Some((stamp, serial)) = complete_suffix_of(suffix) else {
            continue;
        };
        let key = (stamp.to_string(), serial.to_string(), name);
        if latest
            .get(&target)
            .is_none_or(|best| cmp_backup_key(&key, &best.key) == Ordering::Greater)
        {
            latest.insert(
                target,
                LatestBackup {
                    key,
                    path: entry.path(),
                },
            );
        }
    }

    Ok(latest)
}

/// 按原始字节把目录项名拆成 `<原名>` 与最后一个 [`USER_MARKER`] 之后的字节.
///
/// Unix 上用 `OsStrExt::as_bytes` 直接比较标记字节, 因此非 UTF-8 的原文件名也能
/// 命中; 非 Unix 平台退回 `to_str` (平台门禁会在任何文件系统操作之前结束进程).
#[cfg(unix)]
fn split_backup_name(name: &OsStr) -> Option<(OsString, &OsStr)> {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};

    let bytes = name.as_bytes();
    let marker = USER_MARKER.as_bytes();
    let index = bytes
        .windows(marker.len())
        .rposition(|window| window == marker)?;
    let target = OsString::from_vec(bytes[..index].to_vec());
    let suffix = OsStr::from_bytes(&bytes[index + marker.len()..]);
    Some((target, suffix))
}

#[cfg(not(unix))]
fn split_backup_name(name: &OsStr) -> Option<(OsString, &OsStr)> {
    let text = name.to_str()?;
    let (target, suffix) = text.rsplit_once(USER_MARKER)?;
    Some((OsString::from(target), OsStr::new(suffix)))
}

#[cfg(test)]
pub(crate) fn find_latest_backup(target: &Path) -> Result<Option<PathBuf>> {
    BackupFinder::default().find_latest(target)
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
pub(super) fn parent_dir(path: &Path) -> &Path {
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
pub(super) fn timestamp() -> String {
    format_timestamp(OffsetDateTime::now_utc())
}

/// 把任意时刻归一到 UTC 后格式化为 15 字符 `YYYYMMDDThhmmss`.
///
/// 拆出无副作用的格式化函数是为了让带偏移的固定时刻可被单元测试断言: `+08:00` 的输入必须输出
/// UTC 化的字面值, 而不是原时刻的本地字段.
pub(super) fn format_timestamp(at: OffsetDateTime) -> String {
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

/// 完整命名的后缀拆成 `(时间戳, 序号数字串)`; 裸名 (无序号) 的序号记为空串, 数值上是 0.
pub(super) fn complete_suffix_of(suffix: &str) -> Option<(&str, &str)> {
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
fn cmp_backup_key(a: &BackupKey, b: &BackupKey) -> Ordering {
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
///
/// "完整命名" 有两层含义, 二者必须同时成立:
///
/// - **形态**: 定长 15 字符, 前 8 位日期数字, 第 9 位分隔符 `T`, 后 6 位时间数字.
/// - **字段**: 月在 01 至 12 之间, 日落在该月的真实天数内 (含闰年 2 月 29 日), 时在
///   00 至 23 之间, 分与秒在 00 至 59 之间. 年份已由 4 位数字形态固定.
///
/// 只有合法的 UTC 日历时刻才使 "定长字符串序即时间序" 成立; 形态合法但字段非法的
/// 名字 (例如 `20261301T000000`) 取值无意义, 不参与查找
/// (openspec/specs/backup-lifecycle/spec.md 的"时间戳字段非法的备份不参与查找").
/// 日历判定复用 `time` 的构造校验, 不重复实现每月天数与闰年规则.
pub(super) fn is_timestamp(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() != 15
        || !bytes[..8].iter().all(u8::is_ascii_digit)
        || bytes[8] != b'T'
        || !bytes[9..].iter().all(u8::is_ascii_digit)
    {
        return false;
    }

    // 字节切片在形态校验之后是安全的: 15 个字节全是 ASCII 数字或 `T`.
    let year = text[0..4].parse::<i32>().ok();
    let month = text[4..6]
        .parse::<u8>()
        .ok()
        .and_then(|month| Month::try_from(month).ok());
    let day = text[6..8].parse::<u8>().ok();
    let hour = text[9..11].parse::<u8>().ok();
    let minute = text[11..13].parse::<u8>().ok();
    let second = text[13..15].parse::<u8>().ok();
    let (Some(year), Some(month), Some(day), Some(hour), Some(minute), Some(second)) =
        (year, month, day, hour, minute, second)
    else {
        return false;
    };

    Date::from_calendar_date(year, month, day).is_ok()
        && Time::from_hms(hour, minute, second).is_ok()
}

/// 在同目录内给 `path` 追加一个后缀.
pub(super) fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let directory = parent_dir(path);
    let mut name = path
        .file_name()
        .unwrap_or_else(|| OsStr::new(""))
        .to_os_string();
    name.push(suffix);
    directory.join(name)
}

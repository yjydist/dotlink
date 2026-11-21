//! 备份的复制, 发布, 恢复与同步.
//!
//! 本模块只处理"内容"这一层: 把对象按排他暂存名复制到同目录, 完整复制后原子发布,
//! 以及撤销时的恢复. 命名与查找在 `name.rs`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::os;

use super::name::{backup_candidates_at, parent_dir, timestamp, with_suffix};
use super::{BackupKind, PublishedBackup, failed};

#[cfg(test)]
use super::test_support::{take_metadata_failure, take_nested_copy_failure};

/// 未完成备份的后缀, 加在**时间戳与进程号之后**: `x.dotlink-backup.20260101T000000.<pid>.partial`.
pub(super) const PARTIAL_SUFFIX: &str = ".partial";

/// 恢复过程中使用的临时名后缀: 文件, 目录与符号链接的恢复都先写到临时名再改名,
/// 避免半成品.
pub(super) const RESTORE_SUFFIX: &str = ".dotlink-restore.partial";

/// 把 `object` 按 `kind` 的命名空间备份到它的同目录, 返回最终路径与复制起点的元数据.
///
/// `object` 可以是普通文件, 目录 (递归) 或符号链接 (**复制链接本身, 不跟随**).
/// 返回 `Ok` 意味着备份已完整落定并改名到最终名 -- 调用者此时才可以删除原对象
/// (openspec/specs/backup-fidelity/spec.md 的"备份的发布仍以完整复制为前提"). 失败时不留下最终名.
pub(crate) fn write_backup(object: &Path, kind: BackupKind) -> Result<PublishedBackup> {
    write_backup_at(object, &timestamp(), kind)
}

/// 给定时间戳的备份写入 (见 [`backup_candidates_at`]): 测试用它绕过墙钟.
pub(super) fn write_backup_at(
    object: &Path,
    stamp: &str,
    kind: BackupKind,
) -> Result<PublishedBackup> {
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
///
/// `backup` 必须与 `target` **同目录**: 备份的命名与查找 ([`super::backup_path_for`],
/// [`super::BackupFinder`]) 都把备份放在目标同目录, 恢复的暂存名也建在那里. 调用方
/// 已由命名规则保证该前提, debug 构建用 `debug_assert` 锁定它, 避免绕过命名层传入
/// 跨目录的备份而在 rename 后才以难以诊断的方式失败.
pub(crate) fn restore_backup(
    target: &Path,
    backup: &Path,
) -> std::result::Result<(), RestoreError> {
    debug_assert_eq!(
        parent_dir(target),
        parent_dir(backup),
        "备份必须与目标同目录: target={}, backup={}",
        target.display(),
        backup.display()
    );
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
pub(super) enum CopyFailure {
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

pub(super) fn copy_to_staging(
    source: &Path,
    base: &Path,
    suffix: &str,
) -> Result<(PathBuf, fs::Metadata)> {
    let mut last = PathBuf::new();
    for serial in 1..=os::MAX_STAGING_ATTEMPTS {
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
        "暂存名 {} 连续 {} 次尝试均被占用, 已尝试 {} 次",
        last.display(),
        os::MAX_STAGING_ATTEMPTS,
        os::MAX_STAGING_ATTEMPTS
    )))
}

/// 第 `serial` 个暂存候选名: 首次带进程号, 之后在 `.partial` 前追加序号.
pub(super) fn staging_candidate(base: &Path, serial: u64, suffix: &str) -> PathBuf {
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
pub(super) fn copy_object(
    source: &Path,
    dest: &Path,
) -> std::result::Result<fs::Metadata, CopyFailure> {
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
            // `mkdir` 仍会应用进程 umask: 受限 umask 可能剥离 owner 写权限, 使后续
            // 子项创建失败. 暂存根创建成功后立即收紧到 0700, 使复制与元数据应用不受
            // umask 影响; 它只作用于本次创建的私有暂存目录.
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;

                fs::set_permissions(dest, fs::Permissions::from_mode(0o700)).map_err(|error| {
                    CopyFailure::Fatal(copy_error("设置备份目录权限", dest, error))
                })?;
            }
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

/// 尽最大努力清除 `path` (普通文件 / 符号链接 / 目录均可), 失败不外抛.
///
/// 失败清理的对象是 `.partial` 之类的残渣, 其类型取决于复制进行到哪一步 (文件复制
/// 失败留下普通文件, 目录复制失败留下半成品目录), 而 `fs::remove_dir_all` 对普通文件
/// 与符号链接**失败** (std 明确只处理目录), 因此按 `symlink_metadata` 分派; 符号链接
/// 按链接本身移除, 不跟随. 目录在删除前先尽力恢复 owner rwx, 避免暂存树已应用只读源
/// 权限时无法清空. 清理是尽力而为: 它的失败至多留下残渣, 不得盖过调用点正在处理的
/// 原始错误. 本函数只用于**本次创建**的暂存对象, 不触碰既有备份或用户文件.
pub(super) fn remove_any(path: &Path) {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() => {
            make_tree_writable(path);
            let _ = fs::remove_dir_all(path);
        }
        Ok(_) => {
            let _ = fs::remove_file(path);
        }
        Err(_) => {}
    }
}

/// 尽力恢复目录树中每个目录的 owner rwx, 使失败清理不被只读权限位阻挡.
///
/// 先对当前目录 `chmod 0700` 再尝试读取; 读不到时直接放弃该子树 (尽力而为). 只递归
/// 真实目录: `symlink_metadata` 不跟随符号链接, 因此链接指向的树不会被动到. 文件与
/// 符号链接本身只需父目录可写即可删除, 不再单独改权限.
fn make_tree_writable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o700));
    }
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let child = entry.path();
        let Ok(metadata) = fs::symlink_metadata(&child) else {
            continue;
        };
        if metadata.file_type().is_dir() {
            make_tree_writable(&child);
        }
    }
}

/// PID 减少并发命名碰撞; 排他性由创建操作保证, 不能依赖 PID 未被复用.
pub(super) fn unique_staging_suffix(suffix: &str) -> String {
    format!(".{}{suffix}", std::process::id())
}

/// rename 之后同步父目录, 让新名字在崩溃后仍然可见.
///
/// `Unsupported` 与其他元数据的降级语义一致; 其余错误使该条目失败 (设计 D4).
fn sync_parent(path: &Path) -> Result<()> {
    let parent = parent_dir(path);
    skip_unsupported(os::sync_dir(parent)).map_err(|error| failed("同步父目录", parent, &error))
}

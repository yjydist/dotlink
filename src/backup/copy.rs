//! 备份的复制, 发布, 恢复与同步.
//!
//! 本模块只处理"内容"这一层: 把对象按排他暂存名复制到同目录, 完整复制后原子发布,
//! 以及撤销时的恢复. 命名与查找在 `name.rs`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::os::fd::AsFd;

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
    // 顶层源的父目录挂载身份只算一次, 供整棵树复用; 顶层源本身是挂载点时, 第一次
    // 比较就会在创建任何暂存对象或读取内容之前失败.
    let parent_mount = parent_mount_identity(source);
    let mut last = PathBuf::new();
    for serial in 1..=os::MAX_STAGING_ATTEMPTS {
        let partial = staging_candidate(base, serial, suffix);
        match copy_object(source, &partial, parent_mount.as_ref()) {
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

/// 顶层源的父目录挂载身份; 无父目录 (`/`) 或父目录不可读时返回 `None`.
///
/// 顶层源没有父目录时跳过比较: 目标 `/` 已由清单校验拒绝, 该分支不可达. 父目录
/// 读不到时同样跳过; 复制本身随后会以真实错误失败.
fn parent_mount_identity(source: &Path) -> Option<os::MountIdentity> {
    let parent = source
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())?;
    let metadata = fs::symlink_metadata(parent).ok()?;
    Some(os::mount_identity(parent, &metadata))
}

/// 挂载边界导致的复制失败说明: 给出挂载点完整路径, 并提示先卸载或移走.
fn mount_boundary_error(path: &Path) -> io::Error {
    io::Error::other(format!(
        "拒绝复制 {}: 该路径与父目录不属于同一挂载, 是挂载点; 请先卸载或移走该挂载点再重试",
        path.display()
    ))
}

/// 把路径拆成父目录与末段; 末段是 `O_NOFOLLOW` 原语的相对名字.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn parent_and_name<'a>(
    path: &'a Path,
    context: &str,
) -> std::result::Result<(&'a Path, &'a std::ffi::OsStr), CopyFailure> {
    let name = path.file_name().ok_or_else(|| {
        CopyFailure::Fatal(copy_error(
            context,
            path,
            io::Error::other("路径没有末段, 无法以文件描述符相对方式访问"),
        ))
    })?;
    Ok((parent_dir(path), name))
}

/// 把末段转成 `CStr`, 供 fd 相对原语使用.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn c_name(
    name: &std::ffi::OsStr,
    path: &Path,
    context: &str,
) -> std::result::Result<std::ffi::CString, CopyFailure> {
    use std::os::unix::ffi::OsStrExt;

    std::ffi::CString::new(name.as_bytes())
        .map_err(|error| CopyFailure::Fatal(copy_error(context, path, io::Error::other(error))))
}

/// 复制 `source` 顶层的对象到 `dest`; 目录内容递归复制.
///
/// 只有取得目的地所有权之后的失败才允许清理它. 目录与文件的递归都从**源目录文件
/// 描述符**出发, 子项以 `statat(..., SYMLINK_NOFOLLOW)` 判定, 目录以
/// `openat(..., O_DIRECTORY | O_NOFOLLOW)` 打开, 文件副本以
/// `openat(..., O_EXCL | O_NOFOLLOW)` 创建, 符号链接以 `readlinkat` + `symlinkat`
/// 复制链接文本本身. 失败清理仍按路径处理本次创建的暂存对象.
///
/// `parent` 是 `source` 所在目录的挂载身份. 符号链接只复制链接文本, 不参与边界
/// 判定 (链接不是挂载点, 也不会被跟随). 普通文件与目录在创建暂存对象或读取内容
/// 之前比较自身与父目录的挂载身份, 目录通过比较后以自身身份递归子项.
///
/// 返回值区分根名冲突与其余失败: 仅当**本函数自己创建目标根名**时收到
/// `AlreadyExists` 才是 [`CopyFailure::StagingNameTaken`]; 子项递归通过
/// [`copy_child`] 把同类失败包装成带子项路径的致命错误, 避免整树复制被误重试.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(super) fn copy_object(
    source: &Path,
    dest: &Path,
    parent: Option<&os::MountIdentity>,
) -> std::result::Result<fs::Metadata, CopyFailure> {
    let metadata = fs::symlink_metadata(source)
        .map_err(|error| CopyFailure::Fatal(copy_error("读取", source, error)))?;

    let (source_parent, source_name) = parent_and_name(source, "读取")?;
    let (dest_parent, dest_name) = parent_and_name(dest, "创建备份对象")?;
    let source_parent_fd = os::open_directory_ancestor(source_parent)
        .map_err(|error| CopyFailure::Fatal(copy_error("读取目录", source_parent, error)))?;
    let dest_parent_fd = os::open_directory_ancestor(dest_parent)
        .map_err(|error| CopyFailure::Fatal(copy_error("创建备份对象", dest_parent, error)))?;

    let source_name = c_name(source_name, source, "读取")?;
    let dest_name = c_name(dest_name, dest, "创建备份对象")?;
    copy_node(
        &source_parent_fd,
        &source_name,
        source,
        &dest_parent_fd,
        &dest_name,
        dest,
        parent,
    )?;
    Ok(metadata)
}

/// 非 macOS/Linux 平台的可编译占位: 平台门禁会在运行到复制之前退 3.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(super) fn copy_object(
    _source: &Path,
    _dest: &Path,
    _parent: Option<&os::MountIdentity>,
) -> std::result::Result<fs::Metadata, CopyFailure> {
    Err(CopyFailure::Fatal(io::Error::new(
        io::ErrorKind::Unsupported,
        "当前平台不支持以文件描述符相对方式复制",
    )))
}

/// 复制一个对象 (顶层或子项) 到父目录文件描述符下的 `dest_name`.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn copy_node(
    source_parent_fd: &rustix::fd::OwnedFd,
    source_name: &std::ffi::CStr,
    source_path: &Path,
    dest_parent_fd: &rustix::fd::OwnedFd,
    dest_name: &std::ffi::CStr,
    dest_path: &Path,
    parent: Option<&os::MountIdentity>,
) -> std::result::Result<(), CopyFailure> {
    use rustix::fs::{FileType, readlinkat, symlinkat};

    let (file_type, identity, _) =
        os::classify_child(source_parent_fd, source_name, source_path)
            .map_err(|error| CopyFailure::Fatal(copy_error("读取", source_path, error)))?;

    if file_type == FileType::Symlink {
        let raw = readlinkat(source_parent_fd, source_name, Vec::new()).map_err(|error| {
            CopyFailure::Fatal(copy_error("读取链接", source_path, error.into()))
        })?;
        return symlinkat(&raw, dest_parent_fd, dest_name)
            .map_err(|error| CopyFailure::from_root_create("备份链接", dest_path, error.into()));
    }

    // 符号链接已在上方返回; 其余对象比较自身与父目录的挂载身份.
    if let Some(parent) = parent
        && !identity.same_mount(parent)
    {
        return Err(CopyFailure::Fatal(mount_boundary_error(source_path)));
    }

    #[cfg(test)]
    run_copy_descend_hook(source_path);

    if file_type == FileType::Directory {
        return copy_directory_node(
            source_parent_fd,
            source_name,
            source_path,
            dest_parent_fd,
            dest_name,
            dest_path,
            &identity,
        );
    }

    if file_type == FileType::RegularFile {
        return copy_file_node(
            source_parent_fd,
            source_name,
            source_path,
            dest_parent_fd,
            dest_name,
            dest_path,
        );
    }

    Err(CopyFailure::Fatal(io::Error::new(
        io::ErrorKind::Unsupported,
        format!(
            "无法复制特殊文件 {}; 请先手工移走该特殊文件再重试",
            source_path.display()
        ),
    )))
}

/// 递归复制一个目录对象.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn copy_directory_node(
    source_parent_fd: &rustix::fd::OwnedFd,
    source_name: &std::ffi::CStr,
    source_path: &Path,
    dest_parent_fd: &rustix::fd::OwnedFd,
    dest_name: &std::ffi::CStr,
    dest_path: &Path,
    identity: &os::MountIdentity,
) -> std::result::Result<(), CopyFailure> {
    use rustix::fs::{AtFlags, Mode, chmodat, mkdirat};

    mkdirat(dest_parent_fd, dest_name, Mode::from_bits_truncate(0o700))
        .map_err(|error| CopyFailure::from_root_create("创建备份目录", dest_path, error.into()))?;

    let copied = (|| {
        // `mkdir` 仍会应用进程 umask: 受限 umask 可能剥离 owner 访问权限, 使随后
        // 打开目录与创建子项失败. 暂存根创建成功后立即收紧到 0700; 该 chmod 只作用于
        // 本次创建的私有暂存目录.
        chmodat(
            dest_parent_fd,
            dest_name,
            Mode::from_bits_truncate(0o700),
            AtFlags::empty(),
        )
        .map_err(|error| {
            CopyFailure::Fatal(copy_error("设置备份目录权限", dest_path, error.into()))
        })?;

        let source_fd = os::open_child_directory(source_parent_fd, source_name, source_path)
            .map_err(|error| CopyFailure::Fatal(copy_error("读取目录", source_path, error)))?;
        let dest_fd = os::open_child_directory(dest_parent_fd, dest_name, dest_path)
            .map_err(|error| CopyFailure::Fatal(copy_error("创建备份目录", dest_path, error)))?;

        copy_directory_contents(&source_fd, &dest_fd, source_path, dest_path, identity)?;

        // 子项复制完成后再应用元数据: 创建子项会改变目录 mtime, 只读权限也会阻止
        // 后续子项写入.
        apply_metadata(source_fd.as_fd(), dest_fd.as_fd(), dest_path)
            .map_err(CopyFailure::fatal)?;
        // 全部子项与目录自身都同步完成后, 复制产物才算落盘 (设计 D4).
        skip_unsupported(os::sync_dir(dest_path))
            .map_err(|error| CopyFailure::Fatal(copy_error("同步备份目录", dest_path, error)))
    })();

    finish_copy(dest_path, copied)?;
    Ok(())
}

/// 从已打开的源目录文件描述符复制全部子项.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn copy_directory_contents(
    source_fd: &rustix::fd::OwnedFd,
    dest_fd: &rustix::fd::OwnedFd,
    source_path: &Path,
    dest_path: &Path,
    identity: &os::MountIdentity,
) -> std::result::Result<(), CopyFailure> {
    use std::os::unix::ffi::OsStrExt;

    for entry in rustix::fs::Dir::read_from(source_fd)
        .map_err(|error| CopyFailure::Fatal(copy_error("读取目录", source_path, error.into())))?
    {
        let entry = entry.map_err(|error| {
            CopyFailure::Fatal(copy_error("读取目录项", source_path, error.into()))
        })?;
        let name = entry.file_name();
        // 原始 `readdir` 会返回 `.` 与 `..`; 标准库 `read_dir` 已过滤, 这里必须自行跳过.
        if matches!(name.to_bytes(), b"." | b"..") {
            continue;
        }
        let child_source = source_path.join(std::ffi::OsStr::from_bytes(name.to_bytes()));
        let child_dest = dest_path.join(std::ffi::OsStr::from_bytes(name.to_bytes()));
        copy_child(
            source_fd,
            name,
            &child_source,
            dest_fd,
            name,
            &child_dest,
            identity,
        )?;
    }
    Ok(())
}

/// 递归复制一个子项.
///
/// 子项的"目标名已被占用"不再具有重试意义: 暂存根已经取得, 换名只会从头复制整棵
/// 树并再次撞上同一个冲突. 因此把 [`CopyFailure::StagingNameTaken`] 包装成带子项
/// 路径的致命错误 (openspec/specs/backup-lifecycle/spec.md 的"嵌套复制冲突不换名重试").
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn copy_child(
    source_parent_fd: &rustix::fd::OwnedFd,
    source_name: &std::ffi::CStr,
    source_path: &Path,
    dest_parent_fd: &rustix::fd::OwnedFd,
    dest_name: &std::ffi::CStr,
    dest_path: &Path,
    parent: &os::MountIdentity,
) -> std::result::Result<(), CopyFailure> {
    #[cfg(test)]
    let copied = if take_nested_copy_failure() {
        Err(CopyFailure::StagingNameTaken)
    } else {
        copy_node(
            source_parent_fd,
            source_name,
            source_path,
            dest_parent_fd,
            dest_name,
            dest_path,
            Some(parent),
        )
    };
    #[cfg(not(test))]
    let copied = copy_node(
        source_parent_fd,
        source_name,
        source_path,
        dest_parent_fd,
        dest_name,
        dest_path,
        Some(parent),
    );

    copied.map_err(|failure| match failure {
        CopyFailure::StagingNameTaken => nested_conflict(dest_path),
        fatal => fatal,
    })
}

/// 复制一个普通文件: 源以 `O_NOFOLLOW` 打开, 副本以 `O_EXCL | O_NOFOLLOW` 创建.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn copy_file_node(
    source_parent_fd: &rustix::fd::OwnedFd,
    source_name: &std::ffi::CStr,
    source_path: &Path,
    dest_parent_fd: &rustix::fd::OwnedFd,
    dest_name: &std::ffi::CStr,
    dest_path: &Path,
) -> std::result::Result<(), CopyFailure> {
    use rustix::fs::{Mode, OFlags, openat};

    let source_fd = os::open_child_file(source_parent_fd, source_name, source_path)
        .map_err(|error| CopyFailure::Fatal(copy_error("读取", source_path, error)))?;
    let dest_fd = openat(
        dest_parent_fd,
        dest_name,
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::from_bits_truncate(0o600),
    )
    .map_err(|error| CopyFailure::from_root_create("创建备份文件", dest_path, error.into()))?;

    let mut input = fs::File::from(source_fd);
    let mut output = fs::File::from(dest_fd);
    let copied = io::copy(&mut input, &mut output)
        .map_err(|error| CopyFailure::Fatal(copy_error("复制", source_path, error)))
        .and_then(|_| {
            apply_metadata(input.as_fd(), output.as_fd(), dest_path).map_err(CopyFailure::fatal)
        })
        .and_then(|()| {
            // 数据与元数据在 rename 到最终名前同步到稳定存储 (设计 D4).
            skip_unsupported(os::sync_file(&output))
                .map_err(|error| CopyFailure::Fatal(copy_error("同步备份文件", dest_path, error)))
        });
    finish_copy(dest_path, copied)?;
    Ok(())
}

fn nested_conflict(dest: &Path) -> CopyFailure {
    CopyFailure::Fatal(io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!("创建备份对象 {} 失败: 目标名已被占用", dest.display()),
    ))
}

/// 给普通文件或目录应用保真元数据, 顺序固定为**扩展属性 -> 权限位 -> mtime**.
///
/// 全部作用于已打开的目的地文件描述符: 扩展属性用 `fsetxattr`, 权限用 `fchmod`,
/// mtime 用 `futimens`. 扩展属性必须最先设置: 目标此刻仍是创建时的 `0600` /
/// `0700`, 最终权限若只读, `setxattr` 会返回 `EACCES` (macOS 实测). 权限位只改变
/// ctime, mtime 最后设置即可覆盖前面所有写入 (目录还包括子项创建). 平台不支持某类
/// 元数据时返回 `Unsupported`, 视为该平台的降级, 不使备份失败.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn apply_metadata(
    source_fd: std::os::fd::BorrowedFd<'_>,
    dest_fd: std::os::fd::BorrowedFd<'_>,
    dest: &Path,
) -> io::Result<()> {
    use rustix::fs::{Mode, fchmod, fstat};

    #[cfg(test)]
    if take_metadata_failure() {
        return Err(io::Error::other("模拟应用备份元数据失败"));
    }
    skip_unsupported(os::copy_xattrs(source_fd, dest_fd))
        .map_err(|error| copy_error("设置备份扩展属性", dest, error))?;

    let stat = fstat(source_fd)?;
    fchmod(
        dest_fd,
        Mode::from_bits_truncate((stat.st_mode & 0o7777) as _),
    )
    .map_err(|error| copy_error("设置备份权限", dest, error.into()))?;

    skip_unsupported(os::set_mtime(source_fd, dest_fd))
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
            // 清理对象通常不含挂载点, 但并发挂载仍可能让它出现边界; 复用与暂存删除
            // 相同的原语, 在进入边界前停止并保留残渣, 不跨越边界删除挂载内容.
            let _ = os::remove_tree(path);
        }
        Ok(_) => {
            let _ = fs::remove_file(path);
        }
        Err(_) => {}
    }
}

/// 尽力恢复目录树中每个目录的 owner rwx, 使失败清理不被只读权限位阻挡.
///
/// 先对当前目录 `fchmod 0700` 再尝试读取; 读不到时直接放弃该子树 (尽力而为). 只
/// 递归真实目录, 子项以 `statat(..., SYMLINK_NOFOLLOW)` 判定, 目录以
/// `openat(..., O_DIRECTORY | O_NOFOLLOW)` 打开: 符号链接指向的树不会被动到, 遍历
/// 期间被替换的目录项也不会被跟随. 进入子目录前比较挂载身份: 挂载点之后的内容
/// 不会被 chmod, 与删除路径保持同一边界.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn make_tree_writable(path: &Path) {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return;
    };
    if !metadata.file_type().is_dir() {
        return;
    }
    let identity = os::mount_identity(path, &metadata);
    let Ok(fd) = os::open_directory(path) else {
        return;
    };
    make_tree_writable_inner(&fd, path, &identity);
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn make_tree_writable(_path: &Path) {}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn make_tree_writable_inner(fd: &rustix::fd::OwnedFd, path: &Path, own: &os::MountIdentity) {
    use rustix::fs::{FileType, Mode, fchmod};
    use std::os::unix::ffi::OsStrExt;

    let _ = fchmod(fd, Mode::from_bits_truncate(0o700));
    let Ok(entries) = rustix::fs::Dir::read_from(fd) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if matches!(name.to_bytes(), b"." | b"..") {
            continue;
        }
        let child = path.join(std::ffi::OsStr::from_bytes(name.to_bytes()));
        let Ok((file_type, identity, _)) = os::classify_child(fd, name, &child) else {
            continue;
        };
        if file_type != FileType::Directory {
            continue;
        }
        if !identity.same_mount(own) {
            continue;
        }
        let Ok(child_fd) = os::open_child_directory(fd, name, &child) else {
            continue;
        };
        make_tree_writable_inner(&child_fd, &child, &identity);
    }
}

// 测试接缝: 子项被判定为目录或文件之后, 打开它之前运行一次注入动作.
//
// 真实的"判定之后目录项被替换为符号链接"竞态无法在可移植测试里稳定触发; 线程局部
// 钩子让 `copy_node` 的"拒绝跟随"分支可被确定性断言. 钩子可对同一路径重复触发,
// 由用例自行判断; 生产构建不包含该钩子.
#[cfg(test)]
type CopyDescendHook = Box<dyn Fn(&Path)>;

#[cfg(test)]
thread_local! {
    static COPY_DESCEND: std::cell::RefCell<Option<CopyDescendHook>> =
        const { std::cell::RefCell::new(None) };
}

/// 在复制递归打开子项之前注册一个注入钩子, 直到守卫被丢弃.
#[cfg(test)]
pub(super) fn before_copy_descend(hook: impl Fn(&Path) + 'static) -> CopyDescendGuard {
    let previous = COPY_DESCEND.with(|slot| slot.borrow_mut().replace(Box::new(hook)));
    CopyDescendGuard { previous }
}

#[cfg(test)]
pub(super) struct CopyDescendGuard {
    previous: Option<CopyDescendHook>,
}

#[cfg(test)]
impl Drop for CopyDescendGuard {
    fn drop(&mut self) {
        COPY_DESCEND.with(|slot| {
            *slot.borrow_mut() = self.previous.take();
        });
    }
}

#[cfg(test)]
fn run_copy_descend_hook(path: &Path) {
    COPY_DESCEND.with(|slot| {
        if let Some(hook) = slot.borrow().as_ref() {
            hook(path);
        }
    });
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

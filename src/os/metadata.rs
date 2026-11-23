//! 备份保真所需的平台元数据原语: 扩展属性复制与修改时间设置.
//!
//! 只有 macOS/Linux 提供实现; 其他平台不提供, `backup.rs` 的 fd 相对复制只在目标
//! 平台编译, 平台门禁会在运行到复制之前退 3.
//!
//! 错误分类是**逐属性**的, 而不是整体成败: 文件系统不支持扩展属性, 内核保护属性
//! 或权限不足都只跳过该属性; 磁盘满, I/O 失败等仍上抛, 使该条目失败.
//!
//! 全部原语作用于**已打开的文件描述符**: 复制扩展属性用 `flistxattr` /
//! `fgetxattr` / `fsetxattr`, 设置修改时间用 `futimens`, 因此复制递归不依赖路径,
//! 也不会在复制期间跟随被替换的目录项.

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod imp {
    use std::ffi::{OsStr, OsString};
    use std::io;
    use std::os::fd::{AsFd, BorrowedFd};
    use std::os::unix::ffi::OsStrExt;

    use rustix::fs::Timespec;
    use rustix::fs::{
        Timestamps, UTIME_OMIT, XattrFlags, fgetxattr, flistxattr, fsetxattr, fstat, futimens,
    };
    use rustix::io::Errno;

    /// 复制 `source` 上每一个可读取的扩展属性到 `dest`.
    ///
    /// 名称与值原样写入. 列举本身不可用时 (文件系统不支持, 内核未实现) 视为该对象
    /// 没有可复制的属性, 返回 `Ok`. 单个属性读取或写入失败时按 [`is_skippable`]
    /// 分类: 可跳过者忽略, 其余上抛.
    pub(crate) fn copy_xattrs(source: BorrowedFd<'_>, dest: BorrowedFd<'_>) -> io::Result<()> {
        for name in xattr_names(source.as_fd())? {
            let Some(value) = read_xattr(source.as_fd(), &name)? else {
                continue;
            };
            match fsetxattr(dest.as_fd(), &name, &value, XattrFlags::empty()) {
                Ok(()) => {}
                Err(errno) if is_skippable(errno) => {}
                Err(errno) => return Err(xattr_error("写入", &name, errno)),
            }
        }
        Ok(())
    }

    /// 把 `dest` 的修改时间设为 `source` 的修改时间; `atime` 传 `UTIME_OMIT`,
    /// 保持原值, 不在保真范围内.
    pub(crate) fn set_mtime(source: BorrowedFd<'_>, dest: BorrowedFd<'_>) -> io::Result<()> {
        let stat = fstat(source.as_fd())?;
        let times = Timestamps {
            last_access: Timespec {
                tv_sec: 0,
                tv_nsec: UTIME_OMIT,
            },
            last_modification: Timespec {
                tv_sec: stat.st_mtime,
                tv_nsec: stat.st_mtime_nsec as _,
            },
        };
        futimens(dest.as_fd(), &times).map_err(io::Error::from)
    }

    /// 列举 `source` 上的扩展属性名.
    fn xattr_names(source: BorrowedFd<'_>) -> io::Result<Vec<OsString>> {
        let mut buffer: Vec<u8> = Vec::new();
        loop {
            match flistxattr(source.as_fd(), &mut buffer[..]) {
                // 空缓冲区只返回所需长度, 不写入任何字节; 长度超出当前缓冲区时扩容.
                Ok(length) if length > buffer.len() => grow(&mut buffer, length),
                Ok(length) => return Ok(split_names(&buffer[..length])),
                // 属性集合在两次调用之间变大.
                Err(errno) if errno == Errno::RANGE => grow(&mut buffer, 0),
                // 该对象 (或该文件系统) 不提供扩展属性: 整体跳过.
                Err(errno) if is_skippable(errno) => return Ok(Vec::new()),
                Err(errno) => return Err(errno.into()),
            }
        }
    }

    /// 读取一个属性; 属性已消失时返回 `Ok(None)`.
    fn read_xattr(source: BorrowedFd<'_>, name: &OsStr) -> io::Result<Option<Vec<u8>>> {
        let mut buffer: Vec<u8> = Vec::new();
        loop {
            match fgetxattr(source.as_fd(), name, &mut buffer[..]) {
                // 空缓冲区只返回所需长度; 长度超出当前缓冲区时扩容.
                Ok(length) if length > buffer.len() => grow(&mut buffer, length),
                Ok(length) => return Ok(Some(buffer[..length].to_vec())),
                // 值在两次调用之间变大: 扩容后重试同一属性.
                Err(errno) if errno == Errno::RANGE => grow(&mut buffer, 0),
                // 列举与读取之间属性消失, 或该属性不可读: 跳过它.
                Err(errno) if is_skippable(errno) => return Ok(None),
                Err(errno) => return Err(xattr_error("读取", name, errno)),
            }
        }
    }

    /// `ENOTSUP` / `EOPNOTSUPP` / `ENOSYS` / `EPERM` / `EACCES` / `ENODATA`
    /// 视为可跳过; 其他错误视为致命.
    ///
    /// macOS 用 `ENOATTR` 报告"属性不存在", 与 Linux 的 `ENODATA` 对应, 因此一并
    /// 归入可跳过. Windows 等无此原语的平台不会走到这里.
    fn is_skippable(errno: Errno) -> bool {
        let known = errno == Errno::NOTSUP
            || errno == Errno::OPNOTSUPP
            || errno == Errno::NOSYS
            || errno == Errno::PERM
            || errno == Errno::ACCESS
            || errno == Errno::NODATA;
        #[cfg(target_os = "macos")]
        {
            known || errno == Errno::NOATTR
        }
        #[cfg(not(target_os = "macos"))]
        {
            known
        }
    }

    /// 扩到至少 `required` 字节, 且不小于当前的 2 倍.
    fn grow(buffer: &mut Vec<u8>, required: usize) {
        let length = required.max(buffer.len().saturating_mul(2)).max(256);
        buffer.resize(length, 0);
    }

    /// 内核以 NUL 分隔的属性名列表.
    fn split_names(list: &[u8]) -> Vec<OsString> {
        list.split(|byte| *byte == 0)
            .filter(|name| !name.is_empty())
            .map(|name| OsStr::from_bytes(name).to_os_string())
            .collect()
    }

    fn xattr_error(action: &str, name: &OsStr, errno: Errno) -> io::Error {
        io::Error::new(
            io::Error::from(errno).kind(),
            format!("{action}扩展属性 {} 失败: {errno}", name.to_string_lossy()),
        )
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::fs;
        use std::os::fd::AsFd;
        use std::path::Path;

        fn set_xattr(path: &Path, name: &str, value: &[u8]) -> Result<(), Errno> {
            rustix::fs::setxattr(path, name, value, XattrFlags::empty())
        }

        fn get_xattr(path: &Path, name: &str) -> Vec<u8> {
            let mut buffer = vec![0_u8; 64 * 1024];
            let value =
                rustix::fs::getxattr(path, name, &mut buffer[..]).expect("读取测试扩展属性");
            buffer[..value].to_vec()
        }

        #[test]
        fn copy_xattrs_copies_a_large_user_attribute() {
            let dir = tempfile::tempdir().unwrap();
            let source = dir.path().join("source");
            let dest = dir.path().join("dest");
            fs::write(&source, "content").unwrap();
            fs::write(&dest, "content").unwrap();

            let value = vec![0x5a_u8; 8 * 1024];
            match set_xattr(&source, "user.test", &value) {
                Ok(()) => {}
                Err(error) if error == Errno::NOSPC => {
                    eprintln!(
                        "跳过 copy_xattrs_copies_a_large_user_attribute: \
                         文件系统无法保存 8 KiB 扩展属性: {error}"
                    );
                    return;
                }
                Err(error) => panic!("设置测试扩展属性: {error}"),
            }

            let source_file = fs::File::open(&source).unwrap();
            let dest_file = fs::File::open(&dest).unwrap();
            copy_xattrs(source_file.as_fd(), dest_file.as_fd()).unwrap();

            assert_eq!(get_xattr(&dest, "user.test"), value);
        }

        #[test]
        fn copy_xattrs_skips_when_the_filesystem_has_none() {
            let dir = tempfile::tempdir().unwrap();
            let source = dir.path().join("source");
            let dest = dir.path().join("dest");
            fs::write(&source, "content").unwrap();
            fs::write(&dest, "content").unwrap();

            let source_file = fs::File::open(&source).unwrap();
            let dest_file = fs::File::open(&dest).unwrap();
            // 没有属性可复制时是成功, 不是失败.
            copy_xattrs(source_file.as_fd(), dest_file.as_fd()).unwrap();
        }

        #[test]
        fn is_skippable_covers_the_documented_errnos_and_rejects_others() {
            for errno in [
                Errno::NOTSUP,
                Errno::OPNOTSUPP,
                Errno::NOSYS,
                Errno::PERM,
                Errno::ACCESS,
                Errno::NODATA,
            ] {
                assert!(is_skippable(errno), "{errno:?} 应可跳过");
            }
            for errno in [Errno::NOSPC, Errno::IO, Errno::INVAL] {
                assert!(!is_skippable(errno), "{errno:?} 应致命");
            }
        }

        #[test]
        fn set_mtime_round_trips_a_known_timestamp() {
            let dir = tempfile::tempdir().unwrap();
            let source = dir.path().join("source");
            let dest = dir.path().join("dest");
            fs::write(&source, "content").unwrap();
            fs::write(&dest, "content").unwrap();

            let known = Timestamps {
                last_access: Timespec {
                    tv_sec: 0,
                    tv_nsec: UTIME_OMIT,
                },
                last_modification: Timespec {
                    tv_sec: 1_600_000_000,
                    tv_nsec: 123_456_789,
                },
            };
            let source_file = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&source)
                .unwrap();
            futimens(source_file.as_fd(), &known).unwrap();

            let dest_file = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&dest)
                .unwrap();
            set_mtime(source_file.as_fd(), dest_file.as_fd()).unwrap();

            use std::os::unix::fs::MetadataExt;
            let copied = fs::metadata(&dest).unwrap();
            assert_eq!(copied.mtime(), 1_600_000_000);
            assert_eq!(copied.mtime_nsec(), 123_456_789);
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) use imp::{copy_xattrs, set_mtime};

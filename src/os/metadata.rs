//! 备份保真所需的平台元数据原语: 扩展属性复制与修改时间设置.
//!
//! 只有 macOS/Linux 提供实现; 其他平台返回 `Unsupported`, 由调用者视为"本平台
//! 不保留该元数据"并跳过. `backup.rs` 因此不必按平台分支.
//!
//! 错误分类是**逐属性**的, 而不是整体成败: 文件系统不支持扩展属性, 内核保护属性
//! 或权限不足都只跳过该属性; 磁盘满, I/O 失败等仍上抛, 使该条目失败.

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod imp {
    use std::ffi::{OsStr, OsString};
    use std::fs;
    use std::io;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::MetadataExt;
    use std::path::Path;

    use rustix::fs::Timespec;
    use rustix::fs::{
        AtFlags, CWD, Timestamps, UTIME_OMIT, XattrFlags, getxattr, listxattr, setxattr, utimensat,
    };
    use rustix::io::Errno;

    /// 复制 `source` 上每一个可读取的扩展属性到 `dest`.
    ///
    /// 名称与值原样写入. 列举本身不可用时 (文件系统不支持, 内核未实现) 视为该对象
    /// 没有可复制的属性, 返回 `Ok`. 单个属性读取或写入失败时按 [`is_skippable`]
    /// 分类: 可跳过者忽略, 其余上抛.
    pub(crate) fn copy_xattrs(source: &Path, dest: &Path) -> io::Result<()> {
        for name in xattr_names(source)? {
            let Some(value) = read_xattr(source, &name)? else {
                continue;
            };
            match setxattr(dest, &name, &value, XattrFlags::empty()) {
                Ok(()) => {}
                Err(errno) if is_skippable(errno) => {}
                Err(errno) => return Err(xattr_error("写入", &name, errno)),
            }
        }
        Ok(())
    }

    /// 把 `dest` 的修改时间设为 `source` 的修改时间; `atime` 传 `UTIME_OMIT`,
    /// 保持原值, 不在保真范围内.
    pub(crate) fn set_mtime(source: &fs::Metadata, dest: &Path) -> io::Result<()> {
        let times = Timestamps {
            last_access: Timespec {
                tv_sec: 0,
                tv_nsec: UTIME_OMIT,
            },
            last_modification: Timespec {
                tv_sec: source.mtime(),
                tv_nsec: source.mtime_nsec(),
            },
        };
        utimensat(CWD, dest, &times, AtFlags::empty()).map_err(io::Error::from)
    }

    /// 列举 `path` 上的扩展属性名.
    fn xattr_names(path: &Path) -> io::Result<Vec<OsString>> {
        let mut buffer: Vec<u8> = Vec::new();
        loop {
            match listxattr(path, &mut buffer[..]) {
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
    fn read_xattr(path: &Path, name: &OsStr) -> io::Result<Option<Vec<u8>>> {
        let mut buffer: Vec<u8> = Vec::new();
        loop {
            match getxattr(path, name, &mut buffer[..]) {
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

        fn set_xattr(path: &Path, name: &str, value: &[u8]) {
            setxattr(path, name, value, XattrFlags::empty()).expect("设置测试扩展属性");
        }

        fn get_xattr(path: &Path, name: &str) -> Vec<u8> {
            let mut buffer = vec![0_u8; 64 * 1024];
            let value = getxattr(path, name, &mut buffer[..]).expect("读取测试扩展属性");
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
            set_xattr(&source, "user.test", &value);

            copy_xattrs(&source, &dest).unwrap();

            assert_eq!(get_xattr(&dest, "user.test"), value);
        }

        #[test]
        fn copy_xattrs_skips_when_the_filesystem_has_none() {
            let dir = tempfile::tempdir().unwrap();
            let source = dir.path().join("source");
            let dest = dir.path().join("dest");
            fs::write(&source, "content").unwrap();
            fs::write(&dest, "content").unwrap();

            // 没有属性可复制时是成功, 不是失败.
            copy_xattrs(&source, &dest).unwrap();
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
            utimensat(CWD, &source, &known, AtFlags::empty()).unwrap();

            let metadata = fs::metadata(&source).unwrap();
            set_mtime(&metadata, &dest).unwrap();

            let copied = fs::metadata(&dest).unwrap();
            assert_eq!(copied.mtime(), 1_600_000_000);
            assert_eq!(copied.mtime_nsec(), 123_456_789);
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
mod imp {
    use std::fs;
    use std::io;
    use std::path::Path;

    /// 其他平台没有可用的扩展属性原语: 返回 `Unsupported`, 调用者据此跳过.
    pub(crate) fn copy_xattrs(_source: &Path, _dest: &Path) -> io::Result<()> {
        Err(unsupported("扩展属性"))
    }

    /// 其他平台不保证可设置的修改时间原语: 返回 `Unsupported`, 调用者据此跳过.
    pub(crate) fn set_mtime(_source: &fs::Metadata, _dest: &Path) -> io::Result<()> {
        Err(unsupported("修改时间"))
    }

    fn unsupported(feature: &str) -> io::Error {
        io::Error::new(
            io::ErrorKind::Unsupported,
            format!("当前平台不支持保留{feature}"),
        )
    }
}

pub(crate) use imp::{copy_xattrs, set_mtime};

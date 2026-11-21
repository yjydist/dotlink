//! 符号链接, 路径规范化与归属判定的平台层.
//!
//! 链接原语在 unix 下由 `unix.rs` 提供, 不覆盖落点的原子移动仅支持 macOS/Linux.
//! 其他平台提供可编译的拒绝实现 (运行时由平台门禁拒绝, 见
//! `openspec/specs/cli-contract/spec.md` 的"平台门禁先于一切其他步骤").
//!
//! 模块内部按职责拆分: `normalize` 负责路径规范化与归属判定, `fs_ops` 负责暂存,
//! 原子移动与稳定存储同步, `metadata` 负责备份保真元数据. 本文件保留平台 stub,
//! 文件系统身份与 crate 级 re-export.

use std::path::Path;

#[cfg(unix)]
mod unix;

#[cfg(unix)]
pub use unix::{create_symlink, read_link};

// 不提供 `is_symlink`: 早期设计曾要求它, 但审阅确认它**无任何消费者** -- 生产路径
// (含 `classify`) 直接读一次 `symlink_metadata` 的 `file_type()`, 便于同一次 lstat 同时
// 判定"存在"与"是链接"; 若再提供一个 `is_symlink`, 调用处就要 lstat 两次. `tests/` 用的
// 是 std 的 `FileType::is_symlink`. 保留它只会是一个无消费者的公开接口, 与宪章第一条
// "接口是成本"及第六条"禁止投机抽象"相悖 (清单同样只保留 `source` / `target` /
// `on_conflict` 三个键, 见 `openspec/specs/manifest-schema/spec.md` 的 V-8).
// 若将来出现确实需要的调用点, 在**那时**随该调用点一并引入.

#[cfg(not(unix))]
mod stub {
    use std::io;
    use std::path::Path;

    /// 非 unix 平台的空实现: 平台门禁会在任何文件系统操作之前结束进程.
    pub fn create_symlink(_target: &Path, _link_path: &Path) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "当前平台不支持符号链接",
        ))
    }

    /// 非 unix 平台的空实现.
    pub fn read_link(_link_path: &Path) -> io::Result<std::path::PathBuf> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "当前平台不支持符号链接",
        ))
    }
}

#[cfg(not(unix))]
pub use stub::{create_symlink, read_link};

/// 文件系统身份: 同一对 `(设备号, inode)` 即同一对象.
///
/// 只用于判等 (清单校验的别名识别), 不参与读写或排序. 取不到身份时调用方一律退回
/// 路径字符串比较, 因此身份判等只增加拒绝条件, 不改变原本的接受语义.
#[cfg_attr(not(unix), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct FileIdentity {
    device: u64,
    inode: u64,
}

/// 按 lstat 语义取"落点对象"的身份: 不跟随末段符号链接.
///
/// 部署目标的末段就是会被替换或备份的那个对象. 若跟随链接, 已正确部署的链接会与
/// 它指向的源得到同一身份, 使 V-5 与 V-10 把合法清单误判为重复或自指.
#[cfg(unix)]
pub(crate) fn target_identity(path: &Path) -> std::io::Result<FileIdentity> {
    Ok(identity_of(&std::fs::symlink_metadata(path)?))
}

/// 按跟随语义取"基准对象"的身份: 末段符号链接解析到最终对象.
///
/// V-9 的基准点 (文件系统根, 主目录, 仓库根, 清单文件) 使用这一语义: `HOME` 与
/// 清单文件本身都可能是符号链接, 同一对象的两种写法都必须命中.
#[cfg(unix)]
pub(crate) fn basis_identity(path: &Path) -> std::io::Result<FileIdentity> {
    Ok(identity_of(&std::fs::metadata(path)?))
}

#[cfg(unix)]
fn identity_of(metadata: &std::fs::Metadata) -> FileIdentity {
    use std::os::unix::fs::MetadataExt;

    FileIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
    }
}

#[cfg(not(unix))]
pub(crate) fn target_identity(_path: &Path) -> std::io::Result<FileIdentity> {
    Err(unsupported_identity())
}

#[cfg(not(unix))]
pub(crate) fn basis_identity(_path: &Path) -> std::io::Result<FileIdentity> {
    Err(unsupported_identity())
}

#[cfg(not(unix))]
fn unsupported_identity() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "当前平台不支持文件系统身份判等",
    )
}

mod fs_ops;
mod metadata;
mod normalize;

pub(crate) use fs_ops::{MAX_STAGING_ATTEMPTS, rename_noreplace, stage_aside, sync_dir, sync_file};
#[cfg(test)]
pub(crate) use fs_ops::{fail_directory_syncs, fail_syncs, staging_candidate};
pub(crate) use normalize::{comparison_form, is_inside_repo, link_destination, normalize};

pub(crate) use metadata::{copy_xattrs, set_mtime};

#[cfg(all(test, unix))]
mod tests {
    use super::normalize::physical_tempdir;
    use super::*;

    #[test]
    fn target_identity_is_lstat_while_basis_identity_follows_symlinks() {
        let (_dir, root) = physical_tempdir();
        let file = root.join("file");
        std::fs::write(&file, "payload").expect("写入文件");
        let hard = root.join("hard");
        std::fs::hard_link(&file, &hard).expect("建立硬链接");
        let link = root.join("link");
        std::os::unix::fs::symlink(&file, &link).expect("建立符号链接");

        let file_identity = target_identity(&file).expect("文件身份");
        assert_eq!(
            file_identity,
            target_identity(&hard).expect("硬链接身份"),
            "硬链接指向同一对象, 两种语义都必须判等"
        );
        assert_eq!(
            basis_identity(&link).expect("跟随后的身份"),
            file_identity,
            "跟随语义必须落到链接的最终对象"
        );
        assert_ne!(
            target_identity(&link).expect("链接自身身份"),
            file_identity,
            "lstat 语义不得跟随末段"
        );
    }

    #[test]
    fn identity_probes_fail_when_the_path_does_not_exist() {
        let (_dir, root) = physical_tempdir();
        let missing = root.join("missing");
        assert!(target_identity(&missing).is_err(), "不存在的路径没有身份");
        assert!(basis_identity(&missing).is_err(), "不存在的路径没有身份");

        // 悬空链接: lstat 仍能取到链接自身, 跟随语义则失败. 调用方据此回退字符串
        // 比较, 不把"身份不可用"变成新的拒绝条件.
        let dangling = root.join("dangling");
        std::os::unix::fs::symlink(root.join("gone"), &dangling).expect("建立悬空链接");
        assert!(
            target_identity(&dangling).is_ok(),
            "悬空链接本身是存在的对象"
        );
        assert!(
            basis_identity(&dangling).is_err(),
            "悬空链接的最终对象不存在"
        );
    }
}

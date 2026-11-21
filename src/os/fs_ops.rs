//! 暂存, 原子移动与稳定存储同步.
//!
//! 暂存名与平台原语在这里集中实现; `os/mod.rs` 只做 re-export. 符号链接解析与
//! 归属判定在 `normalize.rs`.

use std::path::{Path, PathBuf};

/// 原子移动对象, 落点已存在 (含悬空链接) 时拒绝覆盖.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) fn rename_noreplace(source: &Path, destination: &Path) -> std::io::Result<()> {
    use rustix::fs::{CWD, RenameFlags, renameat_with};

    renameat_with(CWD, source, CWD, destination, RenameFlags::NOREPLACE).map_err(Into::into)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(crate) fn rename_noreplace(_source: &Path, _destination: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "当前平台不支持不覆盖落点的原子移动",
    ))
}

/// 替换与撤销暂存名共用的候选数量上限.
///
/// 与备份/恢复暂存根共用同一数字, 避免同一类暂存名在两条路径上有两套上限. 只有
/// 暂存名的排他取得失败才消耗一次尝试; 正常路径第一次即成功.
pub(crate) const MAX_STAGING_ATTEMPTS: u64 = 16;

/// 所有暂存名共用的完成后缀: 残留因此总能以 `.partial` 结尾被识别.
const STAGING_PARTIAL_SUFFIX: &str = ".partial";

/// 同目录暂存候选名: `<source>.<pid>.<label>.partial`.
///
/// `label` 区分暂存用途 (替换 / 撤销); 消歧序号由 [`staging_candidate`] 插在
/// `label` 与 `.partial` 之间, 因此所有形态都以 `.partial` 结尾.
fn staging_path(source: &Path, label: &str) -> PathBuf {
    let mut name = source.as_os_str().to_os_string();
    name.push(format!(
        ".{}.{label}{STAGING_PARTIAL_SUFFIX}",
        std::process::id()
    ));
    PathBuf::from(name)
}

/// 第 `serial` 个暂存候选名; 首次候选不带序号.
///
/// 形如 `<原名>.<pid>.dotlink-replace.partial` 与
/// `<原名>.<pid>.2.dotlink-replace.partial`: 序号位于 `label` 与 `.partial` 之间,
/// 与备份暂存名的 `.<pid>.<序号>.partial` 形态一致.
pub(crate) fn staging_candidate(source: &Path, label: &str, serial: u64) -> PathBuf {
    if serial == 1 {
        return staging_path(source, label);
    }
    let mut name = source.as_os_str().to_os_string();
    name.push(format!(
        ".{}.{serial}.{label}{STAGING_PARTIAL_SUFFIX}",
        std::process::id()
    ));
    PathBuf::from(name)
}

/// 以原子不覆盖的 rename 把 `source` 移到同目录暂存名, 名称占用时换下一个序号重试,
/// 返回实际取得的暂存路径.
///
/// 候选数量有界 ([`MAX_STAGING_ATTEMPTS`]): 全部被占用时返回 `AlreadyExists` 错误,
/// 说明包含尝试次数与最后一个候选名, 不创建第 17 个候选. 只负责暂存, 不判断对象
/// 身份: 复核由调用方 (替换 / 撤销) 各自完成.
pub(crate) fn stage_aside(source: &Path, label: &str) -> std::io::Result<PathBuf> {
    let mut last = None;
    for serial in 1..=MAX_STAGING_ATTEMPTS {
        let staged = staging_candidate(source, label, serial);
        match rename_noreplace(source, &staged) {
            Ok(()) => return Ok(staged),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                last = Some(staged);
            }
            Err(error) => {
                return Err(std::io::Error::new(
                    error.kind(),
                    format!(
                        "暂存 {} 到 {} 失败: {error}",
                        source.display(),
                        staged.display()
                    ),
                ));
            }
        }
    }
    let last = last.expect("上限内至少尝试一个候选");
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        format!(
            "暂存 {} 连续 {MAX_STAGING_ATTEMPTS} 次尝试均被占用, 已尝试 {MAX_STAGING_ATTEMPTS} 次, 最后候选名 {}",
            source.display(),
            last.display()
        ),
    ))
}

/// 把普通文件的数据与元数据同步到稳定存储.
///
/// macOS 与 Linux 支持; 其他平台返回 `Unsupported`, 由调用方按既有元数据降级策略
/// 处理 (设计 D4).
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) fn sync_file(file: &std::fs::File) -> std::io::Result<()> {
    #[cfg(test)]
    injected_sync_failure(SyncScope::File)?;
    file.sync_all()
}

/// 把目录项同步到稳定存储: 让 rename 产生的新名字在崩溃后仍然可见.
///
/// macOS 与 Linux 支持; 其他平台返回 `Unsupported`.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) fn sync_dir(path: &Path) -> std::io::Result<()> {
    #[cfg(test)]
    injected_sync_failure(SyncScope::Directory)?;
    std::fs::File::open(path)?.sync_all()
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(crate) fn sync_file(_file: &std::fs::File) -> std::io::Result<()> {
    unsupported_sync()
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(crate) fn sync_dir(_path: &Path) -> std::io::Result<()> {
    unsupported_sync()
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn unsupported_sync() -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "当前平台不支持把备份同步到稳定存储",
    ))
}

// 测试接缝: 让当前线程的同步原语返回模拟的 `EIO`.
//
// 真实的同步失败无法在可移植的测试里稳定构造; 线程局部状态避免并行测试互相影响.
// 按 `scope` 区分文件与目录, 才能单独覆盖"rename 之后的父目录同步失败".
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum SyncFailure {
    None,
    All,
    Directory,
}

/// 同步原语的类别; 与 [`SyncFailure`] 分开, 避免把"注入模式"与"被注入的作用域"
/// 混成一枚举, 导致某个模式没有构造点.
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum SyncScope {
    File,
    Directory,
}

#[cfg(test)]
thread_local! {
    static SYNC_FAILURE: std::cell::Cell<SyncFailure> =
        const { std::cell::Cell::new(SyncFailure::None) };
}

/// 在当前线程启用"全部同步失败"注入, 直到返回的守卫被丢弃.
#[cfg(test)]
pub(crate) fn fail_syncs() -> SyncFailureGuard {
    SyncFailureGuard::set(SyncFailure::All)
}

/// 只让目录同步失败.
///
/// 普通文件备份的目录同步只发生在 rename 之后的父目录上, 因此这条注入可以单独
/// 覆盖"备份已发布但父目录同步失败"的路径.
#[cfg(test)]
pub(crate) fn fail_directory_syncs() -> SyncFailureGuard {
    SyncFailureGuard::set(SyncFailure::Directory)
}

#[cfg(test)]
pub(crate) struct SyncFailureGuard {
    previous: SyncFailure,
}

#[cfg(test)]
impl SyncFailureGuard {
    fn set(mode: SyncFailure) -> Self {
        let previous = SYNC_FAILURE.with(|slot| slot.replace(mode));
        Self { previous }
    }
}

#[cfg(test)]
impl Drop for SyncFailureGuard {
    fn drop(&mut self) {
        SYNC_FAILURE.with(|slot| slot.set(self.previous));
    }
}

/// 测试注入点: `scope` 命中当前注入时返回模拟的同步失败 (非 `Unsupported`, 不会被降级吞掉).
#[cfg(test)]
fn injected_sync_failure(scope: SyncScope) -> std::io::Result<()> {
    let active = SYNC_FAILURE.with(|slot| slot.get());
    let fails = match active {
        SyncFailure::None => false,
        SyncFailure::All => true,
        SyncFailure::Directory => scope == SyncScope::Directory,
    };
    if fails {
        return Err(std::io::Error::other("模拟同步失败"));
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn sync_file_flushes_an_open_file() {
        let dir = tempfile::tempdir().expect("创建临时目录");
        let path = dir.path().join("payload");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .expect("创建临时文件");
        std::io::Write::write_all(&mut file, b"payload").expect("写入临时文件");

        sync_file(&file).expect("普通文件必须可以同步");
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn sync_dir_flushes_a_directory() {
        let dir = tempfile::tempdir().expect("创建临时目录");

        sync_dir(dir.path()).expect("目录必须可以同步");
    }

    /// 暂存名形态: 首次候选不带序号, 消歧序号位于 `label` 与 `.partial` 之间.
    #[test]
    fn staging_names_put_the_serial_before_the_partial_suffix() {
        let source = Path::new("/tmp/bashrc");
        let pid = std::process::id();
        for label in ["dotlink-replace", "dotlink-undo"] {
            assert_eq!(
                staging_path(source, label),
                PathBuf::from(format!("/tmp/bashrc.{pid}.{label}.partial")),
                "{label} 的首次候选不得带序号"
            );
            assert_eq!(
                staging_candidate(source, label, 2),
                PathBuf::from(format!("/tmp/bashrc.{pid}.2.{label}.partial")),
                "{label} 的消歧序号必须位于 .partial 之前"
            );
        }
    }

    /// 暂存名候选被占满时失败: 说明给出尝试次数与最后候选名, 不创建第 17 个.
    #[test]
    fn stage_aside_attempts_are_bounded() {
        let dir = tempfile::tempdir().expect("创建临时目录");
        let source = dir.path().join("bashrc");
        std::fs::write(&source, "original").expect("写入源文件");
        for serial in 1..=MAX_STAGING_ATTEMPTS {
            let occupied = staging_candidate(&source, "dotlink-replace", serial);
            std::fs::write(&occupied, format!("occupied-{serial}")).expect("占用暂存候选名");
        }
        let before = std::fs::read_dir(dir.path()).expect("列举临时目录").count();

        let error = stage_aside(&source, "dotlink-replace").expect_err("全部候选被占用时必须失败");

        let text = error.to_string();
        assert!(
            text.contains(&MAX_STAGING_ATTEMPTS.to_string()),
            "超限说明必须给出尝试次数: {text}"
        );
        let last = staging_candidate(&source, "dotlink-replace", MAX_STAGING_ATTEMPTS);
        assert!(
            text.contains(&last.display().to_string()),
            "超限说明必须给出最后一个候选名: {text}"
        );
        assert_eq!(
            std::fs::read_dir(dir.path()).expect("列举临时目录").count(),
            before,
            "不得创建第 17 个候选"
        );
        assert_eq!(
            std::fs::read_to_string(&source).expect("源文件必须保留"),
            "original",
            "失败时不得移动源文件"
        );
    }

    /// 审计 apply/undo 暂存名的后缀: 200 字节 basename 加 pid, 2 位序号与标签
    /// 都必须落在共享预算内.
    #[test]
    fn generated_staging_names_fit_the_shared_basename_budget() {
        let basename = "a".repeat(crate::entry::MAX_TARGET_BASENAME_BYTES);
        let source = Path::new("/tmp").join(&basename);
        for label in ["dotlink-replace", "dotlink-undo"] {
            let first = staging_path(&source, label);
            let longest = staging_candidate(&source, label, MAX_STAGING_ATTEMPTS);
            for name in [&first, &longest] {
                let suffix =
                    name.file_name().unwrap().len() - crate::entry::MAX_TARGET_BASENAME_BYTES;
                assert!(
                    suffix <= crate::entry::MAX_GENERATED_SUFFIX_BYTES,
                    "{label} 的暂存名后缀 {suffix} 超过审计值 {}",
                    crate::entry::MAX_GENERATED_SUFFIX_BYTES
                );
                assert!(crate::entry::MAX_TARGET_BASENAME_BYTES + suffix <= 255);
                assert!(
                    name.to_string_lossy().ends_with(".partial"),
                    "所有暂存形态都必须以 .partial 结尾: {}",
                    name.display()
                );
            }
        }
    }
}

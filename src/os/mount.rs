//! 挂载身份判定与不跨挂载边界的遍历.
//!
//! 复制, 复制失败清理与暂存删除都需要判断"下一步是否会跨过挂载边界". 本模块提供
//! 两个原语: 挂载身份 ([`MountIdentity`]) 与其比较, 以及不跟随符号链接, 不跨边界的
//! 扫描 ([`find_mount_boundary`]) 与递归删除 ([`remove_tree`]).
//!
//! Linux 在可用时用 `statx` 的 `STATX_MNT_ID` 区分同一设备上的 bind mount; 取不到
//! 挂载标识时退回设备号. 后者无法识别同设备挂载, 这是 README "已知限制" 记录的
//! 漏检边界, 不影响本模块的判定接口.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// 路径所在挂载的身份: 设备号, 以及平台能提供时的挂载标识.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MountIdentity {
    device: u64,
    mount: Option<u64>,
}

impl MountIdentity {
    fn new(device: u64, mount: Option<u64>) -> Self {
        Self { device, mount }
    }

    /// 两个对象是否属于同一挂载.
    ///
    /// 两侧都取得挂载标识时比较标识; 任一侧缺失时退回设备号. 设备号相同而挂载标识
    /// 不同的 bind mount 因此被识别为边界 (Linux); 只比较设备号时会漏掉它们, 这是
    /// 平台能力不足时的已知限制.
    pub(crate) fn same_mount(&self, other: &Self) -> bool {
        #[cfg(test)]
        if take_injected_boundary() {
            return false;
        }
        match (self.mount, other.mount) {
            (Some(left), Some(right)) => left == right,
            _ => self.device == other.device,
        }
    }
}

/// 取 `path` 的挂载身份; `metadata` 必须是该路径的 `symlink_metadata`, 不跟随末段
/// 符号链接.
///
/// 本函数不失败: 设备号来自已经取得的元数据, 平台无法提供挂载标识时只保存设备号.
pub(crate) fn mount_identity(path: &Path, metadata: &fs::Metadata) -> MountIdentity {
    MountIdentity::new(device_of(metadata), mount_id(path))
}

#[cfg(unix)]
fn device_of(metadata: &fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;

    metadata.dev()
}

#[cfg(not(unix))]
fn device_of(_metadata: &fs::Metadata) -> u64 {
    0
}

/// Linux 的挂载标识: `statx` 返回并填充了 `STATX_MNT_ID` 时采用 `stx_mnt_id`.
///
/// 旧内核, 受限沙箱或文件系统可能让 `statx` 失败 (`NOSYS`, `EINVAL` 等); 任何失败
/// 都只是退回设备号, 不把"取不到挂载标识"变成新的拒绝条件.
#[cfg(target_os = "linux")]
fn mount_id(path: &Path) -> Option<u64> {
    use rustix::fs::{AtFlags, CWD, StatxFlags, statx};

    let stat = statx(CWD, path, AtFlags::SYMLINK_NOFOLLOW, StatxFlags::MNT_ID).ok()?;
    mount_id_from_mask(stat.stx_mask, stat.stx_mnt_id)
}

/// 只有内核在 `stx_mask` 中声明填充了 `STATX_MNT_ID` 时才使用 `stx_mnt_id`.
///
/// 未声明时该字段的值不可靠 (可能是 0), 采用它会把所有挂载误判为同一个.
#[cfg(target_os = "linux")]
fn mount_id_from_mask(mask: u32, mnt_id: u64) -> Option<u64> {
    (mask & rustix::fs::StatxFlags::MNT_ID.bits() != 0).then_some(mnt_id)
}

/// macOS 与其余平台没有可用的挂载标识: 只比较设备号.
#[cfg(not(target_os = "linux"))]
fn mount_id(_path: &Path) -> Option<u64> {
    None
}

/// 一个挂载边界: 该路径与它的父目录不属于同一挂载.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MountBoundary {
    pub(crate) path: PathBuf,
}

/// 不跨挂载边界, 不跟随符号链接的递归删除的失败原因.
#[derive(Debug)]
pub(crate) enum RemoveTreeError {
    /// 遇到挂载边界: 该路径及其内容尚未删除.
    Boundary(MountBoundary),
    /// 底层删除失败.
    Io(io::Error),
}

/// 在 `root` 之下查找第一个挂载边界, 不跟随符号链接.
///
/// `root` 自身不作为边界: 调用方传入的是本工具创建的, 或刚由原子 rename 移入的
/// 暂存目录, 它不可能是挂载点. 只扫描目录; 非目录返回 `None`. 遍历顺序与
/// `read_dir` 一致, 不保证"第一个"在并发变化下稳定.
pub(crate) fn find_mount_boundary(root: &Path) -> io::Result<Option<MountBoundary>> {
    let metadata = fs::symlink_metadata(root)?;
    if !metadata.file_type().is_dir() {
        return Ok(None);
    }
    let identity = mount_identity(root, &metadata);
    find_in_directory(root, &identity)
}

fn find_in_directory(dir: &Path, parent: &MountIdentity) -> io::Result<Option<MountBoundary>> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        // 符号链接只复制或删除链接本身, 从不跟随, 因此不参与挂载边界判定.
        if metadata.file_type().is_symlink() {
            continue;
        }
        let identity = mount_identity(&path, &metadata);
        if !identity.same_mount(parent) {
            return Ok(Some(MountBoundary { path }));
        }
        if metadata.file_type().is_dir()
            && let Some(boundary) = find_in_directory(&path, &identity)?
        {
            return Ok(Some(boundary));
        }
    }
    Ok(None)
}

/// 递归删除 `root`, 在进入任何子项之前复核挂载边界.
///
/// 与 `fs::remove_dir_all` 不同, 本函数不会跨越挂载边界: 发现边界立即返回
/// [`RemoveTreeError::Boundary`], 该路径及其内容保持原样, 但同级的其他子项可能已被
/// 删除. 符号链接按链接本身移除, 不跟随. 不自动修改权限位: 只读目录仍按既有语义
/// 使删除失败, 由调用方决定是否先恢复权限.
pub(crate) fn remove_tree(root: &Path) -> Result<(), RemoveTreeError> {
    let metadata = fs::symlink_metadata(root).map_err(RemoveTreeError::Io)?;
    if !metadata.file_type().is_dir() {
        return fs::remove_file(root).map_err(RemoveTreeError::Io);
    }
    let identity = mount_identity(root, &metadata);
    remove_directory_contents(root, &identity)?;
    fs::remove_dir(root).map_err(RemoveTreeError::Io)
}

fn remove_directory_contents(dir: &Path, parent: &MountIdentity) -> Result<(), RemoveTreeError> {
    for entry in fs::read_dir(dir).map_err(RemoveTreeError::Io)? {
        let entry = entry.map_err(RemoveTreeError::Io)?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(RemoveTreeError::Io)?;
        if metadata.file_type().is_symlink() {
            fs::remove_file(&path).map_err(RemoveTreeError::Io)?;
            continue;
        }
        let identity = mount_identity(&path, &metadata);
        if !identity.same_mount(parent) {
            return Err(RemoveTreeError::Boundary(MountBoundary { path }));
        }
        if metadata.file_type().is_dir() {
            remove_directory_contents(&path, &identity)?;
            fs::remove_dir(&path).map_err(RemoveTreeError::Io)?;
        } else {
            fs::remove_file(&path).map_err(RemoveTreeError::Io)?;
        }
    }
    Ok(())
}

// 测试接缝: 让挂载身份比较返回模拟的"不同挂载".
//
// 真实的挂载点只在有特权或 user namespace 的端到端测试里构造, 单元测试用线程局部
// 状态覆盖复制中止, 清理停止与暂存目录回滚分支. 生产构建不包含任何注入状态.
#[cfg(test)]
#[derive(Clone, Copy)]
enum InjectedBoundary {
    None,
    Next,
    All,
    After(u32),
}

#[cfg(test)]
thread_local! {
    static INJECTED_BOUNDARY: std::cell::Cell<InjectedBoundary> =
        const { std::cell::Cell::new(InjectedBoundary::None) };
}

/// 让下一次挂载身份比较返回"不同挂载", 直到守卫被丢弃.
#[cfg(test)]
pub(crate) fn fail_next_mount_comparison() -> MountSeamGuard {
    MountSeamGuard::set(InjectedBoundary::Next)
}

/// 让接下来 `skipped` 次比较正常, 再让其后第一次比较返回"不同挂载".
///
/// 用于"扫描通过, 删除阶段命中"的场景: 先跳过扫描产生的比较, 再在删除的第一次
/// 比较上触发边界.
#[cfg(test)]
pub(crate) fn fail_mount_comparison_after(skipped: u32) -> MountSeamGuard {
    MountSeamGuard::set(InjectedBoundary::After(skipped))
}

/// 让所有挂载身份比较返回"不同挂载", 直到守卫被丢弃.
#[cfg(test)]
pub(crate) fn fail_mount_comparisons() -> MountSeamGuard {
    MountSeamGuard::set(InjectedBoundary::All)
}

#[cfg(test)]
pub(crate) struct MountSeamGuard {
    previous: InjectedBoundary,
}

#[cfg(test)]
impl MountSeamGuard {
    fn set(mode: InjectedBoundary) -> Self {
        let previous = INJECTED_BOUNDARY.with(|slot| slot.replace(mode));
        Self { previous }
    }
}

#[cfg(test)]
impl Drop for MountSeamGuard {
    fn drop(&mut self) {
        INJECTED_BOUNDARY.with(|slot| slot.set(self.previous));
    }
}

/// 测试注入点: 消费一次比较, 返回本次是否应当伪装成挂载边界.
#[cfg(test)]
fn take_injected_boundary() -> bool {
    let mode = INJECTED_BOUNDARY.with(std::cell::Cell::get);
    match mode {
        InjectedBoundary::None => false,
        InjectedBoundary::All => true,
        InjectedBoundary::Next | InjectedBoundary::After(0) => {
            INJECTED_BOUNDARY.with(|slot| slot.set(InjectedBoundary::None));
            true
        }
        InjectedBoundary::After(remaining) => {
            INJECTED_BOUNDARY.with(|slot| slot.set(InjectedBoundary::After(remaining - 1)));
            false
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn objects_on_the_same_mount_share_an_identity() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("file");
        fs::write(&file, "payload").unwrap();

        let left = mount_identity(dir.path(), &fs::symlink_metadata(dir.path()).unwrap());
        let right = mount_identity(&file, &fs::symlink_metadata(&file).unwrap());

        assert!(left.same_mount(&right), "同一挂载上的对象不得被判为边界");
        assert_eq!(left, right, "同一挂载上的对象应得到同一身份");
    }

    /// 没有挂载标识时只比较设备号: 不同设备是边界.
    #[test]
    fn a_different_device_is_a_boundary_without_mount_ids() {
        let left = MountIdentity::new(1, None);
        let right = MountIdentity::new(2, None);

        assert!(!left.same_mount(&right), "不同设备号必须判为边界");
        assert!(left.same_mount(&MountIdentity::new(1, None)));
    }

    /// 任一侧缺失挂载标识时退回设备号; 两侧都有时比较标识.
    #[test]
    fn mount_ids_win_only_when_both_sides_have_them() {
        let device_only = MountIdentity::new(1, None);
        let with_id = MountIdentity::new(1, Some(7));

        assert!(
            device_only.same_mount(&with_id),
            "缺失挂载标识时只能退回设备号"
        );
        assert!(
            with_id.same_mount(&MountIdentity::new(1, Some(7))),
            "挂载标识相同即同一挂载"
        );
        assert!(
            !with_id.same_mount(&MountIdentity::new(1, Some(8))),
            "设备号相同但挂载标识不同必须判为边界"
        );
    }

    /// 取不到挂载标识 (例如路径不存在) 不等同于失败: 仍以设备号参与比较.
    #[test]
    fn a_missing_path_falls_back_to_the_device_number() {
        let dir = tempfile::tempdir().unwrap();
        let metadata = fs::symlink_metadata(dir.path()).unwrap();
        let present = mount_identity(dir.path(), &metadata);
        let missing = mount_identity(&dir.path().join("gone"), &metadata);

        assert_eq!(mount_id(&dir.path().join("gone")), None);
        assert!(
            missing.same_mount(&present),
            "挂载标识缺失时设备号相同仍是同一挂载"
        );
    }

    /// `stx_mask` 未声明 `STATX_MNT_ID` 时不得使用该字段.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_mask_decides_whether_the_mount_id_is_used() {
        use rustix::fs::StatxFlags;

        let bit = StatxFlags::MNT_ID.bits();
        assert_eq!(mount_id_from_mask(bit, 42), Some(42));
        assert_eq!(
            mount_id_from_mask(bit | StatxFlags::TYPE.bits(), 42),
            Some(42)
        );
        assert_eq!(mount_id_from_mask(0, 42), None);
        assert_eq!(mount_id_from_mask(StatxFlags::TYPE.bits(), 42), None);
    }

    #[test]
    fn find_mount_boundary_reports_nothing_for_an_ordinary_tree() {
        let dir = tempfile::tempdir().unwrap();
        let tree = dir.path().join("tree");
        fs::create_dir_all(tree.join("nested")).unwrap();
        fs::write(tree.join("nested/a"), "a").unwrap();
        os_symlink(Path::new("missing"), &tree.join("link"));

        assert_eq!(find_mount_boundary(&tree).unwrap(), None);
    }

    #[test]
    fn find_mount_boundary_stops_at_the_injected_boundary() {
        let dir = tempfile::tempdir().unwrap();
        let tree = dir.path().join("tree");
        fs::create_dir_all(&tree).unwrap();
        let child = tree.join("child");
        fs::write(&child, "content").unwrap();

        let guard = fail_mount_comparisons();
        let boundary = find_mount_boundary(&tree).unwrap();
        drop(guard);

        assert_eq!(
            boundary.map(|boundary| boundary.path),
            Some(child),
            "扫描必须报告第一个边界路径"
        );
    }

    /// 递归删除不得越过边界: 边界对象及其内容保持原样.
    #[test]
    fn remove_tree_keeps_the_injected_boundary() {
        let dir = tempfile::tempdir().unwrap();
        let tree = dir.path().join("tree");
        fs::create_dir_all(&tree).unwrap();
        let child = tree.join("child");
        fs::write(&child, "content").unwrap();

        let guard = fail_mount_comparisons();
        let error = remove_tree(&tree).unwrap_err();
        drop(guard);

        match error {
            RemoveTreeError::Boundary(boundary) => assert_eq!(boundary.path, child),
            RemoveTreeError::Io(error) => panic!("期望挂载边界, 实际是 I/O 失败: {error}"),
        }
        assert_eq!(fs::read_to_string(&child).unwrap(), "content");
        assert!(tree.is_dir(), "边界之外的暂存根必须保留");
    }

    /// 普通目录树按原样删除, 符号链接只删除链接本身.
    #[test]
    fn remove_tree_removes_directories_files_and_links() {
        let dir = tempfile::tempdir().unwrap();
        let kept = dir.path().join("kept");
        fs::create_dir(&kept).unwrap();
        fs::write(kept.join("keep"), "keep").unwrap();

        let tree = dir.path().join("tree");
        fs::create_dir_all(tree.join("nested")).unwrap();
        fs::write(tree.join("nested/a"), "a").unwrap();
        os_symlink(&kept, &tree.join("link"));

        remove_tree(&tree).unwrap();

        assert!(fs::symlink_metadata(&tree).is_err(), "暂存树必须被删除");
        assert!(
            fs::symlink_metadata(&kept).is_ok(),
            "不得跟随链接删除其目标"
        );
        assert_eq!(fs::read_to_string(kept.join("keep")).unwrap(), "keep");
    }

    #[test]
    fn the_next_comparison_seam_fires_exactly_once() {
        let left = MountIdentity::new(1, Some(7));
        let right = MountIdentity::new(1, Some(7));
        assert!(left.same_mount(&right));

        let guard = fail_next_mount_comparison();
        assert!(!left.same_mount(&right), "接缝必须让下一次比较失败");
        assert!(left.same_mount(&right), "接缝只影响一次比较");
        drop(guard);
    }

    #[test]
    fn the_skip_seam_allows_an_exact_number_of_comparisons() {
        let left = MountIdentity::new(1, Some(7));
        let right = MountIdentity::new(1, Some(7));

        let guard = fail_mount_comparison_after(2);
        assert!(left.same_mount(&right));
        assert!(left.same_mount(&right));
        assert!(!left.same_mount(&right), "第 3 次比较必须命中边界");
        assert!(left.same_mount(&right), "命中后接缝失效");
        drop(guard);
    }

    fn os_symlink(target: &Path, link: &Path) {
        std::os::unix::fs::symlink(target, link).expect("建立符号链接");
    }
}

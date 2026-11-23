//! 挂载身份判定与不跨挂载边界的遍历.
//!
//! 复制, 复制失败清理与暂存删除都需要判断"下一步是否会跨过挂载边界". 本模块提供
//! 两个原语: 挂载身份 ([`MountIdentity`]) 与其比较, 以及不跟随符号链接, 不跨边界的
//! 扫描 ([`find_mount_boundary`]) 与递归删除 ([`remove_tree`]).
//!
//! Linux 在可用时用 `statx` 的 `STATX_MNT_ID` 区分同一设备上的 bind mount; 取不到
//! 挂载标识时读取 `/proc/self/mountinfo`, 按最长挂载点前缀解析 mount id, mountinfo
//! 也不可用时退回设备号. macOS 使用 `statvfs` 的公开 `f_fsid`; 取不到时同样退回
//! 设备号. 退回设备号时无法识别同设备挂载, 这是 README "已知限制" 记录的漏检边界,
//! 不影响本模块的判定接口.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[cfg(target_os = "linux")]
use std::sync::OnceLock;

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

/// Linux 的挂载标识: 优先采用 `statx` 填充的 `STATX_MNT_ID`.
///
/// 旧内核, 受限沙箱或文件系统可能让 `statx` 失败 (`NOSYS`, `EINVAL` 等), 或返回的
/// `stx_mask` 未声明 `STATX_MNT_ID`; 此时读取 `/proc/self/mountinfo` 并按最长挂载点
/// 前缀解析 mount id. mountinfo 也不可用时退回设备号, 任何一步失败都不把"取不到
/// 挂载标识"变成新的拒绝条件.
#[cfg(target_os = "linux")]
fn mount_id(path: &Path) -> Option<u64> {
    use rustix::fs::{AtFlags, CWD, StatxFlags, statx};

    #[cfg(test)]
    if SKIP_STATX.with(std::cell::Cell::get) {
        return mount_id_without_statx(path);
    }

    if let Ok(stat) = statx(CWD, path, AtFlags::SYMLINK_NOFOLLOW, StatxFlags::MNT_ID)
        && let Some(mount_id) = mount_id_from_mask(stat.stx_mask, stat.stx_mnt_id)
    {
        return Some(mount_id);
    }
    mount_id_without_statx(path)
}

/// `statx` 没有提供挂载标识时的 fallback: 用缓存的 mountinfo 做最长前缀匹配.
///
/// 只对存在的路径解析挂载点: `statx` 失败可能只是因为路径不存在, 而不存在的路径
/// 不属于任何挂载. mountinfo 读不到或解析失败时返回 `None`, 调用方退回设备号.
#[cfg(target_os = "linux")]
fn mount_id_without_statx(path: &Path) -> Option<u64> {
    fs::symlink_metadata(path).ok()?;
    mountinfo_entries().and_then(|entries| mount_id_from_mountinfo(path, entries))
}

/// 只有内核在 `stx_mask` 中声明填充了 `STATX_MNT_ID` 时才使用 `stx_mnt_id`.
///
/// 未声明时该字段的值不可靠 (可能是 0), 采用它会把所有挂载误判为同一个.
#[cfg(target_os = "linux")]
fn mount_id_from_mask(mask: u32, mnt_id: u64) -> Option<u64> {
    (mask & rustix::fs::StatxFlags::MNT_ID.bits() != 0).then_some(mnt_id)
}

/// `/proc/self/mountinfo` 的一个条目: 挂载标识与挂载点.
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, PartialEq, Eq)]
struct MountinfoEntry {
    mount_id: u64,
    mount_point: PathBuf,
}

/// 解析 `/proc/self/mountinfo`, 只取 mount id (第 1 个字段) 与挂载点 (第 5 个字段).
///
/// 字段以空格分隔, 挂载点内的空格, 制表符, 换行与反斜杠被内核转义为 `\040`,
/// `\011`, `\012`, `\134`; 先反转义再作为路径保存. 任一行格式不符时整体返回
/// `None`, 由调用方退回设备号.
#[cfg(target_os = "linux")]
fn parse_mountinfo(contents: &str) -> Option<Vec<MountinfoEntry>> {
    let mut entries = Vec::new();
    for line in contents.lines() {
        if line.is_empty() {
            continue;
        }
        let mut fields = line.split(' ');
        let mount_id: u64 = fields.next()?.parse().ok()?;
        fields.next()?; // parent id
        fields.next()?; // device
        fields.next()?; // root
        let mount_point = fields.next()?;
        entries.push(MountinfoEntry {
            mount_id,
            mount_point: PathBuf::from(unescape_mountinfo_field(mount_point)),
        });
    }
    Some(entries)
}

/// 反转义 mountinfo 挂载点中的 `\040`, `\011`, `\012` 与 `\134`.
#[cfg(target_os = "linux")]
fn unescape_mountinfo_field(field: &str) -> String {
    let bytes = field.as_bytes();
    let mut unescaped = String::with_capacity(field.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\'
            && let Some(byte) = bytes
                .get(index + 1..index + 4)
                .and_then(unescape_mountinfo_octal)
        {
            unescaped.push(char::from(byte));
            index += 4;
            continue;
        }
        let rest = &field[index..];
        let character = rest.chars().next().expect("剩余片段非空");
        unescaped.push(character);
        index += character.len_utf8();
    }
    unescaped
}

#[cfg(target_os = "linux")]
fn unescape_mountinfo_octal(digits: &[u8]) -> Option<u8> {
    match digits {
        b"040" => Some(b' '),
        b"011" => Some(b'\t'),
        b"012" => Some(b'\n'),
        b"134" => Some(b'\\'),
        _ => None,
    }
}

/// 在 mountinfo 条目中为 `path` 选择最长挂载点前缀, 返回其 mount id.
///
/// `Path::starts_with` 按路径组件比较, 因此 `/mnt` 不会匹配 `/mnt-other`. 挂载点
/// 相同 (同一位置叠挂) 时取后出现的条目, 与内核的可见挂载一致. 无匹配返回 `None`.
#[cfg(target_os = "linux")]
fn mount_id_from_mountinfo(path: &Path, entries: &[MountinfoEntry]) -> Option<u64> {
    let path = absolute_lexical(path)?;
    entries
        .iter()
        .filter(|entry| path.starts_with(&entry.mount_point))
        .max_by_key(|entry| entry.mount_point.components().count())
        .map(|entry| entry.mount_id)
}

/// 把查询路径变成绝对且词法归一化的路径, 用于和 mountinfo 挂载点做前缀比较.
///
/// 只消去 `.` 与多余的 `/`, 并消去 `..`; 不解析符号链接, 因为这里是按路径文本
/// 匹配挂载点, 且调用方已在 `symlink_metadata` 层面避免跟随末段链接.
#[cfg(target_os = "linux")]
fn absolute_lexical(path: &Path) -> Option<PathBuf> {
    use std::path::Component;

    let mut normalized = PathBuf::new();
    if path.is_absolute() {
        normalized.push(Path::new("/"));
    } else {
        normalized.push(std::env::current_dir().ok()?);
    }
    for component in path.components() {
        match component {
            Component::RootDir => normalized.push(Path::new("/")),
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(part) => normalized.push(part),
            Component::Prefix(_) => return None,
        }
    }
    Some(normalized)
}

/// 每个进程只读取并解析一次 `/proc/self/mountinfo`.
///
/// 递归复制与删除会对每个对象查询挂载身份, 缓存避免重复读取. 读不到或解析失败时
/// 缓存 `None`, 之后一律退回设备号; 运行中新增的挂载不在本变更的识别范围内.
#[cfg(target_os = "linux")]
fn mountinfo_entries() -> Option<&'static [MountinfoEntry]> {
    static ENTRIES: OnceLock<Option<Vec<MountinfoEntry>>> = OnceLock::new();
    ENTRIES
        .get_or_init(|| {
            fs::read_to_string("/proc/self/mountinfo")
                .ok()
                .and_then(|contents| parse_mountinfo(&contents))
        })
        .as_deref()
}

/// macOS 的挂载标识: `statvfs` 的 `f_fsid`.
///
/// rustix 在 macOS 上把 `StatVfs.f_fsid` 暴露为公开的 `u64`, 取值来自 libc
/// `statvfs` 的 `f_fsid`; 因此不需要读取 `libc::statfs.f_fsid` 的私有字段. 调用
/// 失败或 `f_fsid` 为 0 时返回 `None`, 调用方退回设备号.
#[cfg(target_os = "macos")]
fn mount_id(path: &Path) -> Option<u64> {
    let stat = rustix::fs::statvfs(path).ok()?;
    (stat.f_fsid != 0).then_some(stat.f_fsid)
}

/// 其余平台没有可用的挂载标识: 只比较设备号.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
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

// 测试接缝: 让 `mount_id` 跳过 `statx`, 直接验证 mountinfo fallback 的行为.
//
// 真实内核通常提供 `STATX_MNT_ID`, 无法在单元测试里关闭该能力; 线程局部开关只
// 存在于测试构建, 生产构建不包含任何注入状态.
#[cfg(all(test, target_os = "linux"))]
thread_local! {
    static SKIP_STATX: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(all(test, target_os = "linux"))]
struct SkipStatxGuard;

#[cfg(all(test, target_os = "linux"))]
fn skip_statx_mount_id() -> SkipStatxGuard {
    SKIP_STATX.with(|slot| slot.set(true));
    SkipStatxGuard
}

#[cfg(all(test, target_os = "linux"))]
impl Drop for SkipStatxGuard {
    fn drop(&mut self) {
        SKIP_STATX.with(|slot| slot.set(false));
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

    /// mountinfo 解析: mount id 与反转义后的挂载点.
    #[cfg(target_os = "linux")]
    #[test]
    fn mountinfo_parses_entries_and_unescapes_mount_points() {
        let contents = "\
36 35 98:0 / / rw,noatime - ext4 /dev/root rw
42 36 0:22 / /mnt/with\\040space rw - tmpfs tmpfs rw
43 42 0:23 / /mnt/tab\\011name rw - tmpfs tmpfs rw
44 43 0:24 / /mnt/new\\012line rw - tmpfs tmpfs rw
45 44 0:25 / /mnt/back\\134slash rw - tmpfs tmpfs rw
";
        let entries = parse_mountinfo(contents).unwrap();

        assert_eq!(
            entries,
            vec![
                MountinfoEntry {
                    mount_id: 36,
                    mount_point: PathBuf::from("/"),
                },
                MountinfoEntry {
                    mount_id: 42,
                    mount_point: PathBuf::from("/mnt/with space"),
                },
                MountinfoEntry {
                    mount_id: 43,
                    mount_point: PathBuf::from("/mnt/tab\tname"),
                },
                MountinfoEntry {
                    mount_id: 44,
                    mount_point: PathBuf::from("/mnt/new\nline"),
                },
                MountinfoEntry {
                    mount_id: 45,
                    mount_point: PathBuf::from("/mnt/back\\slash"),
                },
            ]
        );
    }

    /// 最长挂载点前缀按路径组件比较, 根与嵌套挂载都能选中正确的条目.
    #[cfg(target_os = "linux")]
    #[test]
    fn mountinfo_selects_the_longest_mount_point_prefix() {
        let entries = vec![
            MountinfoEntry {
                mount_id: 1,
                mount_point: PathBuf::from("/"),
            },
            MountinfoEntry {
                mount_id: 2,
                mount_point: PathBuf::from("/mnt"),
            },
            MountinfoEntry {
                mount_id: 3,
                mount_point: PathBuf::from("/mnt/data"),
            },
            MountinfoEntry {
                mount_id: 4,
                mount_point: PathBuf::from("/mnt/database"),
            },
        ];

        assert_eq!(mount_id_from_mountinfo(Path::new("/"), &entries), Some(1));
        assert_eq!(
            mount_id_from_mountinfo(Path::new("/mnt/file"), &entries),
            Some(2)
        );
        assert_eq!(
            mount_id_from_mountinfo(Path::new("/mnt/data/file"), &entries),
            Some(3)
        );
        assert_eq!(
            mount_id_from_mountinfo(Path::new("/mnt/data/nested/file"), &entries),
            Some(3)
        );
        assert_eq!(
            mount_id_from_mountinfo(Path::new("/mnt/database/file"), &entries),
            Some(4)
        );
        assert_eq!(
            mount_id_from_mountinfo(Path::new("/mnt-other/file"), &entries),
            Some(1),
            "/mnt 不得匹配 /mnt-other 这类同前缀但不是子路径的挂载点"
        );
    }

    /// 没有任何挂载点前缀时返回 `None`, 由调用方退回设备号.
    #[cfg(target_os = "linux")]
    #[test]
    fn mountinfo_returns_none_without_a_matching_mount_point() {
        let entries = vec![MountinfoEntry {
            mount_id: 7,
            mount_point: PathBuf::from("/mnt"),
        }];

        assert_eq!(
            mount_id_from_mountinfo(Path::new("/tmp/file"), &entries),
            None
        );
        assert_eq!(
            mount_id_from_mountinfo(Path::new("/mnt"), &entries),
            Some(7)
        );
    }

    /// 格式不符的 mountinfo 整体被拒绝, 不会产出部分条目.
    #[cfg(target_os = "linux")]
    #[test]
    fn malformed_mountinfo_is_rejected() {
        assert_eq!(parse_mountinfo("not enough fields"), None);
        assert_eq!(parse_mountinfo("x 1 2 3 / rw"), None);
    }

    /// `statx` 缺失挂载标识时实际选中 mountinfo fallback; 路径不存在时不借用父目录.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_mountinfo_fallback_replaces_a_missing_statx_mount_id() {
        let dir = tempfile::tempdir().unwrap();
        let Some(entries) = mountinfo_entries() else {
            return;
        };

        let guard = skip_statx_mount_id();
        let present = mount_id(dir.path());
        let missing = mount_id(&dir.path().join("gone"));
        drop(guard);

        assert_eq!(present, mount_id_from_mountinfo(dir.path(), entries));
        assert!(
            present.is_some(),
            "存在的路径应由 mountinfo 解析出 mount id"
        );
        assert_eq!(missing, None, "不存在的路径不得解析到父目录的挂载");
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

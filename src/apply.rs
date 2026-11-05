//! 部署: 创建父目录, 建立链接, 备份后替换.
//!
//! 本模块只做文件系统改动, 不做任何判定 -- 逐条目的结论由 `classify` 唯一给出
//! (FR-017), 这里只执行 `EntryState::action`.

use std::fs;
use std::path::{Path, PathBuf};

use crate::backup::{self, BackupKind};
use crate::classify::{Action, Current, EntryState};
use crate::entry::Entry;
use crate::error::{Error, Result};
use crate::os;

/// 替换路径的暂存标签; 与撤销路径的 `dotlink-undo.partial` 区分, 便于诊断残留.
const REPLACE_STAGING_LABEL: &str = "dotlink-replace.partial";

/// `ReplaceAndBackup` 应使用的备份命名空间, 由已经完成的判定结果唯一决定.
///
/// `LinkedWrong` 是本工具留下的错误链接, 备份它只是重建前的安全网, 因此使用自愈
/// 命名空间; 外部占用 (`Foreign*`) 是用户内容, 使用用户命名空间. 其他 `Current`
/// 不会导出 `ReplaceAndBackup`, 在这里穷举是为了让新增状态必须在编译期作出选择.
pub(crate) fn backup_kind_for(current: Current) -> BackupKind {
    match current {
        Current::LinkedWrong => BackupKind::SelfHeal,
        Current::ForeignLink | Current::ForeignFile | Current::ForeignDir => BackupKind::User,
        Current::ForeignOther
        | Current::SourceMissing
        | Current::Absent
        | Current::LinkedCorrect
        | Current::Unreadable => {
            unreachable!("{current:?} 不会请求替换备份")
        }
    }
}

/// 执行单个条目的动作. 返回备份文件的完整路径 (若发生了备份).
///
/// `Ok(None)` 表示该条目未涉及备份: 或本就不需要动作 (`Noop` / `ReportMissingSource`
/// / `ReportUnreadable` / `RefuseConflict`), 或动作是纯粹的链接创建.
/// `Err` 表示该条目失败 (FR-014: 由调用者决定不中断其余条目).
pub fn apply_entry(entry: &Entry, state: &EntryState) -> Result<Option<PathBuf>> {
    match state.action {
        Action::Create => {
            create_link(entry)?;
            Ok(None)
        }
        Action::ReplaceAndBackup => replace_and_backup(entry, backup_kind_for(state.current)),
        Action::Noop | Action::ReportMissingSource | Action::ReportUnreadable => Ok(None),
        Action::RefuseConflict => Ok(None),
    }
}

/// 建立链接前先把缺失的各级父目录建出来 (FR-005).
fn create_link(entry: &Entry) -> Result<()> {
    create_parent_dirs(&entry.target_path)?;

    os::create_symlink(&entry.source_path, &entry.target_path).map_err(|error| {
        Error::EntryFailed(format!(
            "建立链接 {} -> {} 失败: {error}",
            entry.target_path.display(),
            entry.source_path.display()
        ))
    })
}

/// 创建 `path` 缺失的各级父目录.
fn create_parent_dirs(path: &Path) -> Result<()> {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => create_dir_all(parent),
        _ => Ok(()),
    }
}

/// 递归创建目录, 失败时给出条目级错误.
fn create_dir_all(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)
        .map_err(|error| Error::EntryFailed(format!("创建父目录 {} 失败: {error}", dir.display())))
}

/// `Action::ReplaceAndBackup` 的严格顺序 (openspec/specs/link-deployment/spec.md 的
/// "获许可的替换先完成备份再删除原对象" 与 "替换删除前必须原子复核身份").
///
/// 1. 用 `symlink_metadata` 记录目标的对象身份 (设备号, inode 与对象类型);
/// 2. 把 `target_path` 上的**现有对象**备份出去 (文件 / 目录递归 / 符号链接本身);
/// 3. `write_backup` 返回即意味着备份已完整落定**并被改名为最终名**;
/// 4. 以原子不覆盖的 rename 把目标移入同目录暂存名, 复核暂存对象仍是第 1 步记录的
///    那一个, 只有复核通过才删除暂存对象并建立新链接.
///
/// 顺序不可颠倒: 原对象是唯一副本, 先删除再复制会让一次中途失败变成不可恢复的
/// 数据丢失. 暂存与复核把"判定"和"删除"之间的竞态收窄为一次 rename: 外部替换物
/// 会因身份不一致被移回原路径, 不会被当作旧对象删除. 备份失败时**不删除原对象**,
/// 条目报 `Err` 并保留原状.
fn replace_and_backup(entry: &Entry, kind: BackupKind) -> Result<Option<PathBuf>> {
    let target = entry.target_path.as_path();

    let identity = ObjectIdentity::snapshot(target)
        .map_err(|error| Error::EntryFailed(format!("读取 {} 失败: {error}", target.display())))?;

    // 备份落定之后才允许删除原对象, 因此这之后的失败都发生在"原内容已被备份"之后:
    // 错误里带上备份路径, 用户才知道去哪儿找回它 (FR-016).
    let backup_path = backup::write_backup(target, kind)?;
    #[cfg(test)]
    run_after_backup_hook();
    replace_with_backup(entry, identity).map_err(|error| {
        Error::EntryFailed(format!(
            "{error} (原内容已备份到 {})",
            backup_path.display()
        ))
    })?;

    Ok(Some(backup_path))
}

/// 备份完成后的替换: 暂存并复核身份, 复核通过才删除暂存对象, 最后建立链接.
fn replace_with_backup(entry: &Entry, identity: ObjectIdentity) -> Result<()> {
    let target = entry.target_path.as_path();
    let staged = stage_and_verify(target, identity)?;
    remove_staged(target, &staged)?;
    create_link(entry)
}

/// 对象身份快照: 设备号, inode 与对象类型 (设计 D2).
///
/// 只比较这三项: 足以区分"刚备份的对象"与外部替换物, 又不依赖 mtime 等易变字段.
#[cfg(unix)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ObjectIdentity {
    dev: u64,
    ino: u64,
    kind: ObjectKind,
}

/// 对象类型的分类: 符号链接独立于其目标, 其余特殊类型统一为 `Other`.
#[cfg(unix)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ObjectKind {
    File,
    Dir,
    Symlink,
    Other,
}

#[cfg(unix)]
impl ObjectIdentity {
    fn snapshot(path: &Path) -> std::io::Result<Self> {
        let metadata = fs::symlink_metadata(path)?;
        Ok(Self::from_metadata(&metadata))
    }

    fn from_metadata(metadata: &fs::Metadata) -> Self {
        use std::os::unix::fs::MetadataExt;

        let file_type = metadata.file_type();
        let kind = if file_type.is_symlink() {
            ObjectKind::Symlink
        } else if file_type.is_dir() {
            ObjectKind::Dir
        } else if file_type.is_file() {
            ObjectKind::File
        } else {
            ObjectKind::Other
        };
        Self {
            dev: metadata.dev(),
            ino: metadata.ino(),
            kind,
        }
    }
}

/// 不受支持平台的占位实现: 平台门禁会在运行到替换之前拒绝, 这里只需可编译.
#[cfg(not(unix))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ObjectIdentity;

#[cfg(not(unix))]
impl ObjectIdentity {
    fn snapshot(_path: &Path) -> std::io::Result<Self> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "当前平台不支持对象身份复核",
        ))
    }
}

/// 原子暂存目标并复核它仍是备份前快照记录的对象 (设计 D2).
///
/// 复核失败时把暂存对象移回原路径 (原路径为空时), **不删除**它; 无法移回时在
/// 说明里给出暂存路径. 调用者只应在复核通过后删除返回的暂存对象.
fn stage_and_verify(target: &Path, expected: ObjectIdentity) -> Result<PathBuf> {
    let staged = os::stage_aside(target, REPLACE_STAGING_LABEL)
        .map_err(|error| Error::EntryFailed(error.to_string()))?;
    let actual = ObjectIdentity::snapshot(&staged).map_err(|error| {
        staged_failure(
            target,
            &staged,
            format!("复核暂存对象 {} 失败: {error}", staged.display()),
        )
    })?;
    if actual != expected {
        return Err(staged_failure(
            target,
            &staged,
            format!(
                "拒绝删除 {}: 备份之后目标被外部替换 (暂存对象的身份与备份时不一致)",
                target.display()
            ),
        ));
    }
    Ok(staged)
}

/// 复核失败时的收尾: 尝试把暂存对象移回原路径, 不移回就报告暂存位置.
fn staged_failure(target: &Path, staged: &Path, reason: String) -> Error {
    match os::rename_noreplace(staged, target) {
        Ok(()) => Error::EntryFailed(format!("{reason}; 已把暂存对象移回原路径, 未删除")),
        Err(error) => Error::EntryFailed(format!(
            "{reason}; 且移回原路径 {} 失败: {error}; 对象暂存于 {}",
            target.display(),
            staged.display()
        )),
    }
}

/// 删除身份已复核的暂存对象: 目录递归, 文件与符号链接单个删除.
///
/// 删除失败时该对象已被备份. 非目录对象可以原子移回原路径 (原路径为空时), 避免把
/// "已备份但未替换"的对象遗留在暂存名上; 目录可能已被部分删除, **不回滚**, 只报告
/// 备份路径与暂存路径 (设计 D2, 风险取舍).
fn remove_staged(target: &Path, staged: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(staged).map_err(|error| {
        Error::EntryFailed(format!("读取暂存对象 {} 失败: {error}", staged.display()))
    })?;
    let removal = if metadata.is_dir() {
        fs::remove_dir_all(staged)
    } else {
        fs::remove_file(staged)
    };

    match removal {
        Ok(()) => Ok(()),
        Err(error) if metadata.is_dir() => Err(Error::EntryFailed(format!(
            "删除暂存目录 {} 失败: {error}; 该目录可能已被部分删除, 未移回原路径",
            staged.display()
        ))),
        Err(error) => match os::rename_noreplace(staged, target) {
            Ok(()) => Err(Error::EntryFailed(format!(
                "删除暂存对象 {} 失败: {error}; 已把对象移回原路径",
                staged.display()
            ))),
            Err(rollback) => Err(Error::EntryFailed(format!(
                "删除暂存对象 {} 失败: {error}; 且移回原路径 {} 失败: {rollback}; 对象暂存于 {}",
                staged.display(),
                target.display(),
                staged.display()
            ))),
        },
    }
}

// 测试接缝: 备份完成之后, 暂存之前运行一次注入的动作.
//
// 真实的"备份后被外部替换"竞态无法通过公开接口构造; 单元测试用它在两步之间改写
// 目标, 以覆盖身份复核的失败路径 (设计 D5).
#[cfg(test)]
thread_local! {
    static AFTER_BACKUP: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn after_backup_hook(hook: impl FnOnce() + 'static) {
    AFTER_BACKUP.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
}

#[cfg(test)]
fn run_after_backup_hook() {
    AFTER_BACKUP.with(|slot| {
        if let Some(hook) = slot.borrow_mut().take() {
            hook();
        }
    });
}

#[cfg(all(test, unix))]
mod tests {

    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    use super::*;
    use crate::classify::Current;
    use crate::entry::OnConflict;

    /// 构造一个条目: `target` 为绝对路径, 源在仓库中.
    fn entry_for(repo: &Path, source: &str, target: &Path) -> Entry {
        let mut entry = crate::entry::resolve(
            source,
            &target.to_string_lossy(),
            OnConflict::ReplaceAndBackup,
            repo,
        )
        .expect("解析条目");
        entry.source_path = repo.join(source);
        entry
    }

    fn state(action: Action) -> EntryState {
        state_with(Current::Absent, action)
    }

    fn state_with(current: Current, action: Action) -> EntryState {
        EntryState {
            current,
            action,
            detail: String::new(),
        }
    }

    #[test]
    fn create_makes_missing_parent_directories() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        fs::write(repo.join("nvim.lua"), "source").unwrap();

        let target = repo.join("home/.config/nvim/init.lua");
        let entry = entry_for(repo, "nvim.lua", &target);

        assert_eq!(apply_entry(&entry, &state(Action::Create)).unwrap(), None);

        assert!(
            fs::symlink_metadata(&target)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_link(&target).unwrap(), entry.source_path);
        assert_eq!(fs::read_to_string(&target).unwrap(), "source");
    }

    #[test]
    fn create_works_when_the_parent_directory_already_exists() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        fs::write(repo.join("bashrc"), "source").unwrap();
        fs::create_dir_all(repo.join("home")).unwrap();

        let target = repo.join("home/.bashrc");
        let entry = entry_for(repo, "bashrc", &target);
        apply_entry(&entry, &state(Action::Create)).unwrap();

        assert_eq!(fs::read_to_string(&target).unwrap(), "source");
    }

    #[test]
    fn actions_without_side_effects_touch_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        fs::write(repo.join("bashrc"), "source").unwrap();
        let target = repo.join("home/.bashrc");
        let entry = entry_for(repo, "bashrc", &target);

        for action in [
            Action::Noop,
            Action::ReportMissingSource,
            Action::ReportUnreadable,
            Action::RefuseConflict,
        ] {
            assert_eq!(apply_entry(&entry, &state(action)).unwrap(), None);
        }

        assert!(
            fs::symlink_metadata(repo.join("home")).is_err(),
            "无副作用的动作不得创建任何路径"
        );
    }

    #[test]
    fn replace_and_backup_handles_a_regular_file() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        fs::write(repo.join("bashrc"), "managed").unwrap();
        fs::create_dir_all(repo.join("home")).unwrap();

        let target = repo.join("home/bashrc");
        fs::write(&target, "pre-existing").unwrap();
        let entry = entry_for(repo, "bashrc", &target);

        let backup_path = apply_entry(
            &entry,
            &state_with(Current::ForeignFile, Action::ReplaceAndBackup),
        )
        .unwrap()
        .expect("必须报告备份路径");

        assert_eq!(fs::read_to_string(&backup_path).unwrap(), "pre-existing");
        assert!(
            backup_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .contains(".dotlink-backup."),
            "外部占用的备份必须使用用户命名空间: {}",
            backup_path.display()
        );
        assert!(
            !backup_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .contains(".dotlink-heal."),
            "外部占用不得写入自愈命名空间: {}",
            backup_path.display()
        );
        assert!(
            fs::symlink_metadata(&target)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_to_string(&target).unwrap(), "managed");
    }

    /// 本工具的错误链接在重建前使用自愈命名空间, 不占用用户备份序列.
    #[test]
    fn replace_and_backup_uses_the_self_heal_namespace_for_a_wrong_link() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        fs::write(repo.join("bashrc"), "managed").unwrap();
        fs::create_dir_all(repo.join("home")).unwrap();

        let stale = repo.join("stale");
        let target = repo.join("home/bashrc");
        os::create_symlink(&stale, &target).unwrap();
        let entry = entry_for(repo, "bashrc", &target);

        let backup_path = apply_entry(
            &entry,
            &state_with(Current::LinkedWrong, Action::ReplaceAndBackup),
        )
        .unwrap()
        .expect("必须报告备份路径");

        let name = backup_path.file_name().unwrap().to_string_lossy();
        assert!(
            name.starts_with("bashrc.dotlink-heal."),
            "自愈备份必须使用自愈命名空间: {name}"
        );
        assert!(
            fs::symlink_metadata(&backup_path)
                .unwrap()
                .file_type()
                .is_symlink(),
            "备份的必须是旧链接本身"
        );
        assert_eq!(fs::read_link(&backup_path).unwrap(), stale);
        assert!(
            fs::read_dir(repo.join("home"))
                .unwrap()
                .flatten()
                .all(|item| !item
                    .file_name()
                    .to_string_lossy()
                    .contains(".dotlink-backup.")),
            "自愈不得产生用户备份"
        );
        assert_eq!(fs::read_to_string(&target).unwrap(), "managed");
    }

    /// 自愈备份无法写入时, 旧链接必须原样留在原路径, 条目失败.
    #[test]
    fn a_failed_self_heal_backup_keeps_the_wrong_link() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        fs::write(repo.join("bashrc"), "managed").unwrap();
        let home = repo.join("home");
        fs::create_dir_all(&home).unwrap();

        let stale = repo.join("stale");
        let target = home.join("bashrc");
        os::create_symlink(&stale, &target).unwrap();
        let entry = entry_for(repo, "bashrc", &target);

        fs::set_permissions(&home, fs::Permissions::from_mode(0o555)).unwrap();
        if fs::write(home.join("probe"), "").is_ok() {
            fs::remove_file(home.join("probe")).unwrap();
            fs::set_permissions(&home, fs::Permissions::from_mode(0o755)).unwrap();
            eprintln!("以 root 运行: chmod 555 不生效, 跳过自愈备份失败断言");
            return;
        }

        let result = apply_entry(
            &entry,
            &state_with(Current::LinkedWrong, Action::ReplaceAndBackup),
        );
        fs::set_permissions(&home, fs::Permissions::from_mode(0o755)).unwrap();

        let error = result.unwrap_err();
        assert!(matches!(error, Error::EntryFailed(_)));
        assert_eq!(
            fs::read_link(&target).expect("旧链接必须保留"),
            stale,
            "备份失败时不得删除或改写旧链接"
        );
        assert!(
            fs::read_dir(&home).unwrap().flatten().all(|item| !item
                .file_name()
                .to_string_lossy()
                .contains(".dotlink-heal.")),
            "失败的备份不得留下最终名"
        );
    }

    #[test]
    fn replace_and_backup_handles_a_directory_recursively() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        fs::write(repo.join("nvim.lua"), "managed").unwrap();
        fs::create_dir_all(repo.join("home/nvim/lua")).unwrap();
        fs::write(repo.join("home/nvim/init.lua"), "old").unwrap();
        fs::write(repo.join("home/nvim/lua/a.lua"), "old-a").unwrap();

        let target = repo.join("home/nvim");
        let entry = entry_for(repo, "nvim.lua", &target);
        let backup_path = apply_entry(
            &entry,
            &state_with(Current::ForeignDir, Action::ReplaceAndBackup),
        )
        .unwrap()
        .expect("必须报告备份路径");

        assert_eq!(
            fs::read_to_string(backup_path.join("init.lua")).unwrap(),
            "old"
        );
        assert_eq!(
            fs::read_to_string(backup_path.join("lua/a.lua")).unwrap(),
            "old-a"
        );
        assert!(
            fs::symlink_metadata(&target)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_to_string(&target).unwrap(), "managed");
    }

    #[test]
    fn replace_and_backup_copies_a_foreign_link_itself() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        fs::write(repo.join("bashrc"), "managed").unwrap();
        fs::create_dir_all(repo.join("home")).unwrap();

        let elsewhere = repo.join("elsewhere");
        fs::write(&elsewhere, "foreign").unwrap();
        let target = repo.join("home/bashrc");
        os::create_symlink(&elsewhere, &target).unwrap();

        let entry = entry_for(repo, "bashrc", &target);
        let backup_path = apply_entry(
            &entry,
            &state_with(Current::ForeignLink, Action::ReplaceAndBackup),
        )
        .unwrap()
        .expect("必须报告备份路径");

        let metadata = fs::symlink_metadata(&backup_path).unwrap();
        assert!(
            metadata.file_type().is_symlink(),
            "备份的必须是链接本身, 而非其目标的内容"
        );
        assert_eq!(fs::read_link(&backup_path).unwrap(), elsewhere);
        assert!(elsewhere.is_file(), "原链接的目标不得被删除");
        assert_eq!(fs::read_to_string(&target).unwrap(), "managed");
    }

    #[test]
    fn a_replacement_after_backup_is_moved_back_and_the_failure_names_the_backup() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        fs::write(repo.join("bashrc"), "managed").unwrap();
        fs::create_dir_all(repo.join("home")).unwrap();

        let target = repo.join("home/bashrc");
        fs::write(&target, "original").unwrap();
        let entry = entry_for(repo, "bashrc", &target);

        // 备份完成后, 暂存前: 外部把目标换成了另一个文件.
        let replaced = target.clone();
        after_backup_hook(move || {
            fs::remove_file(&replaced).unwrap();
            fs::write(&replaced, "replacement").unwrap();
        });

        let error = apply_entry(
            &entry,
            &state_with(Current::ForeignFile, Action::ReplaceAndBackup),
        )
        .unwrap_err();
        let message = error.to_string();

        // 新对象被移回原路径并保留, 没有被当作旧对象删除, 也没有建立链接.
        assert!(
            !fs::symlink_metadata(&target)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_to_string(&target).unwrap(), "replacement");
        assert!(message.contains("被外部替换"), "{message}");
        assert!(message.contains("原内容已备份到"), "{message}");

        // 备份是替换前那一刻的原内容, 且说明里给出的路径真实存在.
        let backups: Vec<PathBuf> = fs::read_dir(repo.join("home"))
            .unwrap()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.to_string_lossy().contains(".dotlink-backup."))
            .collect();
        assert_eq!(backups.len(), 1, "替换失败也必须留下备份: {backups:?}");
        assert_eq!(fs::read_to_string(&backups[0]).unwrap(), "original");
        assert!(
            message.contains(&backups[0].display().to_string()),
            "说明必须给出备份路径: {message}"
        );
        // 身份不一致时对象已移回原路径, 不留暂存副本.
        assert!(
            fs::read_dir(repo.join("home"))
                .unwrap()
                .flatten()
                .all(|entry| !entry
                    .file_name()
                    .to_string_lossy()
                    .contains(".dotlink-replace.partial")),
            "身份不一致时不得遗留暂存副本"
        );
    }

    #[test]
    fn stage_and_verify_accepts_the_object_the_snapshot_saw() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("bashrc");
        fs::write(&target, "original").unwrap();

        let identity = ObjectIdentity::snapshot(&target).unwrap();
        let staged = stage_and_verify(&target, identity).unwrap();

        assert!(
            fs::symlink_metadata(&target).is_err(),
            "复核通过后目标已被暂存"
        );
        assert_eq!(fs::read_to_string(&staged).unwrap(), "original");
        fs::remove_file(&staged).unwrap();
    }

    #[test]
    fn stage_and_verify_moves_a_replaced_file_back() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("bashrc");
        fs::write(&target, "original").unwrap();
        let identity = ObjectIdentity::snapshot(&target).unwrap();

        // 快照之后, 暂存之前: 外部把目标换成了另一个文件.
        fs::remove_file(&target).unwrap();
        fs::write(&target, "replacement").unwrap();

        let error = stage_and_verify(&target, identity).unwrap_err();

        assert!(error.to_string().contains("被外部替换"), "{error}");
        assert_eq!(fs::read_to_string(&target).unwrap(), "replacement");
        assert!(
            fs::read_dir(dir.path())
                .unwrap()
                .flatten()
                .all(|entry| !entry
                    .file_name()
                    .to_string_lossy()
                    .contains(".dotlink-replace.partial")),
            "身份不一致的对象必须移回原路径, 不留暂存副本"
        );
    }

    #[test]
    fn stage_and_verify_moves_a_replaced_directory_back() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("bashrc");
        fs::write(&target, "original").unwrap();
        let identity = ObjectIdentity::snapshot(&target).unwrap();

        // 目标被替换成目录: 类型不同, 同样必须移回并保留.
        fs::remove_file(&target).unwrap();
        fs::create_dir(&target).unwrap();
        fs::write(target.join("nested"), "replacement").unwrap();

        let error = stage_and_verify(&target, identity).unwrap_err();

        assert!(error.to_string().contains("被外部替换"), "{error}");
        assert!(fs::symlink_metadata(&target).unwrap().is_dir());
        assert_eq!(
            fs::read_to_string(target.join("nested")).unwrap(),
            "replacement"
        );
    }

    #[test]
    fn replace_and_backup_keeps_the_original_when_the_backup_fails() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        fs::write(repo.join("nvim.lua"), "managed").unwrap();

        let target = repo.join("home/nvim");
        let locked = target.join("locked");
        fs::create_dir_all(&locked).unwrap();
        fs::write(locked.join("secret"), "secret").unwrap();
        fs::write(target.join("init.lua"), "old").unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

        // 以 root 运行时权限位无效, 无法构造"复制必然失败"的场景, 只能跳过.
        if fs::read_dir(&locked).is_ok() {
            fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
            return;
        }

        let entry = entry_for(repo, "nvim.lua", &target);
        let error = apply_entry(
            &entry,
            &state_with(Current::ForeignDir, Action::ReplaceAndBackup),
        )
        .unwrap_err();
        assert!(matches!(error, Error::EntryFailed(_)));

        // 原对象完整保留在原路径 (顺序颠倒即数据丢失, 这是唯一的回归防线).
        let metadata = fs::symlink_metadata(&target).unwrap();
        assert!(metadata.is_dir(), "原目录必须保留");
        assert_eq!(fs::read_to_string(target.join("init.lua")).unwrap(), "old");
        // 恢复权限位后才能读回被锁住的子目录内容.
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(fs::read_to_string(locked.join("secret")).unwrap(), "secret");

        // 不得存在最终名的备份: 中断残留只能是 `.partial`.
        let prefix = "nvim.dotlink-backup.";
        let residue: Vec<String> = fs::read_dir(repo.join("home"))
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(prefix))
            .collect();
        for name in &residue {
            assert!(
                name.ends_with(".partial"),
                "{name} 不是 .partial 残留, 会被误认为完整备份"
            );
        }
    }

    /// 备份已落定, 替换后续步骤失败时, 错误必须给出备份路径 (FR-016).
    ///
    /// 构造: 目标目录内含一个只读可进的子目录 (`0o555`), 备份能读取它, 但删除原对象
    /// 时无法清空它 -- 于是备份成功而替换失败, 正是"原内容已备份但目标未就位"的情形.
    #[test]
    fn replace_and_backup_reports_the_backup_path_when_replacing_fails() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        fs::write(repo.join("nvim.lua"), "managed").unwrap();

        let target = repo.join("home/nvim");
        let locked = target.join("locked");
        fs::create_dir_all(&locked).unwrap();
        fs::write(locked.join("secret"), "secret").unwrap();
        fs::write(target.join("init.lua"), "old").unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).unwrap();

        // 以 root 运行时权限位无效, 无法构造"删除必然失败"的场景, 只能跳过.
        if fs::write(locked.join("probe"), "").is_ok() {
            fs::remove_file(locked.join("probe")).unwrap();
            fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
            eprintln!("以 root 运行: chmod 555 不生效, 跳过替换失败的备份路径断言");
            return;
        }

        let entry = entry_for(repo, "nvim.lua", &target);
        let error = apply_entry(
            &entry,
            &state_with(Current::ForeignDir, Action::ReplaceAndBackup),
        )
        .unwrap_err();
        assert!(matches!(error, Error::EntryFailed(_)));

        let message = error.to_string();
        let marker = "(原内容已备份到 ";
        let start = message
            .find(marker)
            .unwrap_or_else(|| panic!("错误必须给出备份路径: {message}"))
            + marker.len();
        let end = message
            .rfind(')')
            .unwrap_or_else(|| panic!("备份路径应以 `)` 收尾: {message}"));
        let backup_path = PathBuf::from(&message[start..end]);
        assert!(
            fs::symlink_metadata(&backup_path).is_ok(),
            "错误里给出的备份路径 {} 必须存在",
            backup_path.display()
        );

        // 删除失败时原对象已原子移入暂存名, 说明必须给出暂存路径, 且不得静默丢失它.
        let staged_marker = "删除暂存目录 ";
        let staged_start = message
            .find(staged_marker)
            .unwrap_or_else(|| panic!("错误必须给出暂存路径: {message}"))
            + staged_marker.len();
        let staged_end = staged_start
            + message[staged_start..]
                .find(" 失败")
                .unwrap_or_else(|| panic!("暂存路径应以 ` 失败` 收尾: {message}"));
        let staged = PathBuf::from(&message[staged_start..staged_end]);
        assert!(
            fs::symlink_metadata(&staged).is_ok(),
            "删除失败时对象应留在暂存名上: {}",
            staged.display()
        );
        assert!(
            fs::symlink_metadata(&target).is_err(),
            "目标已原子移入暂存名, 不得留下半删除的原路径"
        );

        // 备份保真复制了只读子目录的权限位: 暂存树与备份两处都要恢复, 否则 TempDir
        // 无法清空 0o555 目录, 会在 TMPDIR 留下残留.
        fs::set_permissions(
            backup_path.join("locked"),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        fs::set_permissions(staged.join("locked"), fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            fs::read_to_string(staged.join("locked/secret")).unwrap(),
            "secret"
        );
    }
}

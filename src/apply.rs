//! 部署: 创建父目录, 建立链接, 备份后替换.
//!
//! 本模块只做文件系统改动, 不做任何判定 -- 逐条目的结论由 `classify` 唯一给出
//! (FR-017), 这里只执行 `EntryState::action`.

use std::path::{Path, PathBuf};

use crate::backup;
use crate::classify::{Action, EntryState};
use crate::entry::Entry;
use crate::error::{Error, Result};
use crate::os;

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
        Action::ReplaceAndBackup => replace_and_backup(entry),
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

/// `Action::ReplaceAndBackup` 的严格顺序 (data-model.md 第 4 节).
///
/// 1. 把 `target_path` 上的**现有对象**备份出去 (文件 / 目录递归 / 符号链接本身);
/// 2. `write_backup` 返回即意味着备份已完整落定**并被改名为最终名**;
/// 3. 只有到此时才允许删除原对象, 再建立新链接.
///
/// 顺序不可颠倒: 原对象是唯一副本, 先删除再复制会让一次中途失败变成不可恢复的
/// 数据丢失 (SC-004). 备份失败时**不删除原对象**, 条目报 `Err` 并保留原状.
fn replace_and_backup(entry: &Entry) -> Result<Option<PathBuf>> {
    let target = entry.target_path.as_path();

    // 备份落定之后才允许删除原对象, 因此这两步的失败都发生在"原内容已被备份"之后:
    // 错误里带上备份路径, 用户才知道去哪儿找回它 (FR-016).
    let backup_path = backup::write_backup(target)?;
    remove_object(target)
        .and_then(|()| create_link(entry))
        .map_err(|error| {
            Error::EntryFailed(format!(
                "{error} (原内容已备份到 {})",
                backup_path.display()
            ))
        })?;

    Ok(Some(backup_path))
}

/// 删除原对象: 目录用 `remove_dir_all`, 文件与符号链接用 `remove_file`.
///
/// 判定同样使用 lstat 语义 (`symlink_metadata`), 因此指向目录的符号链接走
/// `remove_file` 分支, 只移除链接本身而不动其目标的内容.
fn remove_object(target: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(target)
        .map_err(|error| Error::EntryFailed(format!("读取 {} 失败: {error}", target.display())))?;

    let removal = if metadata.is_dir() {
        std::fs::remove_dir_all(target)
    } else {
        std::fs::remove_file(target)
    };

    removal.map_err(|error| Error::EntryFailed(format!("删除 {} 失败: {error}", target.display())))
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
        EntryState {
            current: Current::Absent,
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

        let backup_path = apply_entry(&entry, &state(Action::ReplaceAndBackup))
            .unwrap()
            .expect("必须报告备份路径");

        assert_eq!(fs::read_to_string(&backup_path).unwrap(), "pre-existing");
        assert!(
            fs::symlink_metadata(&target)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_to_string(&target).unwrap(), "managed");
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
        let backup_path = apply_entry(&entry, &state(Action::ReplaceAndBackup))
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
        let backup_path = apply_entry(&entry, &state(Action::ReplaceAndBackup))
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
        let error = apply_entry(&entry, &state(Action::ReplaceAndBackup)).unwrap_err();
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
        let error = apply_entry(&entry, &state(Action::ReplaceAndBackup)).unwrap_err();
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

        // 备份保真复制了只读子目录的权限位: 两处都要恢复, 否则 TempDir 无法清空
        // 0o555 目录, 会在 TMPDIR 留下残留 (只恢复原对象一处仍会留下备份副本).
        fs::set_permissions(
            backup_path.join("locked"),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(fs::read_to_string(locked.join("secret")).unwrap(), "secret");
    }
}

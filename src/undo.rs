//! 撤销: 移除本工具创建的链接, 恢复备份 (FR-009 / FR-010).
//!
//! 破坏面被 `Current` 的取值直接限制住: 只有 `LinkedCorrect` 与 `LinkedWrong` 才意味着
//! "这是本工具的链接", 其余取值 (尤其 `ForeignLink` / `ForeignFile` / `ForeignDir`)
//! 一律不触碰. 判定与动手之间仍可能被外部替换, 因此动手时先原子暂存, 再按与
//! `classify` 相同的 `os::link_destination` 口径复核暂存对象: 不是符号链接或目标
//! 落在仓库外时移回原路径并失败, 不删除用户内容.
//!
//! 有备份时恢复走"先把链接原子移到暂存名 -> 复制备份就位 -> 失败则把链接移回":
//! 复制或 rename 失败且原路径仍为空时回滚链接, 因此下一次 `undo` 仍可重试; 路径被
//! 重新占用时保留占用物和暂存链接, 并报告暂存位置. rename 之后的父目录同步失败表示
//! 内容已就位, 此时不回滚, 只清理旧链接副本并在说明中指出内容已恢复.

use std::fs;
use std::path::{Path, PathBuf};

use crate::backup;
use crate::classify::{Current, EntryState};
use crate::entry::Entry;
use crate::error::{Error, Result};
use crate::os;

/// 撤销路径的暂存标签; 与替换路径的 `dotlink-replace.partial` 区分, 便于诊断残留.
const UNDO_STAGING_LABEL: &str = "dotlink-undo.partial";

/// 撤销单个条目.
///
/// `backup` 是调用方在**动手之前**取得的同目录最新备份 (lib.rs 的 `undo` 计划), 报告
/// 行的 token 与说明由同一份快照导出; 把它作为恢复的唯一输入, "说明里写的备份"与
/// "实际恢复的备份"就必然是同一个, 本模块也不再持有第二份查找逻辑.
///
/// `repo_root` 是**已规范化**的仓库根, 由调用方在循环外计算一次; 复核暂存链接的
/// 归属时与 `classify` 使用同一基准.
///
/// `Ok(())` 表示该条目已按判定处理完毕 (可能什么都没做). `Err` 表示本工具自己的
/// 链接/备份处理失败 (FR-014: 由调用者决定不中断其余条目).
pub fn undo_entry(
    entry: &Entry,
    state: &EntryState,
    backup: Option<&Path>,
    repo_root: &Path,
) -> Result<()> {
    let target = entry.target_path.as_path();

    match state.current {
        // 本工具创建的链接 (二者都蕴含链接文本落在仓库根之内): 动手时先原子暂存并
        // 复核身份与归属, 再按快照有无备份决定"移除链接"还是"恢复备份".
        Current::LinkedCorrect | Current::LinkedWrong => match backup {
            Some(backup) => replace_link_with_backup(target, backup, entry, repo_root),
            None => remove_link(target, entry, repo_root),
        },
        // 非本工具创建的内容: 绝不删除
        // (`openspec/specs/link-teardown/spec.md` 的"只移除本工具创建的链接").
        Current::ForeignLink
        | Current::ForeignFile
        | Current::ForeignDir
        | Current::ForeignOther => Ok(()),
        // 该路径本就没有本工具创建的链接 (含撤销后目标为空的第二次运行).
        Current::Absent => Ok(()),
        // 源缺失是终态; `Unreadable` 是环境故障. 二者都无文件系统改动.
        Current::SourceMissing | Current::Unreadable => Ok(()),
    }
}

/// 移除目标路径上的符号链接: 先原子暂存并复核身份与归属, 再删除暂存链接.
fn remove_link(target: &Path, entry: &Entry, repo_root: &Path) -> Result<()> {
    let staged = stage_verified_link(target, entry, repo_root)?;
    fs::remove_file(&staged).map_err(|error| {
        Error::EntryFailed(format!(
            "移除链接 {} 失败: {error}; 链接已暂存于 {}",
            target.display(),
            staged.display()
        ))
    })
}

/// 移除链接并把快照指定的备份恢复到原路径.
///
/// 暂存, 复核, 恢复落位及失败回滚都拒绝覆盖落点. 回滚冲突时链接留在暂存名, 错误中给出
/// 位置, 避免为了恢复原状而覆盖期间新建的用户内容.
fn replace_link_with_backup(
    target: &Path,
    backup: &Path,
    entry: &Entry,
    repo_root: &Path,
) -> Result<()> {
    let staged = stage_verified_link(target, entry, repo_root)?;

    match backup::restore_backup(target, backup) {
        Ok(()) => {
            // 恢复已就位; 旧链接副本的清理失败只是残留, 不外抛.
            let _ = fs::remove_file(&staged);
            Ok(())
        }
        // 内容尚未落位: 原路径仍为空, 可以把链接移回以便重试.
        Err(backup::RestoreError::NotLanded(error)) => Err(rollback_link(target, &staged, error)),
        // 内容已就位: 回滚只会因目标被占用而失败, 清理旧链接副本即可.
        Err(backup::RestoreError::LandedButUnsynced(error)) => match fs::remove_file(&staged) {
            Ok(()) => Err(error),
            Err(cleanup) => Err(Error::EntryFailed(format!(
                "{error}; 清理旧链接副本 {} 失败: {cleanup}",
                staged.display()
            ))),
        },
    }
}

/// 原子暂存目标, 再复核暂存对象仍是本工具创建的符号链接.
///
/// 判定与动手之间目标可能被替换: `rename` 与 `remove_file` 对普通文件与目录同样
/// 有效, 只有暂存之后重新读取对象, 才能确保被删除的确实是判定时的那个链接. 复核
/// 失败时把暂存对象移回原路径并保留, 错误中说明"判定之后被替换".
fn stage_verified_link(target: &Path, entry: &Entry, repo_root: &Path) -> Result<PathBuf> {
    let staged = os::stage_aside(target, UNDO_STAGING_LABEL)
        .map_err(|error| Error::EntryFailed(error.to_string()))?;
    match verify_staged_link(&staged, target, entry, repo_root) {
        Ok(()) => Ok(staged),
        Err(error) => Err(rollback_link(target, &staged, error)),
    }
}

/// 暂存之后的身份与归属复核.
///
/// 1. 暂存对象必须仍是符号链接 (`remove_file` 对真实文件同样有效);
/// 2. 重新读取链接文本, 用与 `classify` 相同的 [`os::link_destination`] 解析;
///    目标必须等于本条目的 `source_path` 或落在仓库根内, 否则视为外部链接.
fn verify_staged_link(staged: &Path, target: &Path, entry: &Entry, repo_root: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(staged).map_err(|error| {
        Error::EntryFailed(format!("复核暂存链接 {} 失败: {error}", staged.display()))
    })?;
    if !metadata.file_type().is_symlink() {
        return Err(Error::EntryFailed(format!(
            "拒绝移除 {}: 目标已不是本工具创建的符号链接 (判定之后被替换)",
            target.display()
        )));
    }

    let link_text = os::read_link(staged).map_err(|error| {
        Error::EntryFailed(format!("读取暂存链接 {} 失败: {error}", staged.display()))
    })?;
    let destination = os::link_destination(staged, &link_text);
    if destination != entry.source_path && !os::is_inside_repo(&destination, repo_root) {
        return Err(Error::EntryFailed(format!(
            "拒绝移除 {}: 暂存链接的目标 {} 落在仓库外 (判定之后被替换)",
            target.display(),
            destination.display()
        )));
    }
    Ok(())
}

fn rollback_link(target: &Path, aside: &Path, error: Error) -> Error {
    match os::rename_noreplace(aside, target) {
        Ok(()) => error,
        Err(rollback) => Error::EntryFailed(format!(
            "{error}; 且回滚链接失败, 链接暂存于 {}: {rollback}",
            aside.display()
        )),
    }
}

#[cfg(all(test, unix))]
mod tests {

    use std::fs;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::path::PathBuf;

    use super::*;
    use crate::classify::Action;
    use crate::entry::OnConflict;
    use crate::os;

    /// 测试侧的暂存候选名; 与生产路径共用 `os::staging_path`, 避免两套命名规则.
    fn aside_path(target: &Path) -> PathBuf {
        os::staging_path(target, UNDO_STAGING_LABEL)
    }

    fn entry_for(repo: &Path, source: &str, target: &Path) -> Entry {
        crate::entry::resolve(
            source,
            &target.to_string_lossy(),
            OnConflict::ReplaceAndBackup,
            repo,
        )
        .expect("解析条目")
    }

    fn state(current: Current) -> EntryState {
        EntryState {
            current,
            action: Action::Noop,
            detail: String::new(),
        }
    }

    /// 测试用 `undo_entry`: 仓库根取临时目录的规范形态.
    fn undo_entry_in(
        dir: &Path,
        entry: &Entry,
        state: &EntryState,
        backup: Option<&Path>,
    ) -> Result<()> {
        undo_entry(entry, state, backup, &os::normalize(dir))
    }

    /// 布置一个"已部署"的条目: 源在仓库中, 目标是指向源的链接.
    ///
    /// 返回的仓库根取**物理**形态 (`os::normalize`): 断言侧与 `entry::resolve`
    /// 产出的 `target_path` 必须落在同一套语义上. 系统的临时目录在 macOS 上是
    /// `/var` (`/private/var` 的别名), 若这里保留 `tempfile` 给出的逻辑路径,
    /// 期望值与实现就会分属两种形态而"无谓失败".
    fn deployed(dir: &std::path::Path, name: &str, source: &str) -> (PathBuf, Entry) {
        let repo = os::normalize(dir);
        fs::create_dir_all(&repo).unwrap();
        fs::write(repo.join(source), "managed").unwrap();

        let target = repo.join(name);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        os::create_symlink(&repo.join(source), &target).unwrap();

        let entry = entry_for(&repo, source, &target);
        (target, entry)
    }

    #[test]
    fn linked_correct_without_backup_is_removed_without_a_restore() {
        let dir = tempfile::tempdir().unwrap();
        let (target, entry) = deployed(dir.path(), "bashrc", "managed");

        undo_entry_in(dir.path(), &entry, &state(Current::LinkedCorrect), None).unwrap();

        assert!(fs::symlink_metadata(&target).is_err(), "链接必须已被移除");
    }

    #[test]
    fn linked_wrong_is_also_removed() {
        let dir = tempfile::tempdir().unwrap();
        let (target, entry) = deployed(dir.path(), "bashrc", "managed");

        undo_entry_in(dir.path(), &entry, &state(Current::LinkedWrong), None).unwrap();

        assert!(fs::symlink_metadata(&target).is_err());
    }

    #[test]
    fn foreign_and_absent_states_are_never_touched() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        fs::write(repo.join("bashrc"), "managed").unwrap();

        // 别人创建的链接 (落在仓库之外).
        let outside = repo.join("outside");
        fs::write(&outside, "foreign").unwrap();
        let link_target = repo.join("home/link");
        fs::create_dir_all(repo.join("home")).unwrap();
        os::create_symlink(&outside, &link_target).unwrap();
        let link_entry = entry_for(repo, "bashrc", &link_target);

        for current in [Current::ForeignLink, Current::Absent] {
            undo_entry_in(dir.path(), &link_entry, &state(current), None).unwrap();
            assert!(
                fs::symlink_metadata(&link_target).is_ok(),
                "{current:?} 下不得删除任何东西"
            );
        }

        // 真实文件与真实目录.
        let file_target = repo.join("home/real-file");
        fs::write(&file_target, "keep me").unwrap();
        let dir_target = repo.join("home/real-dir");
        fs::create_dir_all(dir_target.join("nested")).unwrap();
        fs::write(dir_target.join("nested/a"), "keep me too").unwrap();

        let file_entry = entry_for(repo, "bashrc", &file_target);
        let dir_entry = entry_for(repo, "bashrc", &dir_target);
        for current in [
            Current::ForeignFile,
            Current::ForeignDir,
            Current::SourceMissing,
            Current::Unreadable,
        ] {
            undo_entry_in(dir.path(), &file_entry, &state(current), None).unwrap();
            undo_entry_in(dir.path(), &dir_entry, &state(current), None).unwrap();
        }

        assert_eq!(fs::read_to_string(&file_target).unwrap(), "keep me");
        assert_eq!(
            fs::read_to_string(dir_target.join("nested/a")).unwrap(),
            "keep me too"
        );
    }

    /// 恢复用的是调用方快照指定的那一个备份; "取同目录最新"的发现逻辑属
    /// `backup::find_latest_backup` (其单测已覆盖), 这里锁定恢复行为本身.
    #[test]
    fn undo_restores_the_snapshot_backup_after_removing_the_link() {
        let dir = tempfile::tempdir().unwrap();
        let (target, entry) = deployed(dir.path(), "bashrc", "managed");

        // 备份与被覆盖对象同目录; 以 `target` 的父目录为基准, 与实现同源.
        let parent = target.parent().unwrap();
        let backup_path = parent.join("bashrc.dotlink-backup.20260101T000000");
        fs::write(&backup_path, "original").unwrap();

        undo_entry_in(
            dir.path(),
            &entry,
            &state(Current::LinkedWrong),
            Some(&backup_path),
        )
        .unwrap();

        assert_eq!(fs::read_to_string(&target).unwrap(), "original");
        assert!(
            fs::symlink_metadata(&backup_path).is_ok(),
            "备份文件本身必须保留"
        );
    }

    /// rename 成功后的父目录同步失败: 内容已恢复, 不得回滚链接, 失败说明指出已恢复.
    #[test]
    fn a_failed_restore_parent_sync_keeps_the_restored_content() {
        let dir = tempfile::tempdir().unwrap();
        let (target, entry) = deployed(dir.path(), "bashrc", "managed");

        let backup_path = target.with_file_name("bashrc.dotlink-backup.20260101T000000");
        fs::write(&backup_path, "original").unwrap();

        // 只让目录同步失败: 备份内容的文件同步会先通过, 失败点落在恢复 rename 后的
        // 父目录同步上.
        let guard = os::fail_directory_syncs();
        let error = undo_entry_in(
            dir.path(),
            &entry,
            &state(Current::LinkedWrong),
            Some(&backup_path),
        )
        .unwrap_err();
        drop(guard);

        let message = error.to_string();
        assert!(message.contains("同步父目录"), "{message}");
        assert!(message.contains("备份内容已恢复到"), "{message}");
        assert!(
            !message.contains("回滚链接失败"),
            "内容已恢复时不得尝试回滚: {message}"
        );

        // 目标是恢复出的真实文件, 不是链接; 备份保留, 旧链接副本已清理.
        assert!(
            !fs::symlink_metadata(&target)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_to_string(&target).unwrap(), "original");
        assert!(
            fs::symlink_metadata(&backup_path).is_ok(),
            "备份文件本身必须保留"
        );
        assert!(
            fs::read_dir(target.parent().unwrap())
                .unwrap()
                .flatten()
                .all(|entry| !entry
                    .file_name()
                    .to_string_lossy()
                    .contains(".dotlink-undo.partial")),
            "内容已恢复后旧链接副本必须被清理"
        );
    }

    #[test]
    fn undo_preserves_occupied_staging_names() {
        let dir = tempfile::tempdir().unwrap();
        let (target, entry) = deployed(dir.path(), "bashrc", "managed");
        let backup = target.with_file_name("bashrc.dotlink-backup.20260101T000000");
        fs::write(&backup, "original").unwrap();

        let base = aside_path(&target);
        let mut occupied = vec![base.clone()];
        for serial in 2..=4 {
            let mut name = base.as_os_str().to_os_string();
            name.push(format!(".{serial}"));
            occupied.push(PathBuf::from(name));
        }
        fs::write(&occupied[0], "user file").unwrap();
        os::create_symlink(Path::new("managed"), &occupied[1]).unwrap();
        os::create_symlink(Path::new("missing"), &occupied[2]).unwrap();
        fs::create_dir(&occupied[3]).unwrap();
        fs::write(occupied[3].join("nested"), "user directory").unwrap();
        let inodes: Vec<_> = occupied
            .iter()
            .map(|path| fs::symlink_metadata(path).unwrap().ino())
            .collect();

        undo_entry_in(
            dir.path(),
            &entry,
            &state(Current::LinkedCorrect),
            Some(&backup),
        )
        .unwrap();

        for (path, inode) in occupied.iter().zip(inodes) {
            assert_eq!(fs::symlink_metadata(path).unwrap().ino(), inode);
        }
        assert_eq!(fs::read_to_string(&occupied[0]).unwrap(), "user file");
        assert_eq!(fs::read_link(&occupied[1]).unwrap(), Path::new("managed"));
        assert_eq!(fs::read_link(&occupied[2]).unwrap(), Path::new("missing"));
        assert_eq!(
            fs::read_to_string(occupied[3].join("nested")).unwrap(),
            "user directory"
        );
        assert_eq!(fs::read_to_string(&target).unwrap(), "original");
        assert_eq!(fs::read_to_string(&backup).unwrap(), "original");
        let residue = fs::read_dir(target.parent().unwrap())
            .unwrap()
            .map(|item| item.unwrap().path())
            .filter(|path| path.to_string_lossy().contains(".dotlink-undo.partial"))
            .count();
        assert_eq!(residue, occupied.len(), "只清理本次取得的暂存名");
    }

    #[test]
    fn rollback_preserves_an_occupied_target_and_the_aside() {
        for kind in ["file", "link", "dangling", "directory"] {
            let dir = tempfile::tempdir().unwrap();
            let (target, _) = deployed(dir.path(), "bashrc", "managed");
            let aside = aside_path(&target);
            fs::rename(&target, &aside).unwrap();
            let link_text = fs::read_link(&aside).unwrap();
            match kind {
                "file" => fs::write(&target, "new user content").unwrap(),
                "link" => os::create_symlink(Path::new("managed"), &target).unwrap(),
                "dangling" => os::create_symlink(Path::new("missing"), &target).unwrap(),
                "directory" => {
                    fs::create_dir(&target).unwrap();
                    fs::write(target.join("nested"), "new user content").unwrap();
                }
                _ => unreachable!(),
            }
            let inode = fs::symlink_metadata(&target).unwrap().ino();

            let error = rollback_link(
                &target,
                &aside,
                Error::EntryFailed("恢复备份失败: 原始原因".to_string()),
            );

            assert_eq!(
                fs::symlink_metadata(&target).unwrap().ino(),
                inode,
                "{kind}"
            );
            match kind {
                "file" => assert_eq!(fs::read_to_string(&target).unwrap(), "new user content"),
                "link" => assert_eq!(fs::read_link(&target).unwrap(), Path::new("managed")),
                "dangling" => assert_eq!(fs::read_link(&target).unwrap(), Path::new("missing")),
                "directory" => assert_eq!(
                    fs::read_to_string(target.join("nested")).unwrap(),
                    "new user content"
                ),
                _ => unreachable!(),
            }
            assert_eq!(fs::read_link(&aside).unwrap(), link_text);
            let detail = error.to_string();
            assert!(detail.contains("恢复备份失败: 原始原因"), "{detail}");
            assert!(detail.contains("回滚链接失败"), "{detail}");
            assert!(detail.contains(&aside.display().to_string()), "{detail}");
        }
    }

    #[test]
    fn rollback_restores_relative_and_dangling_links_to_an_empty_target() {
        for raw in ["managed", "missing"] {
            let dir = tempfile::tempdir().unwrap();
            let target = dir.path().join("bashrc");
            let aside = aside_path(&target);
            fs::write(dir.path().join("managed"), "managed").unwrap();
            os::create_symlink(Path::new(raw), &aside).unwrap();
            let inode = fs::symlink_metadata(&aside).unwrap().ino();

            let error = rollback_link(
                &target,
                &aside,
                Error::EntryFailed("恢复备份失败: 原始原因".to_string()),
            );

            assert_eq!(error.to_string(), "恢复备份失败: 原始原因");
            assert_eq!(fs::read_link(&target).unwrap(), Path::new(raw));
            assert_eq!(fs::symlink_metadata(&target).unwrap().ino(), inode);
            assert_eq!(
                fs::symlink_metadata(&aside).unwrap_err().kind(),
                std::io::ErrorKind::NotFound
            );
        }
    }

    #[test]
    fn undo_can_retry_after_a_missing_backup_returns() {
        let dir = tempfile::tempdir().unwrap();
        let (target, entry) = deployed(dir.path(), "bashrc", "managed");
        let backup = target.with_file_name("bashrc.dotlink-backup.20260101T000000");
        let link_text = fs::read_link(&target).unwrap();

        let error = undo_entry_in(
            dir.path(),
            &entry,
            &state(Current::LinkedCorrect),
            Some(&backup),
        )
        .unwrap_err();

        assert!(error.to_string().contains(&backup.display().to_string()));
        assert_eq!(fs::read_link(&target).unwrap(), link_text);
        assert!(fs::symlink_metadata(aside_path(&target)).is_err());
        fs::write(&backup, "original").unwrap();
        undo_entry_in(
            dir.path(),
            &entry,
            &state(Current::LinkedCorrect),
            Some(&backup),
        )
        .unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "original");
        assert_eq!(fs::read_to_string(&backup).unwrap(), "original");
        assert!(fs::symlink_metadata(aside_path(&target)).is_err());
    }

    #[test]
    fn undo_restores_a_backup_that_is_itself_a_link() {
        let dir = tempfile::tempdir().unwrap();
        let (target, entry) = deployed(dir.path(), "bashrc", "managed");

        let parent = target.parent().unwrap();
        let elsewhere = parent.join("elsewhere");
        fs::write(&elsewhere, "foreign").unwrap();
        let backup_path = parent.join("bashrc.dotlink-backup.20260101T000000");
        os::create_symlink(&elsewhere, &backup_path).unwrap();

        undo_entry_in(
            dir.path(),
            &entry,
            &state(Current::LinkedCorrect),
            Some(&backup_path),
        )
        .unwrap();

        assert!(
            fs::symlink_metadata(&target)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_link(&target).unwrap(), elsewhere);
    }

    #[test]
    fn undo_restores_a_directory_backup_recursively() {
        let dir = tempfile::tempdir().unwrap();
        let (target, entry) = deployed(dir.path(), "home/nvim", "managed");

        let parent = target.parent().unwrap();
        let backup_path = parent.join("nvim.dotlink-backup.20260101T000000");
        fs::create_dir_all(backup_path.join("lua")).unwrap();
        fs::write(backup_path.join("lua/a.lua"), "old-a").unwrap();

        undo_entry_in(
            dir.path(),
            &entry,
            &state(Current::LinkedCorrect),
            Some(&backup_path),
        )
        .unwrap();

        assert!(fs::symlink_metadata(&target).unwrap().is_dir());
        assert_eq!(
            fs::read_to_string(target.join("lua/a.lua")).unwrap(),
            "old-a"
        );
    }

    /// `.partial` 不是"已完整命名"的备份: 快照查找返回 `None`, 撤销因此只移除链接,
    /// 不做任何恢复. 发现的唯一来源是 `backup::find_latest_backup`, 故在此断言它.
    #[test]
    fn undo_ignores_partial_backup_residue() {
        let dir = tempfile::tempdir().unwrap();
        let (target, entry) = deployed(dir.path(), "bashrc", "managed");

        fs::write(
            target
                .parent()
                .unwrap()
                .join("bashrc.dotlink-backup.20990101T000000.partial"),
            "half",
        )
        .unwrap();
        assert_eq!(
            backup::find_latest_backup(&target).unwrap(),
            None,
            ".partial 不得被认作可用备份"
        );

        undo_entry_in(dir.path(), &entry, &state(Current::LinkedCorrect), None).unwrap();

        assert!(fs::symlink_metadata(&target).is_err(), "链接仍须被移除");
        assert!(fs::symlink_metadata(dir.path().join("bashrc")).is_err());
    }

    #[test]
    fn undo_fails_when_the_link_cannot_be_removed() {
        let dir = tempfile::tempdir().unwrap();
        let (target, entry) = deployed(dir.path(), "home/bashrc", "managed");

        // 让父目录不可写, 使 remove_file 必然失败.
        let parent = target.parent().unwrap().to_path_buf();
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o555)).unwrap();

        // 以 root 运行时权限位无效, 无法构造"移除必然失败"的场景, 只能跳过.
        let writable_as_root = fs::write(parent.join("probe"), "").is_ok();
        if writable_as_root {
            fs::remove_file(parent.join("probe")).unwrap();
            fs::set_permissions(&parent, fs::Permissions::from_mode(0o755)).unwrap();
            eprintln!("以 root 运行: chmod 555 不生效, 跳过移除失败断言");
            return;
        }

        let error =
            undo_entry_in(dir.path(), &entry, &state(Current::LinkedCorrect), None).unwrap_err();
        assert!(matches!(error, Error::EntryFailed(_)));
        assert!(
            fs::symlink_metadata(&target).is_ok(),
            "移除失败时链接必须保留"
        );

        fs::set_permissions(&parent, fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// 判定与撤销之间目标可能被替换 (判定横跨全部条目的执行时间): 替换后的真实
    /// 文件不是本工具的链接, 必须拒绝移除. `remove_file` 对普通文件同样有效, 不复核
    /// 符号链接身份就会把用户数据当成"本工具的链接"删掉.
    #[test]
    fn a_real_file_that_replaced_the_link_is_refused_and_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let repo = os::normalize(dir.path());
        fs::create_dir_all(&repo).unwrap();
        fs::write(repo.join("managed"), "managed").unwrap();

        let target = repo.join("bashrc");
        // 判定时刻: 它是本工具的链接.
        os::create_symlink(&repo.join("managed"), &target).unwrap();

        // 判定之后, 撤销之前: 用户在原路径放上了自己的真实文件.
        fs::remove_file(&target).unwrap();
        fs::write(&target, "user own content").unwrap();

        let entry = entry_for(&repo, "managed", &target);
        // 同目录确有一份备份: "复核失败时不消费备份"必须有真实备份才可验证.
        let backup = repo.join("bashrc.dotlink-backup.20260101T000000");
        fs::write(&backup, "original").unwrap();

        let error = undo_entry_in(
            dir.path(),
            &entry,
            &state(Current::LinkedCorrect),
            Some(&backup),
        )
        .unwrap_err();

        assert!(matches!(error, Error::EntryFailed(_)));
        assert!(
            error.to_string().contains("拒绝"),
            "说明应指出拒绝移除的原因: {error}"
        );
        assert!(
            error.to_string().contains("判定之后被替换"),
            "说明必须指出判定与动手之间目标已改变: {error}"
        );
        assert_eq!(
            fs::read_to_string(&target).unwrap(),
            "user own content",
            "判定之后出现在目标路径上的真实文件不得被删除"
        );
        assert_eq!(
            fs::read_to_string(&backup).unwrap(),
            "original",
            "复核失败时备份不得被消费或删除"
        );
    }

    /// 判定与撤销之间目标可能被替换成指向仓库外的链接: 它不再是本工具的链接,
    /// 必须移回原路径并保留, 不删除, 备份也不得被消费.
    #[test]
    fn a_link_that_replaced_the_original_and_points_outside_is_refused_and_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let (target, entry) = deployed(dir.path(), "bashrc", "managed");

        // 同目录确有一份备份: "复核失败时不消费备份"必须有真实备份才可验证.
        let backup = target.with_file_name("bashrc.dotlink-backup.20260101T000000");
        fs::write(&backup, "original").unwrap();

        // 判定之后, 撤销之前: 用户把链接换成了指向仓库外的链接.
        let outside_dir = tempfile::tempdir().unwrap();
        let outside = outside_dir.path().join("foreign");
        fs::write(&outside, "foreign").unwrap();
        fs::remove_file(&target).unwrap();
        os::create_symlink(&outside, &target).unwrap();

        let error = undo_entry_in(
            dir.path(),
            &entry,
            &state(Current::LinkedCorrect),
            Some(&backup),
        )
        .unwrap_err();

        assert!(matches!(error, Error::EntryFailed(_)));
        assert!(
            error.to_string().contains("仓库外"),
            "说明应指出暂存链接的目标落在仓库外: {error}"
        );
        assert!(
            fs::symlink_metadata(&target)
                .unwrap()
                .file_type()
                .is_symlink(),
            "仓库外链接必须被移回原路径"
        );
        assert_eq!(fs::read_link(&target).unwrap(), outside);
        assert!(
            fs::symlink_metadata(&outside).is_ok(),
            "链接指向的内容不得被删除"
        );
        assert_eq!(
            fs::read_to_string(&backup).unwrap(),
            "original",
            "复核失败时备份不得被消费或删除"
        );
    }

    /// 恢复出的是普通文件, 不是链接: 真实的第二次 `undo` 会由 `classify` 判为
    /// `ForeignFile` 并报 `skip` (见 `openspec/specs/link-teardown/spec.md` 的
    /// "撤销幂等且退出码随备份情形而异"). 这里直接以
    /// `Absent` 调用, 只锁定"`Absent` 不触碰文件系统"这一行.
    #[test]
    fn undo_entry_is_a_noop_when_the_target_is_already_absent() {
        let dir = tempfile::tempdir().unwrap();
        let (target, entry) = deployed(dir.path(), "bashrc", "managed");
        let backup_path = target
            .parent()
            .unwrap()
            .join("bashrc.dotlink-backup.20260101T000000");
        fs::write(&backup_path, "original").unwrap();

        undo_entry_in(
            dir.path(),
            &entry,
            &state(Current::LinkedCorrect),
            Some(&backup_path),
        )
        .unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "original");

        undo_entry_in(dir.path(), &entry, &state(Current::Absent), None).unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "original");
        assert!(fs::symlink_metadata(&backup_path).is_ok());
    }

    /// 恢复失败时链接必须原样保留: 恢复先把链接移到暂存名再复制备份, 复制阶段失败
    /// 会把链接移回, 因此下一次 `undo` 仍可重试, 不留下"链接已删而备份没恢复"的
    /// 半成品.
    #[test]
    fn a_failed_restore_keeps_the_link_so_undo_can_be_retried() {
        let dir = tempfile::tempdir().unwrap();
        let (target, entry) = deployed(dir.path(), "bashrc", "managed");

        let backup_path = target
            .parent()
            .unwrap()
            .join("bashrc.dotlink-backup.20260101T000000");
        fs::write(&backup_path, "original").unwrap();
        // 备份不可读 -> 复制到暂存名阶段失败.
        fs::set_permissions(&backup_path, fs::Permissions::from_mode(0o000)).unwrap();

        // 以 root 运行时权限位无效, 无法构造"复制必然失败"的场景, 只能跳过.
        if fs::read_to_string(&backup_path).is_ok() {
            fs::set_permissions(&backup_path, fs::Permissions::from_mode(0o644)).unwrap();
            eprintln!("以 root 运行: chmod 000 不生效, 跳过恢复失败的原子性断言");
            return;
        }

        let error = undo_entry_in(
            dir.path(),
            &entry,
            &state(Current::LinkedCorrect),
            Some(&backup_path),
        )
        .unwrap_err();
        assert!(matches!(error, Error::EntryFailed(_)));
        assert!(
            fs::symlink_metadata(&target)
                .unwrap()
                .file_type()
                .is_symlink(),
            "恢复失败时链接必须保留, 以便重试"
        );

        // 恢复权限后重试: 链接仍在, 因此能走通恢复.
        fs::set_permissions(&backup_path, fs::Permissions::from_mode(0o644)).unwrap();
        undo_entry_in(
            dir.path(),
            &entry,
            &state(Current::LinkedCorrect),
            Some(&backup_path),
        )
        .unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "original");
        assert!(fs::symlink_metadata(&backup_path).is_ok());
    }
}

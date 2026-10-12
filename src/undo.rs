//! 撤销: 移除本工具创建的链接, 恢复备份 (FR-009 / FR-010).
//!
//! 破坏面被 `Current` 的取值直接限制住: 只有 `LinkedCorrect` 与 `LinkedWrong` 才意味着
//! "这是本工具的链接", 其余取值 (尤其 `ForeignLink` / `ForeignFile` / `ForeignDir`)
//! 一律不触碰. 因此本模块**不需要**再次做归属判定 (`os::is_inside_repo`, 其入参需先过
//! `os::normalize`) -- 归属判定已经被吸收进 `Current`, 重复判定既冗余, 又可能因两处
//! 规范化语义不同而引入分歧.
//!
//! 有备份时恢复走"先把链接原子移到暂存名 -> 复制备份就位 -> 失败则把链接移回":
//! 复制失败时链接原样保留, 因此该条目在下一次 `undo` 仍可重试, 不会留下"链接已删而
//! 备份没恢复"的半成品状态.

use std::fs;
use std::path::{Path, PathBuf};

use crate::backup;
use crate::classify::{Current, EntryState};
use crate::entry::Entry;
use crate::error::{Error, Result};

/// 撤销单个条目.
///
/// `backup` 是调用方在**动手之前**取得的同目录最新备份 (lib.rs 的 `undo` 计划), 报告
/// 行的 token 与说明由同一份快照导出; 把它作为恢复的唯一输入, "说明里写的备份"与
/// "实际恢复的备份"就必然是同一个, 本模块也不再持有第二份查找逻辑.
///
/// `Ok(())` 表示该条目已按判定处理完毕 (可能什么都没做). `Err` 表示本工具自己的
/// 链接/备份处理失败 (FR-014: 由调用者决定不中断其余条目).
pub fn undo_entry(entry: &Entry, state: &EntryState, backup: Option<&Path>) -> Result<()> {
    let target = entry.target_path.as_path();

    match state.current {
        // 本工具创建的链接 (二者都蕴含链接文本落在仓库根之内): 动手前先复核身份,
        // 再按快照有无备份决定"移除链接"还是"恢复备份".
        Current::LinkedCorrect | Current::LinkedWrong => match backup {
            Some(backup) => replace_link_with_backup(target, backup),
            None => remove_link(target),
        },
        // 非本工具创建的内容: 绝不删除 (`contracts/cli.md` 的 `undo` 分支表).
        Current::ForeignLink | Current::ForeignFile | Current::ForeignDir => Ok(()),
        // 该路径本就没有本工具创建的链接 (含撤销后目标为空的第二次运行).
        Current::Absent => Ok(()),
        // 源缺失是终态; `Unreadable` 是环境故障. 二者都无文件系统改动.
        Current::SourceMissing | Current::Unreadable => Ok(()),
    }
}

/// 复核目标路径仍是符号链接.
///
/// 判定与撤销之间目标可能被替换: `remove_file` 与 `rename` 对普通文件与目录同样有效,
/// 不复核身份就会把用户数据当成"本工具的链接"删除或覆盖. 因此动手前以
/// `symlink_metadata` 复核, 已不是符号链接的一律拒绝 (与 apply 在实况变化时失败对偶).
fn ensure_still_a_link(target: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(target).map_err(|error| {
        Error::EntryFailed(format!("移除链接 {} 失败: {error}", target.display()))
    })?;
    if !metadata.file_type().is_symlink() {
        return Err(Error::EntryFailed(format!(
            "拒绝移除 {}: 目标已不是本工具创建的符号链接 (判定之后被替换), 未做任何改动",
            target.display()
        )));
    }
    Ok(())
}

/// 移除目标路径上的符号链接 (先复核身份).
fn remove_link(target: &Path) -> Result<()> {
    ensure_still_a_link(target)?;
    fs::remove_file(target)
        .map_err(|error| Error::EntryFailed(format!("移除链接 {} 失败: {error}", target.display())))
}

/// 移除链接并把快照指定的备份恢复到原路径.
///
/// 顺序刻意拆成三步, 使失败发生在"移除链接"之前:
///
/// 1. 复核目标仍是符号链接;
/// 2. 把链接**原子移到同目录的暂存名**, 使原路径空出;
/// 3. 复制备份到原路径 (`restore_backup` 内部走"临时名 -> 改名", 目录恢复中途失败
///    不会留下半成品). 失败时把链接移回原路径 -- 该条目保持"未撤销"的原状, 下一次
///    `undo` 可重试, 不会留下"链接已删而备份没恢复"的半成品.
fn replace_link_with_backup(target: &Path, backup: &Path) -> Result<()> {
    ensure_still_a_link(target)?;

    let aside = aside_path(target);
    fs::rename(target, &aside).map_err(|error| {
        Error::EntryFailed(format!("暂存链接 {} 失败: {error}", target.display()))
    })?;

    match backup::restore_backup(target, backup) {
        Ok(()) => {
            // 恢复已就位; 旧链接副本的清理失败只是残留, 不外抛.
            let _ = fs::remove_file(&aside);
            Ok(())
        }
        Err(error) => {
            // 恢复失败: 把链接移回原路径, 恢复"未撤销"的原状.
            let _ = fs::rename(&aside, target);
            Err(error)
        }
    }
}

/// 链接在恢复期间被移到的同目录暂存名 (进程级唯一).
fn aside_path(target: &Path) -> PathBuf {
    let mut name = target.as_os_str().to_os_string();
    name.push(format!(".{}.dotlink-undo.partial", std::process::id()));
    PathBuf::from(name)
}

#[cfg(all(test, unix))]
mod tests {

    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    use super::*;
    use crate::classify::Action;
    use crate::entry::OnConflict;
    use crate::os;

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

        undo_entry(&entry, &state(Current::LinkedCorrect), None).unwrap();

        assert!(fs::symlink_metadata(&target).is_err(), "链接必须已被移除");
    }

    #[test]
    fn linked_wrong_is_also_removed() {
        let dir = tempfile::tempdir().unwrap();
        let (target, entry) = deployed(dir.path(), "bashrc", "managed");

        undo_entry(&entry, &state(Current::LinkedWrong), None).unwrap();

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
            undo_entry(&link_entry, &state(current), None).unwrap();
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
            undo_entry(&file_entry, &state(current), None).unwrap();
            undo_entry(&dir_entry, &state(current), None).unwrap();
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

        undo_entry(&entry, &state(Current::LinkedWrong), Some(&backup_path)).unwrap();

        assert_eq!(fs::read_to_string(&target).unwrap(), "original");
        assert!(
            fs::symlink_metadata(&backup_path).is_ok(),
            "备份文件本身必须保留"
        );
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

        undo_entry(&entry, &state(Current::LinkedCorrect), Some(&backup_path)).unwrap();

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

        undo_entry(&entry, &state(Current::LinkedCorrect), Some(&backup_path)).unwrap();

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

        undo_entry(&entry, &state(Current::LinkedCorrect), None).unwrap();

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

        let error = undo_entry(&entry, &state(Current::LinkedCorrect), None).unwrap_err();
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

        // 判定之后、撤销之前: 用户在原路径放上了自己的真实文件.
        fs::remove_file(&target).unwrap();
        fs::write(&target, "user own content").unwrap();

        let entry = entry_for(&repo, "managed", &target);
        let error = undo_entry(&entry, &state(Current::LinkedCorrect), None).unwrap_err();

        assert!(matches!(error, Error::EntryFailed(_)));
        assert!(
            error.to_string().contains("拒绝"),
            "说明应指出拒绝移除的原因: {error}"
        );
        assert_eq!(
            fs::read_to_string(&target).unwrap(),
            "user own content",
            "判定之后出现在目标路径上的真实文件不得被删除"
        );
    }

    /// 恢复出的是普通文件, 不是链接: 真实的第二次 `undo` 会由 `classify` 判为
    /// `ForeignFile` 并报 `skip` (见 `contracts/cli.md` 的幂等说明). 这里直接以
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

        undo_entry(&entry, &state(Current::LinkedCorrect), Some(&backup_path)).unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "original");

        undo_entry(&entry, &state(Current::Absent), None).unwrap();
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

        let error =
            undo_entry(&entry, &state(Current::LinkedCorrect), Some(&backup_path)).unwrap_err();
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
        undo_entry(&entry, &state(Current::LinkedCorrect), Some(&backup_path)).unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "original");
        assert!(fs::symlink_metadata(&backup_path).is_ok());
    }
}

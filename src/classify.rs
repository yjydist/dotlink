//! 唯一的条目状态判定 (FR-017), 完全只读.

use std::fs::Metadata;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::entry::{Entry, OnConflict};
use crate::manifest::Manifest;
use crate::os;

/// 条目在目标路径上的现状.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Current {
    SourceMissing,
    Absent,
    LinkedCorrect,
    LinkedWrong,
    ForeignLink,
    ForeignFile,
    ForeignDir,
    Unreadable,
}

/// 由现状与 `on_conflict` 导出的应该执行的动作.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    ReportMissingSource,
    Create,
    Noop,
    RefuseConflict,
    ReplaceAndBackup,
    ReportUnreadable,
}

/// 单条目的判定结果: 现状 + 动作 + 人类可读说明.
#[derive(Debug, Clone)]
pub struct EntryState {
    pub current: Current,
    pub action: Action,
    pub detail: String,
}

/// 判定全部条目.
///
/// 全函数只读: 只做两次 `lstat` 与一次 `read_link`, 不产生任何文件系统改动.
pub fn classify(manifest: &Manifest) -> Vec<EntryState> {
    manifest
        .entries
        .iter()
        .map(|entry| classify_entry(entry, &manifest.repo_root))
        .collect()
}

/// 判定单个条目 (`data-model.md` 3.1).
///
/// 求值顺序严格为: 两次 `lstat` 的故障裁决 (`Unreadable` 先于表内各行) ->
/// `SourceMissing` -> `Absent` -> 自上而下取首个命中者.
fn classify_entry(entry: &Entry, repo_root: &Path) -> EntryState {
    // 两次 lstat: source_path 与 target_path 各一次, 不做任何穿透.
    let source = lstat(&entry.source_path);
    let target = lstat(&entry.target_path);

    // 求值语义 1: 非 NotFound 的失败是环境故障; 它不是"某个条件命中", 而是
    // "表中各行的条件根本无法求值", 因此先于表中所有行, 且两侧独立裁决.
    let source_side_failed = source
        .as_ref()
        .is_err_and(|error| error.kind() != ErrorKind::NotFound);
    let target_side_failed = target
        .as_ref()
        .is_err_and(|error| error.kind() != ErrorKind::NotFound);
    if source_side_failed || target_side_failed {
        let sides = match (source_side_failed, target_side_failed) {
            (true, true) => "源侧与目标侧",
            (true, false) => "源侧",
            _ => "目标侧",
        };
        let reason = source
            .as_ref()
            .err()
            .or_else(|| target.as_ref().err())
            .map(|error| error.to_string())
            .unwrap_or_default();
        return EntryState {
            current: Current::Unreadable,
            action: Action::ReportUnreadable,
            detail: format!("{sides}读取失败: {reason}"),
        };
    }

    // 求值语义 2: 源侧 NotFound -> SourceMissing, 必须先于 Absent 与冲突判定.
    //
    // `detail` 是给 `render.rs` 的原因补充 (它已自行冠以"仓库中不存在: <source>"),
    // 因此这里给出实际找过的规范化路径, 便于用户对照"仓库根 + source"的解析规则.
    if source.is_err() {
        return new_state(
            Current::SourceMissing,
            format!("解析为 {}", entry.source_path.display()),
            entry.on_conflict,
        );
    }

    // 求值语义 3: 目标侧 NotFound -> Absent (含其父目录不存在).
    let Ok(target_meta) = target else {
        return new_state(
            Current::Absent,
            "未部署 (目标路径不存在)".to_string(),
            entry.on_conflict,
        );
    };

    let kind = target_meta.file_type();
    if !kind.is_symlink() {
        let occupied = if kind.is_dir() {
            Current::ForeignDir
        } else {
            Current::ForeignFile
        };
        let occupied_note = if kind.is_dir() {
            "目标被目录占用"
        } else {
            "目标被普通文件占用"
        };
        return new_state(occupied, occupied_note.to_string(), entry.on_conflict);
    }

    // 求值语义 4: 两次 lstat 均成功, 从第 3 行起自上而下取首个命中者.
    let link_text = match os::read_link(&entry.target_path) {
        Ok(text) => text,
        Err(error) => {
            // lstat 已确认它是符号链接, 此处读不到只可能是竞争或环境故障.
            return EntryState {
                current: Current::Unreadable,
                action: Action::ReportUnreadable,
                detail: format!("目标侧读取链接失败: {error}"),
            };
        }
    };
    let actual = link_destination(&entry.target_path, &link_text);
    let source = os::normalize(&entry.source_path);

    if actual == source {
        return new_state(
            Current::LinkedCorrect,
            "已是最新".to_string(),
            entry.on_conflict,
        );
    }

    // 归属分界 (research.md R2): 落在仓库内 -> 本工具的错误链接; 之外 -> 别人的链接.
    // 二者互斥且穷尽, 且都以同一个 os::normalize 为基准, 因此悬空链接也能正确归类.
    //
    // `detail` 与 `render.rs` 的分工 (`render` 不可改, 它按如下方式解释本字段):
    // `wrong` 行取"实际指向"; `foreign` 行取链接文本并自行冠以"符号链接";
    // `missing` 行冠以"仓库中不存在: <source>"; 其余行原样透出. 因此这里只放原始值.
    let link_text = link_text.display().to_string();
    if os::link_is_inside_repo(&entry.target_path, repo_root) {
        new_state(Current::LinkedWrong, link_text, entry.on_conflict)
    } else {
        new_state(Current::ForeignLink, link_text, entry.on_conflict)
    }
}

/// 目标路径上的符号链接文本解析为绝对路径后的非严格规范化结果 (`L`).
///
/// 相对链接以链接所在目录为基准; 链接文本与基准都不必存在, 因此不得用
/// `Path::canonicalize` (`research.md` R2 的悬空链接陷阱).
fn link_destination(link_path: &Path, link_text: &Path) -> PathBuf {
    let absolute = if link_text.is_absolute() {
        link_text.to_path_buf()
    } else {
        match link_path.parent() {
            Some(parent) => parent.join(link_text),
            None => link_text.to_path_buf(),
        }
    };
    os::normalize(&absolute)
}

/// 一次 lstat. 必须用 `symlink_metadata`: `metadata` 会穿透到链接的目标,
/// 使 `ForeignLink` 与 `LinkedWrong` 无法区分.
fn lstat(path: &Path) -> std::io::Result<Metadata> {
    std::fs::symlink_metadata(path)
}

/// 组装判定结果. `detail` 是自由文本, 但必须携带 `contracts/cli.md` 的必现信息
/// (失败原因 / 实际指向 / 占用物类型).
fn new_state(current: Current, detail: String, on_conflict: OnConflict) -> EntryState {
    EntryState {
        current,
        action: action_for(current, on_conflict),
        detail,
    }
}

/// 由现状与 `on_conflict` 导出动作 (严格按 `data-model.md` 3.2 表).
///
/// `SourceMissing` / `Unreadable` / `Absent` / `LinkedCorrect` 四种与
/// `on_conflict` 无关; 其余四种冲突类只在"拒绝"与"备份后替换"之间二选一.
fn action_for(current: Current, on_conflict: OnConflict) -> Action {
    match current {
        Current::SourceMissing => Action::ReportMissingSource,
        Current::Unreadable => Action::ReportUnreadable,
        Current::Absent => Action::Create,
        Current::LinkedCorrect => Action::Noop,
        Current::LinkedWrong
        | Current::ForeignLink
        | Current::ForeignFile
        | Current::ForeignDir => match on_conflict {
            OnConflict::Skip => Action::RefuseConflict,
            OnConflict::ReplaceAndBackup => Action::ReplaceAndBackup,
        },
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::fs;
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::path::{Path, PathBuf};

    use tempfile::TempDir;

    use super::*;
    use crate::entry::{self, Entry};
    use crate::manifest;
    use crate::os;

    /// 临时仓库 + 临时主目录; 目标路径一律落在 `home` 之下 (即仓库之外).
    struct Fixture {
        /// 仅用于持有临时目录 (drop 时清理).
        _dir: TempDir,
        repo: PathBuf,
        home: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            Self::inside(tempfile::tempdir().expect("创建临时目录"))
        }

        /// 以给定的临时根目录构造夹具: `<root>/repo` 为仓库根, `<root>/home` 为
        /// 目标侧的落脚点, `<root>/outside` 为仓库之外的区域.
        fn inside(dir: TempDir) -> Self {
            let repo = os::normalize(&dir.path().join("repo"));
            let home = os::normalize(&dir.path().join("home"));
            fs::create_dir_all(&repo).expect("创建仓库根");
            fs::create_dir_all(&home).expect("创建临时主目录");
            Self {
                _dir: dir,
                repo,
                home,
            }
        }

        /// 在仓库中写入源文件, 返回其规范化后的绝对路径.
        fn write_source(&self, rel: &str, content: &str) -> PathBuf {
            let path = os::normalize(&self.repo.join(rel));
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("创建源的父目录");
            }
            fs::write(&path, content).expect("写入源文件");
            path
        }

        /// 目标侧路径 (仓库之外).
        fn home_path(&self, rel: &str) -> PathBuf {
            os::normalize(&self.home.join(rel))
        }

        /// 仓库之外的路径 (存在).
        fn outside_path(&self, rel: &str) -> PathBuf {
            let root = self.repo.parent().expect("临时根目录");
            let path = os::normalize(&root.join("outside").join(rel));
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("创建仓库之外的目录");
            }
            fs::write(&path, "user own content").expect("写入仓库之外的文件");
            path
        }

        fn entry(&self, source_rel: &str, target: &Path, on_conflict: OnConflict) -> Entry {
            Entry {
                source: source_rel.to_string(),
                target: target.display().to_string(),
                on_conflict,
                source_path: os::normalize(&self.repo.join(source_rel)),
                target_path: os::normalize(target),
            }
        }

        /// 判定单个条目.
        fn state(&self, source_rel: &str, target: &Path, on_conflict: OnConflict) -> EntryState {
            let manifest = Manifest {
                repo_root: self.repo.clone(),
                entries: vec![self.entry(source_rel, target, on_conflict)],
            };
            let mut states = classify(&manifest);
            assert_eq!(states.len(), 1, "classify 必须为每个条目给出一个结果");
            states.remove(0)
        }
    }

    fn chmod(path: &Path, mode: u32) {
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("设置权限位");
    }

    /// 以 root 运行时权限位不生效, 依赖 `EACCES` 的断言必须跳过.
    fn permissions_are_ignored(dir: &Path) -> bool {
        fs::read_dir(dir).is_ok()
    }

    // --- 3.1 的八种 Current 取值 ---

    #[test]
    fn source_missing_wins_over_an_occupied_target() {
        let f = Fixture::new();
        let target = f.home_path(".bashrc");
        fs::write(&target, "user own content").expect("写入占用物");

        // on_conflict 取 replace_and_backup: 若实现漏判判定顺序, 会得到 ForeignFile.
        let state = f.state("shell/bashrc", &target, OnConflict::ReplaceAndBackup);
        assert_eq!(state.current, Current::SourceMissing);
        assert_eq!(state.action, Action::ReportMissingSource);
        assert!(
            state
                .detail
                .contains(&f.repo.join("shell/bashrc").display().to_string()),
            "说明必须给出实际找过的源路径: {}",
            state.detail
        );
    }

    #[test]
    fn absent_target_is_created() {
        let f = Fixture::new();
        f.write_source("nvim/init.lua", "set number");

        // 父目录不存在也属于 Absent.
        let target = f.home_path("nested/deep/init.lua");
        let state = f.state("nvim/init.lua", &target, OnConflict::Skip);
        assert_eq!(state.current, Current::Absent);
        assert_eq!(state.action, Action::Create);
    }

    #[test]
    fn linked_correct_is_a_noop() {
        let f = Fixture::new();
        let source = f.write_source("shell/bashrc", "hello");
        let target = f.home_path(".bashrc");
        symlink(&source, &target).expect("建立链接");

        let state = f.state("shell/bashrc", &target, OnConflict::Skip);
        assert_eq!(state.current, Current::LinkedCorrect);
        assert_eq!(state.action, Action::Noop);
    }

    #[test]
    fn link_inside_the_repo_pointing_elsewhere_is_linked_wrong() {
        let f = Fixture::new();
        f.write_source("shell/bashrc", "hello");
        let other = f.write_source("shell/zshrc", "other");
        let target = f.home_path(".bashrc");
        symlink(&other, &target).expect("建立链接");

        let state = f.state("shell/bashrc", &target, OnConflict::Skip);
        assert_eq!(state.current, Current::LinkedWrong);
        assert_eq!(state.action, Action::RefuseConflict);
        assert_eq!(
            state.detail,
            other.display().to_string(),
            "wrong 行的说明即链接实际指向 (由 render.rs 冠以措辞)"
        );
    }

    #[test]
    fn link_outside_the_repo_is_foreign() {
        let f = Fixture::new();
        f.write_source("shell/bashrc", "hello");
        let outside = f.outside_path("zshrc");
        let target = f.home_path(".bashrc");
        symlink(&outside, &target).expect("建立链接");

        let state = f.state("shell/bashrc", &target, OnConflict::Skip);
        assert_eq!(state.current, Current::ForeignLink);
        assert_eq!(state.action, Action::RefuseConflict);
        assert_eq!(
            state.detail,
            outside.display().to_string(),
            "foreign 行的说明即链接实际指向 (占用物类型由 Current 给出)"
        );
    }

    #[test]
    fn plain_file_occupying_the_target_is_foreign_file() {
        let f = Fixture::new();
        f.write_source("shell/bashrc", "hello");
        let target = f.home_path(".bashrc");
        fs::write(&target, "user own content").expect("写入占用物");

        let state = f.state("shell/bashrc", &target, OnConflict::ReplaceAndBackup);
        assert_eq!(state.current, Current::ForeignFile);
        assert_eq!(state.action, Action::ReplaceAndBackup);
    }

    #[test]
    fn directory_occupying_the_target_is_foreign_dir() {
        let f = Fixture::new();
        f.write_source("shell/bashrc", "hello");
        let target = f.home_path(".bashrc");
        fs::create_dir_all(&target).expect("创建占用目录");

        let state = f.state("shell/bashrc", &target, OnConflict::ReplaceAndBackup);
        assert_eq!(state.current, Current::ForeignDir);
        assert_eq!(state.action, Action::ReplaceAndBackup);
    }

    #[test]
    fn unreadable_source_beats_a_normal_target() {
        let f = Fixture::new();
        let blocked = os::normalize(&f.repo.join("blocked"));
        fs::create_dir_all(&blocked).expect("创建被阻断的目录");
        fs::write(blocked.join("rc"), "hello").expect("写入源文件");

        // 目标侧一切正常: 若把非 NotFound 的失败读作"不存在", 会得到 SourceMissing.
        let target = f.home_path(".bashrc");
        symlink(f.repo.join("shell/bashrc"), &target).expect("建立链接");

        chmod(&blocked, 0o000);
        if permissions_are_ignored(&blocked) {
            chmod(&blocked, 0o755);
            println!(
                "跳过 unreadable_source_beats_a_normal_target: 当前以 root 运行, chmod 000 无效"
            );
            return;
        }
        for on_conflict in [OnConflict::Skip, OnConflict::ReplaceAndBackup] {
            let state = f.state("blocked/rc", &target, on_conflict);
            assert_eq!(state.current, Current::Unreadable);
            assert_eq!(state.action, Action::ReportUnreadable);
            assert!(
                state.detail.contains("源侧"),
                "说明必须指出失败的是源侧: {}",
                state.detail
            );
        }
        chmod(&blocked, 0o755);
    }

    #[test]
    fn unreadable_target_beats_a_missing_source() {
        let f = Fixture::new();
        let blocked = f.home_path("blocked");
        fs::create_dir_all(&blocked).expect("创建被阻断的目录");
        let target = os::normalize(&blocked.join("rc"));

        chmod(&blocked, 0o000);
        if permissions_are_ignored(&blocked) {
            chmod(&blocked, 0o755);
            println!(
                "跳过 unreadable_target_beats_a_missing_source: 当前以 root 运行, chmod 000 无效"
            );
            return;
        }
        // 源侧不存在 (NotFound), 目标侧 EACCES: 必须是 Unreadable 而非 SourceMissing.
        for on_conflict in [OnConflict::Skip, OnConflict::ReplaceAndBackup] {
            let state = f.state("gone/rc", &target, on_conflict);
            assert_eq!(state.current, Current::Unreadable);
            assert_eq!(state.action, Action::ReportUnreadable);
            assert!(
                state.detail.contains("目标侧"),
                "说明必须指出失败的是目标侧: {}",
                state.detail
            );
        }
        chmod(&blocked, 0o755);
    }

    // --- 判定陷阱 ---

    #[test]
    fn not_found_is_never_unreadable() {
        let f = Fixture::new();

        // 源侧不存在 -> SourceMissing, 不是 Unreadable.
        let state = f.state("gone/rc", &f.home_path(".bashrc"), OnConflict::Skip);
        assert_eq!(state.current, Current::SourceMissing);

        // 目标侧不存在 -> Absent, 不是 Unreadable.
        f.write_source("shell/bashrc", "hello");
        let state = f.state("shell/bashrc", &f.home_path(".bashrc"), OnConflict::Skip);
        assert_eq!(state.current, Current::Absent);
    }

    #[test]
    fn a_dangling_link_inside_the_repo_is_linked_wrong() {
        let f = Fixture::new();
        f.write_source("nvim/init.lua", "set number");

        // 链接文本指向仓库内一个不存在的文件: 非严格规范化必须仍判定为"仓库内".
        let gone = os::normalize(&f.repo.join("nvim/removed.lua"));
        let target = f.home_path(".config/init.lua");
        fs::create_dir_all(target.parent().expect("目标父目录")).expect("创建目标父目录");
        symlink(&gone, &target).expect("建立链接");

        let state = f.state("nvim/init.lua", &target, OnConflict::Skip);
        assert_eq!(
            state.current,
            Current::LinkedWrong,
            "悬空链接必须靠 os::normalize 的非严格性归入仓库内: {}",
            state.detail
        );

        // 仓库之外的悬空链接则必须是 ForeignLink.
        let outside = os::normalize(&f.repo.parent().expect("临时根目录").join("outside/gone"));
        let dangling = f.home_path(".config/dangling");
        symlink(&outside, &dangling).expect("建立链接");
        let state = f.state("nvim/init.lua", &dangling, OnConflict::Skip);
        assert_eq!(state.current, Current::ForeignLink);
    }

    #[test]
    fn symlink_shapes_are_partitioned_by_link_ownership() {
        let f = Fixture::new();
        let source = f.write_source("shell/bashrc", "hello");
        let sibling = f.write_source("shell/zshrc", "other");
        let outside = f.outside_path("zshrc");

        let cases = [
            (source, Current::LinkedCorrect),
            (sibling, Current::LinkedWrong),
            (outside, Current::ForeignLink),
        ];

        for (index, (link_text, expected)) in cases.into_iter().enumerate() {
            let target = f.home_path(&format!("shape{index}"));
            symlink(&link_text, &target).expect("建立链接");
            let state = f.state("shell/bashrc", &target, OnConflict::Skip);
            assert_eq!(
                state.current,
                expected,
                "{} 应判定为 {:?}, 实际为 {:?}",
                link_text.display(),
                expected,
                state.current
            );
        }
    }

    #[test]
    fn normalize_trap_under_tmp_is_still_linked_correct() {
        // macOS 上 /tmp 是指向 /private/tmp 的符号链接 (research.md R2 的实测情形):
        // 链接文本含 `..` 且两侧都过同一个非严格 normalize 时必须仍是 LinkedCorrect.
        let Some(dir) = tempfile::Builder::new().tempdir_in("/tmp").ok() else {
            println!(
                "跳过 normalize_trap_under_tmp_is_still_linked_correct: 无法在 /tmp 下创建临时目录"
            );
            return;
        };
        let f = Fixture::inside(dir);
        let source = f.write_source("shell/bashrc", "hello");

        // 链接文本刻意写成 <repo>/../<repo-name>/shell/bashrc 的形态.
        let raw = f.repo.join("..").join("repo").join("shell/bashrc");
        assert_ne!(raw, source, "夹具必须让链接文本含有 .. 段");
        let target = f.home_path(".bashrc");
        symlink(&raw, &target).expect("建立链接");

        let state = f.state("shell/bashrc", &target, OnConflict::Skip);
        assert_eq!(
            state.current,
            Current::LinkedCorrect,
            "含 .. 的链接文本必须与 source_path 落在同一套规范化语义上: {}",
            state.detail
        );
    }

    // --- 3.2 的动作映射 ---

    #[test]
    fn action_mapping_follows_the_table() {
        let cases = [
            (
                Current::SourceMissing,
                OnConflict::Skip,
                Action::ReportMissingSource,
            ),
            (
                Current::SourceMissing,
                OnConflict::ReplaceAndBackup,
                Action::ReportMissingSource,
            ),
            (
                Current::Unreadable,
                OnConflict::Skip,
                Action::ReportUnreadable,
            ),
            (
                Current::Unreadable,
                OnConflict::ReplaceAndBackup,
                Action::ReportUnreadable,
            ),
            (Current::Absent, OnConflict::Skip, Action::Create),
            (
                Current::Absent,
                OnConflict::ReplaceAndBackup,
                Action::Create,
            ),
            (Current::LinkedCorrect, OnConflict::Skip, Action::Noop),
            (
                Current::LinkedCorrect,
                OnConflict::ReplaceAndBackup,
                Action::Noop,
            ),
            (
                Current::LinkedWrong,
                OnConflict::Skip,
                Action::RefuseConflict,
            ),
            (
                Current::LinkedWrong,
                OnConflict::ReplaceAndBackup,
                Action::ReplaceAndBackup,
            ),
            (
                Current::ForeignLink,
                OnConflict::Skip,
                Action::RefuseConflict,
            ),
            (
                Current::ForeignLink,
                OnConflict::ReplaceAndBackup,
                Action::ReplaceAndBackup,
            ),
            (
                Current::ForeignFile,
                OnConflict::Skip,
                Action::RefuseConflict,
            ),
            (
                Current::ForeignFile,
                OnConflict::ReplaceAndBackup,
                Action::ReplaceAndBackup,
            ),
            (
                Current::ForeignDir,
                OnConflict::Skip,
                Action::RefuseConflict,
            ),
            (
                Current::ForeignDir,
                OnConflict::ReplaceAndBackup,
                Action::ReplaceAndBackup,
            ),
        ];

        for (current, on_conflict, expected) in cases {
            assert_eq!(
                action_for(current, on_conflict),
                expected,
                "{:?} + {:?} 的动作不符",
                current,
                on_conflict
            );
        }
    }

    #[test]
    fn classify_reports_one_state_per_entry() {
        let f = Fixture::new();
        let first = f.entry("shell/bashrc", &f.home_path("a"), OnConflict::Skip);
        let second = f.entry("shell/zshrc", &f.home_path("b"), OnConflict::Skip);
        let manifest = Manifest {
            repo_root: f.repo.clone(),
            entries: vec![first, second],
        };
        assert_eq!(classify(&manifest).len(), 2);
    }

    // =======================================================================
    // T040 不变量回归
    //
    // `data-model.md` 3.1 / 3.2 的**逐格**覆盖已由 T047 在上面完成, 本节不重复
    // 逐格用例, 只补四类在实现过程中暴露出的**不变量** (3.1 的"求值语义"第 1 条
    // 与 3.2a, `research.md` R2, `data-model.md` 3.4):
    //
    //   1. 互斥且穷尽   -- 8 种 `Current` 恰好命中一种; `Current` x `on_conflict`
    //                      的每一种组合都落在 3.2 表内;
    //   2. 判定顺序     -- 先判目标侧就会退化的两个反例 (相对 T047 的正面断言,
    //                      这里补的是"目标侧为**符号链接**"的那一格, 以及
    //                      **两条反例一起**成立所给出的"与目标侧状态无关");
    //   3. 迁移自洽     -- 3.4 的迁移路径逐条走一遍, 且 `SourceMissing` 是终态;
    //   4. 规范化同源   -- 仓库根路径含 `..` 段时仍为 `LinkedCorrect`, 条目必须由
    //                      `entry::resolve` / `manifest::parse` 产出.
    // =======================================================================

    // --- T040.1 互斥且穷尽 ---

    /// 3.1 的八种取值, 供穷尽性断言使用.
    const ALL_CURRENTS: [Current; 8] = [
        Current::SourceMissing,
        Current::Absent,
        Current::LinkedCorrect,
        Current::LinkedWrong,
        Current::ForeignLink,
        Current::ForeignFile,
        Current::ForeignDir,
        Current::Unreadable,
    ];

    /// 全部可构造的**符号链接**情形, 返回它们取到的 `Current` (去重后恰为 3 种).
    ///
    /// 五种链接文本形态 (指向正确 / 仓库内指错 / 仓库内悬空 / 仓库外 / 仓库外悬空)
    /// 的**期望取值**在此一并锁定; 返回值是实际取到的取值集合, 供调用方断言
    /// "符号链接情形只落在 `LinkedCorrect` / `LinkedWrong` / `ForeignLink` 三者之内".
    ///
    /// T047 的 `symlink_shapes_are_partitioned_by_link_ownership` 覆盖了前三种形态,
    /// 两种悬空形态则散落于 `a_dangling_link_inside_the_repo_is_linked_wrong` 的两半.
    /// 这里把它们收进同一组, 并补上 T047 没有的那条断言: **取值集合的大小与内容**.
    /// 若两条归属分支被合并 (例如让 `LinkedWrong` 收下所有指错的链接), 集合会塌缩,
    /// 这条断言即失败.
    fn symlink_currents() -> [Current; 3] {
        let f = Fixture::new();
        let source = f.write_source("shell/bashrc", "hello");
        let sibling = f.write_source("shell/zshrc", "other");
        let outside = f.outside_path("zshrc");
        let gone_inside = os::normalize(&f.repo.join("shell/gone"));
        let gone_outside =
            os::normalize(&f.repo.parent().expect("临时根目录").join("outside/gone"));

        // 链接文本 -> 该情形应落在的 `Current`.
        let cases = [
            (source, Current::LinkedCorrect),
            (sibling, Current::LinkedWrong),
            (gone_inside, Current::LinkedWrong),
            (outside, Current::ForeignLink),
            (gone_outside, Current::ForeignLink),
        ];

        let mut seen: Vec<Current> = Vec::new();
        for (index, (link_text, expected)) in cases.into_iter().enumerate() {
            let target = f.home_path(&format!("symlink_shape_{index}"));
            symlink(&link_text, &target).expect("建立链接");
            let state = f.state("shell/bashrc", &target, OnConflict::Skip);
            assert_eq!(
                state.current,
                expected,
                "{} 应判定为 {:?}",
                link_text.display(),
                expected
            );
            if !seen.contains(&expected) {
                seen.push(expected);
            }
        }

        assert_eq!(
            seen.len(),
            3,
            "符号链接情形必须恰好落在 LinkedCorrect / LinkedWrong / ForeignLink 三者之内, \
             实际取到 {seen:?}"
        );

        [seen[0], seen[1], seen[2]]
    }

    #[test]
    fn a_symlink_never_lands_in_both_linked_wrong_and_foreign_link() {
        let seen = symlink_currents();
        assert_eq!(
            seen,
            [
                Current::LinkedCorrect,
                Current::LinkedWrong,
                Current::ForeignLink,
            ],
            "符号链接情形的取值集合"
        );
        assert_ne!(
            seen[1], seen[2],
            "同一情形不得同时落入 LinkedWrong 与 ForeignLink"
        );
    }

    /// 八种 `Current` **恰好命中一种** (互斥且穷尽), 且以独立场景逐一取到.
    ///
    /// 八个场景的 `Current` 期望值在此逐条锁定, 且每次都**无条件**压入 `seen`:
    /// 因此末尾的 `seen.len() == 8` 同时证明了覆盖与去重 -- 任何让两个场景塌缩到
    /// 同一取值的实现 (例如 `Unreadable` 吸收 `NotFound`, 或两条归属分支合并) 都会
    /// 使长度小于 8.
    ///
    /// `Unreadable` 用**不依赖权限位**的方式构造: 目标路径穿过一个普通文件
    /// (`<home>/occupier/rc`), `lstat` 因此返回 `NotADirectory` -- 它是 `NotFound`
    /// **之外**的错误, 因此在 root 下也确定生效. 这与 T047 用 chmod `000` 构造的
    /// `EACCES` 是不同的触发路径, 且断言不再需要"权限位生效"这一前提.
    #[test]
    fn the_eight_current_values_are_pairwise_distinct() {
        assert_eq!(ALL_CURRENTS.len(), 8, "3.1 的取值集合不得增删");

        let f = Fixture::new();
        let source = f.write_source("shell/bashrc", "hello");
        let sibling = f.write_source("shell/zshrc", "other");
        let outside = f.outside_path("zshrc");

        // 目标路径上放一个普通文件, 使其下的路径 lstat 得到 ENOTDIR.
        let occupier = f.home_path("occupier");
        fs::write(&occupier, "user own content").expect("写入穿过文件");

        let symlinked = f.home_path("shape_link");
        symlink(&source, &symlinked).expect("建立链接");
        let wrong = f.home_path("shape_wrong");
        symlink(&sibling, &wrong).expect("建立链接");
        let foreign = f.home_path("shape_foreign");
        symlink(&outside, &foreign).expect("建立链接");
        let plain = f.home_path("shape_file");
        fs::write(&plain, "user own content").expect("写入占用物");
        let directory = f.home_path("shape_dir");
        fs::create_dir_all(&directory).expect("创建占用目录");

        // 场景按 (source 相对路径, 目标路径, 期望 Current) 列举, 一次一个取值.
        let cases: [(&str, PathBuf, Current); 8] = [
            (
                "gone/rc",
                f.home_path("shape_missing_source"),
                Current::SourceMissing,
            ),
            ("shell/bashrc", f.home_path("shape_absent"), Current::Absent),
            ("shell/bashrc", symlinked, Current::LinkedCorrect),
            ("shell/bashrc", wrong, Current::LinkedWrong),
            ("shell/bashrc", foreign, Current::ForeignLink),
            ("shell/bashrc", plain, Current::ForeignFile),
            ("shell/bashrc", directory, Current::ForeignDir),
            (
                "shell/bashrc",
                os::normalize(&occupier.join("rc")),
                Current::Unreadable,
            ),
        ];

        let mut seen: Vec<Current> = Vec::new();
        for (source_rel, target, expected) in &cases {
            let state = f.state(source_rel, target, OnConflict::Skip);
            assert_eq!(
                state.current,
                *expected,
                "{} 应判定为 {expected:?}: {}",
                target.display(),
                state.detail
            );
            seen.push(state.current);
        }

        for current in ALL_CURRENTS {
            assert!(
                seen.contains(&current),
                "{current:?} 未被任何场景取到, 3.1 的取值集合不是穷尽的: {seen:?}"
            );
        }
        assert_eq!(
            seen.len(),
            ALL_CURRENTS.len(),
            "八个场景必须恰好命中八个互不相同的取值: {seen:?}"
        );
    }

    /// "每种 `Current` 恰好命中所属的那一类动作"的可自动化形式: 每个
    /// `Current` x `on_conflict` 的组合都必须落在 3.2 表内, 不存在"配到表外
    /// `Action`"的情形.
    ///
    /// T047 的 `action_mapping_follows_the_table` 逐格断言了 8 x 2 的具体取值;
    /// 这里断言的是**补集** -- 组合的**结构**而不是取值本身:
    ///
    /// - 3.2 表中带 "-" (与 `on_conflict` 无关) 的四种取值: 两个 `on_conflict`
    ///   必须给出**同一个**动作, 且该动作不得是冲突类的两个之一;
    /// - 冲突类的四种取值: 两个 `on_conflict` 必须给出**恰好一对**冲突动作.
    #[test]
    fn every_current_lands_inside_the_action_table() {
        assert_eq!(ALL_CURRENTS.len(), 8, "3.2 表的左侧必须恰有 8 种 Current");

        for current in ALL_CURRENTS {
            let skip = action_for(current, OnConflict::Skip);
            let replace = action_for(current, OnConflict::ReplaceAndBackup);
            let conflict_action = matches!(skip, Action::RefuseConflict | Action::ReplaceAndBackup);

            match current {
                Current::SourceMissing
                | Current::Unreadable
                | Current::Absent
                | Current::LinkedCorrect => {
                    assert!(
                        !conflict_action,
                        "{current:?} 在 3.2 表中与 on_conflict 无关, 不得落到冲突动作 ({skip:?})"
                    );
                    assert_eq!(
                        skip, replace,
                        "{current:?} 与 on_conflict 无关, 两种策略必须给出同一个动作"
                    );
                }
                Current::LinkedWrong
                | Current::ForeignLink
                | Current::ForeignFile
                | Current::ForeignDir => {
                    assert_eq!(
                        skip,
                        Action::RefuseConflict,
                        "3.2 表: {current:?} + skip 必须是 RefuseConflict"
                    );
                    assert_eq!(
                        replace,
                        Action::ReplaceAndBackup,
                        "3.2 表: {current:?} + replace_and_backup 必须是 ReplaceAndBackup"
                    );
                }
            }
        }
    }

    /// 3.2 的六个动作恰好覆盖全部 `Current` x `on_conflict` 组合的取值集合.
    #[test]
    fn the_action_set_is_closed_over_all_currents() {
        let mut reachable: Vec<Action> = Vec::new();
        for current in ALL_CURRENTS {
            for on_conflict in [OnConflict::Skip, OnConflict::ReplaceAndBackup] {
                let action = action_for(current, on_conflict);
                if !reachable.contains(&action) {
                    reachable.push(action);
                }
            }
        }
        // 3.2 的副作用一列共六种动作, 全部可达; 且没有第七种.
        for expected in [
            Action::ReportMissingSource,
            Action::ReportUnreadable,
            Action::Create,
            Action::Noop,
            Action::RefuseConflict,
            Action::ReplaceAndBackup,
        ] {
            assert!(
                reachable.contains(&expected),
                "{expected:?} 在 3.2 表中不可达"
            );
        }
        assert_eq!(
            reachable.len(),
            6,
            "8 种 Current x 2 种 on_conflict 只能导出 3.2 表的六种动作: {reachable:?}"
        );
    }

    // --- T040.2 判定顺序的负面断言 ---

    /// 若实现被改成先判目标侧, "源缺失 + 目标被占用"会退化为 `conflict`.
    ///
    /// T047 的 `source_missing_wins_over_an_occupied_target` 只覆盖了"目标被**普通
    /// 文件**占用"这一格; 这里补的是"目标侧是**符号链接**"(对应 `conflict` 分支),
    /// 且两条反例一并断言结果与目标侧的状态**无关** -- 这正是"判定顺序"这一不变量的
    /// 内容, 而非某一格的具体取值.
    #[test]
    fn source_missing_is_decided_before_the_target_side() {
        let f = Fixture::new();
        f.write_source("shell/bashrc", "hello");
        let elsewhere = f.write_source("shell/zshrc", "other");

        // 四种目标侧状态: 不存在 / 被普通文件占用 / 是符号链接 / 是目录.
        let absent = f.home_path("order_absent");
        let plain = f.home_path("order_plain");
        fs::write(&plain, "user own content").expect("写入占用物");
        let link = f.home_path("order_link");
        symlink(&elsewhere, &link).expect("建立链接");
        let directory = f.home_path("order_dir");
        fs::create_dir_all(&directory).expect("创建占用目录");

        for on_conflict in [OnConflict::Skip, OnConflict::ReplaceAndBackup] {
            for target in [&absent, &plain, &link, &directory] {
                let state = f.state("gone/rc", target, on_conflict);
                assert_eq!(
                    state.current,
                    Current::SourceMissing,
                    "源缺失时结果必须与目标侧状态 ({}) 无关, 实际为 {:?}",
                    target.display(),
                    state.current
                );
                assert_eq!(state.action, Action::ReportMissingSource);
                assert!(
                    !state.detail.contains("占用"),
                    "不得给出目标侧占用物的说明: {}",
                    state.detail
                );
            }
        }

        // 占位对照: 源存在时同一批目标路径**确实**会落到目标侧的那些取值.
        // 没有这一半, 上面的断言可能因为"目标侧根本不参与判定"而恒真.
        assert_eq!(
            f.state("shell/bashrc", &plain, OnConflict::Skip).current,
            Current::ForeignFile
        );
        assert_eq!(
            f.state("shell/bashrc", &link, OnConflict::Skip).current,
            Current::LinkedWrong
        );
        assert_eq!(
            f.state("shell/bashrc", &directory, OnConflict::Skip)
                .current,
            Current::ForeignDir
        );
        assert_eq!(
            f.state("shell/bashrc", &absent, OnConflict::Skip).current,
            Current::Absent
        );
    }

    /// `Unreadable` 只吸收 `lstat` 的**非 `NotFound`** 错误, 不吸收"路径不存在".
    ///
    /// T047 的 `not_found_is_never_unreadable` 覆盖了 `SourceMissing` / `Absent`
    /// 两侧; 这里把它扩成"同一批缺失路径在**两种** `on_conflict` 下都不得为
    /// `Unreadable`", 并把"非 `NotFound` 的那一侧才是 `Unreadable`"这一互补结论
    /// 放在同一个用例里对照.
    #[test]
    fn not_found_is_absorbed_only_by_the_two_missing_states() {
        let f = Fixture::new();
        f.write_source("shell/bashrc", "hello");

        // 三种"路径不存在"的形态, 无一可以落到 Unreadable.
        let cases: [(&str, PathBuf, Current); 3] = [
            ("shell/bashrc", f.home_path("nf_absent"), Current::Absent),
            ("shell/bashrc", f.home_path("nf/deep/leaf"), Current::Absent),
            (
                "gone/rc",
                f.home_path("nf_missing_source"),
                Current::SourceMissing,
            ),
        ];
        for on_conflict in [OnConflict::Skip, OnConflict::ReplaceAndBackup] {
            for (source, target, expected) in &cases {
                let state = f.state(source, target, on_conflict);
                assert_eq!(
                    state.current,
                    *expected,
                    "{} 不存在必须落入 {expected:?}",
                    target.display()
                );
                assert_ne!(
                    state.current,
                    Current::Unreadable,
                    "NotFound 不得被 Unreadable 吸收: {}",
                    state.detail
                );
                assert_ne!(state.action, Action::ReportUnreadable);
            }
        }

        // 互补的一面: 非 NotFound 的失败才是 Unreadable (EACCES).
        let blocked = f.home_path("nf_blocked");
        fs::create_dir_all(&blocked).expect("创建被阻断的目录");
        let unreachable = os::normalize(&blocked.join("rc"));
        chmod(&blocked, 0o000);
        if !permissions_are_ignored(&blocked) {
            let state = f.state("shell/bashrc", &unreachable, OnConflict::Skip);
            assert_eq!(state.current, Current::Unreadable);
        }
        chmod(&blocked, 0o755);
    }

    // --- T040.3 迁移表自洽 (data-model.md 3.4) ---

    /// 3.4 的迁移路径逐条走一遍, 每次迁移后重新调用 `classify` 并断言 `Current`.
    ///
    /// 全部迁移都用**真实文件系统操作**模拟 (而不只是断言枚举本身), 因此它同时
    /// 覆盖了"判定是只读的"这一前提: 每次判定都只看实况, 不依赖上一次的结论.
    #[test]
    fn the_transition_graph_is_consistent() {
        let f = Fixture::new();
        let source = f.write_source("shell/bashrc", "hello");
        let sibling = f.write_source("shell/zshrc", "other");
        let outside = f.outside_path("zshrc");
        let target = f.home_path(".bashrc");

        // [Absent] --apply/Create--> [LinkedCorrect]
        assert_eq!(
            f.state("shell/bashrc", &target, OnConflict::Skip).current,
            Current::Absent
        );
        os::create_symlink(&source, &target).expect("模拟 apply/Create");
        assert_eq!(
            f.state("shell/bashrc", &target, OnConflict::Skip).current,
            Current::LinkedCorrect
        );

        // [LinkedCorrect] --手工改坏--> [ForeignFile] / [ForeignDir] / [ForeignLink]
        // 每一次都从 LinkedCorrect 出发, 因此改坏之后必须重新摆回正确链接.
        let broken: [(&str, Current); 3] = [
            ("migrate_file", Current::ForeignFile),
            ("migrate_dir", Current::ForeignDir),
            ("migrate_link", Current::ForeignLink),
        ];
        for (shape, expected) in broken {
            let broken_target = f.home_path(shape);
            os::create_symlink(&source, &broken_target).expect("先摆成 LinkedCorrect");
            assert_eq!(
                f.state("shell/bashrc", &broken_target, OnConflict::Skip)
                    .current,
                Current::LinkedCorrect
            );

            fs::remove_file(&broken_target).expect("移除正确链接");
            match expected {
                Current::ForeignFile => {
                    fs::write(&broken_target, "user own content").expect("手工改成普通文件");
                }
                Current::ForeignDir => {
                    fs::create_dir_all(&broken_target).expect("手工改成目录");
                }
                _ => {
                    symlink(&outside, &broken_target).expect("手工改成外部链接");
                }
            }
            let state = f.state("shell/bashrc", &broken_target, OnConflict::Skip);
            assert_eq!(
                state.current, expected,
                "LinkedCorrect 被手工改坏后必须落到 {expected:?}"
            );
            assert_eq!(state.action, Action::RefuseConflict);

            // --apply/ReplaceAndBackup--> 回到 [LinkedCorrect].
            let state = f.state("shell/bashrc", &broken_target, OnConflict::ReplaceAndBackup);
            assert_eq!(
                state.action,
                Action::ReplaceAndBackup,
                "{expected:?} 在 replace_and_backup 下必须备份替换"
            );
            if expected == Current::ForeignDir {
                fs::remove_dir(&broken_target).expect("移除占用目录");
            } else {
                fs::remove_file(&broken_target).expect("移除占用物");
            }
            os::create_symlink(&source, &broken_target).expect("模拟 apply/ReplaceAndBackup");
            assert_eq!(
                f.state("shell/bashrc", &broken_target, OnConflict::Skip)
                    .current,
                Current::LinkedCorrect,
                "ReplaceAndBackup 之后必须回到 LinkedCorrect"
            );
        }

        // [LinkedWrong] 也是"本工具的链接", 同一条回边回到 LinkedCorrect.
        fs::remove_file(&target).expect("移除 target 上的正确链接");
        os::create_symlink(&sibling, &target).expect("摆成 LinkedWrong");
        assert_eq!(
            f.state("shell/bashrc", &target, OnConflict::Skip).current,
            Current::LinkedWrong
        );
        fs::remove_file(&target).expect("移除错误链接");
        os::create_symlink(&source, &target).expect("重新建立正确链接");
        assert_eq!(
            f.state("shell/bashrc", &target, OnConflict::Skip).current,
            Current::LinkedCorrect
        );

        // [Absent] --undo (移除链接, 若有备份则恢复)--> [Absent] (幂等回边).
        fs::remove_file(&target).expect("模拟 undo");
        assert_eq!(
            f.state("shell/bashrc", &target, OnConflict::Skip).current,
            Current::Absent
        );
    }

    /// `SourceMissing` 是**终态**: 在源条目被补回仓库之前, 任何目标侧改动都不
    /// 改变它; 补回源之后它按实况转为 `Absent` 或冲突类, 再建立链接即为
    /// `LinkedCorrect`.
    #[test]
    fn source_missing_is_a_terminal_state_until_the_source_returns() {
        let f = Fixture::new();
        let source = f.write_source("shell/bashrc", "hello");
        let target = f.home_path(".bashrc");

        // 先把目标侧摆成 LinkedCorrect, 再删掉源文件: 判定必须立刻转为 SourceMissing.
        os::create_symlink(&source, &target).expect("建立链接");
        assert_eq!(
            f.state("shell/bashrc", &target, OnConflict::Skip).current,
            Current::LinkedCorrect
        );
        fs::remove_file(&source).expect("删除源文件");
        assert_eq!(
            f.state("shell/bashrc", &target, OnConflict::Skip).current,
            Current::SourceMissing,
            "源被删除后, 悬空链接不得被读作 LinkedCorrect / LinkedWrong"
        );

        // 源缺失期间: 目标侧换成普通文件也改变不了它.
        fs::remove_file(&target).expect("移除链接");
        fs::write(&target, "user own content").expect("换成普通文件");
        assert_eq!(
            f.state("shell/bashrc", &target, OnConflict::Skip).current,
            Current::SourceMissing,
            "源缺失是终态: 目标侧被占用也不得转为冲突类"
        );

        // 补回源: 目标侧被占用 -> 立刻转为冲突类 (不再是 SourceMissing).
        fs::write(&source, "hello").expect("补回源文件");
        assert_eq!(
            f.state("shell/bashrc", &target, OnConflict::Skip).current,
            Current::ForeignFile,
            "补回源后必须立刻按目标侧实况重新判定"
        );

        // 目标侧不存在 -> 转为 Absent; 再建立链接 -> 转为 LinkedCorrect.
        fs::remove_file(&target).expect("移除占用物");
        assert_eq!(
            f.state("shell/bashrc", &target, OnConflict::Skip).current,
            Current::Absent
        );
        os::create_symlink(&source, &target).expect("建立链接");
        assert_eq!(
            f.state("shell/bashrc", &target, OnConflict::Skip).current,
            Current::LinkedCorrect
        );
    }

    // --- T040.4 os::normalize 的三个调用点同源 (research.md R2) ---

    /// 仓库根路径含 `..` 段时, 链接必须仍判定为 `LinkedCorrect`.
    ///
    /// 关键在于条目必须走 `manifest::parse` 的解析路径: 手工拼 `Entry` 字面量会绕过
    /// `entry.rs` 对 `source_path` 的规范化, 而那正是本用例要钉住的一环
    /// (T047 的 `normalize_trap_under_tmp_is_still_linked_correct` 走的正是字面量).
    ///
    /// 构造: 临时根 `<tmp>` 下建 `<tmp>/repo` 并写入 `shell/bashrc`; 部署链接时链接
    /// 文本取**规范化后**的源路径 `<tmp>/repo/shell/bashrc` (即 `ln -sfn $PWD/shell/bashrc`
    /// 的产物). 关键在于传给 `parse` 的 `repo_root` 写成 `<tmp>/repo/../repo` 而非
    /// `<tmp>/repo`: 该形态在未规范化时与 `<tmp>/repo` 是**两个不同的字符串**, 于是
    /// `S = repo_root + "shell/bashrc"` (未过 `normalize` 时) 与已规范化的 `L` 不相等.
    /// 两侧都过同一个 `os::normalize` 时 `L == S` 成立, 因此本用例断言 `LinkedCorrect`;
    /// 若 `source_path` 未过 `normalize` (或只规范化了一侧), 本用例必失败 -- 这正是
    /// 它的回归价值 (`research.md` R2 的第 3 个调用点).
    ///
    /// 两层断言, 缺一不可 -- 说明为何不能只留 `classify` 那一层:
    ///
    /// 1. `classify` 层 (`current == LinkedCorrect`): 这是本用例的实质断言, 对应
    ///    T040 的"三个调用点同源". 但 `classify` 内部会把两侧都过一遍 `os::normalize`
    ///    (`link_destination` 与 `let source = os::normalize(...)`), 因此当
    ///    `normalize` 是幂等的, 它**无法**区分"`source_path` 在解析阶段就已规范化"
    ///    与"`source_path` 是字面拼接, 由 `classify` 兜底规范化"; 后者会让同一个
    ///    路径被规范化两次而结论不变. 只靠这一层, 本用例会在 `entry.rs` 漏掉
    ///    `normalize` 时仍然通过 (实测已复现).
    /// 2. 解析层 (`manifest.entries[0].source_path == source`): 直接钉住解析产物
    ///    本身, 是唯一能捕捉"`source_path` 未过 `normalize`"的断言
    ///    (`research.md` R2 的第 3 个调用点). 与上一层的差别只在**规范化发生的位置**,
    ///    而这正是 T040 要钉住的内容.
    ///
    /// 不依赖机器上真实存在的符号链接, 也不假设 `normalize` 是纯词法的: 只依赖
    /// "`L` 与 `S` 共用同一个 `os::normalize`" 这一属性.
    #[test]
    fn a_moved_repository_stays_linked_correct() {
        let dir = tempfile::tempdir().expect("创建临时根");
        let repo = os::normalize(&dir.path().join("repo"));
        let home = os::normalize(&dir.path().join("home"));
        fs::create_dir_all(repo.join("shell")).expect("创建仓库根");
        fs::create_dir_all(&home).expect("创建临时主目录");

        let source = os::normalize(&repo.join("shell/bashrc"));
        fs::write(&source, "hello").expect("写入源文件");

        // 部署: 链接文本取规范化后的源路径 (本工具的链接文本形态).
        let target = os::normalize(&home.join(".bashrc"));
        symlink(&source, &target).expect("建立链接");

        // 传给 `parse` 的仓库根刻意含 `..` 段. 先证明这一形态真的被制造出来了:
        // 未规范化时它与 `repo` 是两个不同的字符串.
        let indirect = repo.join("..").join("repo");
        assert_ne!(indirect, repo, "夹具必须让仓库根路径含有 .. 段");
        assert_eq!(
            os::normalize(&indirect),
            repo,
            "含 .. 段的仓库根与真实仓库根必须规范化到同一路径"
        );

        // `target` 取绝对路径: 相对 target 也以仓库根为基准 (R5), 那会把本用例的
        // 焦点从 `source_path` 的规范化上移开.
        let text = format!(
            "[[entry]]\nsource = \"shell/bashrc\"\ntarget = \"{}\"\n",
            target.display()
        );

        // 真实解析路径: 相对 source 由 `entry::resolve` 挂到仓库根上并过 `os::normalize`.
        let manifest = manifest::parse(&text, &indirect).expect("清单合法");
        // 这一层直接钉住解析产物本身: `classify` 内部会把两侧都再规范化一次, 因此
        // 它无法区分"解析阶段已规范化"与"字面拼接, 由 classify 兜底"; 只有此处能
        // 捕捉 `entry.rs` 漏掉 `normalize` 的缺陷 (实测: 去掉该断言后, 把
        // `entry.rs` 的 `source_path` 改成 `expand(...)` 字面拼接, 本用例仍会通过).
        assert_eq!(
            manifest.entries[0].source_path, source,
            "source_path 必须是规范化后的绝对路径, 而不是 repo_root + source 的字面拼接"
        );
        // 不复用 `resolve_puts_both_sides_on_the_same_normal_form` 的 `target_path`
        // 断言: 这里的 `target` 自身是符号链接, 其形态属 `normalize` 的语义范围,
        // 与本用例要钉住的"source_path 同源"无关.

        let states = classify(&manifest);
        assert_eq!(
            states[0].current,
            Current::LinkedCorrect,
            "仓库根含 .. 段时, 链接文本解析出的 L 与 source_path 的 S 必须落在同一套 \
             规范化语义上, 否则本用例失败 (错误: 得到 {:?}, 说明: {})",
            states[0].current,
            states[0].detail
        );
        assert_eq!(states[0].action, Action::Noop, "已是最新的链接必须是 Noop");
    }

    /// 同一断言的最小化对照: `entry::resolve` 也必须让两侧同源.
    ///
    /// 上一条依赖 `manifest::parse` 的完整路径; 这一条直接喂给 `resolve` 一个
    /// **含 `..` 与符号链接段**的仓库根 (即 macOS 上 `/var` -> `/private/var` 的
    /// 现实形态), 断言 `target_path` 与 `source_path` 落在同一套规范化语义上.
    #[test]
    fn resolve_puts_both_sides_on_the_same_normal_form() {
        let f = Fixture::new();

        // 以 `<repo>/../<repo-name>` 形态的仓库根解析: 未规范化时它与 `repo` 不同.
        let indirect = f.repo.join("..").join("repo");
        assert_ne!(indirect, f.repo, "夹具必须让仓库根含 .. 段");
        let entry = entry::resolve(
            "./shell/bashrc",
            "../home/.bashrc",
            OnConflict::Skip,
            &indirect,
        )
        .expect("条目解析失败");
        assert_eq!(
            entry.source_path,
            os::normalize(&f.repo.join("shell/bashrc"))
        );
        assert_eq!(
            entry.target_path,
            os::normalize(&f.home.join(".bashrc")),
            "target_path 也必须过同一个 os::normalize"
        );
    }
}

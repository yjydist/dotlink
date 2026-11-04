//! 条目与路径解析 (research.md R5).

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::os;

/// 目标被非本工具内容占用时的策略.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OnConflict {
    #[default]
    Skip,
    ReplaceAndBackup,
}

/// 清单中的一个条目, 含解析后的绝对路径.
///
/// `source` / `target` 保留用户书写的原始字符串 (报告中展示它, 便于对照清单);
/// `source_path` / `target_path` 是绝对路径且**均已过 `os::normalize`** -- 只规范化
/// 一侧会使 `classify` 的 `L == S` 比较落在两套路径语义上 (`research.md` R2).
#[derive(Debug, Clone)]
pub struct Entry {
    pub source: String,
    pub target: String,
    pub on_conflict: OnConflict,
    pub source_path: PathBuf,
    pub target_path: PathBuf,
}

/// 测试用的单条目便捷入口: 内部把仓库根与主目录各规范化一次.
///
/// 生产路径 (逐条目) 走 [`resolve_with`], 复用循环外算好的形态; 本函数只为单测
/// 提供"一步解析"的写法, 不参与生产二进制.
#[cfg(test)]
pub fn resolve(
    source: &str,
    target: &str,
    on_conflict: OnConflict,
    repo_root: &Path,
) -> Result<Entry> {
    let repo = os::normalize(repo_root);
    let home = home().map(|path| os::normalize(&path));
    resolve_with(
        source,
        target,
        on_conflict,
        repo_root,
        &repo,
        home.as_deref(),
    )
}

/// 由调用方传入已规范化的仓库根与主目录, 复用清单解析上下文.
///
/// `target` 支持主目录简写 (`~` / `~/...`), 绝对路径与相对仓库根的路径; `source` 只
/// 接受仓库内的相对路径, 且解析与归一化后不得越出仓库根. 两侧路径形态的裁决分别是
/// `expand_target` / `expand_source`, 越界判定在归一化之后.
///
/// `repo` 必须等于 `os::normalize(repo_root)`; `home` 是已规范化的有效主目录或错误.
/// `manifest::parse` 在整个条目循环之外只算一次, 因此条目
/// 再多也不会重复规范化这两个固定路径 (它们与条目数无关).
pub(crate) fn resolve_with(
    source: &str,
    target: &str,
    on_conflict: OnConflict,
    repo_root: &Path,
    repo: &Path,
    home: std::result::Result<&Path, &Error>,
) -> Result<Entry> {
    let target_path = os::normalize(&expand_target(target, repo_root, home)?);
    let source_path = os::normalize(&expand_source(source, repo_root)?);

    // 源条目解析与归一化后必须仍落在仓库根之内: `..` 段或仓库内的符号链接能把
    // `repo_root.join(...)` 的结果带出仓库. 越出仓库的源建出的链接指向仓库外, 而
    // `undo` 的归属判定要求链接文本落在仓库内才算本工具创建 (`os::is_inside_repo`),
    // 于是自己的产物会被自己拒删 (`undo.rs` 的不变量). 与 `~` 前缀 / 绝对路径同一类
    // 整体拒绝. 两侧都过 `os::normalize` 再比较: 祖先段解析符号链接的语义必须与归属
    // 判定完全一致, 末段不解析 (`normalize` 的既有语义).
    if !source_path.starts_with(repo) {
        return Err(Error::ManifestInvalid(format!(
            "`{source}` 解析后落在仓库根之外: 源条目必须落在仓库根之内 (归一化后为 `{}`)",
            source_path.display()
        )));
    }

    Ok(Entry {
        source: source.to_string(),
        target: target.to_string(),
        on_conflict,
        source_path,
        target_path,
    })
}

/// 解析 `target` 为未规范化的绝对路径.
///
/// 规则 (R5): `~/...` 与裸 `~` 展开为主目录; 绝对路径原样; 相对路径挂到仓库根.
/// 以 `~` 开头但不以 `~/` 开头 (例如漏写斜杠的 `~.bashrc`, 或指其他用户的 `~foo/bar`)
/// 既非主目录简写也不是绝对路径 -- 若放行会落到"相对仓库根"分支, 在受版本控制的
/// 仓库里静默建出名为 `~.bashrc` 的路径 (`contracts/manifest.md` 的 V-6). 与 V-8 的
/// 立场一致, 一律整体拒绝并指出书写形态.
fn expand_target(
    written: &str,
    repo_root: &Path,
    home: std::result::Result<&Path, &Error>,
) -> Result<PathBuf> {
    if let Some(rest) = written.strip_prefix("~/") {
        // `~/` 之后多写的分隔符仍属主目录简写: 剩余段以分隔符开头时若原样交给
        // `PathBuf::join`, 它会被当成绝对路径并**丢弃主目录** (`~//etc/hosts` ->
        // `/etc/hosts`), 把一次笔误变成对主目录之外路径的替换 (V-6).
        let rest = rest.trim_start_matches(std::path::MAIN_SEPARATOR);
        return Ok(require_home(home)?.join(rest));
    }
    if written == "~" {
        return Ok(require_home(home)?.to_path_buf());
    }
    if written.starts_with('~') {
        return Err(Error::ManifestInvalid(format!(
            "`{written}` 以 `~` 开头但不以 `~/` 开头, 无法解析为绝对路径: 主目录简写只支持 `~` 或 `~/...`"
        )));
    }

    let path = Path::new(written);
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    Ok(repo_root.join(path))
}

/// 解析 `source` 为未规范化的绝对路径.
///
/// 源条目必须是仓库内的相对路径 (`contracts/manifest.md` 的字段表; `research.md` R5:
/// "源条目路径始终相对于清单文件所在目录解析"). `~` 前缀与绝对路径都不属于"仓库内
/// 的相对路径", 按 V-4 / V-6 整体拒绝, 以维持"条目来自仓库"这一心智模型. 词法拼接
/// 之后仍可能越出仓库 (见 `resolve` 中归一化后的越界判定), 那一半不在此处裁决.
fn expand_source(written: &str, repo_root: &Path) -> Result<PathBuf> {
    if written.starts_with('~') || Path::new(written).is_absolute() {
        return Err(Error::ManifestInvalid(format!(
            "`{written}` 不是仓库内的相对路径: 源条目不能以 `~` 开头, 也不能是绝对路径"
        )));
    }
    Ok(repo_root.join(written))
}

/// `HOME` 缺失时的说明.
const HOME_MISSING: &str = "无法确定主目录: 环境变量 HOME 未设置或为空 (无法把 `~` 解析为绝对路径)";

/// 展开 `~` 所需的当前用户主目录 (原始形态, 未规范化).
///
/// 无法确定时属于 FR-013 的整体拒绝场景 (退出码 2). `manifest` 的 V-9 也用它识别
/// "落点恰是主目录本身"的退化形态, 与 `~` 展开共用同一个来源.
pub(crate) fn home() -> Result<PathBuf> {
    match std::env::var_os("HOME") {
        Some(value) if !value.is_empty() => {
            let path = PathBuf::from(value);
            if !path.is_absolute() {
                return Err(Error::ManifestInvalid(format!(
                    "无法确定主目录: 环境变量 HOME 必须是绝对路径 (实际为 `{}`)",
                    path.display()
                )));
            }
            Ok(path)
        }
        _ => Err(Error::ManifestInvalid(HOME_MISSING.to_string())),
    }
}

/// 取已规范化的主目录; 不可用时保留其具体原因.
///
/// `expand_target` 只在确实需要展开 `~` 时才要求主目录, 因此把"是否存在"与"展开"
/// 分开: 循环外预取一次, 绝对路径与相对路径不受 `HOME` 缺失影响.
fn require_home<'a>(home: std::result::Result<&'a Path, &Error>) -> Result<&'a Path> {
    home.map_err(|error| Error::ManifestInvalid(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> PathBuf {
        PathBuf::from("/repo")
    }

    #[test]
    fn absolute_paths_are_used_as_written() {
        // 绝对路径不依赖仓库根; 断言的是"与实现同源的规范化结果", 而不是某个具体
        // 物理路径: macOS 上 `/etc` 是 `/private/etc` 的符号链接, 把它写成字面量
        // 会把本机细节钉进断言.
        let written = "/etc/bashrc";
        let entry = resolve("shell/bashrc", written, OnConflict::Skip, &repo()).unwrap();
        assert_eq!(entry.target_path, os::normalize(Path::new(written)));
        assert_eq!(
            entry.source_path,
            os::normalize(Path::new("/repo/shell/bashrc"))
        );
    }

    #[test]
    fn relative_paths_are_resolved_against_the_repo_root() {
        let entry = resolve("shell/bashrc", "link/bashrc", OnConflict::Skip, &repo()).unwrap();
        assert_eq!(entry.target_path, PathBuf::from("/repo/link/bashrc"));
        assert_eq!(entry.source_path, PathBuf::from("/repo/shell/bashrc"));
    }

    #[test]
    fn original_strings_are_preserved_for_the_report() {
        let entry = resolve("shell/bashrc", "link/bashrc", OnConflict::Skip, &repo()).unwrap();
        assert_eq!(entry.source, "shell/bashrc");
        assert_eq!(entry.target, "link/bashrc");
    }

    #[test]
    fn tilde_is_expanded_using_home() {
        let Some(home) = std::env::var_os("HOME").filter(|v| !v.is_empty()) else {
            return;
        };
        let home = PathBuf::from(home);

        let entry = resolve("shell/bashrc", "~/.bashrc", OnConflict::Skip, &repo()).unwrap();
        assert_eq!(entry.target_path, os::normalize(&home.join(".bashrc")));

        let bare = resolve("shell/bashrc", "~", OnConflict::Skip, &repo()).unwrap();
        assert_eq!(bare.target_path, os::normalize(&home));
    }

    #[test]
    fn duplicate_separators_after_tilde_still_expand_under_home() {
        // `~//x` 仍属 `~/...`: 剩余段不得被当成绝对路径 (`PathBuf::join` 会丢弃
        // 主目录, 使 `~//etc/hosts` 落到 `/etc/hosts`). 主目录不可用时两种写法同样
        // 报错 -- 它们都需要 `~` 展开, 不属于"绝对路径不受 HOME 影响"的例外.
        let Some(home) = std::env::var_os("HOME").filter(|v| !v.is_empty()) else {
            return;
        };
        let home = PathBuf::from(home);

        for written in ["~//.bashrc", "~///.bashrc"] {
            let entry = resolve("shell/bashrc", written, OnConflict::Skip, &repo()).unwrap();
            assert_eq!(
                entry.target_path,
                os::normalize(&home.join(".bashrc")),
                "{} 必须展开到主目录下",
                written
            );
        }
    }

    #[test]
    fn target_tilde_without_slash_is_rejected() {
        // `~` 开头的简写只认 `~/...` 与裸 `~`. `~.bashrc` (漏写斜杠) 与 `~foo/bar`
        // (指其他用户) 若被放行会在仓库内静默建出同名路径, 因此必须整体拒绝 (V-6).
        for written in ["~.bashrc", "~foo/bar"] {
            let error = resolve("shell/bashrc", written, OnConflict::Skip, &repo())
                .expect_err("以 `~` 开头但不以 `~/` 开头的 target 必须被拒绝");
            let Error::ManifestInvalid(message) = error else {
                panic!("必须归入 ManifestInvalid");
            };
            assert!(message.contains(written), "说明应含书写形态: {message}");
        }
    }

    #[test]
    fn source_must_be_a_relative_path_inside_the_repo() {
        // 源条目只接受仓库内相对路径: `~` 前缀与绝对路径都不合法 (T059).
        for written in ["~/src/rc", "/etc/hosts"] {
            let error = resolve(written, "link/bashrc", OnConflict::Skip, &repo())
                .expect_err("非仓库内相对路径的 source 必须被拒绝");
            let Error::ManifestInvalid(message) = error else {
                panic!("必须归入 ManifestInvalid");
            };
            assert!(message.contains(written), "说明应含书写形态: {message}");
        }
    }

    #[test]
    fn source_resolving_outside_the_repo_is_rejected() {
        // `..` 折叠后落到仓库根之外: 建出的链接指向仓库外, undo 的归属判定会拒删
        // 自己的产物, 因此与 `~` / 绝对路径一样整体拒绝.
        for written in ["../evil", "sub/../../evil"] {
            let error = resolve(written, "link/x", OnConflict::Skip, &repo())
                .expect_err("解析后落在仓库根之外的 source 必须被拒绝");
            let Error::ManifestInvalid(message) = error else {
                panic!("必须归入 ManifestInvalid");
            };
            assert!(message.contains(written), "说明应含书写形态: {message}");
            assert!(message.contains("仓库根之外"), "说明应指出越界: {message}");
        }
    }

    #[test]
    fn source_escaping_through_a_symlink_inside_the_repo_is_rejected() {
        // 仓库内的符号链接同样能把源带出仓库: `os::normalize` 解析祖先段的符号链接
        // (末段不解析), 越界判定必须沿用同一语义.
        let dir = tempfile::tempdir().expect("创建临时目录");
        let root = std::fs::canonicalize(dir.path()).expect("临时目录的物理路径");
        let repo_root = root.join("repo");
        std::fs::create_dir_all(&repo_root).expect("创建仓库根");
        let outside = root.join("outside");
        std::fs::create_dir_all(&outside).expect("创建仓库外目录");
        std::os::unix::fs::symlink(&outside, repo_root.join("escape")).expect("建立逃逸链接");

        let error = resolve("escape/evil", "link/x", OnConflict::Skip, &repo_root)
            .expect_err("经仓库内符号链接逃逸的 source 必须被拒绝");
        let Error::ManifestInvalid(message) = error else {
            panic!("必须归入 ManifestInvalid");
        };
        assert!(
            message.contains("escape/evil"),
            "说明应含书写形态: {message}"
        );
    }

    #[test]
    fn parent_segments_that_fold_back_inside_the_repo_are_accepted() {
        // 仓库内的 `..` 折叠 (`a/../b` 形态) 归一化后仍在仓库内, 不得被越界规则误伤.
        let entry = resolve("a/../b", "link/x", OnConflict::Skip, &repo()).unwrap();
        assert_eq!(entry.source_path, PathBuf::from("/repo/b"));
    }

    #[test]
    fn on_conflict_is_carried_through() {
        let entry = resolve(
            "shell/bashrc",
            "/tmp/bashrc",
            OnConflict::ReplaceAndBackup,
            &repo(),
        )
        .unwrap();
        assert_eq!(entry.on_conflict, OnConflict::ReplaceAndBackup);
    }

    #[test]
    fn resolution_normalizes_both_sides() {
        let entry = resolve(
            "./shell/../shell/bashrc",
            "link/../link/rc",
            OnConflict::Skip,
            &repo(),
        )
        .unwrap();
        assert_eq!(entry.source_path, PathBuf::from("/repo/shell/bashrc"));
        assert_eq!(entry.target_path, PathBuf::from("/repo/link/rc"));
    }
}

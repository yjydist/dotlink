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

/// 按 R5 的规则解析一条目的两侧路径.
///
/// `target` 支持主目录简写 (`~` / `~/...`), 绝对路径与相对仓库根的路径;
/// `source` 只接受仓库内相对路径. 两侧的裁决分别是 `expand_target` / `expand_source`.
pub fn resolve(
    source: &str,
    target: &str,
    on_conflict: OnConflict,
    repo_root: &Path,
) -> Result<Entry> {
    let target_path = os::normalize(&expand_target(target, repo_root)?);
    let source_path = os::normalize(&expand_source(source, repo_root)?);

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
fn expand_target(written: &str, repo_root: &Path) -> Result<PathBuf> {
    if let Some(rest) = written.strip_prefix("~/") {
        return Ok(home()?.join(rest));
    }
    if written == "~" {
        return home();
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
/// 的相对路径", 按 V-4 / V-6 整体拒绝, 以维持"条目来自仓库"这一心智模型.
fn expand_source(written: &str, repo_root: &Path) -> Result<PathBuf> {
    if written.starts_with('~') || Path::new(written).is_absolute() {
        return Err(Error::ManifestInvalid(format!(
            "`{written}` 不是仓库内的相对路径: 源条目不能以 `~` 开头, 也不能是绝对路径"
        )));
    }
    Ok(repo_root.join(written))
}

/// 展开 `~` 所需的当前用户主目录.
///
/// 无法确定时属于 FR-013 的整体拒绝场景 (退出码 2).
fn home() -> Result<PathBuf> {
    match std::env::var_os("HOME") {
        Some(value) if !value.is_empty() => Ok(PathBuf::from(value)),
        _ => Err(Error::ManifestInvalid(
            "无法确定主目录: 环境变量 HOME 未设置 (无法把 `~` 解析为绝对路径)".to_string(),
        )),
    }
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

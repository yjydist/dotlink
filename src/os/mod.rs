//! 符号链接, 路径规范化与归属判定的平台层.
//!
//! 链接原语在 unix 下由 `unix.rs` 提供, 不覆盖落点的原子移动仅支持 macOS/Linux.
//! 其他平台提供可编译的拒绝实现 (运行时由平台门禁拒绝, 见
//! `openspec/specs/cli-contract/spec.md` 的"平台门禁先于一切其他步骤").

use std::collections::VecDeque;
use std::ffi::{OsStr, OsString};
use std::path::{Component, MAIN_SEPARATOR_STR, Path, PathBuf};

#[cfg(unix)]
mod unix;

#[cfg(unix)]
pub use unix::{create_symlink, read_link};

// 不提供 `is_symlink`: 早期设计曾要求它, 但审阅确认它**无任何消费者** -- 生产路径
// (含 `classify`) 直接读一次 `symlink_metadata` 的 `file_type()`, 便于同一次 lstat 同时
// 判定"存在"与"是链接"; 若再提供一个 `is_symlink`, 调用处就要 lstat 两次. `tests/` 用的
// 是 std 的 `FileType::is_symlink`. 保留它只会是一个无消费者的公开接口, 与宪章第一条
// "接口是成本"及第六条"禁止投机抽象"相悖 (清单同样只保留 `source` / `target` /
// `on_conflict` 三个键, 见 `openspec/specs/manifest-schema/spec.md` 的 V-8).
// 若将来出现确实需要的调用点, 在**那时**随该调用点一并引入.

#[cfg(not(unix))]
mod stub {
    use std::io;
    use std::path::Path;

    /// 非 unix 平台的空实现: 平台门禁会在任何文件系统操作之前结束进程.
    pub fn create_symlink(_target: &Path, _link_path: &Path) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "当前平台不支持符号链接",
        ))
    }

    /// 非 unix 平台的空实现.
    pub fn read_link(_link_path: &Path) -> io::Result<std::path::PathBuf> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "当前平台不支持符号链接",
        ))
    }
}

#[cfg(not(unix))]
pub use stub::{create_symlink, read_link};

mod metadata;

pub(crate) use metadata::{copy_xattrs, set_mtime};

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

/// 符号链接解析的深度上限.
///
/// 链接链 (含环) 超过该层数即停止解析, 退化为词法处理: 归属判定本就只要求"两侧同
/// 语义", 截断对双方一致, 而无限解析会耗尽栈或永不返回.
const MAX_SYMLINK_DEPTH: usize = 40;

/// 待处理的一段路径 (符号链接文本要插回队首继续走).
enum Step {
    Prefix(OsString),
    Root,
    Cur,
    Parent,
    Part(OsString),
}

/// 把一段路径的组件转换为待处理队列.
fn steps_of(path: &Path) -> VecDeque<Step> {
    path.components()
        .map(|component| match component {
            Component::Prefix(p) => Step::Prefix(p.as_os_str().to_os_string()),
            Component::RootDir => Step::Root,
            Component::CurDir => Step::Cur,
            Component::ParentDir => Step::Parent,
            Component::Normal(part) => Step::Part(part.to_os_string()),
        })
        .collect()
}

/// 逐段解析状态: 输出栈 + 已按物理形态解析的**前缀**.
///
/// `stack` 是词法结果的段列表 (输出用它); `base` 是这些段实际对应的物理路径 (符号
/// 链接段已被其链接文本取代, 故两者的段不一一对应). `lexical_depth` 记录尚未解析的
/// 尾段数, 使 `..` 退回已知祖先时可以恢复物理解析. 相对路径无物理前缀.
struct Resolver {
    prefix: Option<OsString>,
    rooted: bool,
    stack: Vec<OsString>,
    base: Option<PathBuf>,
    lexical_depth: usize,
    depth: usize,
}

impl Resolver {
    /// 相对路径的起点: 无物理根基 (`base` 全程为 `None`), 因此只做词法处理.
    fn new() -> Self {
        Self {
            prefix: None,
            rooted: false,
            stack: Vec::new(),
            base: None,
            lexical_depth: 0,
            depth: 0,
        }
    }

    /// 进入一个普通段.
    fn enter(&mut self, part: &OsStr, pending: &mut VecDeque<Step>) {
        if self.lexical_depth > 0 {
            self.stack.push(part.to_os_string());
            self.lexical_depth += 1;
            return;
        }
        let Some(parent) = self.base.as_ref() else {
            self.stack.push(part.to_os_string());
            return;
        };

        // 末段不解析: 它就是"待判定的那个路径本身". 解析它会把 `target_path` 上的
        // 部署链接穿透到其目标, 于是 `Absent` 变成 `ForeignFile` / `ForeignLink` /
        // `LinkedCorrect`, `undo` 也再也找不到要移除的那个链接. 只解析**祖先段**,
        // 既消除别名形态与物理形态的差异 (`/tmp` -> `/private/tmp`), 又不穿透末端.
        //
        // 达到深度上限时同样不再解析 (环被有界截断), 并断开物理链使其后全走词法.
        if pending.is_empty() {
            self.stack.push(part.to_os_string());
            return;
        }
        if self.depth >= MAX_SYMLINK_DEPTH {
            self.stack.push(part.to_os_string());
            self.lexical_depth = 1;
            return;
        }

        let candidate = parent.join(part);
        match std::fs::symlink_metadata(&candidate) {
            Ok(meta) if meta.file_type().is_symlink() => {
                let Ok(text) = std::fs::read_link(&candidate) else {
                    // 读不到的尾段保留原样, 已验证的祖先仍可供后续 `..` 回退.
                    self.stack.push(part.to_os_string());
                    self.lexical_depth = 1;
                    return;
                };
                self.depth += 1;
                let text = steps_of(&text);
                // 链接文本取代该段在路径中的**位置**: 相对文本以链接所在目录为基准
                // (故 `base` 保持不变), 绝对文本等于从根重新起步 (已累积的输出作废).
                if text.iter().any(|step| matches!(step, Step::Root)) {
                    self.stack.clear();
                    self.base = Some(PathBuf::from(MAIN_SEPARATOR_STR));
                }
                if let Some(Step::Prefix(prefix)) = text.front() {
                    self.prefix = Some(prefix.clone());
                }
                for step in text.into_iter().rev() {
                    pending.push_front(step);
                }
            }
            // 已存在的普通段: 物理前缀与词法段列表同步延长一段.
            Ok(_) => {
                self.stack.push(part.to_os_string());
                self.base = Some(candidate);
            }
            // 未消去的缺失尾段保持词法形态, 因而悬空链接仍能判定归属.
            Err(_) => {
                self.stack.push(part.to_os_string());
                self.lexical_depth = 1;
            }
        }
    }

    /// 退出一个段 (`..`): 词法上弹出栈顶, 物理上退回父目录.
    ///
    /// 返回是否弹出了段; 相对路径的起点处返回 false, 由调用方记入 `leading`.
    fn leave(&mut self) -> bool {
        if self.lexical_depth > 0 {
            self.lexical_depth -= 1;
        } else if let Some(base) = self.base.as_mut() {
            // `base` 是已解析过的物理前缀 (无 `.` / `..` / 链接段), 其父目录即它的
            // `..`; 退到根后不再回退.
            base.pop();
            if base.as_os_str().is_empty() {
                *base = PathBuf::from(MAIN_SEPARATOR_STR);
            }
        }
        self.stack.pop().is_some()
    }
}

/// 非严格 realpath: 解析**已存在段**的符号链接, 对**不存在的尾段**只做词法规整
/// (逐段消除 `.` 与 `..`). 不要求路径存在.
///
/// 逐段自根向下走: 每遇到一个已存在的符号链接段就读取其文本并按链接所在目录解析
/// (相对链接), 直到得到非链接的已存在段. 因此 macOS 上的别名形态 `/tmp/x/...` 会
/// 被规范化为物理形态 `/private/tmp/x/...` (`/var` 同理). 这对归属判定是必需的:
/// 链接文本可能写成别名形态, 而另一侧 (`current_dir()`) 由内核给出物理形态, 二者
/// 指向同一文件却无法按字面判等, 链接会被误判为 `ForeignLink`, 使 `undo` 拒绝清理.
///
/// 末段**不解析**: 它往往正是被判定的那个部署链接, 穿透它会让 `Absent` /
/// `LinkedWrong` / `ForeignLink` 三者全部失真. 只解析祖先段已足以消除别名差异.
///
/// 不存在的尾段只做词法处理; `..` 消去全部缺失段后恢复祖先解析. 未消去的缺失尾段
/// 保留原样, 指向仓库内但已失效的**悬空链接**仍归类为"落在仓库内"
/// (openspec/specs/path-normalization/spec.md 的 R2).
///
/// 不得改用 `Path::canonicalize` -- 它对悬空链接返回 `Err` (实测已确认), 会把
/// 一个指向仓库内但已失效的链接误判为落在仓库外, 使 `undo` 拒绝清理自己的产物.
/// 绝对路径的根前缀被保留, `..` 上溯到根后不再回退; 相对路径开头的 `..` 保留.
pub fn normalize(path: &Path) -> PathBuf {
    let mut state = Resolver::new();
    let mut leading: usize = 0;
    let mut pending = steps_of(path);

    while let Some(step) = pending.pop_front() {
        match step {
            Step::Prefix(prefix) => state.prefix = Some(prefix),
            Step::Root => {
                state.rooted = true;
                if state.prefix.is_none() {
                    state.base = Some(PathBuf::from(MAIN_SEPARATOR_STR));
                }
            }
            Step::Cur => {}
            Step::Part(part) => state.enter(&part, &mut pending),
            Step::Parent => {
                // 已到根时 `..` 不再上溯; 相对路径开头的 `..` 必须保留, 不得被后续的
                // `..` 抵消 (否则 `../../a` 会退化成 `a`).
                if !state.leave() && !state.rooted {
                    leading += 1;
                }
            }
        }
    }

    let mut parts: Vec<OsString> = Vec::new();
    parts.extend(std::iter::repeat_n(OsString::from(".."), leading));
    parts.extend(state.stack);

    let mut out = OsString::new();
    if let Some(prefix) = state.prefix {
        out.push(prefix);
    }
    if state.rooted {
        out.push(MAIN_SEPARATOR_STR);
    }
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            out.push(MAIN_SEPARATOR_STR);
        }
        out.push(part);
    }

    if out.is_empty() {
        PathBuf::from(".")
    } else {
        PathBuf::from(out)
    }
}

/// 判断链接的解析目标是否落在 `repo_root` 之内 (即该链接由本工具创建).
///
/// 归属分界 (openspec/specs/path-normalization/spec.md 的 R2) 的唯一口径: 仓库内 -> 本工具的错误链接; 之外 -> 别人的
/// 链接. 落在 `repo_root` 自身上也算在内. 两侧都必须已经过 [`normalize`] -- 链接目标
/// 由调用方从链接文本解析得出, 仓库根由调用方每运行只算一次; 本判定纯词法,
/// 不再访问文件系统 (链接文本也由调用方读取后传入其解析结果, 不重复 `read_link`).
pub fn is_inside_repo(destination: &Path, repo_root: &Path) -> bool {
    destination.starts_with(repo_root)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 建一个临时目录, 返回"持有者 + 其**物理**路径".
    ///
    /// macOS 的 `TMPDIR` 本身就在 `/var` (`/private/var` 的别名) 之下, 直接用逻辑
    /// 路径会让"别名形态"的断言失去意义, 故这里统一以物理路径为基准.
    fn physical_tempdir() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("创建临时目录");
        let root = std::fs::canonicalize(dir.path()).expect("解析临时目录的物理路径");
        (dir, root)
    }

    #[test]
    fn normalize_removes_parent_dir_segments() {
        assert_eq!(normalize(Path::new("/a/b/../c")), PathBuf::from("/a/c"));

        // 已存在的祖先段若是符号链接, `..` 必须落在**物理**形态上.
        let (_dir, root) = physical_tempdir();
        let real = root.join("real");
        std::fs::create_dir_all(real.join("repo")).expect("创建真实目录");
        let alias = root.join("alias");
        std::os::unix::fs::symlink(&real, &alias).expect("建立别名链接");
        assert_eq!(
            normalize(&alias.join("repo").join("..").join("repo").join("shell")),
            real.join("repo").join("shell"),
            "别名形态必须规范化到物理形态"
        );
    }

    #[test]
    fn normalize_removes_cur_dir_segments() {
        assert_eq!(normalize(Path::new("/a/./b/.")), PathBuf::from("/a/b"));
        assert_eq!(normalize(Path::new("./a")), PathBuf::from("a"));
    }

    #[test]
    fn normalize_does_not_require_existence() {
        assert_eq!(
            normalize(Path::new("/definitely/not/here/../gone")),
            PathBuf::from("/definitely/not/gone")
        );
        let (_dir, root) = physical_tempdir();
        assert_eq!(
            normalize(&root.join("not/here/../gone")),
            root.join("not/gone"),
            "不存在的段只做词法处理"
        );
    }

    #[test]
    fn normalize_keeps_root_prefix_and_stops_at_root() {
        assert_eq!(normalize(Path::new("/")), PathBuf::from("/"));
        assert_eq!(normalize(Path::new("/..")), PathBuf::from("/"));
        assert_eq!(normalize(Path::new("/../..")), PathBuf::from("/"));
        assert_eq!(normalize(Path::new("/a/../../b")), PathBuf::from("/b"));
    }

    #[test]
    fn normalize_keeps_leading_parent_dirs_of_relative_paths() {
        assert_eq!(normalize(Path::new("../a")), PathBuf::from("../a"));
        assert_eq!(normalize(Path::new("../../a")), PathBuf::from("../../a"));
        assert_eq!(normalize(Path::new("a/../../b")), PathBuf::from("../b"));
    }

    #[test]
    fn normalize_is_idempotent_on_plain_absolute_paths() {
        assert_eq!(normalize(Path::new("/a/b/c")), PathBuf::from("/a/b/c"));
        let once = normalize(Path::new("/a/b/c"));
        assert_eq!(normalize(&once), once, "规范化必须是幂等的");
    }

    #[test]
    fn an_existing_symlinked_ancestor_resolves_to_its_physical_form() {
        let (_dir, root) = physical_tempdir();
        let real = root.join("real");
        std::fs::create_dir_all(real.join("shell")).expect("创建真实目录");
        let alias = root.join("alias");
        std::os::unix::fs::symlink(&real, &alias).expect("建立别名链接");

        // 别名形态 (末段尚不存在) 与物理形态必须规范化到同一个路径.
        assert_eq!(
            normalize(&alias.join("shell").join("bashrc")),
            normalize(&real.join("shell").join("bashrc"))
        );
        assert_eq!(
            normalize(&alias.join("shell").join("bashrc")),
            real.join("shell").join("bashrc")
        );

        // 相对链接文本以链接所在目录为基准: `<root>/nested/up -> ../real`.
        let nested = root.join("nested");
        std::fs::create_dir_all(&nested).expect("创建嵌套目录");
        let up = nested.join("up");
        std::os::unix::fs::symlink(Path::new("..").join("real"), &up).expect("建立相对链接");
        assert_eq!(normalize(&up.join("shell")), real.join("shell"));
    }

    #[test]
    fn the_final_segment_is_never_dereferenced() {
        let (_dir, root) = physical_tempdir();
        let repo = root.join("repo");
        std::fs::create_dir_all(repo.join("shell")).expect("创建仓库根");
        let source = repo.join("shell/bashrc");
        std::fs::write(&source, "managed").expect("写入源文件");
        let home = root.join("home");
        std::fs::create_dir_all(&home).expect("创建主目录");

        // 末段本身是**真实存在**的符号链接.
        let link = home.join(".bashrc");
        std::os::unix::fs::symlink(&source, &link).expect("建立链接");

        // 路径身份是链接自身所在的位置, 不是它指向的目标: `normalize` 必须保留它.
        // 穿透尾段会让 `target_path` 在 `classify` 做 lstat 之前就变成源文件本身,
        // 于是 `LinkedCorrect` / `LinkedWrong` / `ForeignLink` 全部塌缩成
        // `ForeignFile`, `undo` 也再找不到要移除的那个链接.
        assert_eq!(normalize(&link), link, "末段是链接时必须保留链接自身");

        // 祖先段经别名形态抵达时, 末段的链接同样必须保留.
        let alias = root.join("alias");
        std::os::unix::fs::symlink(&root, &alias).expect("建立别名链接");
        assert_eq!(
            normalize(&alias.join("home").join(".bashrc")),
            link,
            "祖先段解析后, 末段的链接仍必须保留"
        );

        // 但**祖先**段里的链接仍要解析: 别名形态必须落到物理形态.
        let repo_alias = root.join("repo-alias");
        std::os::unix::fs::symlink(&repo, &repo_alias).expect("建立仓库别名链接");
        assert_eq!(
            normalize(&repo_alias.join("shell").join("bashrc")),
            source,
            "祖先段的链接仍须解析"
        );
    }

    #[test]
    fn a_dangling_symlink_tail_stays_lexical_and_keeps_the_repo_prefix() {
        let (_dir, root) = physical_tempdir();
        let repo = root.join("repo");
        std::fs::create_dir_all(repo.join("shell")).expect("创建仓库根");

        // 指向仓库内但已失效: 必须仍归入仓库内 (R2 的硬性要求).
        let dangling = repo.join("shell/gone");
        assert_eq!(
            normalize(&dangling),
            repo.join("shell/gone"),
            "悬空链接的尾段只做词法处理"
        );

        // 悬空链接**自身**作为末段时不得被穿透 (穿透会让 `Absent` 变成别的状态).
        let link = root.join("link-to-missing");
        std::os::unix::fs::symlink(&dangling, &link).expect("建立悬空链接");
        assert_eq!(normalize(&link), link, "末段是链接时不得解析");
    }

    #[test]
    fn a_symlink_loop_is_bounded_and_never_panics() {
        let (_dir, root) = physical_tempdir();
        // 自指链接: 解析不限深则永不返回.
        let loop_link = root.join("loop");
        std::os::unix::fs::symlink(&loop_link, &loop_link).expect("建立自指链接");

        // 只是"能返回"还不够: 结果必须仍是该临时目录之下的一条路径.
        assert_eq!(normalize(&loop_link.join("x")), loop_link.join("x"));

        // 两节点互指同样必须被有界截断.
        let a = root.join("a");
        let b = root.join("b");
        std::os::unix::fs::symlink(&b, &a).expect("建立 a -> b");
        std::os::unix::fs::symlink(&a, &b).expect("建立 b -> a");
        assert_eq!(normalize(&a.join("x")), a.join("x"));
    }

    #[test]
    fn parent_steps_out_of_a_missing_tail_resume_physical_resolution() {
        let (_dir, root) = physical_tempdir();
        std::fs::create_dir(root.join("actual")).unwrap();
        std::os::unix::fs::symlink("actual", root.join("alias")).unwrap();
        for written in [
            "missing/../alias/rc",
            "missing/nested/../../alias/rc",
            "missing/../absent/../alias/rc",
            "alias/missing/../../alias/rc",
        ] {
            let resolved = normalize(&root.join(written));
            assert_eq!(resolved, root.join("actual/rc"), "{written}");
            assert_eq!(normalize(&resolved), resolved, "{written}");
        }
        assert_eq!(
            normalize(&root.join("missing/alias/rc")),
            root.join("missing/alias/rc")
        );
        assert_eq!(
            normalize(&root.join("missing/../alias")),
            root.join("alias")
        );
    }

    /// 相对路径没有已知的物理前缀, 因此只做词法规整: 已存在的符号链接段不被解析.
    ///
    /// 这条边界是刻意的 (见 [`normalize`] 的文档): 相对路径无法确定"当前目录"的物理
    /// 形态, 解析链接段只会得到与绝对形态不一致的结果. 全部生产调用点都传绝对路径
    /// (`entry` 解析后的源与目标, 仓库根, 链接文本解析后的绝对路径).
    #[test]
    fn relative_paths_are_normalized_lexically_only() {
        let (_dir, root) = physical_tempdir();
        std::fs::create_dir(root.join("actual")).unwrap();
        std::os::unix::fs::symlink("actual", root.join("alias")).unwrap();

        let relative = Path::new("alias/rc");
        assert_eq!(normalize(relative), relative);

        // 同一段在绝对形态下会被解析: 证明上一条断言来自"相对路径不解析",
        // 而不是链接不存在.
        assert_eq!(normalize(&root.join(relative)), root.join("actual/rc"));
    }

    #[test]
    fn is_inside_repo_counts_the_root_itself_but_not_a_sibling_prefix() {
        assert!(is_inside_repo(
            Path::new("/repo/shell/bashrc"),
            Path::new("/repo")
        ));
        assert!(is_inside_repo(Path::new("/repo"), Path::new("/repo")));

        // 逐段比较: `/repo-other` 不得以字符串前缀被误判为 `/repo` 之内.
        assert!(!is_inside_repo(
            Path::new("/repo-other/rc"),
            Path::new("/repo")
        ));
        assert!(!is_inside_repo(Path::new("/etc/rc"), Path::new("/repo")));
    }
}

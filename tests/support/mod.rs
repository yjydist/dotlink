//! 端到端测试夹具: 临时仓库 + 临时 HOME.
//!
//! 每个测试文件只用到其中一部分方法, 故整个模块允许 dead_code.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;

/// 临时环境: 一个同时充当仓库根与 HOME 的临时目录.
///
/// 子进程的当前工作目录被设为仓库根, `HOME` 被设为临时主目录, 因此测试绝不会
/// 触碰真实主目录.
pub struct TempEnv {
    _dir: tempfile::TempDir,
    repo: PathBuf,
    home: PathBuf,
}

impl TempEnv {
    /// 创建临时目录; 仓库根即该目录, HOME 为其下的 `home` 子目录.
    ///
    /// 仓库根取**物理**路径: 子进程的 `current_dir()` 由内核 `getcwd` 解析, 在 macOS
    /// 上会把 `/var/...` 返回为 `/private/var/...`, 而 dotlink 的链接文本正是由该物理
    /// 路径拼出. 若此处保留 `tempfile` 给出的逻辑路径, 断言侧的期望值与链接文本会落在
    /// 两套路径语义上, 使 `assert_symlink_to` 无谓失败.
    pub fn new() -> Self {
        let dir = tempfile::tempdir().expect("创建临时目录");
        let repo = fs::canonicalize(dir.path()).expect("解析临时目录的物理路径");
        let home = repo.join("home");
        fs::create_dir_all(&home).expect("创建临时 HOME");
        Self {
            _dir: dir,
            repo,
            home,
        }
    }

    /// 仓库根 (即清单文件所在目录).
    pub fn repo(&self) -> &Path {
        &self.repo
    }

    /// 临时主目录.
    pub fn home(&self) -> &Path {
        &self.home
    }

    /// 把清单写到 `repo/dotlink.toml`.
    pub fn write_manifest(&self, toml: &str) {
        fs::write(self.repo().join("dotlink.toml"), toml).expect("写入清单");
    }

    /// 在仓库中创建源文件 (自动建父目录).
    pub fn write_source(&self, rel: &str, content: &str) {
        let path = self.repo().join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("创建源的父目录");
        }
        fs::write(&path, content).expect("写入源文件");
    }

    /// 在仓库中创建源目录.
    pub fn make_source_dir(&self, rel: &str) {
        fs::create_dir_all(self.repo().join(rel)).expect("创建源目录");
    }

    /// 构造 `dotlink` 命令: `cwd` = 仓库根, `HOME` = 临时主目录.
    pub fn cmd(&self) -> Command {
        let mut command = Command::cargo_bin("dotlink").expect("找到 dotlink 二进制");
        command.current_dir(self.repo());
        command.env("HOME", self.home());
        command.env_remove("XDG_CONFIG_HOME");
        command.env_remove("USERPROFILE");
        command
    }

    /// 断言 `link` 是指向 `expected_target` 的符号链接.
    pub fn assert_symlink_to(&self, link: &Path, expected_target: &Path) {
        let raw = fs::read_link(link)
            .unwrap_or_else(|error| panic!("读取链接 {} 失败: {error}", link.display()));
        assert_eq!(
            resolve_lexically(&raw, link.parent()),
            resolve_lexically(expected_target, None),
            "{} 指向了 {} 而非 {}",
            link.display(),
            raw.display(),
            expected_target.display()
        );
    }

    /// 断言 `path` 的内容等于 `expected`.
    pub fn assert_content(&self, path: &Path, expected: &str) {
        let actual = fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("读取 {} 失败: {error}", path.display()));
        assert_eq!(actual, expected, "{} 的内容不符", path.display());
    }

    /// 目录下的文件名 (排序), 用于检测多出的备份文件.
    pub fn list_dir(&self, dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap_or_else(|error| panic!("列举 {} 失败: {error}", dir.display()))
            .map(|entry| {
                entry
                    .expect("目录项")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        names
    }

    /// 递归快照: 每个条目的路径 + (文件内容 | 链接文本), 用于断言"零改动".
    ///
    /// 只覆盖 `root` 之下的路径, 不包含 `root` 自身.
    pub fn snapshot(&self, root: &Path) -> Vec<(PathBuf, String)> {
        let mut rows = Vec::new();
        collect(root, &mut rows);
        rows.sort();
        rows
    }
}

impl Default for TempEnv {
    fn default() -> Self {
        Self::new()
    }
}

/// 递归收集 `path` 之下的全部条目.
fn collect(path: &Path, rows: &mut Vec<(PathBuf, String)>) {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return;
    };

    if meta.file_type().is_symlink() {
        let raw = fs::read_link(path).expect("读取链接文本");
        rows.push((path.to_path_buf(), format!("link -> {}", raw.display())));
        return;
    }

    if meta.is_file() {
        let content = fs::read_to_string(path).unwrap_or_else(|_| "<binary>".to_string());
        rows.push((path.to_path_buf(), content));
        return;
    }

    rows.push((path.to_path_buf(), "dir".to_string()));
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        collect(&entry.path(), rows);
    }
}

/// 词法解析: 以 `base` 为基准把相对路径变为绝对路径, 并消除 `.` 与 `..`.
///
/// 仅用于断言, 与 `src/os/mod.rs` 的 `normalize` 同语义. 后者在 `os` 模块内为
/// crate 私有路径, 测试无法直接复用, 故此处保留一份最小副本.
pub fn resolve_lexically(path: &Path, base: Option<&Path>) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        match base {
            Some(base) => base.join(path),
            None => path.to_path_buf(),
        }
    };

    let mut prefix: Option<std::ffi::OsString> = None;
    let mut rooted = false;
    let mut leading: usize = 0;
    let mut stack: Vec<std::ffi::OsString> = Vec::new();

    for component in absolute.components() {
        match component {
            std::path::Component::Prefix(p) => prefix = Some(p.as_os_str().to_os_string()),
            std::path::Component::RootDir => rooted = true,
            std::path::Component::CurDir => {}
            std::path::Component::Normal(part) => stack.push(part.to_os_string()),
            std::path::Component::ParentDir => {
                if stack.pop().is_none() && !rooted {
                    leading += 1;
                }
            }
        }
    }

    let mut parts: Vec<std::ffi::OsString> = Vec::new();
    parts.extend(std::iter::repeat_n(std::ffi::OsString::from(".."), leading));
    parts.extend(stack);

    let separator = std::path::MAIN_SEPARATOR_STR;
    let mut text = std::ffi::OsString::new();
    if let Some(prefix) = prefix {
        text.push(prefix);
    }
    if rooted {
        text.push(separator);
    }
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            text.push(separator);
        }
        text.push(part);
    }
    PathBuf::from(text)
}

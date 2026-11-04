//! 端到端测试夹具与共用辅助: 临时仓库 + 临时 HOME + 各套件共享的观测辅助.
//!
//! 每个测试文件只用到其中一部分条目, 故整个模块允许 dead_code.

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
        Command::from_std(self.raw_cmd())
    }

    /// 未包装的 `std` 命令, 供需要自行接管管道 (例如关闭读端) 的用例使用.
    pub fn raw_cmd(&self) -> std::process::Command {
        let mut command = std::process::Command::new(assert_cmd::cargo::cargo_bin("dotlink"));
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

/// 一次命令调用的观测结果.
pub struct Run {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// 运行 `dotlink <args...>` (cwd = 仓库根, HOME = 临时主目录).
pub fn run(env: &TempEnv, args: &[&str]) -> Run {
    let assert = env.cmd().args(args).assert();
    let output = assert.get_output();
    Run {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// 构造清单: 每个条目为 `(source, target, on_conflict)`, 策略写 `"-"` 表示缺省.
pub fn manifest_of(entries: &[(&str, &str, &str)]) -> String {
    let mut text = String::new();
    for (source, target, policy) in entries {
        text.push_str("[[entry]]\n");
        text.push_str(&format!("source = \"{source}\"\n"));
        text.push_str(&format!("target = \"{target}\"\n"));
        if *policy != "-" {
            text.push_str(&format!("on_conflict = \"{policy}\"\n"));
        }
    }
    text
}

/// 该行是否是逐条目行 (`<序号>/<总数> ...`), 用于把末尾汇总行排除在外.
pub fn is_entry_line(line: &str) -> bool {
    match line
        .split_whitespace()
        .next()
        .and_then(|word| word.split_once('/'))
    {
        Some((index, total)) => index.parse::<usize>().is_ok() && total.parse::<usize>().is_ok(),
        None => false,
    }
}

/// 全部逐条目行 (排除末尾汇总行).
pub fn entry_lines(stdout: &str) -> Vec<&str> {
    stdout.lines().filter(|line| is_entry_line(line)).collect()
}

/// 逐条目行数.
pub fn entry_line_count(stdout: &str) -> usize {
    stdout.lines().filter(|line| is_entry_line(line)).count()
}

/// 含 `needle` 的那一行.
#[track_caller]
pub fn line_for<'a>(stdout: &'a str, needle: &str) -> &'a str {
    stdout
        .lines()
        .find(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("输出中没有含 {needle} 的行:\n{stdout}"))
}

/// 行内的 token (第 2 个字段).
#[track_caller]
pub fn token_of(line: &str) -> &str {
    line.split_whitespace()
        .nth(1)
        .unwrap_or_else(|| panic!("行内没有 token: {line}"))
}

/// 行内展示的 target (第 3 个字段, 即用户书写的原始字符串).
#[track_caller]
pub fn target_of(line: &str) -> &str {
    line.split_whitespace()
        .nth(2)
        .unwrap_or_else(|| panic!("行内没有 target: {line}"))
}

/// 断言输出中出现了 token `token` (以整词出现).
#[track_caller]
pub fn assert_token(run: &Run, token: &str) {
    assert!(
        run.stdout.split_whitespace().any(|word| word == token),
        "输出中缺少 token {token}, 实际输出: {}",
        run.stdout
    );
}

/// 仓库 + HOME 的递归快照 (路径 + 文件内容 / 链接文本), 用于断言"零改动".
///
/// HOME 位于仓库之内 (夹具如此), 两份递归会重叠出重复项; 这里排序后按路径去重,
/// 使"前后比对"不受重复影响.
pub fn snapshot_all(env: &TempEnv) -> Vec<(PathBuf, String)> {
    let mut rows = env.snapshot(env.repo());
    rows.extend(env.snapshot(env.home()));
    rows.sort();
    rows.dedup();
    rows
}

/// 仓库根的**物理**路径.
///
/// 子进程的 `std::env::current_dir()` 返回物理路径 (macOS 上临时目录位于
/// `/var/folders/...`, 即 `/private/var/folders/...`), 而 `TempEnv::repo()` 返回的是
/// 未解析的那一份. 链接归属判定以物理路径为基准 (夹具的 `assert_symlink_to` 是纯
/// 词法比较, 两种书写形态在它眼中是不同的路径), 因此手工构造链接时必须用同一基准,
/// 否则会构造出一个指向错误位置的链接 (得到 `wrong` 而非 `ok`).
pub fn physical_repo(env: &TempEnv) -> PathBuf {
    fs::canonicalize(env.repo()).expect("解析仓库根的物理路径")
}

/// 某目录下所有备份文件名 (含最终名与 `.partial` 残留).
pub fn backups_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("列举 {} 失败: {error}", dir.display()))
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".dotlink-backup."))
        .collect();
    names.sort();
    names
}

/// 从命令输出中取出备份文件的完整路径 (FR-016 的"必现信息").
///
/// 输出里的路径可能被标点 (`,;.` 与 `)`) 收尾, 因此先试原词, 再试去掉收尾标点的形态.
#[track_caller]
pub fn backup_path_from_output(stdout: &str) -> PathBuf {
    for word in stdout.split_whitespace() {
        if !word.contains(".dotlink-backup.") {
            continue;
        }
        for candidate in [
            word.to_string(),
            word.trim_end_matches([',', ';', '.', ')']).to_string(),
        ] {
            let path = PathBuf::from(&candidate);
            if fs::symlink_metadata(&path).is_ok() {
                return path;
            }
        }
    }
    panic!("输出中未给出可定位的备份文件完整路径: {stdout}");
}

/// 从命令输出中取出备份路径的词, **不要求该路径已存在** (preview 的预测值).
///
/// 与 [`backup_path_from_output`] 共用同一套收尾标点处理; preview 只报告"将会使用"
/// 的名字, 因此不能像 apply / undo 那样用存在性筛选.
#[track_caller]
pub fn backup_candidate_from_output(stdout: &str) -> PathBuf {
    for word in stdout.split_whitespace() {
        if !word.contains(".dotlink-backup.") {
            continue;
        }
        return PathBuf::from(word.trim_end_matches([',', ';', '.', ')']));
    }
    panic!("输出中未给出备份路径: {stdout}");
}

/// 以 root 运行时权限位无效 (chmod 000 不阻止读取), 依赖 `EACCES` 的断言必须跳过.
pub fn running_as_root() -> bool {
    let probe = std::env::temp_dir().join(format!("dotlink-root-probe-{}", std::process::id()));
    let _ = fs::remove_dir_all(&probe);
    if fs::create_dir_all(&probe).is_err() {
        return false;
    }
    let locked = probe.join("locked");
    let outcome = fs::create_dir_all(&locked)
        .and_then(|()| {
            fs::set_permissions(&locked, std::os::unix::fs::PermissionsExt::from_mode(0o000))
        })
        .and_then(|()| fs::read_dir(&locked).map(|_| ()));
    let as_root = outcome.is_ok();
    let _ = fs::remove_dir_all(&probe);
    as_root
}

/// 一份"目标父目录不可进入"的清单与被打上 chmod 000 的目录.
///
/// 第一条目标是正常的 (用于断言"其余条目正常处理"), 第二条落在不可进入的目录里.
pub fn env_with_locked_target() -> (TempEnv, PathBuf) {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_source("shell/zshrc", "managed-zsh\n");
    env.write_manifest(
        "[[entry]]\nsource = \"shell/bashrc\"\ntarget = \"~/.bashrc\"\n\n\
         [[entry]]\nsource = \"shell/zshrc\"\ntarget = \"~/.locked/rc\"\n",
    );

    let locked = env.home().join(".locked");
    fs::create_dir_all(&locked).expect("创建目标父目录");
    fs::set_permissions(&locked, std::os::unix::fs::PermissionsExt::from_mode(0o000))
        .expect("设置权限位");
    (env, locked)
}

/// 恢复目录的可列举性, 使临时目录能被正常清理.
pub fn unlock(dir: &Path) {
    let _ = fs::set_permissions(dir, std::os::unix::fs::PermissionsExt::from_mode(0o755));
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
/// 纯词法折叠, **不解析**路径中的符号链接; 仅供测试计算断言的期望值. 它与
/// `src/os` 的 `normalize` 并不同语义 (后者还会解析祖先段的符号链接, 只有末段
/// 不解析), 那份实现在 crate 私有路径上, 测试无法直接复用.
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

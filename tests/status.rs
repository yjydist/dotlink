//! US5 / FR-011 / FR-016 / FR-020: 只读地报告清单与实际状态的偏离 (tasks.md 的 T033).
//!
//! 覆盖 (a)-(g). 本文件经**真实二进制**断言, 因此依赖 `src/lib.rs` 的命令接入
//! (T034/T035). 在那些接入完成之前, 这里的断言必然失败 -- 这是预期的 TDD 红阶段
//! (测试先于实现存在), 不得为了让本文件变绿而改动 `src/` 下的任何文件.

mod support;

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use support::{
    Run, TempEnv, entry_lines, line_for, manifest_of, physical_repo, run, snapshot_all, token_of,
};

/// 某条目的 token (`status` 输出中按 target 定位).
#[track_caller]
fn token_for(env: &TempEnv, target: &str) -> String {
    let run = run(env, &["status"]);
    token_of(line_for(&run.stdout, target)).to_string()
}

/// 断言 `status` 的退出码.
#[track_caller]
fn assert_status_code(env: &TempEnv, expected: i32) {
    let run = run(env, &["status"]);
    assert_eq!(
        run.code, expected,
        "status 期望退出码 {expected}; stdout:\n{}\nstderr:\n{}",
        run.stdout, run.stderr
    );
}

/// 建立"正确链接" (指向清单声明的源).
fn link_correct(env: &TempEnv, source: &str, target: &str) -> PathBuf {
    let link = env.home().join(target);
    if let Some(parent) = link.parent() {
        fs::create_dir_all(parent).expect("创建目标父目录");
    }
    // 该函数在断言"存在偏离"的用例里被反复调用, 因此先清掉既有占用物 (链接或文件),
    // 否则第二次调用会以 `File exists` 失败.
    if fs::symlink_metadata(&link).is_ok() {
        fs::remove_file(&link).expect("清除既有的目标占用物");
    }
    std::os::unix::fs::symlink(physical_repo(env).join(source), &link).expect("建立正确链接");
    link
}

/// 仓库之外的一个路径 (且存在于磁盘上), 用于构造 `foreign`.
///
/// 夹具的 HOME 位于**仓库之内** (`<repo>/home`), 因此"指向 HOME 的链接"会被正确地
/// 判为仓库内; 构造 `foreign` 必须落到仓库之外的区域.
fn outside_file(env: &TempEnv, name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dotlink-outside-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("创建仓库之外的目录");
    let path = dir.join(name);
    fs::write(&path, "user own content\n").expect("写入仓库之外的文件");
    assert!(
        !path.starts_with(env.repo()),
        "仓库之外的夹具路径不得落在仓库内: {}",
        path.display()
    );
    path
}

/// 仓库之外的一个路径 (不存在), 用于构造悬空的外部链接.
fn outside_dangling(env: &TempEnv, name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("dotlink-outside-{}", std::process::id()));
    let path = path.join(name);
    assert!(!path.starts_with(env.repo()));
    let _ = fs::remove_file(&path);
    path
}

// ---------------------------------------------------------------------------
// T033 (a): 目标不存在 -> absent; 源不存在 -> missing (两者分别断言)
// ---------------------------------------------------------------------------

#[test]
fn an_undeployed_target_is_absent_and_a_missing_source_is_missing() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_manifest(&manifest_of(&[
        ("shell/bashrc", "~/.bashrc", "-"),
        ("shell/gone", "~/.gone", "-"),
    ]));

    let run = run(&env, &["status"]);

    assert_eq!(run.code, 1, "存在偏离; stdout:\n{}", run.stdout);

    let absent = line_for(&run.stdout, "~/.bashrc");
    assert_eq!(
        token_of(absent),
        "absent",
        "声明了但目标路径不存在 -> absent (Current::Absent): {absent}"
    );
    let missing = line_for(&run.stdout, "~/.gone");
    assert_eq!(
        token_of(missing),
        "missing",
        "源条目不存在 -> missing (Current::SourceMissing): {missing}"
    );

    // 二者是不同的 token, 且落在不同的汇总列: absent 计入"未处理", missing 计入"失败".
    assert_ne!(token_of(absent), token_of(missing));
    let summary = run.stdout.lines().last().unwrap_or_default();
    assert_eq!(
        summary, "2 个条目: 0 成功, 0 已是最新, 1 未处理, 1 失败",
        "absent 与 missing 必须落在不同的汇总列"
    );
}

// ---------------------------------------------------------------------------
// T033 (b): 归属分界 -- 仓库外 -> foreign (附实际指向); 仓库内但源不对 -> wrong
// ---------------------------------------------------------------------------

#[test]
fn a_link_outside_the_repo_is_foreign_and_a_link_inside_is_wrong() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_source("shell/other", "other\n");
    env.write_manifest(&manifest_of(&[("shell/bashrc", "~/.bashrc", "-")]));

    // 落在仓库之外: 别人的链接.
    let outside = outside_file(&env, "zshrc");
    std::os::unix::fs::symlink(&outside, env.home().join(".bashrc")).expect("建立外部链接");

    let foreign = token_for(&env, "~/.bashrc");
    assert_eq!(
        foreign, "foreign",
        "链接落在仓库之外 -> foreign (FR-011 的归属分界)"
    );
    assert_status_code(&env, 1);

    let line = line_for(&run(&env, &["status"]).stdout, "~/.bashrc").to_string();
    assert_eq!(line.split_whitespace().nth(1), Some("foreign"), "{line}");
    assert!(
        line.contains(&outside.display().to_string()),
        "foreign (占用物为链接时) 必须给出实际指向: {line}"
    );
    assert!(
        line.contains("符号链接"),
        "foreign 必须说明占用物类型: {line}"
    );

    // 落在仓库之内但指向的不是清单声明的源: 本工具的错误链接.
    fs::remove_file(env.home().join(".bashrc")).expect("移除外部链接");
    std::os::unix::fs::symlink(
        physical_repo(&env).join("shell/other"),
        env.home().join(".bashrc"),
    )
    .expect("建立仓库内的错误链接");

    let wrong = token_for(&env, "~/.bashrc");
    assert_eq!(
        wrong, "wrong",
        "链接落在仓库内但源不对 -> wrong (FR-011 的归属分界)"
    );
    assert_ne!(
        foreign, wrong,
        "foreign 与 wrong 必须是不同的 token, 不得混淆"
    );

    let line = line_for(&run(&env, &["status"]).stdout, "~/.bashrc").to_string();
    assert!(
        line.contains(
            &physical_repo(&env)
                .join("shell/other")
                .display()
                .to_string()
        ),
        "wrong 必须给出实际指向: {line}"
    );

    let _ = fs::remove_file(&outside);
}

// ---------------------------------------------------------------------------
// T033 (c): 目标被普通文件 / 目录占用 -> foreign 并说明占用物类型
// ---------------------------------------------------------------------------

#[test]
fn a_plain_file_occupying_the_target_is_foreign_with_the_occupant_type() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_manifest(&manifest_of(&[("shell/bashrc", "~/.bashrc", "-")]));
    fs::write(env.home().join(".bashrc"), "pre-existing\n").expect("写入占用物");

    assert_eq!(token_for(&env, "~/.bashrc"), "foreign");
    assert_status_code(&env, 1);

    let line = line_for(&run(&env, &["status"]).stdout, "~/.bashrc").to_string();
    assert!(
        line.contains("普通文件"),
        "foreign 必须说明占用物是符号链接, 文件还是目录: {line}"
    );
}

#[test]
fn a_directory_occupying_the_target_is_foreign_with_the_occupant_type() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_manifest(&manifest_of(&[("shell/bashrc", "~/.bashrc", "-")]));
    fs::create_dir_all(env.home().join(".bashrc")).expect("创建占用目录");

    assert_eq!(token_for(&env, "~/.bashrc"), "foreign");

    let line = line_for(&run(&env, &["status"]).stdout, "~/.bashrc").to_string();
    assert!(
        line.contains("目录"),
        "foreign 必须说明占用物是目录: {line}"
    );
}

// ---------------------------------------------------------------------------
// T033 (d): 全部符合退 0; 存在任一偏离退 1
// ---------------------------------------------------------------------------

#[test]
fn status_exits_zero_when_everything_conforms() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_source("shell/zshrc", "managed-zsh\n");
    env.write_manifest(&manifest_of(&[
        ("shell/bashrc", "~/.bashrc", "-"),
        ("shell/zshrc", "~/.zshrc", "-"),
    ]));

    link_correct(&env, "shell/bashrc", ".bashrc");
    link_correct(&env, "shell/zshrc", ".zshrc");

    let run = run(&env, &["status"]);
    assert_eq!(run.code, 0, "全部符合时退 0; stdout:\n{}", run.stdout);
    for line in entry_lines(&run.stdout) {
        assert_eq!(token_of(line), "ok", "全部符合时每行都是 ok: {line}");
    }
    let summary = run.stdout.lines().last().unwrap_or_default();
    assert_eq!(summary, "2 个条目: 0 成功, 2 已是最新, 0 未处理, 0 失败");
}

#[test]
fn status_exits_one_on_any_deviation() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_source("shell/zshrc", "managed-zsh\n");
    env.write_manifest(&manifest_of(&[
        ("shell/bashrc", "~/.bashrc", "-"),
        ("shell/zshrc", "~/.zshrc", "-"),
    ]));

    link_correct(&env, "shell/bashrc", ".bashrc");
    link_correct(&env, "shell/zshrc", ".zshrc");
    assert_status_code(&env, 0);

    // 未部署 (absent).
    fs::remove_file(env.home().join(".zshrc")).expect("移除链接");
    assert_status_code(&env, 1);
    link_correct(&env, "shell/zshrc", ".zshrc");

    // 被非本工具的内容占用 (foreign).
    fs::remove_file(env.home().join(".zshrc")).expect("移除链接");
    fs::write(env.home().join(".zshrc"), "pre-existing\n").expect("写入占用物");
    assert_status_code(&env, 1);
    fs::remove_file(env.home().join(".zshrc")).expect("移除占用物");

    // 指向错误 (wrong).
    std::os::unix::fs::symlink(
        physical_repo(&env).join("shell/bashrc"),
        env.home().join(".zshrc"),
    )
    .expect("建立错误链接");
    assert_status_code(&env, 1);

    // 源缺失 (missing).
    link_correct(&env, "shell/zshrc", ".zshrc");
    env.write_manifest(&manifest_of(&[
        ("shell/bashrc", "~/.bashrc", "-"),
        ("shell/gone", "~/.zshrc", "-"),
    ]));
    assert_status_code(&env, 1);
}

// ---------------------------------------------------------------------------
// T033 (e): 只读 -- 前后文件系统完全一致
// ---------------------------------------------------------------------------

#[test]
fn status_never_touches_the_filesystem() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_source("shell/zshrc", "managed-zsh\n");
    env.write_source("nvim/init.lua", "managed-nvim\n");
    env.write_manifest(&manifest_of(&[
        ("shell/bashrc", "~/.bashrc", "-"),
        ("shell/zshrc", "~/.zshrc", "-"),
        ("shell/gone", "~/.gone", "-"),
        ("nvim/init.lua", "~/.config/nvim", "replace_and_backup"),
    ]));

    // 四种状态并存: 已是最新 / 目标被普通文件占用 / 源缺失 / 父目录不存在.
    link_correct(&env, "shell/bashrc", ".bashrc");
    fs::write(env.home().join(".zshrc"), "pre-existing\n").expect("写入占用物");

    let before = snapshot_all(&env);

    let run = run(&env, &["status"]);

    let after = snapshot_all(&env);
    assert_eq!(
        before, after,
        "status 必须完全只读; stdout:\n{}\nstderr:\n{}",
        run.stdout, run.stderr
    );
    assert_eq!(run.code, 1, "存在偏离; stdout:\n{}", run.stdout);

    // 四种状态各得其所, 使这条只读断言确实覆盖了多种偏离.
    assert_eq!(token_of(line_for(&run.stdout, "~/.bashrc")), "ok");
    assert_eq!(token_of(line_for(&run.stdout, "~/.zshrc")), "foreign");
    assert_eq!(token_of(line_for(&run.stdout, "~/.gone")), "missing");
    assert_eq!(token_of(line_for(&run.stdout, "~/.config/nvim")), "absent");
}

// ---------------------------------------------------------------------------
// T033 (f): 规范化陷阱 (research.md R2)
// ---------------------------------------------------------------------------

/// 链接文本含 `..`: 与源路径的字面形态不同, 但规范化后指向同一个源 -> 必须是 `ok`.
///
/// 若实现只做字面前缀比较 (或用 `Path::canonicalize` 而非 `os::normalize`), 这条
/// 必然失败: 前者会把结构合法的 `..` 段误判为"偏离", 后者对仍存在的路径给出物理
/// 路径而丢失了比较基准的一致性.
#[test]
fn a_link_text_containing_parent_dir_segments_is_still_ok() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_manifest(&manifest_of(&[("shell/bashrc", "~/.bashrc", "-")]));

    let repo = physical_repo(&env);
    let name = repo.file_name().expect("仓库根目录名").to_os_string();
    // <repo>/../<repo-name>/shell/bashrc -- 含 .. 段, 规范化后等于 source_path.
    let raw = repo.join("..").join(&name).join("shell/bashrc");
    assert!(
        raw.components().any(|part| part.as_os_str() == ".."),
        "夹具必须让链接文本含有 .. 段: {}",
        raw.display()
    );

    std::os::unix::fs::symlink(&raw, env.home().join(".bashrc")).expect("建立含 .. 的链接");

    let run = run(&env, &["status"]);
    assert_eq!(
        token_of(line_for(&run.stdout, "~/.bashrc")),
        "ok",
        "含 .. 的链接文本仍须报 ok 而非 foreign; stdout:\n{}",
        run.stdout
    );
    assert_eq!(run.code, 0, "stdout:\n{}", run.stdout);
}

/// 仓库位于 macOS 的 `/tmp` 之下 (`/tmp` 是指向 `/private/tmp` 的符号链接).
///
/// 链接文本用 `/tmp/...` 这一**别名**形态书写, 而子进程的 `current_dir` 返回的是
/// `/private/tmp/...` 这一物理形态 (`research.md` R2 的实测情形). 二者指向同一个
/// 文件, 因此必须报 `ok` 而非 `foreign` -- 这正是 R2 中"归属判定必须对两侧都做
/// 规范化"的判据.
///
/// 判据依赖 `os::normalize` 的实际语义: 它在**祖先段**解析符号链接 (末段不解析),
/// 因此别名形态与物理形态会折叠到同一路径, 该链接才得以报 `ok`. 本用例把这一要求
/// 放到真实进程上 (子进程的 cwd 是物理形态, 链接文本是别名形态), 使"规范化退回纯
/// 词法"这类回归直接可见.
#[test]
fn a_link_written_with_the_tmp_alias_is_still_ok() {
    let Some(dir) = tempfile::Builder::new().tempdir_in("/tmp").ok() else {
        eprintln!("跳过 a_link_written_with_the_tmp_alias_is_still_ok: 无法在 /tmp 下创建临时目录");
        return;
    };

    let repo = dir.path().to_path_buf();
    let home = repo.join("home");
    fs::create_dir_all(repo.join("shell")).expect("创建源目录");
    fs::create_dir_all(&home).expect("创建临时 HOME");
    fs::write(repo.join("shell/bashrc"), "managed\n").expect("写入源文件");
    fs::write(
        repo.join("dotlink.toml"),
        "[[entry]]\nsource = \"shell/bashrc\"\ntarget = \"~/.bashrc\"\n",
    )
    .expect("写入清单");

    // 链接文本用别名形态 (/tmp/...), 而子进程的工作目录会被解析为物理形态.
    let alias = repo.join("shell/bashrc");
    std::os::unix::fs::symlink(&alias, home.join(".bashrc")).expect("建立别名形态的链接");

    let mut command = assert_cmd::Command::cargo_bin("dotlink").expect("找到 dotlink 二进制");
    command.current_dir(&repo);
    command.env("HOME", &home);
    command.env_remove("XDG_CONFIG_HOME");
    let output = command.arg("status").output().expect("运行 status");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();

    let run = Run {
        code: output.status.code().unwrap_or(-1),
        stdout,
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    };

    // 前置条件自证: 别名形态与物理形态必须**指向同一个文件**, 否则本用例什么也没证明.
    // 这正是 R2"macOS 路径别名"陷阱的成立条件.
    let physical_source = fs::canonicalize(&alias).expect("解析源文件的物理路径");
    assert_ne!(
        alias, physical_source,
        "前置条件: /tmp 别名与物理路径必须不同, 否则本用例测不到 R2 的别名分支"
    );
    let alias_meta = fs::metadata(&alias).expect("别名形态必须可 stat");
    let physical_meta = fs::metadata(&physical_source).expect("物理形态必须可 stat");
    assert_eq!(
        alias_meta.ino(),
        physical_meta.ino(),
        "前置条件: {} 与 {} 必须是同一个文件",
        alias.display(),
        physical_source.display()
    );

    assert_eq!(
        token_of(line_for(&run.stdout, "~/.bashrc")),
        "ok",
        "链接文本与仓库根指向同一个文件, 只是书写形态不同 (别名 {} vs 物理 {});\
         R2 的归属判定必须对两侧做同一套规范化, 使该链接仍归为本工具创建.\
         实际判成了别的结论, 输出:\n{}",
        alias.display(),
        physical_source.display(),
        run.stdout
    );
    assert_eq!(run.code, 0, "stdout:\n{}", run.stdout);
}

// ---------------------------------------------------------------------------
// T033 (g): 悬空链接 -- 链接文本指向仓库内不存在的文件 -> wrong 而非 foreign
// ---------------------------------------------------------------------------

#[test]
fn a_dangling_link_inside_the_repo_is_wrong_not_foreign() {
    let env = TempEnv::new();
    env.write_source("nvim/init.lua", "managed\n");
    env.write_manifest(&manifest_of(&[(
        "nvim/init.lua",
        "~/.config/init.lua",
        "-",
    )]));

    // 链接文本指向仓库内一个**不存在**的文件 (用户在仓库中重命名/删除了源).
    let gone = physical_repo(&env).join("nvim/removed.lua");
    assert!(
        fs::symlink_metadata(&gone).is_err(),
        "前置条件: 该源必须不存在"
    );
    let link = env.home().join(".config/init.lua");
    fs::create_dir_all(link.parent().unwrap()).expect("创建目标父目录");
    std::os::unix::fs::symlink(&gone, &link).expect("建立悬空链接");

    let first_run = run(&env, &["status"]);

    let line = line_for(&first_run.stdout, "~/.config/init.lua").to_string();
    assert_eq!(
        token_of(&line),
        "wrong",
        "悬空链接必须靠 os::normalize 的非严格性归入仓库内, 而非 foreign: {line}"
    );
    assert_eq!(first_run.code, 1, "stdout:\n{}", first_run.stdout);

    // 仓库之外的悬空链接则必须是 foreign.
    let dangling = outside_dangling(&env, "gone");
    let second = env.home().join(".config/second.lua");
    std::os::unix::fs::symlink(&dangling, &second).expect("建立仓库外的悬空链接");
    env.write_manifest(&manifest_of(&[
        ("nvim/init.lua", "~/.config/init.lua", "-"),
        ("nvim/init.lua", "~/.config/second.lua", "-"),
    ]));

    let second_run = run(&env, &["status"]);
    assert_eq!(
        token_of(line_for(&second_run.stdout, "~/.config/second.lua")),
        "foreign",
        "仓库之外的悬空链接仍是 foreign; stdout:\n{}",
        second_run.stdout
    );
}

// ---------------------------------------------------------------------------
// 补充: status 不接受任何参数, 不报告动作
// ---------------------------------------------------------------------------

#[test]
fn status_reports_current_state_only() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_manifest(&manifest_of(&[("shell/bashrc", "~/.bashrc", "-")]));

    // 未部署: 只读命令不得报告"将执行的动作"措辞, 只报告现状.
    let run = run(&env, &["status"]);
    let line = line_for(&run.stdout, "~/.bashrc");
    assert_eq!(token_of(line), "absent", "{line}");
    assert!(
        !line.contains("将创建"),
        "status 只渲染现状, 不渲染动作: {line}"
    );
}

/// 夹具的 `home` 位于仓库**之内**, `outside_file` 必须落在仓库之外 -- 这条自检
/// 保证 T033(b) 的 `foreign` 断言不是因为误判而通过.
#[test]
fn the_fixture_layout_actually_puts_home_inside_the_repo() {
    let env = TempEnv::new();
    let home = fs::canonicalize(env.home()).expect("解析 HOME 的物理路径");
    let repo = physical_repo(&env);
    assert!(
        home.starts_with(&repo),
        "夹具布局变了: HOME ({}) 应位于仓库 ({}) 之内",
        home.display(),
        repo.display()
    );
    assert!(!outside_file(&env, "probe").starts_with(&repo));
}

/// 未使用参数的探针: 确认夹具给出的 cwd 与 HOME 都被真实生效.
#[test]
fn the_fixture_pins_cwd_and_home_for_the_child_process() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_manifest(&manifest_of(&[("shell/bashrc", "~/.bashrc", "-")]));

    // 若 HOME 未生效, `~` 会指向真实主目录, 该条目会落到真实用户目录上;
    // 这里以"目标路径落在临时 HOME 之下"来确认 HOME 确实生效.
    link_correct(&env, "shell/bashrc", ".bashrc");
    let link: &Path = &env.home().join(".bashrc");
    assert!(fs::symlink_metadata(link).is_ok());
    assert_status_code(&env, 0);
}

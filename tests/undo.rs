//! US4 / FR-009 / FR-010 / SC-005: 撤销部署 (tasks.md 的 T028 全部条目).
//!
//! 覆盖 (a)-(h). 本文件经真实二进制断言, 因此**依赖 `src/lib.rs` 的命令接入**
//! (T015 / T026 / T031). 在那些接入完成之前, 这里的断言必然失败 -- 这是预期的
//! TDD 红阶段 (测试先于实现存在), 不得为了让本文件变绿而改动 `src/lib.rs`.

mod support;

use std::fs;
use std::path::{Path, PathBuf};

use support::TempEnv;

/// 一次命令调用的观测结果.
struct Run {
    code: i32,
    stdout: String,
    #[allow(dead_code)]
    stderr: String,
}

/// 运行 `dotlink <args...>` (cwd = 仓库根, HOME = 临时主目录).
fn run(env: &TempEnv, args: &[&str]) -> Run {
    let assert = env.cmd().args(args).assert();
    let output = assert.get_output();
    Run {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// 构造清单: 每个条目为 `(source, target, on_conflict)`, 策略写 `"-"` 表示缺省.
fn manifest_of(entries: &[(&str, &str, &str)]) -> String {
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

/// 某目录下所有备份文件名.
fn backups_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("列举 {} 失败: {error}", dir.display()))
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".dotlink-backup."))
        .collect();
    names.sort();
    names
}

/// 从命令输出中取出备份来源路径 (FR-016 的"必现信息").
#[track_caller]
fn backup_path_from_output(stdout: &str) -> PathBuf {
    for word in stdout.split_whitespace() {
        if !word.contains(".dotlink-backup.") {
            continue;
        }
        let candidate = PathBuf::from(word.trim_end_matches([',', ';', '.', ')']));
        if fs::symlink_metadata(&candidate).is_ok() {
            return candidate;
        }
    }
    panic!("输出中未给出可定位的备份来源路径: {stdout}");
}

/// 输出中某一行 (逐条目行, 排除末尾汇总行).
#[track_caller]
fn line_containing(run: &Run, needle: &str) -> String {
    run.stdout
        .lines()
        .find(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("输出中没有含 {needle} 的行: {}", run.stdout))
        .to_string()
}

#[track_caller]
fn assert_token(run: &Run, token: &str) {
    assert!(
        run.stdout.split_whitespace().any(|word| word == token),
        "输出中缺少 token {token}, 实际输出: {}",
        run.stdout
    );
}

/// 已部署的环境: 源在仓库中, 目标是指向源的链接.
fn deployed(env: &TempEnv, source: &str, target: &str) -> PathBuf {
    env.write_source(source, "managed\n");
    let deployed = run(env, &["apply"]);
    assert_eq!(deployed.code, 0, "前置部署必须成功: {}", deployed.stdout);
    let path = env.home().join(target);
    assert!(
        fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_symlink()),
        "前置条件失败: apply 之后 {} 应当是指向源的符号链接 (命令接入由 T015/T031 负责, 尚未完成)",
        path.display()
    );
    env.assert_symlink_to(&path, &env.repo().join(source));
    path
}

// ---------------------------------------------------------------------------
// (a) 已部署且曾有备份: 链接被移除, 原内容恢复到原路径, 行为 restore 且给出备份来源路径
// ---------------------------------------------------------------------------

#[test]
fn undo_restores_the_previous_content_and_reports_the_backup_source() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_manifest(&manifest_of(&[(
        "shell/bashrc",
        "~/.bashrc",
        "replace_and_backup",
    )]));

    let target = env.home().join(".bashrc");
    fs::write(&target, "original\n").unwrap();
    let applied = run(&env, &["apply"]);
    assert_eq!(applied.code, 0, "{}", applied.stdout);
    let backup = backup_path_from_output(&applied.stdout);
    env.assert_content(&backup, "original\n");

    // restore 行必须给出备份来源路径 (FR-016).
    let undone = run(&env, &["undo"]);
    assert_token(&undone, "restore");
    assert_eq!(
        undone.code, 1,
        "undo 的目标谓词在动手之前求值: 撤销了东西 (restore) 故退 1; stdout:\n{}",
        undone.stdout
    );
    let line = line_containing(&undone, "restore");
    assert!(
        line.contains(&backup.display().to_string()),
        "restore 行必须给出备份文件的完整路径: {line}"
    );

    assert!(
        fs::symlink_metadata(&target).is_ok_and(|meta| !meta.file_type().is_symlink()),
        "恢复后 {} 不应再是链接",
        target.display()
    );
    env.assert_content(&target, "original\n");

    // 恢复成功后不删除备份文件本身.
    assert!(
        fs::symlink_metadata(&backup).is_ok(),
        "备份文件必须保留在磁盘上"
    );
    env.assert_content(&backup, "original\n");
}

// ---------------------------------------------------------------------------
// (b) 已部署但无备份: 链接被移除, 行为 undo, 原路径不再存在
// ---------------------------------------------------------------------------

#[test]
fn undo_removes_a_link_without_a_backup() {
    let env = TempEnv::new();
    env.write_manifest(&manifest_of(&[("shell/bashrc", "~/.bashrc", "-")]));
    let target = deployed(&env, "shell/bashrc", ".bashrc");

    let undone = run(&env, &["undo"]);
    assert_token(&undone, "undo");
    assert_eq!(
        undone.code, 1,
        "撤销了东西 (undo) 故退 1; stdout:\n{}",
        undone.stdout
    );

    assert!(
        fs::symlink_metadata(&target).is_err(),
        "{} 应已被移除",
        target.display()
    );
    assert!(
        backups_in(env.home()).is_empty(),
        "无备份时不得凭空产生备份文件: {:?}",
        backups_in(env.home())
    );
    assert!(
        env.repo().join("shell/bashrc").is_file(),
        "源条目必须原样保留"
    );
}

// ---------------------------------------------------------------------------
// (c) 安全反例 (SC-005): 手工换成真实目录 -> skip 且目录完整保留
// ---------------------------------------------------------------------------

#[test]
fn undo_skips_a_real_directory_and_keeps_its_content() {
    let env = TempEnv::new();
    env.write_manifest(&manifest_of(&[("shell/bashrc", "~/.bashrc", "-")]));
    let target = deployed(&env, "shell/bashrc", ".bashrc");

    fs::remove_file(&target).unwrap();
    fs::create_dir_all(target.join("nested")).unwrap();
    fs::write(target.join("nested/keep"), "keep me\n").unwrap();

    let undone = run(&env, &["undo"]);
    assert_token(&undone, "skip");
    assert_eq!(undone.code, 1, "skip 使条件 2 不成立");

    assert!(
        fs::symlink_metadata(&target).unwrap().is_dir(),
        "非本工具创建的目录绝不允许被删除"
    );
    env.assert_content(&target.join("nested/keep"), "keep me\n");
    assert!(
        line_containing(&undone, "skip").contains("目录"),
        "skip 行必须说明原因与占用物类型"
    );
}

// ---------------------------------------------------------------------------
// (d) 目标被替换成指向仓库外的链接 -> skip, 不删除该链接
// ---------------------------------------------------------------------------

#[test]
fn undo_skips_a_link_pointing_outside_the_repo() {
    let env = TempEnv::new();
    env.write_manifest(&manifest_of(&[("shell/bashrc", "~/.bashrc", "-")]));
    let target = deployed(&env, "shell/bashrc", ".bashrc");

    // 必须落在仓库**之外**: 夹具的 HOME 位于仓库之内, 因此"仓库内的别处"仍会被
    // 正确判为本工具创建的链接 (`LinkedWrong`), 报 `undo` 而非 `skip`.
    let elsewhere = std::env::temp_dir()
        .join(format!("dotlink-undo-outside-{}", std::process::id()))
        .join("elsewhere");
    fs::create_dir_all(elsewhere.parent().unwrap()).unwrap();
    fs::write(&elsewhere, "foreign\n").unwrap();
    assert!(
        !elsewhere.starts_with(env.repo()),
        "构造 foreign 的路径必须落在仓库之外: {}",
        elsewhere.display()
    );
    fs::remove_file(&target).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &target).unwrap();

    let undone = run(&env, &["undo"]);
    assert_token(&undone, "skip");
    assert_eq!(undone.code, 1);

    let _ = fs::remove_file(&elsewhere);

    assert!(
        fs::symlink_metadata(&target)
            .unwrap()
            .file_type()
            .is_symlink(),
        "外部链接可能是用户自己的配置, 绝不删除"
    );
    assert_eq!(fs::read_link(&target).unwrap(), elsewhere);
}

// ---------------------------------------------------------------------------
// (e) 目标被替换成普通文件 -> 只 skip, 且不消费备份 (并存语义)
// ---------------------------------------------------------------------------

#[test]
fn undo_skips_a_plain_file_and_does_not_consume_the_backup() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_manifest(&manifest_of(&[(
        "shell/bashrc",
        "~/.bashrc",
        "replace_and_backup",
    )]));

    let target = env.home().join(".bashrc");
    fs::write(&target, "original\n").unwrap();
    let applied = run(&env, &["apply"]);
    assert_eq!(applied.code, 0, "{}", applied.stdout);
    let backup = backup_path_from_output(&applied.stdout);

    // 手工把链接换成普通文件: 恢复备份的前提是"我先移除了自己创建的链接".
    fs::remove_file(&target).unwrap();
    fs::write(&target, "hand written\n").unwrap();

    let undone = run(&env, &["undo"]);
    assert_token(&undone, "skip");
    assert_eq!(undone.code, 1);

    env.assert_content(&target, "hand written\n");
    assert!(fs::symlink_metadata(&backup).is_ok(), "备份不得被消费/删除");
    env.assert_content(&backup, "original\n");
}

// ---------------------------------------------------------------------------
// (f) 幂等: 第二次 undo 每行 token 为 ok, 退出码 0
// ---------------------------------------------------------------------------

#[test]
fn undo_is_idempotent_and_the_second_run_reports_ok() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env.write_source("shell/zshrc", "managed\n");
    env.write_manifest(&manifest_of(&[
        ("shell/bashrc", "~/.bashrc", "-"),
        ("shell/zshrc", "~/.zshrc", "-"),
    ]));
    let applied = run(&env, &["apply"]);
    assert_eq!(applied.code, 0, "前置部署必须成功: {}", applied.stdout);

    let first = run(&env, &["undo"]);
    assert_eq!(
        first.code, 1,
        "撤销了东西 (两条 undo) 故退 1; stdout:\n{}",
        first.stdout
    );
    assert!(
        first
            .stdout
            .lines()
            .filter(|line| line.contains("undo") || line.contains("restore"))
            .count()
            >= 2,
        "两条都应被撤销: {}",
        first.stdout
    );

    // 第二次: `Current` 为 `Absent` -> token `ok`, 归入"已是最新"列 -> 退 0.
    let second = run(&env, &["undo"]);
    assert_eq!(second.code, 0, "撤销必须幂等: {}", second.stdout);
    let entry_lines: Vec<&str> = second
        .stdout
        .lines()
        .filter(|line| line.trim_start().starts_with("1/") || line.trim_start().starts_with("2/"))
        .collect();
    assert_eq!(entry_lines.len(), 2, "应有两条逐条目行: {}", second.stdout);
    for line in entry_lines {
        assert!(
            line.split_whitespace().nth(1) == Some("ok"),
            "第二次 undo 的每一行 token 必须是 ok (若实现把 Absent 归为 skip, 本断言必失败): {line}"
        );
    }
    assert!(
        fs::symlink_metadata(env.home().join(".bashrc")).is_err(),
        "第二次 undo 不得产生多余删除"
    );
}

// ---------------------------------------------------------------------------
// (g) 混合场景: 可撤销的被处理, 须跳过的不受影响, 退出码 1
// ---------------------------------------------------------------------------

#[test]
fn undo_handles_a_mixed_manifest() {
    let env = TempEnv::new();
    // 两个源都必须在部署**之前**写入: `deployed()` 会对整份两条目的清单执行一次
    // `apply`, 若此时第二个源还缺失, 它会以 `missing` 使该次 apply 退 1, 从而让
    // `deployed()` 的前置断言失败 (那条断言本身是对的).
    env.write_source("shell/bashrc", "managed\n");
    env.write_source("shell/zshrc", "managed-zsh\n");
    env.write_manifest(&manifest_of(&[
        ("shell/bashrc", "~/.bashrc", "-"),
        ("shell/zshrc", "~/.zshrc", "-"),
    ]));
    let removable = deployed(&env, "shell/bashrc", ".bashrc");
    let blocked = deployed(&env, "shell/zshrc", ".zshrc");

    // 把第二条手工换成真实目录.
    fs::remove_file(&blocked).unwrap();
    fs::create_dir_all(blocked.join("nested")).unwrap();
    fs::write(blocked.join("nested/keep"), "keep me\n").unwrap();

    let undone = run(&env, &["undo"]);
    assert_token(&undone, "undo");
    assert_token(&undone, "skip");
    assert_eq!(undone.code, 1, "存在 skip 条目");

    assert!(
        fs::symlink_metadata(&removable).is_err(),
        "可撤销的条目必须被处理"
    );
    assert!(fs::symlink_metadata(&blocked).unwrap().is_dir());
    env.assert_content(&blocked.join("nested/keep"), "keep me\n");
}

// ---------------------------------------------------------------------------
// (h) 归属判定不依赖状态文件
// ---------------------------------------------------------------------------

#[test]
fn undo_never_creates_a_state_or_index_file() {
    let env = TempEnv::new();
    env.write_manifest(&manifest_of(&[("shell/bashrc", "~/.bashrc", "-")]));
    // 前置部署**之后**才取基准: 该快照是"undo 之前"的状态, 因此它必须包含 `apply`
    // 建立的链接. 若在部署之前取, 基准里就没有那条链接, 断言会把 `apply` 的产物
    // 误报成"undo 往仓库里写了东西".
    deployed(&env, "shell/bashrc", ".bashrc");

    let home_before = env.snapshot(env.home());
    let repo_before = env.snapshot(env.repo());

    let undone = run(&env, &["undo"]);
    assert_eq!(
        undone.code, 1,
        "撤销了东西 (undo) 故退 1; stdout:\n{}",
        undone.stdout
    );

    // 仓库侧只允许"目标路径被移除"这一处变化; HOME 侧除目标路径外必须逐字节不变.
    let home_after = env.snapshot(env.home());
    let removed: Vec<PathBuf> = home_before
        .iter()
        .filter(|row| !home_after.contains(row))
        .map(|row| row.0.clone())
        .collect();
    assert_eq!(
        removed,
        vec![env.home().join(".bashrc")],
        "除待撤销的链接之外, HOME 不得有任何改动 (也不得出现状态/索引文件)"
    );
    for row in &home_after {
        assert!(
            home_before.contains(row) || row.0.starts_with(env.home().join(".bashrc")),
            "HOME 中出现了新路径: {}",
            row.0.display()
        );
    }

    // 仓库侧: HOME 位于仓库之内, 因此"被撤销的链接被移除"会在仓库快照里体现为
    // 少了一行. 这里逐行核对: 消失的只允许是那条链接, 且不得出现任何**新**路径
    // (新路径才是"写入了状态/索引文件"的证据).
    let repo_after = env.snapshot(env.repo());
    let disappeared: Vec<PathBuf> = repo_before
        .iter()
        .filter(|row| !repo_after.contains(row))
        .map(|row| row.0.clone())
        .collect();
    assert_eq!(
        disappeared,
        vec![env.home().join(".bashrc")],
        "仓库中只允许少掉那条被撤销的链接"
    );
    for row in &repo_after {
        assert!(
            repo_before.contains(row),
            "undo 往仓库里写入了新路径 (状态/索引文件?): {}",
            row.0.display()
        );
    }
}

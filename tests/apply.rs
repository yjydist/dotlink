//! US1 / FR-005 / FR-018 / SC-002 / SC-003: 部署 (tasks.md 的 T013 与 T039).
//!
//! 覆盖 T013 的 (a)-(g) 与 T039 的三条收尾用例 (50 条目的性能断言, 单条目失败不阻塞
//! 其余条目, 并发调用不产生指向错误位置的链接).
//!
//! 本文件经**真实二进制**断言, 因此依赖 `src/lib.rs` 的命令接入 (T015/T016). 在那些
//! 接入完成之前, 这里的断言必然失败 -- 这是预期的 TDD 红阶段 (测试先于实现存在),
//! 不得为了让本文件变绿而改动 `src/` 下的任何文件.

mod support;

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use support::{
    Run, TempEnv, entry_lines, line_for, manifest_of, physical_repo, run, snapshot_all, target_of,
    token_of,
};

/// 断言每一次条目行的 token 都是 `token`.
#[track_caller]
fn assert_all_tokens(run: &Run, token: &str) {
    let lines = entry_lines(&run.stdout);
    assert!(
        !lines.is_empty(),
        "输出中没有逐条目行 (命令接入由 T015 负责, 尚未完成):\n{}",
        run.stdout
    );
    for line in lines {
        assert_eq!(token_of(line), token, "该行的 token 不符: {line}");
    }
}

/// 断言退出码.
#[track_caller]
fn assert_code(run: &Run, expected: i32) {
    assert_eq!(
        run.code, expected,
        "期望退出码 {expected}, 实际 {}; stdout:\n{}\nstderr:\n{}",
        run.code, run.stdout, run.stderr
    );
}

/// 断言末尾汇总行逐字等于 `expected`.
#[track_caller]
fn assert_summary(run: &Run, expected: &str) {
    let actual = run.stdout.lines().last().unwrap_or_default();
    assert_eq!(
        actual, expected,
        "末尾汇总行不符; 完整输出:\n{}",
        run.stdout
    );
}

/// 断言 `link` 指向仓库中的 `source`.
#[track_caller]
fn assert_link(env: &TempEnv, link: &std::path::Path, source: &str) {
    env.assert_symlink_to(link, &physical_repo(env).join(source));
}

/// 快照中是否出现了备份文件 (`<原名>.dotlink-backup.*`).
fn has_backup_file(rows: &[(PathBuf, String)]) -> bool {
    rows.iter()
        .any(|(path, _)| path.to_string_lossy().contains(".dotlink-backup."))
}

// ---------------------------------------------------------------------------
// T013 (a): 3 个条目全不存在时退 0 且三行均为 create
// ---------------------------------------------------------------------------

#[test]
fn three_absent_targets_are_all_created_and_apply_exits_zero() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "export A=1\n");
    env.write_source("shell/zshrc", "export B=2\n");
    env.write_source("git/gitconfig", "[user]\n");
    env.write_manifest(&manifest_of(&[
        ("shell/bashrc", "~/.bashrc", "-"),
        ("shell/zshrc", "~/.zshrc", "-"),
        ("git/gitconfig", "~/.gitconfig", "-"),
    ]));

    let run = run(&env, &["apply"]);

    assert_code(&run, 0);
    assert_eq!(
        entry_lines(&run.stdout).len(),
        3,
        "三个条目应有三行逐条目结果:\n{}",
        run.stdout
    );
    assert_all_tokens(&run, "create");
    assert_summary(&run, "3 个条目: 3 成功, 0 已是最新, 0 未处理, 0 失败");

    // 报告中展示的是用户书写的原始 target 字符串, 而不是解析后的绝对路径.
    for raw in ["~/.bashrc", "~/.zshrc", "~/.gitconfig"] {
        assert_eq!(
            target_of(line_for(&run.stdout, raw)),
            raw,
            "报告必须展示用户书写的原始 target:\n{}",
            run.stdout
        );
    }
}

// ---------------------------------------------------------------------------
// T013 (b): 每个目标都是指向仓库内文件的正确链接, 且能读到内容
// ---------------------------------------------------------------------------

#[test]
fn every_created_target_is_a_link_into_the_repo_and_reads_back_the_source() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "export A=1\n");
    env.write_source("shell/zshrc", "export B=2\n");
    env.write_manifest(&manifest_of(&[
        ("shell/bashrc", "~/.bashrc", "-"),
        ("shell/zshrc", "~/.zshrc", "-"),
    ]));

    assert_code(&run(&env, &["apply"]), 0);

    for (source, target, content) in [
        ("shell/bashrc", ".bashrc", "export A=1\n"),
        ("shell/zshrc", ".zshrc", "export B=2\n"),
    ] {
        let link = env.home().join(target);
        let meta = fs::symlink_metadata(&link)
            .unwrap_or_else(|error| panic!("{} 不存在: {error}", link.display()));
        assert!(
            meta.file_type().is_symlink(),
            "{} 必须是符号链接而非副本",
            link.display()
        );
        assert_link(&env, &link, source);
        // 经链接读到的是仓库内的内容 (等价于 `cat <链接>`).
        env.assert_content(&link, content);
    }

    // 源文件本身不得被改动或删除.
    env.assert_content(&env.repo().join("shell/bashrc"), "export A=1\n");
}

// ---------------------------------------------------------------------------
// T013 (c): 目标父目录不存在时被自动创建 (FR-005)
// ---------------------------------------------------------------------------

#[test]
fn a_missing_parent_directory_is_created_automatically() {
    let env = TempEnv::new();
    env.write_source("nvim/init.lua", "set number\n");
    env.write_manifest(&manifest_of(&[(
        "nvim/init.lua",
        "~/.config/nvim/init.lua",
        "-",
    )]));

    let parent = env.home().join(".config/nvim");
    assert!(
        fs::symlink_metadata(&parent).is_err(),
        "前置条件: 父目录必须尚不存在"
    );

    let run = run(&env, &["apply"]);

    assert_code(&run, 0);
    assert_all_tokens(&run, "create");
    assert!(
        fs::symlink_metadata(&parent).is_ok_and(|meta| meta.is_dir()),
        "FR-005: 缺失的各级父目录必须被自动创建 ({})",
        parent.display()
    );
    assert_link(&env, &parent.join("init.lua"), "nvim/init.lua");
}

// ---------------------------------------------------------------------------
// T013 (d): 目录条目建立的是整目录软链接 (FR-018)
// ---------------------------------------------------------------------------

#[test]
fn a_directory_entry_links_the_whole_directory() {
    let env = TempEnv::new();
    env.write_source("nvim/init.lua", "set number\n");
    env.write_source("nvim/lua/plugins.lua", "return {}\n");
    env.write_manifest(&manifest_of(&[("nvim", "~/.config/nvim", "-")]));

    let run = run(&env, &["apply"]);

    assert_code(&run, 0);
    assert_all_tokens(&run, "create");

    let link = env.home().join(".config/nvim");
    let meta = fs::symlink_metadata(&link)
        .unwrap_or_else(|error| panic!("{} 不存在: {error}", link.display()));
    assert!(
        meta.file_type().is_symlink(),
        "FR-018: 目录条目必须建立整目录软链接, 而不是复制或逐文件链接"
    );
    assert_link(&env, &link, "nvim");

    // 经链接读到的是仓库内目录的内容 (整目录生效).
    env.assert_content(&link.join("init.lua"), "set number\n");
    env.assert_content(&link.join("lua/plugins.lua"), "return {}\n");

    // 整目录链接是单个条目: 目标父目录下只有这一项.
    assert_eq!(
        env.list_dir(&env.home().join(".config")),
        vec!["nvim".to_string()]
    );
}

// ---------------------------------------------------------------------------
// T013 (e): 幂等 (SC-002)
// ---------------------------------------------------------------------------

#[test]
fn a_second_apply_is_idempotent_and_produces_no_new_files() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "export A=1\n");
    env.write_source("nvim/init.lua", "set number\n");
    env.write_source("shell/zshrc", "export B=2\n");
    env.write_manifest(&manifest_of(&[
        ("shell/bashrc", "~/.bashrc", "-"),
        ("nvim", "~/.config/nvim", "-"),
        ("shell/zshrc", "~/.zshrc", "-"),
    ]));

    let first = run(&env, &["apply"]);
    assert_code(&first, 0);
    assert_all_tokens(&first, "create");

    let before = snapshot_all(&env);

    let second = run(&env, &["apply"]);

    assert_code(&second, 0);
    assert_eq!(
        entry_lines(&second.stdout).len(),
        3,
        "第二次仍应有三条逐条目结果:\n{}",
        second.stdout
    );
    assert_all_tokens(&second, "ok");
    assert_summary(&second, "3 个条目: 0 成功, 3 已是最新, 0 未处理, 0 失败");

    let after = snapshot_all(&env);
    assert_eq!(
        before, after,
        "SC-002: 第二次 apply 不得产生任何文件系统改动"
    );
    assert!(
        !has_backup_file(&after),
        "幂等运行不得产生备份文件: {after:?}"
    );
}

// ---------------------------------------------------------------------------
// T013 (f): 增量 -- 新增条目后只有新条目为 create
// ---------------------------------------------------------------------------

#[test]
fn adding_an_entry_only_creates_the_new_one() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "export A=1\n");
    env.write_source("shell/zshrc", "export B=2\n");
    env.write_manifest(&manifest_of(&[("shell/bashrc", "~/.bashrc", "-")]));
    assert_code(&run(&env, &["apply"]), 0);

    let before = env.snapshot(env.home());

    env.write_manifest(&manifest_of(&[
        ("shell/bashrc", "~/.bashrc", "-"),
        ("shell/zshrc", "~/.zshrc", "-"),
    ]));
    let run = run(&env, &["apply"]);

    assert_code(&run, 0);
    assert_eq!(
        token_of(line_for(&run.stdout, "~/.bashrc")),
        "ok",
        "已有条目必须保持 ok:\n{}",
        run.stdout
    );
    assert_eq!(
        token_of(line_for(&run.stdout, "~/.zshrc")),
        "create",
        "只有新条目是 create:\n{}",
        run.stdout
    );
    assert_summary(&run, "2 个条目: 1 成功, 1 已是最新, 0 未处理, 0 失败");

    // 已有链接不得被重建: 增量运行只新增 .zshrc 这一个路径.
    let after = env.snapshot(env.home());
    let added: Vec<PathBuf> = after
        .iter()
        .filter(|row| !before.contains(row))
        .map(|row| row.0.clone())
        .collect();
    assert_eq!(
        added,
        vec![env.home().join(".zshrc")],
        "增量运行只允许新增新条目的目标路径"
    );
}

// ---------------------------------------------------------------------------
// T013 (g): 多条目声明同一个源但目标不同, 两侧都成功
// ---------------------------------------------------------------------------

#[test]
fn two_entries_sharing_one_source_both_succeed() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "export A=1\n");
    env.write_manifest(&manifest_of(&[
        ("shell/bashrc", "~/.bashrc", "-"),
        ("shell/bashrc", "~/.profile", "-"),
    ]));

    let run = run(&env, &["apply"]);

    assert_code(&run, 0);
    assert_all_tokens(&run, "create");
    assert_summary(&run, "2 个条目: 2 成功, 0 已是最新, 0 未处理, 0 失败");

    assert_link(&env, &env.home().join(".bashrc"), "shell/bashrc");
    assert_link(&env, &env.home().join(".profile"), "shell/bashrc");
    env.assert_content(&env.home().join(".profile"), "export A=1\n");
}

// ---------------------------------------------------------------------------
// T039: SC-003 性能断言 (50 个条目, 墙钟 < 2 秒, 输出恰好 50 行)
// ---------------------------------------------------------------------------

#[test]
fn fifty_entries_preview_and_apply_finish_under_two_seconds_with_fifty_rows() {
    const COUNT: usize = 50;

    let env = TempEnv::new();
    let mut entries: Vec<(String, String)> = Vec::new();
    for index in 0..COUNT {
        let source = format!("files/entry-{index:02}.conf");
        let target = format!("~/.dotlink-perf/entry-{index:02}.conf");
        env.write_source(&source, &format!("content {index}\n"));
        entries.push((source, target));
    }
    let manifest: String = entries
        .iter()
        .map(|(source, target)| {
            format!("[[entry]]\nsource = \"{source}\"\ntarget = \"{target}\"\n")
        })
        .collect();
    env.write_manifest(&manifest);

    let budget = Duration::from_secs(2);

    let started = Instant::now();
    let preview = run(&env, &["preview"]);
    let preview_elapsed = started.elapsed();

    assert!(
        preview_elapsed < budget,
        "SC-003: preview 在 {COUNT} 个条目上耗时 {preview_elapsed:?}, 超出 {budget:?}"
    );
    assert_eq!(
        entry_lines(&preview.stdout).len(),
        COUNT,
        "preview 必须恰好给出 {COUNT} 行逐条目结果:\n{}",
        preview.stdout
    );
    assert_all_tokens(&preview, "create");
    assert_code(&preview, 1);
    assert_summary(
        &preview,
        &format!("{COUNT} 个条目: {COUNT} 成功, 0 已是最新, 0 未处理, 0 失败"),
    );

    let started = Instant::now();
    let apply = run(&env, &["apply"]);
    let apply_elapsed = started.elapsed();

    assert!(
        apply_elapsed < budget,
        "SC-003: apply 在 {COUNT} 个条目上耗时 {apply_elapsed:?}, 超出 {budget:?}"
    );
    assert_eq!(
        entry_lines(&apply.stdout).len(),
        COUNT,
        "apply 必须恰好给出 {COUNT} 行逐条目结果:\n{}",
        apply.stdout
    );
    assert_all_tokens(&apply, "create");
    assert_code(&apply, 0);
    assert_summary(
        &apply,
        &format!("{COUNT} 个条目: {COUNT} 成功, 0 已是最新, 0 未处理, 0 失败"),
    );

    for (source, target) in &entries {
        let link = env.home().join(target.trim_start_matches("~/"));
        assert_link(&env, &link, source);
    }
}

// ---------------------------------------------------------------------------
// T039: 单条目失败不阻塞其余条目 (FR-014)
// ---------------------------------------------------------------------------

#[test]
fn a_missing_source_does_not_block_the_other_entries() {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "export A=1\n");
    env.write_source("shell/zshrc", "export B=2\n");
    env.write_manifest(&manifest_of(&[
        ("shell/bashrc", "~/.bashrc", "-"),
        ("shell/gone", "~/.gone", "-"),
        ("shell/zshrc", "~/.zshrc", "-"),
    ]));

    let run = run(&env, &["apply"]);

    assert_code(&run, 1);
    assert_eq!(token_of(line_for(&run.stdout, "~/.gone")), "missing");
    assert_eq!(token_of(line_for(&run.stdout, "~/.bashrc")), "create");
    assert_eq!(token_of(line_for(&run.stdout, "~/.zshrc")), "create");
    assert_summary(&run, "3 个条目: 2 成功, 0 已是最新, 0 未处理, 1 失败");

    // 正常条目仍被创建.
    assert_link(&env, &env.home().join(".bashrc"), "shell/bashrc");
    assert_link(&env, &env.home().join(".zshrc"), "shell/zshrc");
    assert!(
        fs::symlink_metadata(env.home().join(".gone")).is_err(),
        "源缺失的条目不得建立任何链接"
    );
}

// ---------------------------------------------------------------------------
// T039: 并发调用不产生指向错误位置的链接 (research.md R8)
// ---------------------------------------------------------------------------

#[test]
fn concurrent_apply_never_creates_a_link_to_the_wrong_place() {
    let env = TempEnv::new();
    let mut entries: Vec<(String, String)> = Vec::new();
    for index in 0..4 {
        let source = format!("files/entry-{index}.conf");
        let target = format!("~/.concurrent-{index}");
        env.write_source(&source, &format!("content {index}\n"));
        entries.push((source, target));
    }
    let manifest: String = entries
        .iter()
        .map(|(source, target)| {
            format!("[[entry]]\nsource = \"{source}\"\ntarget = \"{target}\"\n")
        })
        .collect();
    env.write_manifest(&manifest);

    let mut first = env.cmd();
    first.arg("apply");
    let mut second = env.cmd();
    second.arg("apply");

    let handle = std::thread::spawn(move || first.output().expect("并发运行的第一个进程"));
    let second_output = second.output().expect("并发运行的第二个进程");
    let first_output = handle.join().expect("等待第一个进程");

    for output in [&first_output, &second_output] {
        let code = output.status.code().unwrap_or(-1);
        assert!(
            code == 0 || code == 1,
            "并发调用不得以崩溃等异常结局结束 (退出码 {code})"
        );
        let text = String::from_utf8_lossy(&output.stdout).into_owned();
        let lines = entry_lines(&text);
        assert_eq!(
            lines.len(),
            entries.len(),
            "并发调用的输出行数不符:\n{text}"
        );
        for line in lines {
            let token = token_of(line);
            assert!(
                token == "create" || token == "ok" || token == "fail",
                "并发下出现了非预期 token {token}: {line}"
            );
            if token == "fail" {
                // 两个进程在同一路径上竞争时, 落败者可能遇到 `File exists`. R8 允许
                // 这种竞争, 但它必须**如实报告** (`fail`), 并说明原因 -- 不得静默.
                assert!(
                    line.contains("失败") || line.contains("File exists"),
                    "fail 行必须给出原因 (FR-016): {line}"
                );
            }
        }
    }

    // R8: 只断言"不静默产生错误结果" -- 最终每个目标都是指向正确源的链接, 因此
    // 即便某一侧报了 `fail`, 落点也绝不是"指向错误位置的链接".
    // 不断言备份文件的唯一性 (并发下允许出现重复备份, 那是已记录的已知限制).
    for (source, target) in &entries {
        let link = env.home().join(target.trim_start_matches("~/"));
        assert_link(&env, &link, source);
    }
}

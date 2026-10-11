//! FR-013 / FR-015 / FR-020: 清单校验失败的整体拒绝 (tasks.md 的 T038).
//!
//! 本文件的职责是 T038 的**端到端矩阵**: V-1 至 V-10 逐条的"退出码 2 + 文件系统零
//! 改动", 外加 T038 的四条补充 (FR-015 不向上搜索 / 当前工作目录不可用 / 主目录无法
//! 确定 / `Unreadable` 不是校验失败).
//!
//! 与 `src/manifest.rs` 的单元测试分工: 那里以纯函数形式断言 V-1 到 V-10 的**校验逻辑
//! 本身** (能直接看到条目下标与错误值); 这里经**真实二进制**断言校验失败对外承担的
//! 契约 -- 退出码与零改动 -- 以及单测覆盖不到的"从当前工作目录取清单"这一步.
//!
//! 本文件经真实二进制断言, 因此依赖 `src/lib.rs` 的命令接入 (T011/T015). 在那些接入
//! 完成之前, 这里的断言必然失败 -- 这是预期的 TDD 红阶段 (测试先于实现存在), 不得
//! 为了让本文件变绿而改动 `src/` 下的任何文件.

mod support;

use std::fs;
use std::path::Path;
use std::process::Command;

use support::{
    Run, TempEnv, entry_line_count, env_with_locked_target, line_for, run, running_as_root,
    snapshot_all, token_of,
};

/// V-1 到 V-10 逐条: 四条命令都必须拒绝, 且拒绝必须是零改动.
const COMMANDS: [&str; 4] = ["preview", "apply", "status", "undo"];

/// 在指定的 **cwd** 下运行 `dotlink <args...>`, 其余环境同夹具.
fn run_in(env: &TempEnv, cwd: &Path, args: &[&str]) -> Run {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin("dotlink"));
    command.current_dir(cwd);
    command.env("HOME", env.home());
    command.env_remove("XDG_CONFIG_HOME");
    command.env_remove("USERPROFILE");
    command.args(args);
    observe(command.output().expect("运行 dotlink"))
}

/// 在**没有** `HOME` 的子进程中运行 `dotlink <args...>`.
fn run_without_home(env: &TempEnv, args: &[&str]) -> Run {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin("dotlink"));
    command.current_dir(env.repo());
    command.env_remove("HOME");
    command.env_remove("XDG_CONFIG_HOME");
    command.env_remove("USERPROFILE");
    command.args(args);
    observe(command.output().expect("运行 dotlink"))
}

/// 把子进程的 cwd 设为一个**随后被删除**的目录, 再 `exec` 目标二进制.
///
/// 不能用 `Command::current_dir` 指向一个已删除的目录: 那会让子进程在 `exec` **之前**
/// 就失败 (`run()` 根本不会被调用), 断言会退化成"进程启动失败", 永远测不到真正的
/// "当前工作目录不可用"路径. 因此用 `sh -c` 在进程内完成 `cd` 与 `rmdir`.
///
/// 同一处还有第二个陷阱: 必须 `unset PWD`. shell 会用继承来的 `PWD` 把逻辑 cwd 恢复
/// 成一个仍然可用的路径 (即使内含的目录已被删除), 于是 `current_dir()` 又能成功,
/// 用例同样退化成"清单不存在". 这两条是该用例沦为假阳性的两道防线.
fn run_in_deleted_cwd(env: &TempEnv, args: &[&str]) -> Run {
    let binary = assert_cmd::cargo::cargo_bin("dotlink");
    let gone = env.repo().join("gone-cwd");

    let mut script =
        String::from("mkdir -p \"$1\" && cd \"$1\" && rmdir \"$1\" && unset PWD && exec \"$2\"");
    for arg in args {
        script.push_str(&format!(" \"{arg}\""));
    }

    let mut command = Command::new("/bin/sh");
    command.arg("-c").arg(script);
    command.arg("sh");
    command.arg(&gone);
    command.arg(&binary);
    command.current_dir(env.repo());
    command.env("HOME", env.home());
    command.env_remove("PWD");
    command.env_remove("XDG_CONFIG_HOME");
    command.env_remove("USERPROFILE");

    observe(command.output().expect("运行 sh 包装"))
}

/// 把 `Output` 折成观测结果.
fn observe(output: std::process::Output) -> Run {
    Run {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// 一条清单违规的完整断言: 四条命令都退 2, 标准输出为空, 标准错误指出 `needle`,
/// 且整个过程**零改动**.
///
/// `needle` 对应 `src/manifest.rs` 中该条规则的说明措辞 (V-1 到 V-10 的规则各有一
/// 组措辞), 因此断言锁定的不是自由文本的措辞, 而是"哪一条规则被触发".
#[track_caller]
fn assert_all_commands_reject(env: &TempEnv, needle: &str) {
    let before = snapshot_all(env);

    for command in COMMANDS {
        let run = run(env, &[command]);
        assert_eq!(
            run.code, 2,
            "`{command}` 必须把清单校验失败报成退出码 2; 实际 {};\nstdout:\n{}\nstderr:\n{}",
            run.code, run.stdout, run.stderr
        );
        assert_eq!(
            run.stdout, "",
            "清单校验失败不得产生任何标准输出 (`{command}`)"
        );
        assert!(
            run.stderr.contains(needle),
            "`{command}` 的标准错误必须指出原因 {needle:?}; 实际:\n{}",
            run.stderr
        );
        assert!(
            !run.stderr.contains("Usage"),
            "`{command}` 落的不是 clap 的用法错误路径: {}",
            run.stderr
        );
    }

    assert_eq!(
        before,
        snapshot_all(env),
        "FR-013: 清单校验失败必须在任何文件系统改动之前完成 (零改动)"
    );
}

/// 夹具: 一份合法清单所需的源文件, 使"零改动"具有判别力 (若命令真的跑起来, 它会
/// 建立链接或备份, 快照就会变).
fn env_with_source() -> TempEnv {
    let env = TempEnv::new();
    env.write_source("shell/bashrc", "managed\n");
    env
}

// ---------------------------------------------------------------------------
// V-1: 清单文件存在且可读
// ---------------------------------------------------------------------------

#[test]
fn v1_a_missing_manifest_is_rejected() {
    let env = env_with_source();
    // 根本没有 dotlink.toml.

    assert_all_commands_reject(&env, "无法读取清单");

    // 说明必须给出相对当前工作目录的清单路径, 而不是泛泛的"失败".
    let run = run(&env, &["preview"]);
    assert!(
        run.stderr.contains("dotlink.toml"),
        "V-1 的说明必须给出清单位置: {}",
        run.stderr
    );
}

// ---------------------------------------------------------------------------
// V-2: TOML 语法合法 (报告出错位置与原因)
// ---------------------------------------------------------------------------

#[test]
fn v2_a_syntax_error_is_rejected_with_its_position() {
    let env = env_with_source();
    env.write_manifest("[[entry]]\nsource = a\ntarget = \"b\"\n");

    assert_all_commands_reject(&env, "清单语法错误");

    let run = run(&env, &["preview"]);
    assert!(
        run.stderr.contains("第 2 行"),
        "V-2 必须报告出错位置: {}",
        run.stderr
    );
}

// ---------------------------------------------------------------------------
// V-3: `entry` 非空
// ---------------------------------------------------------------------------

#[test]
fn v3_an_empty_entry_array_is_rejected() {
    let env = env_with_source();
    env.write_manifest("entry = []\n");

    assert_all_commands_reject(&env, "entry");
}

#[test]
fn v3_a_manifest_without_the_entry_key_is_rejected() {
    let env = env_with_source();
    env.write_manifest("# 只有注释, 没有任何条目\n");

    assert_all_commands_reject(&env, "entry");
}

// ---------------------------------------------------------------------------
// V-4: 每个条目 `source` 与 `target` 均非空
// ---------------------------------------------------------------------------

#[test]
fn v4_a_missing_target_is_rejected() {
    let env = env_with_source();
    env.write_manifest("[[entry]]\nsource = \"shell/bashrc\"\n");

    assert_all_commands_reject(&env, "target");

    let run = run(&env, &["preview"]);
    assert!(
        run.stderr.contains("entry[0]"),
        "V-4 必须指出条目下标: {}",
        run.stderr
    );
}

#[test]
fn v4_a_missing_source_is_rejected() {
    let env = env_with_source();
    env.write_manifest("[[entry]]\ntarget = \"~/.bashrc\"\n");

    assert_all_commands_reject(&env, "source");

    let run = run(&env, &["preview"]);
    assert!(
        run.stderr.contains("entry[0]"),
        "V-4 必须指出条目下标: {}",
        run.stderr
    );
}

#[test]
fn v4_a_source_outside_the_repo_shape_is_rejected() {
    let env = env_with_source();
    // 源条目只接受仓库内相对路径: `~/...` 与绝对路径都属非法形态 (T059).
    for written in ["~/src/rc", "/etc/hosts"] {
        env.write_manifest(&format!(
            "[[entry]]\nsource = \"{written}\"\ntarget = \"~/.bashrc\"\n"
        ));

        assert_all_commands_reject(&env, "源条目");

        let run = run(&env, &["preview"]);
        assert!(
            run.stderr.contains("entry[0]"),
            "非法 source 形态必须指出条目下标: {}",
            run.stderr
        );
    }
}

#[test]
fn v4_a_source_that_resolves_outside_the_repo_is_rejected() {
    let env = env_with_source();
    // `..` 折叠能把 `repo_root.join(source)` 带出仓库: 建出的链接指向仓库外,
    // undo 的归属判定 (落在仓库内才算本工具创建) 会拒删自己的产物, 因此与
    // `~` / 绝对路径一样整体拒绝.
    for written in ["../evil", "sub/../../evil"] {
        env.write_manifest(&format!(
            "[[entry]]\nsource = \"{written}\"\ntarget = \"~/.bashrc\"\n"
        ));

        assert_all_commands_reject(&env, "仓库根之外");

        let run = run(&env, &["preview"]);
        assert!(
            run.stderr.contains("entry[0]") && run.stderr.contains(written),
            "必须指出条目下标与书写形态 {}: {}",
            written,
            run.stderr
        );
    }
}

#[test]
fn v4_a_source_escaping_through_a_repo_symlink_is_rejected() {
    let env = env_with_source();
    // 仓库内的符号链接同样能把源带出仓库: `os::normalize` 解析祖先段的符号链接,
    // 越界判定沿用同一语义.
    let outside = tempfile::tempdir().expect("创建仓库外目录");
    let outside = fs::canonicalize(outside.path()).expect("仓库外目录的物理路径");
    std::os::unix::fs::symlink(&outside, env.repo().join("escape")).expect("建立逃逸链接");

    env.write_manifest("[[entry]]\nsource = \"escape/evil\"\ntarget = \"~/.bashrc\"\n");

    assert_all_commands_reject(&env, "仓库根之外");

    let run = run(&env, &["preview"]);
    assert!(
        run.stderr.contains("entry[0]") && run.stderr.contains("escape/evil"),
        "必须指出条目下标与书写形态: {}",
        run.stderr
    );
}

#[test]
fn v4_parent_segments_that_fold_back_inside_the_repo_are_accepted() {
    let env = env_with_source();
    // 仓库内的 `..` 折叠 (`a/../b` 形态) 归一化后仍在仓库内, 不得被越界规则误伤.
    env.write_manifest(
        "[[entry]]\nsource = \"shell/../shell/bashrc\"\ntarget = \"~/sub/bashrc\"\n",
    );

    let run = run(&env, &["apply"]);
    assert_eq!(
        run.code, 0,
        "折叠后仍在仓库内的源必须被接受:\nstdout:\n{}\nstderr:\n{}",
        run.stdout, run.stderr
    );
    env.assert_symlink_to(
        &env.home().join("sub/bashrc"),
        &env.repo().join("shell/bashrc"),
    );
}

// ---------------------------------------------------------------------------
// V-5: 不存在两个条目声明同一个目标路径
// ---------------------------------------------------------------------------

#[test]
fn v5_two_entries_with_the_same_target_are_rejected() {
    let env = env_with_source();
    // 书写形态不同, 规范化后同一路径.
    env.write_manifest(
        "[[entry]]\nsource = \"shell/bashrc\"\ntarget = \"~/.bashrc\"\n\n\
         [[entry]]\nsource = \"shell/bashrc\"\ntarget = \"~/./.bashrc\"\n",
    );

    assert_all_commands_reject(&env, "目标路径");

    let run = run(&env, &["preview"]);
    assert!(
        run.stderr.contains("entry[0]") && run.stderr.contains("entry[1]"),
        "V-5 必须指出冲突的两个条目下标: {}",
        run.stderr
    );
}

// ---------------------------------------------------------------------------
// V-6: 每个条目的 `target` 可解析为绝对路径
// ---------------------------------------------------------------------------

#[test]
fn v6_an_unresolvable_target_is_rejected() {
    let env = env_with_source();
    // `HOME` 被夹具显式设为临时主目录, 因此这里只能构造"条目内部形态非法"的一类:
    // 目标为空串. 真正依赖"主目录无法确定"的那条由本文件末尾的
    // `home_is_unresolvable_without_home_variable` 覆盖.
    env.write_manifest("[[entry]]\nsource = \"shell/bashrc\"\ntarget = \"\"\n");

    assert_all_commands_reject(&env, "target");

    let run = run(&env, &["preview"]);
    assert!(
        run.stderr.contains("entry[0]"),
        "V-6 / V-4 必须指出条目下标: {}",
        run.stderr
    );
}

#[test]
fn v6_a_tilde_without_slash_target_is_rejected() {
    let env = env_with_source();
    // 漏写斜杠的 `~.bashrc`: 若放行会落到"相对仓库根"分支, 在仓库内静默建出名为
    // `~.bashrc` 的路径 (而仓库恰是受版本控制的目录), 因此必须整体拒绝 (T058).
    env.write_manifest("[[entry]]\nsource = \"shell/bashrc\"\ntarget = \"~.bashrc\"\n");

    assert_all_commands_reject(&env, "~.bashrc");

    let run = run(&env, &["preview"]);
    assert!(
        run.stderr.contains("entry[0]"),
        "V-6 必须指出条目下标: {}",
        run.stderr
    );
}

// ---------------------------------------------------------------------------
// V-7: `on_conflict` 取值属于 {skip, replace_and_backup}
// ---------------------------------------------------------------------------

#[test]
fn v7_an_illegal_on_conflict_value_is_rejected() {
    let env = env_with_source();
    env.write_manifest(
        "[[entry]]\nsource = \"shell/bashrc\"\ntarget = \"~/.bashrc\"\non_conflict = \"yes\"\n",
    );

    assert_all_commands_reject(&env, "on_conflict");

    let run = run(&env, &["preview"]);
    assert!(
        run.stderr.contains("yes"),
        "V-7 必须指出非法取值: {}",
        run.stderr
    );
    assert!(
        run.stderr.contains("entry[0]"),
        "V-7 必须指出条目下标: {}",
        run.stderr
    );
}

// ---------------------------------------------------------------------------
// V-8: 只出现已定义的键 (两层各自校验)
// ---------------------------------------------------------------------------

#[test]
fn v8_an_unknown_key_inside_an_entry_is_rejected() {
    let env = env_with_source();
    // 拼错的 on_conflict: 若被静默忽略, 用户会以为配置生效了却没有生效.
    env.write_manifest(
        "[[entry]]\nsource = \"shell/bashrc\"\ntarget = \"~/.bashrc\"\n\
         onconflict = \"replace_and_backup\"\n",
    );

    assert_all_commands_reject(&env, "onconflict");

    let run = run(&env, &["preview"]);
    assert!(
        run.stderr.contains("条目内"),
        "V-8 必须指出键名与所在层级: {}",
        run.stderr
    );
}

#[test]
fn v8_an_unknown_top_level_table_is_rejected() {
    let env = env_with_source();
    env.write_manifest(
        "[[entry]]\nsource = \"shell/bashrc\"\ntarget = \"~/.bashrc\"\n\n[foo]\nbar = 1\n",
    );

    assert_all_commands_reject(&env, "foo");

    let run = run(&env, &["preview"]);
    assert!(
        run.stderr.contains("顶层"),
        "V-8 必须指出键名与所在层级: {}",
        run.stderr
    );
}

// ---------------------------------------------------------------------------
// V-9: 目标落点不得退化 (文件系统根 / 当前用户主目录 / 仓库根本身或其祖先)
// ---------------------------------------------------------------------------

#[test]
fn v9_the_file_system_root_target_is_rejected() {
    let env = env_with_source();
    // 落点是整个文件系统根: 一旦落地 (`replace_and_backup` 尤甚) 毁掉的是一整棵树.
    env.write_manifest("[[entry]]\nsource = \"shell/bashrc\"\ntarget = \"/\"\n");

    assert_all_commands_reject(&env, "文件系统根");

    let run = run(&env, &["preview"]);
    assert!(
        run.stderr.contains("entry[0]") && run.stderr.contains("`/`"),
        "V-9 必须指出条目下标与书写形态: {}",
        run.stderr
    );
}

#[test]
fn v9_the_home_target_is_rejected() {
    let env = env_with_source();
    // 裸 `~` 与书成绝对路径的等价形态都指向主目录本身.
    for written in ["~".to_string(), env.home().display().to_string()] {
        env.write_manifest(&format!(
            "[[entry]]\nsource = \"shell/bashrc\"\ntarget = \"{written}\"\n"
        ));

        assert_all_commands_reject(&env, "主目录");

        let run = run(&env, &["preview"]);
        assert!(
            run.stderr.contains("entry[0]") && run.stderr.contains(&format!("`{written}`")),
            "V-9 必须指出条目下标与书写形态 {written}: {}",
            run.stderr
        );
    }
}

#[test]
fn v9_the_repo_root_and_its_ancestors_are_rejected() {
    let env = env_with_source();
    // `.` 归一化后是仓库本身, `..` 是它的祖先: 命中源仓库的落点一旦被
    // `replace_and_backup` 替换备份, 毁掉的就是整个仓库.
    for written in [".", ".."] {
        env.write_manifest(&format!(
            "[[entry]]\nsource = \"shell/bashrc\"\ntarget = \"{written}\"\n"
        ));

        assert_all_commands_reject(&env, "仓库根本身或其祖先");

        let run = run(&env, &["preview"]);
        assert!(
            run.stderr.contains("entry[0]") && run.stderr.contains(&format!("`{written}`")),
            "V-9 必须指出条目下标与书写形态 {written}: {}",
            run.stderr
        );
    }
}

#[test]
fn v9_legal_target_shapes_are_not_harmed() {
    let env = env_with_source();
    env.write_source("git/gitconfig", "managed\n");
    // 三种合法形态不得被误伤: 主目录之下的 `~/sub`, `/etc/hosts` 这类绝对路径
    // (此处取仓库与 HOME 之外的一个临时目录), 以及相对仓库根的 `link/bashrc`.
    let outside = tempfile::tempdir().expect("创建仓库外目录");
    let outside = fs::canonicalize(outside.path()).expect("仓库外目录的物理路径");
    env.write_manifest(&format!(
        "[[entry]]\nsource = \"shell/bashrc\"\ntarget = \"~/sub/bashrc\"\n\n\
         [[entry]]\nsource = \"git/gitconfig\"\ntarget = \"{}/abs/gitconfig\"\n\n\
         [[entry]]\nsource = \"shell/bashrc\"\ntarget = \"link/bashrc\"\n",
        outside.display()
    ));

    let run = run(&env, &["apply"]);
    assert_eq!(
        run.code, 0,
        "合法落点不得被误伤:\nstdout:\n{}\nstderr:\n{}",
        run.stdout, run.stderr
    );
    env.assert_symlink_to(
        &env.home().join("sub/bashrc"),
        &env.repo().join("shell/bashrc"),
    );
    env.assert_symlink_to(
        &outside.join("abs/gitconfig"),
        &env.repo().join("git/gitconfig"),
    );
    env.assert_symlink_to(
        &env.repo().join("link/bashrc"),
        &env.repo().join("shell/bashrc"),
    );
}

// ---------------------------------------------------------------------------
// V-10: 目标不得指向本条目自己的 source (自指链接恒为断链)
// ---------------------------------------------------------------------------

#[test]
fn v10_a_self_referential_target_is_rejected() {
    let env = env_with_source();
    env.write_manifest("[[entry]]\nsource = \"shell/bashrc\"\ntarget = \"shell/bashrc\"\n");

    assert_all_commands_reject(&env, "自指");

    let run = run(&env, &["preview"]);
    assert!(
        run.stderr.contains("entry[0]") && run.stderr.contains("`shell/bashrc`"),
        "V-10 必须指出条目下标与书写形态: {}",
        run.stderr
    );
}

// ---------------------------------------------------------------------------
// T038 补充 1: FR-015 不向上搜索父目录
// ---------------------------------------------------------------------------

#[test]
fn the_manifest_is_not_searched_in_parent_directories() {
    let env = env_with_source();
    env.write_manifest("[[entry]]\nsource = \"shell/bashrc\"\ntarget = \"~/.bashrc\"\n");

    // 仓库的子目录: 它自己没有清单, 父目录 (仓库根) 有.
    let nested = env.repo().join("sub/deeper");
    fs::create_dir_all(&nested).expect("创建子目录");

    let before = snapshot_all(&env);

    for command in COMMANDS {
        let run = run_in(&env, &nested, &[command]);
        assert_eq!(
            run.code, 2,
            "FR-015: 在仓库子目录中执行视为清单不存在; `{command}` 实际退 {};\nstderr:\n{}",
            run.code, run.stderr
        );
        assert_eq!(run.stdout, "", "校验失败不得产生标准输出 (`{command}`)");
        assert!(
            run.stderr.contains("无法读取清单"),
            "说明必须可区分于 `当前工作目录不可用`: {}",
            run.stderr
        );
    }

    assert_eq!(before, snapshot_all(&env), "零改动");
}

// ---------------------------------------------------------------------------
// T038 补充 2: 当前工作目录不可用 (FR-015 的边界, research.md R5)
// ---------------------------------------------------------------------------

#[test]
fn an_unavailable_cwd_is_rejected_and_distinguished_from_a_missing_manifest() {
    let env = env_with_source();
    env.write_manifest("[[entry]]\nsource = \"shell/bashrc\"\ntarget = \"~/.bashrc\"\n");

    let before = snapshot_all(&env);

    for command in COMMANDS {
        let run = run_in_deleted_cwd(&env, &[command]);
        assert_eq!(
            run.code, 2,
            "cwd 不可用时同样是校验失败 (退出码 2); `{command}` 实际退 {};\nstdout:\n{}\nstderr:\n{}",
            run.code, run.stdout, run.stderr
        );
        assert_eq!(run.stdout, "", "校验失败不得产生标准输出 (`{command}`)");
        assert!(
            run.stderr.contains("当前工作目录不可用"),
            "`{command}` 的说明必须指出目录不可用, 而非找不到清单: {}",
            run.stderr
        );
        assert!(
            !run.stderr.contains("无法读取清单"),
            "目录不可用时不得报成清单缺失: {}",
            run.stderr
        );
        assert!(
            !run.stderr.contains("dotlink.toml"),
            "目录不可用时连清单位置都无从谈起: {}",
            run.stderr
        );
        // 若 shell 包装没有真正 exec 到二进制, 退出码会是 126/127; 上面已断言为 2,
        // 这里再确认标准错误不是 shell 自己的报错.
        assert!(
            !run.stderr.contains("sh:") && !run.stderr.contains("not found"),
            "子进程必须真的跑到了 dotlink: {}",
            run.stderr
        );
    }

    assert_eq!(before, snapshot_all(&env), "零改动");
}

// ---------------------------------------------------------------------------
// T038 补充 3: 主目录无法确定 (FR-002 的边界, research.md R5)
// ---------------------------------------------------------------------------

#[test]
fn home_is_unresolvable_without_the_home_variable() {
    let env = env_with_source();
    // 绝对路径的目标不需要 HOME, 因此该清单只在 `~` 上依赖主目录.
    env.write_manifest("[[entry]]\nsource = \"shell/bashrc\"\ntarget = \"~/.bashrc\"\n");

    let before = snapshot_all(&env);

    for command in COMMANDS {
        let run = run_without_home(&env, &[command]);
        assert_eq!(
            run.code, 2,
            "V-6: 主目录无法确定时整体拒绝; `{command}` 实际退 {};\nstderr:\n{}",
            run.code, run.stderr
        );
        assert_eq!(run.stdout, "", "校验失败不得产生标准输出 (`{command}`)");
        assert!(
            run.stderr.contains("HOME"),
            "说明必须指出主目录无法确定: {}",
            run.stderr
        );
    }

    assert_eq!(before, snapshot_all(&env), "零改动");
}

#[test]
fn a_target_that_does_not_need_home_is_accepted_without_the_home_variable() {
    let env = env_with_source();
    // 绝对路径的目标不依赖 HOME: 该清单在无 HOME 时仍必须被接受 (否则上一条断言
    // 只是"任何情况下都退 2", 没有把 V-6 钉在 `~` 上).
    env.write_manifest(&format!(
        "[[entry]]\nsource = \"shell/bashrc\"\ntarget = \"{}\"\n",
        env.repo().join("linked/bashrc").display()
    ));

    let run = run_without_home(&env, &["preview"]);
    assert_ne!(
        run.code, 2,
        "绝对路径的目标不需要 HOME, 不得因缺少 HOME 而被拒绝;\nstderr:\n{}",
        run.stderr
    );
}

// ---------------------------------------------------------------------------
// T038 补充 4: `Unreadable` 不是校验失败 (退出码 1, 非 2/3)
// ---------------------------------------------------------------------------

#[test]
fn an_unreadable_entry_is_a_failure_not_a_validation_error() {
    if running_as_root() {
        eprintln!("以 root 运行: chmod 000 不生效, 跳过 Unreadable 的退出码断言");
        return;
    }

    for command in COMMANDS {
        let (env, locked) = env_with_locked_target();

        let run = run(&env, &[command]);

        assert_eq!(
            run.code, 1,
            "`Unreadable` 是判定阶段的环境故障, 不是清单校验失败: 退出码必须是 1 而非 2/3;\
             `{command}` 实际退 {};\nstdout:\n{}\nstderr:\n{}",
            run.code, run.stdout, run.stderr
        );
        assert_ne!(run.code, 2, "不得与清单校验失败同码 (`{command}`)");
        assert_ne!(run.code, 3, "不得与平台不受支持同码 (`{command}`)");

        let line = line_for(&run.stdout, "~/.locked/rc");
        assert_eq!(
            token_of(line),
            "fail",
            "`Unreadable` 在四条命令下的 token 都是 fail (`{command}`): {line}"
        );

        let summary = run.stdout.lines().last().unwrap_or_default();
        assert!(
            summary.contains("1 失败"),
            "该条目必须计入汇总的 `失败` 列 (`{command}`): {summary}"
        );
        assert_eq!(
            entry_line_count(&run.stdout),
            2,
            "其余条目仍必须被处理并各占一行 (`{command}`):\n{}",
            run.stdout
        );

        // 其余条目按各自命令的正常语义处理.
        let expected_other = match command {
            "preview" => "create",
            "apply" => "create",
            "status" => "absent",
            _ => "ok",
        };
        assert_eq!(
            token_of(line_for(&run.stdout, "~/.bashrc")),
            expected_other,
            "`{command}` 下正常条目必须照常处理:\n{}",
            run.stdout
        );

        // 权限位恢复, 使临时目录能被正常清理, 同时确认该条目没有留下任何链接.
        fs::set_permissions(&locked, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .expect("恢复权限位");
        assert!(
            fs::symlink_metadata(locked.join("rc")).is_err(),
            "`{command}` 不得在不可进入的目录中留下任何东西: {}",
            locked.display()
        );
    }
}

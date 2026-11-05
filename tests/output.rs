//! 契约 "报告写出失败不改变命令结局": 标准输出 / 标准错误被提前关闭时不得 panic.
//!
//! 用真实进程与管道覆盖内核路径 (`BrokenPipe` 只在读端关闭时出现), 单元测试
//! (`src/lib.rs` 的 `write_report` 用例) 锁定错误分类本身.

mod support;

use std::process::Stdio;

use support::{TempEnv, manifest_of, run};

/// 测试侧的近似显示宽度: CJK 与常见 emoji 计 2 列, 组合记号与变体选择符计 0 列.
///
/// 这是只覆盖用例字符的独立近似; `src/render.rs` 的完整区间表由单元测试逐码点锁定.
fn display_width(text: &str) -> usize {
    text.chars()
        .map(|character| match u32::from(character) {
            0x0300..=0x036F | 0xFE00..=0xFE0F => 0,
            0x2E80..=0x9FFF | 0xF900..=0xFAFF | 0xFF00..=0xFF60 => 2,
            0x231A..=0x231B | 0x1F300..=0x1FAFF => 2,
            _ => 1,
        })
        .sum()
}

/// 生成含 `count` 条目的清单: 所有条目共享一个已存在的源, 目标全部不存在.
fn large_manifest(env: &TempEnv, count: usize) {
    env.write_source("shared/include.sh", "export SHARED=1\n");
    let mut manifest = String::new();
    for index in 0..count {
        manifest.push_str(&format!(
            "[[entry]]\nsource = \"shared/include.sh\"\ntarget = \"~/.targets/t{index}\"\n"
        ));
    }
    env.write_manifest(&manifest);
}

/// 标准输出被提前关闭 (例如 `dotlink status | head -1`): 不 panic, 标准错误为空,
/// 退出码仍按目标谓词计算 (这里目标未达成, 退 1).
#[test]
fn a_closed_stdout_does_not_panic_and_keeps_the_exit_code() {
    let env = TempEnv::new();
    // 报告远大于管道缓冲: 读端关闭后写端必然拿到 `BrokenPipe`.
    large_manifest(&env, 5000);

    let mut child = env
        .raw_cmd()
        .arg("status")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("启动 dotlink status");

    // 立即关闭标准输出读端, 子进程在写出报告时收到 `BrokenPipe`.
    drop(child.stdout.take());

    let output = child.wait_with_output().expect("等待 dotlink status");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !stderr.contains("panicked"),
        "关闭标准输出不得 panic: {stderr}"
    );
    assert_eq!(stderr, "", "BrokenPipe 必须静默: {stderr}");
    assert_eq!(
        output.status.code(),
        Some(1),
        "写出失败不参与结局: 目标未达成的 status 仍退 1"
    );
}

/// 标准错误被提前关闭: 清单错误的说明无法写出, 不 panic, 仍退 2.
#[test]
fn a_closed_stderr_does_not_panic_and_keeps_the_exit_code() {
    // 没有清单的临时目录: 清单错误路径会写一行标准错误, 然后以 2 退出.
    let env = TempEnv::new();

    let mut child = env
        .raw_cmd()
        .arg("status")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("启动 dotlink status");

    // 立即关闭标准错误读端: `eprintln!` 在这条路径上会 panic 并退 101.
    drop(child.stderr.take());

    let output = child.wait_with_output().expect("等待 dotlink status");

    assert_eq!(
        output.status.code(),
        Some(2),
        "清单错误在标准错误不可写时仍退 2, 而不是 panic 的 101"
    );
    assert!(output.stdout.is_empty(), "清单错误不得写标准输出");
}

/// 12 条含 ASCII 与 CJK target 的清单: 序号右对齐, token 与说明列按显示宽度对齐.
#[test]
fn report_columns_align_for_twelve_entries_with_cjk_targets() {
    let env = TempEnv::new();
    env.write_source("shared/rc", "x\n");
    let mut manifest = String::new();
    for index in 1..=12 {
        let target = if index % 2 == 0 {
            format!("~/.配置{index}")
        } else {
            format!("~/.target{index}")
        };
        manifest.push_str(&format!(
            "[[entry]]\nsource = \"shared/rc\"\ntarget = \"{target}\"\n"
        ));
    }
    env.write_manifest(&manifest);

    let run = run(&env, &["status"]);
    assert_eq!(run.code, 1, "{}", run.stdout);
    let lines: Vec<&str> = run.stdout.lines().collect();
    assert_eq!(lines.len(), 13, "12 条条目加一行汇总:\n{}", run.stdout);

    let mut token_starts = Vec::new();
    let mut note_starts = Vec::new();
    for line in &lines[..12] {
        let token_index = line.find("absent").expect("token");
        let note_index = line.find("未部署").expect("note");
        token_starts.push(display_width(&line[..token_index]));
        note_starts.push(display_width(&line[..note_index]));
    }
    assert!(
        token_starts.iter().all(|start| *start == token_starts[0]),
        "token 列未对齐: {token_starts:?}\n{}",
        run.stdout
    );
    assert!(
        note_starts.iter().all(|start| *start == note_starts[0]),
        "说明列未对齐: {note_starts:?}\n{}",
        run.stdout
    );
    // ` 1/12 ` 宽 6, token 列 9, 分隔空格 1, target 列宽 20.
    assert_eq!(token_starts[0], 6);
    assert_eq!(note_starts[0], 6 + 9 + 1 + 20);

    assert_eq!(
        lines[12], "12 个条目: 0 成功, 0 已是最新, 12 未处理, 0 失败",
        "汇总行必须是最后一行"
    );
}

/// 等显示宽度的 target 说明列起点相同: 宽字符符号与 emoji 各计 2 列.
#[test]
fn wide_symbols_and_emoji_align_with_ascii_targets_of_equal_width() {
    let env = TempEnv::new();
    env.write_source("shared/rc", "x\n");
    let entries = [
        ("shared/rc", "~/ab", "-"),
        ("shared/rc", "~/⌚", "-"),
        ("shared/rc", "~/🚀", "-"),
        ("shared/rc", "~/.配置", "-"),
        ("shared/rc", "~/.abcd", "-"),
    ];
    env.write_manifest(&manifest_of(&entries));

    let run = run(&env, &["preview"]);
    assert_eq!(run.code, 1, "{}", run.stdout);
    let lines: Vec<&str> = run.stdout.lines().collect();
    assert_eq!(lines.len(), 6, "5 条条目加一行汇总:\n{}", run.stdout);
    for (index, (_, target, _)) in entries.iter().copied().enumerate() {
        assert!(
            lines[index].contains(target),
            "行内应原样保留 target {target}: {}",
            lines[index]
        );
    }

    let note_starts: Vec<usize> = lines[..5]
        .iter()
        .map(|line| {
            let index = line.find("未部署").expect("note");
            display_width(&line[..index])
        })
        .collect();
    assert!(
        note_starts.iter().all(|start| *start == note_starts[0]),
        "等显示宽度 target 的说明列未对齐: {note_starts:?}\n{}",
        run.stdout
    );
}

/// 整块保留的码点 (块内未分配码点与文本呈现型 emoji) 也计 2 列, 与等宽 ASCII target 对齐.
#[test]
fn retained_whole_block_codepoints_align_with_ascii_targets_of_equal_width() {
    let env = TempEnv::new();
    env.write_source("shared/rc", "x\n");
    let entries = [
        ("shared/rc", "~/ab", "-"),
        ("shared/rc", "~/\u{1F321}", "-"),
        ("shared/rc", "~/\u{3248}", "-"),
    ];
    env.write_manifest(&manifest_of(&entries));

    let run = run(&env, &["preview"]);
    assert_eq!(run.code, 1, "{}", run.stdout);
    let lines: Vec<&str> = run.stdout.lines().collect();
    assert_eq!(lines.len(), 4, "3 条条目加一行汇总:\n{}", run.stdout);

    let note_starts: Vec<usize> = lines[..3]
        .iter()
        .map(|line| {
            let index = line.find("未部署").expect("note");
            display_width(&line[..index])
        })
        .collect();
    assert_eq!(
        note_starts[0], note_starts[1],
        "文本呈现型 emoji 应计 2 列: {note_starts:?}\n{}",
        run.stdout
    );
    assert_eq!(
        note_starts[0], note_starts[2],
        "CJK 区块内的 Ambiguous 码点应计 2 列: {note_starts:?}\n{}",
        run.stdout
    );
}

/// 区段之外的 Wide 组合记号 (U+3099) 按 2 列计, 与等显示宽度的 ASCII target 对齐.
#[test]
fn wide_combining_mark_outside_zero_width_ranges_counts_as_two_columns() {
    let env = TempEnv::new();
    env.write_source("shared/rc", "x\n");
    let entries = [
        ("shared/rc", "~/.abc", "-"),
        ("shared/rc", "~/.a\u{3099}", "-"),
    ];
    env.write_manifest(&manifest_of(&entries));

    let run = run(&env, &["preview"]);
    assert_eq!(run.code, 1, "{}", run.stdout);
    let lines: Vec<&str> = run.stdout.lines().collect();
    assert_eq!(lines.len(), 3, "2 条条目加一行汇总:\n{}", run.stdout);
    assert!(
        lines[1].contains("~/.a\u{3099}"),
        "区段之外的组合记号应原样保留在行内: {}",
        lines[1]
    );

    let note_starts: Vec<usize> = lines[..2]
        .iter()
        .map(|line| {
            let index = line.find("未部署").expect("note");
            display_width(&line[..index])
        })
        .collect();
    assert_eq!(
        display_width("~/.a\u{3099}"),
        6,
        "区段之外的 Wide 组合记号应计 2 列"
    );
    assert_eq!(
        note_starts[0], note_starts[1],
        "区段之外的 Wide 组合记号应与等显示宽度的 ASCII target 对齐: {note_starts:?}\n{}",
        run.stdout
    );
}

/// 组合记号不占列; 超宽 target 不截断, 只把该行的说明列右移.
#[test]
fn combining_marks_align_and_overwide_targets_only_shift_their_own_note() {
    let env = TempEnv::new();
    env.write_source("shared/rc", "x\n");
    let long_target = format!("~/.{}", "a".repeat(25));
    let entries = [
        ("shared/rc", "~/.e\u{301}", "-"),
        ("shared/rc", "~/ab", "-"),
        ("shared/rc", long_target.as_str(), "-"),
    ];
    env.write_manifest(&manifest_of(&entries));

    let run = run(&env, &["preview"]);
    assert_eq!(run.code, 1, "{}", run.stdout);
    let lines: Vec<&str> = run.stdout.lines().collect();
    assert_eq!(lines.len(), 4, "3 条条目加一行汇总:\n{}", run.stdout);
    assert!(
        lines[0].contains("~/.e\u{301}"),
        "组合记号应原样保留在行内: {}",
        lines[0]
    );
    assert!(
        lines[1].contains("~/ab"),
        "行内应原样保留 target: {}",
        lines[1]
    );

    let note_starts: Vec<usize> = lines[..3]
        .iter()
        .map(|line| {
            let index = line.find("未部署").expect("note");
            display_width(&line[..index])
        })
        .collect();
    assert_eq!(
        note_starts[0], note_starts[1],
        "组合记号不得占列: {note_starts:?}\n{}",
        run.stdout
    );
    assert!(
        note_starts[2] > note_starts[0],
        "超宽 target 应使说明列右移: {note_starts:?}\n{}",
        run.stdout
    );
    assert!(
        lines[2].contains(&long_target),
        "超宽 target 不得被截断: {}",
        lines[2]
    );
}

//! 契约 "报告写出失败不改变命令结局": 标准输出 / 标准错误被提前关闭时不得 panic.
//!
//! 用真实进程与管道覆盖内核路径 (`BrokenPipe` 只在读端关闭时出现), 单元测试
//! (`src/lib.rs` 的 `write_report` 用例) 锁定错误分类本身.

mod support;

use std::process::Stdio;

use support::{TempEnv, run};

/// 测试侧的近似显示宽度: CJK 计 2 列, 其余计 1 列 (与报告层规则一致).
fn display_width(text: &str) -> usize {
    text.chars()
        .map(|character| match u32::from(character) {
            0x2E80..=0x9FFF | 0xF900..=0xFAFF | 0xFF00..=0xFF60 => 2,
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

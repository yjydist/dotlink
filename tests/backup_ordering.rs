//! 跨时区的备份排序回归: 备份名的时间戳必须是 UTC, 否则同一目标在不同 `TZ` 下
//! 生成的备份不在同一时间轴上, `undo` 会恢复较旧的内容 (openspec/changes/
//! harden-backup-ordering 的 "时间戳可比较性" Scenario).
//!
//! 用例经真实二进制断言, 并把 `TZ` 只设在子进程上, 不改测试进程的全局环境.

mod support;

use std::fs;
use std::time::Duration;

use support::{TempEnv, manifest_of, run};

/// 带 `TZ` 的子进程调用: 时间戳必须由 UTC 决定, 不受该变量影响.
fn run_with_tz(env: &TempEnv, tz: &str, args: &[&str]) -> support::Run {
    let mut command = env.raw_cmd();
    command.env("TZ", tz);
    command.args(args);
    let output = command.output().expect("运行 dotlink");
    support::Run {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// 主目录下 `~/.rc` 的全部备份时间戳 (按名字排序).
fn backup_stamps(env: &TempEnv) -> Vec<String> {
    let mut stamps: Vec<String> = fs::read_dir(env.home())
        .expect("列举临时主目录")
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let suffix = name.strip_prefix(".rc.dotlink-backup.")?.to_string();
            // 只取已完整命名的备份: 15 位时间戳, 可带 `.<序号>`.
            (suffix.len() >= 15).then(|| suffix[..15].to_string())
        })
        .collect();
    stamps.sort();
    stamps
}

/// 把 `YYYYMMDDThhmmss` 按 UTC 解析为 Unix 秒.
fn unix_seconds(stamp: &str) -> i64 {
    let year: i32 = stamp[0..4].parse().expect("年份");
    let month: u8 = stamp[4..6].parse().expect("月份");
    let day: u8 = stamp[6..8].parse().expect("日期");
    let hour: u8 = stamp[9..11].parse().expect("小时");
    let minute: u8 = stamp[11..13].parse().expect("分钟");
    let second: u8 = stamp[13..15].parse().expect("秒");
    let date =
        time::Date::from_calendar_date(year, time::Month::try_from(month).expect("合法月份"), day)
            .expect("合法日期");
    time::PrimitiveDateTime::new(
        date,
        time::Time::from_hms(hour, minute, second).expect("合法时间"),
    )
    .assume_utc()
    .unix_timestamp()
}

/// 先在 `TZ=Asia/Shanghai` 下生成旧内容备份, 等待至少 1 秒后在同一目标上以
/// `TZ=UTC` 生成新内容备份. 两个时间戳必须相差不足 60 秒 (而不是 8 小时), 且
/// `undo` 必须恢复较新的内容.
#[test]
fn undo_restores_the_newer_backup_when_tz_changes_between_runs() {
    let env = TempEnv::new();
    env.write_source("rc", "managed\n");
    env.write_manifest(&manifest_of(&[("rc", "~/.rc", "replace_and_backup")]));

    let target = env.home().join(".rc");
    fs::write(&target, "OLD-A\n").unwrap();

    let first = run_with_tz(&env, "Asia/Shanghai", &["apply"]);
    assert_eq!(
        first.code, 0,
        "首次 apply 必须成功:\nstdout:\n{}\nstderr:\n{}",
        first.stdout, first.stderr
    );

    // 第二次替换前清空目标: 移除本工具创建的链接, 再放入新的用户内容.
    fs::remove_file(&target).unwrap();
    std::thread::sleep(Duration::from_secs(1));
    fs::write(&target, "NEW-B\n").unwrap();

    let second = run_with_tz(&env, "UTC", &["apply"]);
    assert_eq!(
        second.code, 0,
        "第二次 apply 必须成功:\nstdout:\n{}\nstderr:\n{}",
        second.stdout, second.stderr
    );

    let stamps = backup_stamps(&env);
    assert_eq!(stamps.len(), 2, "两次替换必须各留一份备份: {stamps:?}");
    assert!(stamps.iter().all(|stamp| stamp.len() == 15), "{stamps:?}");
    let difference = (unix_seconds(&stamps[1]) - unix_seconds(&stamps[0])).abs();
    assert!(
        difference < 60,
        "两份备份的时间戳必须同轴 (相差 {difference} 秒); 本地时区混入时约为 28800 秒: {stamps:?}"
    );

    let undone = run(&env, &["undo"]);
    assert_eq!(
        undone.code, 1,
        "undo 撤销了内容故退 1:\nstdout:\n{}",
        undone.stdout
    );
    env.assert_content(&target, "NEW-B\n");
}

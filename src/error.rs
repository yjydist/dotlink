//! 错误类型与退出码映射.
//!
//! `Error::exit_code` 覆盖派发**之前**的结局 (清单校验失败 / 平台不支持), 并保留
//! `EntryFailed` 的便捷映射; 顶层 `run` 只使用前两者, 逐条结果侧的 0/1 的唯一来源是
//! `exit_code_for`, 二者不重复硬编码同一个数字.

use crate::cli::Command;

use thiserror::Error;

/// 派发之前可能出现的结局.
#[derive(Debug, Error)]
pub enum Error {
    /// 清单缺失 / 不可读 / 语法非法 / 校验失败 / 主目录或当前工作目录不可用.
    #[error("{0}")]
    ManifestInvalid(String),

    /// 条目级失败或被拒绝.
    #[error("{0}")]
    EntryFailed(String),

    /// 平台不受支持 (仅支持 macOS 与 Linux).
    #[error("不支持当前平台 (仅支持 macOS 与 Linux)")]
    UnsupportedPlatform,
}

/// 本 crate 的统一结果类型.
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// 该错误对应的退出码.
    ///
    /// `ManifestInvalid` / `UnsupportedPlatform` 是派发之前的进程结局; `EntryFailed`
    /// 的 1 只是条目级失败的便捷映射, 顶层 `run` 不使用它 (逐条结果侧的 0/1 唯一
    /// 来源是 `exit_code_for`).
    pub fn exit_code(&self) -> i32 {
        match self {
            Error::ManifestInvalid(_) => 2,
            Error::EntryFailed(_) => 1,
            Error::UnsupportedPlatform => 3,
        }
    }
}

/// 单条目落在哪一个汇总列 (由 `(命令, Current, Action)` 三元组唯一决定).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryOutcome {
    Create,
    Ok,
    Replace,
    Conflict,
    Absent,
    Missing,
    Foreign,
    Wrong,
    Undo,
    Restore,
    Skip,
    Fail,
}

/// 由"逐条结果 + 该命令的目标谓词"导出退出码.
///
/// 这是四个命令**共用**的唯一路径. 退出 0 当且仅当两个条件同时成立:
///
/// 1. **目标谓词成立** -- 从 `outcomes` 推导该命令自身的期望状态是否达成.
///    `Missing` / `Fail` 不参与条件 1 (它们的残留待办只由条件 2 表达).
/// 2. **无遗留待办** -- `outcomes` 中只出现"成功"列与"已是最新"列.
///
/// 绝不返回 2 或 3: 那两个码属于派发之前的结局, 唯一来源是 `Error::exit_code`.
pub fn exit_code_for(command: Command, outcomes: &[EntryOutcome]) -> i32 {
    if goal_reached(command, outcomes) && no_pending_work(outcomes) {
        0
    } else {
        1
    }
}

/// [`exit_code_for`] 的迭代器入口: 生产路径直接从报告行取 token, 不物化 `Vec`.
///
/// 与 slice 版本共用 [`blocks_goal`] 与 [`is_settled`] 两个谓词, 因此两个入口对同一
/// token 序列必然得出同一结论. 只返回 0 或 1.
pub(crate) fn exit_code_for_iter<I>(command: Command, outcomes: I) -> i32
where
    I: IntoIterator<Item = EntryOutcome>,
{
    let mut goal_reached = true;
    let mut has_pending_work = false;
    for outcome in outcomes {
        if blocks_goal(command, outcome) {
            goal_reached = false;
        }
        if !is_settled(outcome) {
            has_pending_work = true;
        }
    }
    if goal_reached && !has_pending_work {
        0
    } else {
        1
    }
}

/// 条件 1: 该命令自身的目标谓词是否成立.
///
/// `preview` 与 `apply` 的谓词同源 ("每个条目的目标路径都已成为指向其源条目的正确
/// 链接"), 但求值时刻不同, 而本函数的输入是**求值时刻**的逐条结果:
///
/// - `preview` 在动手**之前**求值, 此时 `Create` / `Replace` 仍是"待办", 使谓词为假;
/// - `apply` 在动手**之后**求值, 此时成功的建立已被记为 `Create` / `Replace`
///   (它们落在汇总的"成功"列), 因此只有 `Conflict` 表示目标未达成 --
///   若执行**失败**, token 是 `Fail` 而非 `Create` / `Replace`, 由条件 2 拦下.
///
/// 这份非对称是契约要求的行为, 不是实现疏漏: 未部署的清单上 `preview` 退 1 而
/// `apply` 退 0 (`openspec/specs/cli-contract/spec.md` 的"只读与写入命令的求值时刻").
fn goal_reached(command: Command, outcomes: &[EntryOutcome]) -> bool {
    !outcomes
        .iter()
        .copied()
        .any(|outcome| blocks_goal(command, outcome))
}

/// 条件 1 的单项判定: 该 token 是否使该命令的目标谓词为假.
///
/// 对 12 个 token 全部穷尽, 新增变体时这里会编译失败, 而不是被 `matches!` 静默
/// 归入"不阻塞"的一侧. 语义与 `openspec/specs/cli-contract/spec.md` 的"两条件判据"
/// 一致: `Missing` / `Fail` 不参与条件 1, 它们的残留待办只由条件 2 表达.
fn blocks_goal(command: Command, outcome: EntryOutcome) -> bool {
    match command {
        Command::Preview => match outcome {
            EntryOutcome::Create | EntryOutcome::Replace | EntryOutcome::Conflict => true,
            EntryOutcome::Ok
            | EntryOutcome::Absent
            | EntryOutcome::Missing
            | EntryOutcome::Foreign
            | EntryOutcome::Wrong
            | EntryOutcome::Undo
            | EntryOutcome::Restore
            | EntryOutcome::Skip
            | EntryOutcome::Fail => false,
        },
        Command::Apply => match outcome {
            EntryOutcome::Conflict => true,
            EntryOutcome::Create
            | EntryOutcome::Ok
            | EntryOutcome::Replace
            | EntryOutcome::Absent
            | EntryOutcome::Missing
            | EntryOutcome::Foreign
            | EntryOutcome::Wrong
            | EntryOutcome::Undo
            | EntryOutcome::Restore
            | EntryOutcome::Skip
            | EntryOutcome::Fail => false,
        },
        Command::Undo => match outcome {
            EntryOutcome::Undo | EntryOutcome::Restore => true,
            EntryOutcome::Create
            | EntryOutcome::Ok
            | EntryOutcome::Replace
            | EntryOutcome::Conflict
            | EntryOutcome::Absent
            | EntryOutcome::Missing
            | EntryOutcome::Foreign
            | EntryOutcome::Wrong
            | EntryOutcome::Skip
            | EntryOutcome::Fail => false,
        },
        Command::Status => match outcome {
            EntryOutcome::Absent | EntryOutcome::Foreign | EntryOutcome::Wrong => true,
            EntryOutcome::Create
            | EntryOutcome::Ok
            | EntryOutcome::Replace
            | EntryOutcome::Conflict
            | EntryOutcome::Missing
            | EntryOutcome::Undo
            | EntryOutcome::Restore
            | EntryOutcome::Skip
            | EntryOutcome::Fail => false,
        },
    }
}

/// 条件 2 的单项判定: token 是否落在"成功"或"已是最新"两列.
///
/// 对 12 个 token 全部穷尽, 新增变体时这里会编译失败.
fn is_settled(outcome: EntryOutcome) -> bool {
    match outcome {
        EntryOutcome::Create
        | EntryOutcome::Ok
        | EntryOutcome::Replace
        | EntryOutcome::Undo
        | EntryOutcome::Restore => true,
        EntryOutcome::Conflict
        | EntryOutcome::Absent
        | EntryOutcome::Missing
        | EntryOutcome::Foreign
        | EntryOutcome::Wrong
        | EntryOutcome::Skip
        | EntryOutcome::Fail => false,
    }
}

/// 条件 2: 全部条目都落在"成功"或"已是最新"两列.
fn no_pending_work(outcomes: &[EntryOutcome]) -> bool {
    outcomes.iter().copied().all(is_settled)
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMANDS: [Command; 4] = [
        Command::Preview,
        Command::Apply,
        Command::Status,
        Command::Undo,
    ];

    /// 全部 12 个 token; 新增变体时必须同时更新本表.
    const ALL_OUTCOMES: [EntryOutcome; 12] = [
        EntryOutcome::Create,
        EntryOutcome::Ok,
        EntryOutcome::Replace,
        EntryOutcome::Conflict,
        EntryOutcome::Absent,
        EntryOutcome::Missing,
        EntryOutcome::Foreign,
        EntryOutcome::Wrong,
        EntryOutcome::Undo,
        EntryOutcome::Restore,
        EntryOutcome::Skip,
        EntryOutcome::Fail,
    ];

    #[test]
    fn error_exit_codes_are_fixed() {
        assert_eq!(Error::ManifestInvalid(String::new()).exit_code(), 2);
        assert_eq!(Error::EntryFailed(String::new()).exit_code(), 1);
        assert_eq!(Error::UnsupportedPlatform.exit_code(), 3);
    }

    #[test]
    fn preview_reports_pending_work_while_apply_reports_success() {
        let created = [EntryOutcome::Create, EntryOutcome::Create];
        assert_eq!(exit_code_for(Command::Preview, &created), 1);
        assert_eq!(exit_code_for(Command::Apply, &created), 0);
    }

    #[test]
    fn both_preview_and_apply_are_ok_when_already_up_to_date() {
        let all_ok = [EntryOutcome::Ok, EntryOutcome::Ok, EntryOutcome::Ok];
        assert_eq!(exit_code_for(Command::Preview, &all_ok), 0);
        assert_eq!(exit_code_for(Command::Apply, &all_ok), 0);
    }

    #[test]
    fn apply_that_actually_created_links_exits_zero() {
        let replaced = [EntryOutcome::Replace, EntryOutcome::Create];
        assert_eq!(exit_code_for(Command::Apply, &replaced), 0);
    }

    #[test]
    fn apply_with_a_refused_conflict_exits_one() {
        assert_eq!(exit_code_for(Command::Apply, &[EntryOutcome::Conflict]), 1);
    }

    #[test]
    fn apply_is_ok_while_undo_reports_pending_work_on_a_deployed_manifest() {
        let undone = [EntryOutcome::Undo, EntryOutcome::Restore];
        assert_eq!(exit_code_for(Command::Apply, &undone), 0);
        assert_eq!(exit_code_for(Command::Undo, &undone), 1);
    }

    #[test]
    fn undo_is_idempotent_when_everything_was_already_removed() {
        let all_ok = [EntryOutcome::Ok, EntryOutcome::Ok];
        assert_eq!(exit_code_for(Command::Undo, &all_ok), 0);
    }

    #[test]
    fn missing_source_forces_exit_code_one_for_every_command() {
        for command in COMMANDS {
            assert_eq!(exit_code_for(command, &[EntryOutcome::Missing]), 1);
        }
    }

    /// 契约 (`entry-classification` 的"目标谓词对特殊状态的计数基准"): `SourceMissing`
    /// 与 `Unreadable` (token `missing` / `fail`) **不参与**条件 1, 其残留待办只由
    /// 条件 2 表达. 若谓词误计它们, 条件 1 与条件 2 会在同一批条目上重复生效; 退出码
    /// 的行为级断言无法区分, 因此这里直接对两个条件做分解断言.
    #[test]
    fn special_states_are_excluded_from_the_target_predicate() {
        for command in COMMANDS {
            for outcome in [EntryOutcome::Missing, EntryOutcome::Fail] {
                assert!(
                    goal_reached(command, &[outcome]),
                    "{command:?}: {outcome:?} 不得使目标谓词为假"
                );
                assert!(
                    !no_pending_work(&[outcome]),
                    "{command:?}: {outcome:?} 属于遗留待办"
                );
                assert_eq!(exit_code_for(command, &[outcome]), 1);
            }
        }
    }

    #[test]
    fn skipped_entry_forces_undo_to_exit_code_one() {
        assert_eq!(exit_code_for(Command::Undo, &[EntryOutcome::Skip]), 1);
    }

    #[test]
    fn conflict_forces_apply_and_status_to_exit_code_one() {
        assert_eq!(exit_code_for(Command::Apply, &[EntryOutcome::Conflict]), 1);
        assert_eq!(exit_code_for(Command::Status, &[EntryOutcome::Conflict]), 1);
    }

    #[test]
    fn status_reflects_deviation() {
        let all_ok = [EntryOutcome::Ok, EntryOutcome::Ok];
        assert_eq!(exit_code_for(Command::Status, &all_ok), 0);
        assert_eq!(exit_code_for(Command::Status, &[EntryOutcome::Absent]), 1);
        assert_eq!(exit_code_for(Command::Status, &[EntryOutcome::Foreign]), 1);
        assert_eq!(exit_code_for(Command::Status, &[EntryOutcome::Wrong]), 1);
    }

    /// 12 个 token 都必须被两个条件显式分类; 生产侧的穷尽 `match` 与本表同时要求
    /// 新增变体被处理, 不会出现"新 token 静默落入未覆盖分支".
    #[test]
    fn every_token_variant_is_explicitly_classified() {
        const SETTLED: [(EntryOutcome, bool); 12] = [
            (EntryOutcome::Create, true),
            (EntryOutcome::Ok, true),
            (EntryOutcome::Replace, true),
            (EntryOutcome::Conflict, false),
            (EntryOutcome::Absent, false),
            (EntryOutcome::Missing, false),
            (EntryOutcome::Foreign, false),
            (EntryOutcome::Wrong, false),
            (EntryOutcome::Undo, true),
            (EntryOutcome::Restore, true),
            (EntryOutcome::Skip, false),
            (EntryOutcome::Fail, false),
        ];
        assert_eq!(SETTLED.len(), ALL_OUTCOMES.len());

        for (outcome, settled) in SETTLED {
            assert!(
                ALL_OUTCOMES.contains(&outcome),
                "{outcome:?} 不在 ALL_OUTCOMES 中"
            );
            assert_eq!(is_settled(outcome), settled, "{outcome:?}");
            assert_eq!(no_pending_work(&[outcome]), settled, "{outcome:?}");
            for command in COMMANDS {
                assert_eq!(
                    goal_reached(command, &[outcome]),
                    !blocks_goal(command, outcome),
                    "{command:?} / {outcome:?}"
                );
            }
        }
    }

    #[test]
    fn exit_code_for_never_returns_two_or_three() {
        for command in COMMANDS {
            assert_eq!(exit_code_for(command, &[]), 0);
            for outcome in ALL_OUTCOMES {
                let code = exit_code_for(command, &[outcome]);
                assert!(
                    code == 0 || code == 1,
                    "exit_code_for({command:?}, [{outcome:?}]) 返回了 {code}"
                );
                let code = exit_code_for(command, &ALL_OUTCOMES);
                assert!(
                    code == 0 || code == 1,
                    "exit_code_for({command:?}, ..) 返回了 {code}"
                );
            }
        }
    }
}

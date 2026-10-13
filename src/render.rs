//! 人类可读输出 (FR-016).
//!
//! 本模块是状态 token 与末尾汇总分列的**唯一**来源. `contracts/cli.md` 要求
//! "实现时不得在 `render.rs` 里另起一套 token 命名"; 其反面同样成立 -- token 与
//! 分列口径只在这里确定, 各命令接入时不得自行拼凑.
//!
//! 导出面各有分工, 调用者只要不绕过它们, 结论就必然一致:
//!
//! - [`token_for`]: `(命令, Current, Action, 同目录是否有备份)` -> token;
//! - [`row_from_state`]: 由同一次判定同时产出 token 与说明, 保证二者不会错配;
//! - [`fail_row`]: 执行失败行的**唯一**构造点, "操作失败"措辞只在这里拼写;
//! - [`render`]: 逐条目行 (含 `<序号>/<总数>`) 与末尾汇总.
//!
//! [`row_for`] 是上述入口共用的组装原语: token 与说明都已定, 它只负责落进报告行.

use std::path::Path;

use crate::classify::{Action, Current};
use crate::cli::Command;
use crate::entry::Entry;
use crate::error::EntryOutcome;

/// token 列宽, 对齐 `contracts/cli.md` 的示例.
///
/// 示例中 `create` 与 `conflict` 之后到 target 之间的空格数不同 (3 与 1), 但
/// token 加其后空白的宽度恒为 9.
const TOKEN_WIDTH: usize = 9;
/// target 列宽 (含其后的分隔空格), 对齐示例: `~/.bashrc` 之后到说明之间有 11 个
/// 空格 (`9 + 11 = 20`), `~/.config/nvim` 之后有 6 个 (`14 + 6 = 20`).
const TARGET_WIDTH: usize = 20;

/// 报告中的一行.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportRow {
    /// 用户书写的原始 target 字符串.
    pub target_display: String,
    /// 机器可断言的稳定取值.
    pub token: EntryOutcome,
    /// 人类可读说明.
    pub note: String,
}

/// 由 `(命令, Current, Action, 同目录是否有备份)` 唯一决定 token.
///
/// 唯一权威为 `contracts/cli.md` 的"状态 token 表"与"`undo` 分支". `has_backup`
/// 只为 `undo` 而存在: `undo` / `restore` 的 token 不由 `Action` 导出, 而由
/// `(Current, 同目录是否有备份)` 决定, 因为 `Action` 枚举不含移除类取值.
///
/// `Current::Unreadable` 在四条命令下都是 `Fail`, 不随命令变化.
pub fn token_for(
    command: Command,
    current: Current,
    action: Action,
    has_backup: bool,
) -> EntryOutcome {
    match command {
        Command::Preview | Command::Apply => deploy_token(current, action),
        Command::Status => status_token(current),
        Command::Undo => undo_token(current, has_backup),
    }
}

/// token 的稳定文本 (可供脚本与测试断言).
///
/// 这是 token 命名的唯一来源; 各命令与测试都不得另写一份字面量.
pub fn token_name(token: EntryOutcome) -> &'static str {
    match token {
        EntryOutcome::Create => "create",
        EntryOutcome::Ok => "ok",
        EntryOutcome::Replace => "replace",
        EntryOutcome::Conflict => "conflict",
        EntryOutcome::Absent => "absent",
        EntryOutcome::Missing => "missing",
        EntryOutcome::Foreign => "foreign",
        EntryOutcome::Wrong => "wrong",
        EntryOutcome::Undo => "undo",
        EntryOutcome::Restore => "restore",
        EntryOutcome::Skip => "skip",
        EntryOutcome::Fail => "fail",
    }
}

/// 由判定结果与模块内计算出的 token 组装一行.
///
/// `token` 必须来自 [`token_for`] (由 `row_for` 保证), 这样说明措辞与 token 不会错配.
/// 只是模块内的组装原语: 命令接入请走 [`row_from_state`] 或 [`fail_row`].
fn row_for(entry: &Entry, token: EntryOutcome, note: String) -> ReportRow {
    ReportRow {
        target_display: entry.target.clone(),
        token,
        note,
    }
}

/// 由同一次判定同时产出 token 与说明.
///
/// 命令接入时**应当用本函数而不是分别调用 [`token_for`] 与 `note_for`**:
/// 说明的分支由 token 驱动, 若分别调用则同一行可能带着与 token 不符的措辞.
pub fn row_from_state(
    command: Command,
    entry: &Entry,
    current: Current,
    action: Action,
    detail: &str,
    has_backup: bool,
    backup_path: Option<&Path>,
) -> ReportRow {
    let token = token_for(command, current, action, has_backup);
    let note = note_for_token(command, token, entry, current, detail, backup_path);
    row_for(entry, token, note)
}

/// 执行失败行 (`fail`) 的**唯一**构造点: token 与"操作失败"措辞同出一处 (FR-016).
///
/// 不能走 [`row_from_state`]: 它按**动手之前**的 `(Current, Action)` 导出 token, 而那
/// 正是失败了的那个状态 (例如 `Create`). 动作失败 (含 `undo` 的备份快照 / 移除恢复
/// 失败) 一律从这里产出, 使"操作失败: {原因}"的拼写全仓只有这一处.
pub fn fail_row(entry: &Entry, reason: &str) -> ReportRow {
    row_for(entry, EntryOutcome::Fail, fail_note(reason))
}

/// `fail` 行非 `Unreadable` 情形的说明措辞: 有原因给"操作失败: {原因}",
/// 原因空白时给裸的"操作失败". [`note_for_token`] 与 [`fail_row`] 共用它.
fn fail_note(detail: &str) -> String {
    match non_empty(detail) {
        Some(reason) => format!("操作失败: {reason}"),
        None => "操作失败".to_string(),
    }
}

/// 人类可读说明 (FR-016 的必现信息都落在这里).
///
/// `backup_path` 仅对 `replace` / `restore` 有意义 (备份文件的**完整路径**);
/// 其余 token 传 `None` 即可.
///
/// 当前仅有测试消费它 (`row_from_state` 走的是同一个 `note_for_token`, 且保证
/// token 与措辞出自同一次判定). 保留它是为了让"由状态导出措辞"这一层可被独立断言,
/// 故以 `cfg(test)` 收口, 不参与生产二进制.
#[cfg(test)]
pub fn note_for(
    command: Command,
    current: Current,
    action: Action,
    entry: &Entry,
    detail: &str,
    backup_path: Option<&Path>,
) -> String {
    let token = token_for(command, current, action, backup_path.is_some());
    note_for_token(command, token, entry, current, detail, backup_path)
}

/// 逐条目行 (`<序号>/<总数> <token> <用户书写的 target> <说明>`) 与末尾汇总.
///
/// 第三列是**用户书写的原始 `entry.target` 字符串**, 不是解析后的绝对路径;
/// 每行末尾换行; 最后一行是汇总.
pub fn render(rows: &[ReportRow]) -> String {
    let total = rows.len();
    let mut out = String::new();

    for (index, row) in rows.iter().enumerate() {
        out.push_str(&format!(
            "{}/{} {:<width$} {:<target$} {}\n",
            index + 1,
            total,
            token_name(row.token),
            row.target_display,
            row.note,
            width = TOKEN_WIDTH,
            // `TARGET_WIDTH` 含其后那一格分隔空格, 因此字段本身少一格.
            target = TARGET_WIDTH - 1,
        ));
    }

    out.push_str(&summary_line_of(rows));
    out
}

/// 汇总行的计数: 成功 / 已是最新 / 未处理 / 失败.
///
/// 四项按 token 严格分桶且**互斥**, 相加恒等于 token 总数 (`render` 保证其等于 N).
pub fn tally(outcomes: &[EntryOutcome]) -> (usize, usize, usize, usize) {
    let mut tally = (0, 0, 0, 0);
    for outcome in outcomes {
        match column_of(*outcome) {
            Column::Succeeded => tally.0 += 1,
            Column::UpToDate => tally.1 += 1,
            Column::Pending => tally.2 += 1,
            Column::Failed => tally.3 += 1,
        }
    }
    tally
}

/// 末尾汇总行.
///
/// 第三列的列名必须是**"未处理"**而非"跳过": 它是 token 的分桶名, 同时收容
/// "有意不处理"的 `conflict` / `skip` 与"没有动作可执行"的 `foreign` / `absent` /
/// `wrong`, 后三个出现在只读的 `status` 上 (`contracts/cli.md` 的"输出契约").
pub fn summary_line(outcomes: &[EntryOutcome]) -> String {
    let counts = tally(outcomes);
    format!(
        "{} 个条目: {} 成功, {} 已是最新, {} 未处理, {} 失败\n",
        outcomes.len(),
        counts.0,
        counts.1,
        counts.2,
        counts.3
    )
}

/// 汇总列 (token 的分桶名, 不是对动作的陈述).
enum Column {
    Succeeded,
    UpToDate,
    Pending,
    Failed,
}

/// token 归入哪一列, 严格按 `contracts/cli.md` 的"汇总列 / token"对照表.
///
/// 该表是一个**划分**: 12 个 token 各有唯一归属, 因此不存在同一条目计入两列的可能.
fn column_of(token: EntryOutcome) -> Column {
    match token {
        EntryOutcome::Create
        | EntryOutcome::Replace
        | EntryOutcome::Undo
        | EntryOutcome::Restore => Column::Succeeded,
        EntryOutcome::Ok => Column::UpToDate,
        EntryOutcome::Conflict
        | EntryOutcome::Skip
        | EntryOutcome::Foreign
        | EntryOutcome::Absent
        | EntryOutcome::Wrong => Column::Pending,
        EntryOutcome::Missing | EntryOutcome::Fail => Column::Failed,
    }
}

/// `preview` 与 `apply` 的 token: 由 `(Current, Action)` 决定, 二者共用同一张表.
///
/// 两个命令在同一 token 下的措辞必须可辨 (将来时 / 完成时), 由
/// [`note_for_token`] 按命令分支实现 (T020); token 本身完全一致 (FR-017).
fn deploy_token(current: Current, action: Action) -> EntryOutcome {
    match current {
        // 环境故障先于一切: 判定阶段的 `lstat` 失败使该条目无法被归类为一个确定的
        // 文件系统事实 (`data-model.md` 3.1 的"求值语义"), 因此四条命令下都是 `fail`.
        Current::Unreadable => EntryOutcome::Fail,
        Current::SourceMissing => EntryOutcome::Missing,
        Current::Absent if action == Action::Create => EntryOutcome::Create,
        Current::LinkedCorrect if action == Action::Noop => EntryOutcome::Ok,
        Current::LinkedWrong
        | Current::ForeignLink
        | Current::ForeignFile
        | Current::ForeignDir => match action {
            Action::RefuseConflict => EntryOutcome::Conflict,
            Action::ReplaceAndBackup => EntryOutcome::Replace,
            _ => status_token(current),
        },
        // 表外组合 (例如 `preview` 拿到了它不该有的 `Create`): 一律回落到该 `Current`
        // 自身的分桶, 不退化为 `fail` -- `fail` 只表示"真的没成功".
        _ => status_token(current),
    }
}

/// `status` 的 token: **只渲染 `Current`, 不渲染 `Action`** (`data-model.md` 3.3).
///
/// `foreign` 与 `wrong` 的分工由 `Current` 的取值直接给出 -- `LinkedWrong` 蕴含
/// "链接落在仓库内", `ForeignLink` 蕴含"落在仓库外", 二者互斥且穷尽.
fn status_token(current: Current) -> EntryOutcome {
    match current {
        Current::SourceMissing => EntryOutcome::Missing,
        Current::Absent => EntryOutcome::Absent,
        Current::LinkedCorrect => EntryOutcome::Ok,
        Current::LinkedWrong => EntryOutcome::Wrong,
        Current::ForeignLink | Current::ForeignFile | Current::ForeignDir => EntryOutcome::Foreign,
        Current::Unreadable => EntryOutcome::Fail,
    }
}

/// `undo` 的 token: 按 `Current` 取, **不得一概归为 `skip`**.
///
/// `LinkedCorrect` / `LinkedWrong` 的 token 由同目录**有无备份**决定 -- 与 `Current`
/// 是哪一个无关 (后者只决定"是否允许移除", 前者决定"移除后是否恢复").
///
/// `Absent` -> `Ok` 是幂等的关键行: 它使"撤销后目标为空的清单上再次运行 `undo`"落入
/// "已是最新"列并退 0; 若误归为 `skip`, 第二次 `undo` 将退 1. 若上一次恢复过备份,
/// 目标被原内容占用而判为冲突类, 第二次 `undo` 报 `skip` 并退 1 (状态不变, 但不退 0).
fn undo_token(current: Current, has_backup: bool) -> EntryOutcome {
    match current {
        Current::LinkedCorrect | Current::LinkedWrong => {
            if has_backup {
                EntryOutcome::Restore
            } else {
                EntryOutcome::Undo
            }
        }
        Current::ForeignLink | Current::ForeignFile | Current::ForeignDir => EntryOutcome::Skip,
        Current::Absent => EntryOutcome::Ok,
        Current::SourceMissing => EntryOutcome::Missing,
        Current::Unreadable => EntryOutcome::Fail,
    }
}

/// 由 token (而非原始的三元组) 导出说明措辞.
///
/// 这样做使说明的分支数与 token 数一一对应, 不会出现"某个 token 忘了写说明";
/// 而 `preview` / `apply` 的将来时/完成时差异仍可从 `command` 取出 -- 二者在
/// `create` 与 `replace` 上共享 token, 只在这两处按 `command` 分支.
fn note_for_token(
    command: Command,
    token: EntryOutcome,
    entry: &Entry,
    current: Current,
    detail: &str,
    backup_path: Option<&Path>,
) -> String {
    match token {
        EntryOutcome::Create => match command {
            Command::Preview => "未部署 -> 将创建链接".to_string(),
            _ => "未部署 -> 已创建链接".to_string(),
        },
        EntryOutcome::Ok => match command {
            Command::Undo => "该路径本就没有本工具创建的链接".to_string(),
            _ => "已是最新".to_string(),
        },
        EntryOutcome::Replace => {
            let source = entry.source.as_str();
            match (command, backup_path) {
                (Command::Preview, Some(path)) => {
                    format!(
                        "冲突占用 ({source}) -> 将备份到 {} 并建立链接",
                        path.display()
                    )
                }
                (Command::Preview, None) => {
                    format!("冲突占用 ({source}) -> 将备份后建立链接")
                }
                (_, Some(path)) => {
                    format!(
                        "冲突占用 ({source}) -> 已备份到 {} 并建立链接",
                        path.display()
                    )
                }
                (_, None) => format!("冲突占用 ({source}) -> 已备份并建立链接"),
            }
        }
        EntryOutcome::Conflict => {
            let on_conflict = match entry.on_conflict {
                crate::entry::OnConflict::Skip => "skip",
                crate::entry::OnConflict::ReplaceAndBackup => "replace_and_backup",
            };
            format!(
                "冲突占用 ({}, 未覆盖; 该条目 on_conflict = \"{on_conflict}\")",
                occupied(current, detail)
            )
        }
        EntryOutcome::Absent => "未部署: 目标路径不存在".to_string(),
        EntryOutcome::Missing => format!("源缺失 ({})", missing_detail(entry, detail)),
        EntryOutcome::Foreign => {
            format!("被非本工具创建的内容占用 ({})", occupied(current, detail))
        }
        EntryOutcome::Wrong => format!("指向错误 (实际指向: {})", actual_target(detail)),
        EntryOutcome::Undo => "已移除本工具创建的链接 (该条目无备份可恢复)".to_string(),
        EntryOutcome::Restore => match backup_path {
            Some(path) => format!("已移除链接, 并从备份恢复: {}", path.display()),
            None => "已移除链接, 并从备份恢复".to_string(),
        },
        EntryOutcome::Skip => format!(
            "该路径非本工具创建, 已跳过 (不删除): {}",
            occupied(current, detail)
        ),
        EntryOutcome::Fail => match current {
            Current::Unreadable => format!("无法判定: {}", unreadable_detail(entry, detail)),
            _ => fail_note(detail),
        },
    }
}

/// 占用物类型与 (符号链接时的) 实际指向.
///
/// `foreign` / `wrong` / `conflict` / `skip` 的必现信息都靠它: 实际指向取自
/// `classify` 写在 `detail` 里的链接文本, 占用物类型取自 `Current` 的取值.
fn occupied(current: Current, detail: &str) -> String {
    match current {
        Current::LinkedWrong | Current::ForeignLink => match non_empty(detail) {
            Some(target) => format!("符号链接, 实际指向: {target}"),
            None => "符号链接".to_string(),
        },
        Current::ForeignFile => "普通文件".to_string(),
        Current::ForeignDir => "目录".to_string(),
        _ => "未知占用物".to_string(),
    }
}

/// 链接的实际指向: 取自 `detail` (由 `classify` 写入的链接文本).
fn actual_target(detail: &str) -> String {
    match non_empty(detail) {
        Some(target) => target.to_string(),
        None => "未报告".to_string(),
    }
}

/// `missing` 行的原因: 给出仓库中缺失的源条目 (它是需要用户修正的配置缺陷).
fn missing_detail(entry: &Entry, detail: &str) -> String {
    let source = entry.source.as_str();
    match non_empty(detail) {
        Some(reason) => format!("仓库中不存在: {source}; {reason}"),
        None => format!("仓库中不存在: {source}"),
    }
}

/// `Unreadable` 行的原因: 判定阶段的 `lstat` 故障, 说明中给出底层错误.
fn unreadable_detail(entry: &Entry, detail: &str) -> String {
    let target = entry.target.as_str();
    match non_empty(detail) {
        Some(reason) => format!("判定阶段读取失败 (条目 target = {target}): {reason}"),
        None => format!("判定阶段读取失败 (条目 target = {target})"),
    }
}

/// 剔除空白的 `detail`, 使必现信息在 `detail` 缺失时也能给出可读回退.
fn non_empty(detail: &str) -> Option<&str> {
    let trimmed = detail.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// 由行集合导出的汇总行.
fn summary_line_of(rows: &[ReportRow]) -> String {
    summary_line(&rows.iter().map(|row| row.token).collect::<Vec<_>>())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::OnConflict;
    use std::path::PathBuf;

    const COMMANDS: [Command; 4] = [
        Command::Preview,
        Command::Apply,
        Command::Status,
        Command::Undo,
    ];
    const CURRENTS: [Current; 8] = [
        Current::SourceMissing,
        Current::Absent,
        Current::LinkedCorrect,
        Current::LinkedWrong,
        Current::ForeignLink,
        Current::ForeignFile,
        Current::ForeignDir,
        Current::Unreadable,
    ];
    const ACTIONS: [Action; 6] = [
        Action::ReportMissingSource,
        Action::Create,
        Action::Noop,
        Action::RefuseConflict,
        Action::ReplaceAndBackup,
        Action::ReportUnreadable,
    ];
    const BACKUPS: [bool; 2] = [false, true];

    /// 每个 token 的稳定文本, 逐字抄自 `contracts/cli.md` 的"状态 token 表".
    const TOKEN_NAMES: [(EntryOutcome, &str); 12] = [
        (EntryOutcome::Create, "create"),
        (EntryOutcome::Ok, "ok"),
        (EntryOutcome::Replace, "replace"),
        (EntryOutcome::Conflict, "conflict"),
        (EntryOutcome::Absent, "absent"),
        (EntryOutcome::Missing, "missing"),
        (EntryOutcome::Foreign, "foreign"),
        (EntryOutcome::Wrong, "wrong"),
        (EntryOutcome::Undo, "undo"),
        (EntryOutcome::Restore, "restore"),
        (EntryOutcome::Skip, "skip"),
        (EntryOutcome::Fail, "fail"),
    ];

    /// "汇总列 / token"对照表 (成功 / 已是最新 / 未处理 / 失败), 独立于实现抄自契约.
    const COLUMNS: [&[EntryOutcome]; 4] = [
        &[
            EntryOutcome::Create,
            EntryOutcome::Replace,
            EntryOutcome::Undo,
            EntryOutcome::Restore,
        ],
        &[EntryOutcome::Ok],
        &[
            EntryOutcome::Conflict,
            EntryOutcome::Skip,
            EntryOutcome::Foreign,
            EntryOutcome::Absent,
            EntryOutcome::Wrong,
        ],
        &[EntryOutcome::Missing, EntryOutcome::Fail],
    ];

    fn entry(source: &str, target: &str) -> Entry {
        Entry {
            source: source.to_string(),
            target: target.to_string(),
            on_conflict: OnConflict::Skip,
            source_path: PathBuf::from("/repo").join(source),
            target_path: PathBuf::from("/home/u").join(target),
        }
    }

    /// 契约期望的 token, 独立于 `render.rs` 的实现而手工抄自 `contracts/cli.md`.
    /// `None` 表示该组合不在 contract 表内 (实现仍须给出确定取值且不 panic).
    fn contract_token(
        command: Command,
        current: Current,
        action: Action,
        has_backup: bool,
    ) -> Option<EntryOutcome> {
        match command {
            Command::Status => Some(contract_status(current)),
            Command::Undo => Some(contract_undo(current, has_backup)),
            Command::Preview | Command::Apply => contract_deploy(current, action),
        }
    }

    /// `status` 只渲染 `Current`, 不渲染 `Action` -> 与 `action` 无关.
    fn contract_status(current: Current) -> EntryOutcome {
        match current {
            Current::SourceMissing => EntryOutcome::Missing,
            Current::Absent => EntryOutcome::Absent,
            Current::LinkedCorrect => EntryOutcome::Ok,
            Current::LinkedWrong => EntryOutcome::Wrong,
            Current::ForeignLink | Current::ForeignFile | Current::ForeignDir => {
                EntryOutcome::Foreign
            }
            Current::Unreadable => EntryOutcome::Fail,
        }
    }

    /// `undo` 的 token 由 `(Current, 同目录是否有备份)` 决定, 不由 `Action` 导出.
    fn contract_undo(current: Current, has_backup: bool) -> EntryOutcome {
        match current {
            Current::LinkedCorrect | Current::LinkedWrong => {
                if has_backup {
                    EntryOutcome::Restore
                } else {
                    EntryOutcome::Undo
                }
            }
            Current::ForeignLink | Current::ForeignFile | Current::ForeignDir => EntryOutcome::Skip,
            Current::Absent => EntryOutcome::Ok,
            Current::SourceMissing => EntryOutcome::Missing,
            Current::Unreadable => EntryOutcome::Fail,
        }
    }

    /// `preview` / `apply` 的 token: 由 `(Current, Action)` 决定.
    fn contract_deploy(current: Current, action: Action) -> Option<EntryOutcome> {
        match current {
            Current::Unreadable => Some(EntryOutcome::Fail),
            Current::SourceMissing => Some(EntryOutcome::Missing),
            Current::Absent if action == Action::Create => Some(EntryOutcome::Create),
            Current::LinkedCorrect if action == Action::Noop => Some(EntryOutcome::Ok),
            Current::LinkedWrong
            | Current::ForeignLink
            | Current::ForeignFile
            | Current::ForeignDir => match action {
                Action::RefuseConflict => Some(EntryOutcome::Conflict),
                Action::ReplaceAndBackup => Some(EntryOutcome::Replace),
                _ => None,
            },
            _ => None,
        }
    }

    #[test]
    fn token_table_is_covered_cell_by_cell() {
        let mut asserted = 0;
        for command in COMMANDS {
            for current in CURRENTS {
                for action in ACTIONS {
                    for has_backup in BACKUPS {
                        let actual = token_for(command, current, action, has_backup);
                        match contract_token(command, current, action, has_backup) {
                            Some(expected) => {
                                assert_eq!(
                                    actual, expected,
                                    "token_for({command:?}, {current:?}, {action:?}, {has_backup})"
                                );
                                asserted += 1;
                            }
                            None => {
                                // 表外组合: 实现仍须落在某个确定的分桶里, 且不退化为 fail.
                                assert_ne!(
                                    actual,
                                    EntryOutcome::Fail,
                                    "表外组合 {command:?} / {current:?} / {action:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
        // 合法格子数, 由契约独立推得:
        //   preview / apply: (Unreadable 6 + SourceMissing 6 + Absent 1 + LinkedCorrect 1
        //                     + 冲突类 4 x 2) = 22, x 2 命令 x 2 个 has_backup = 88;
        //   status / undo:   8 个 Current x 6 个 Action x 2 个 has_backup x 2 命令 = 192.
        assert_eq!(asserted, 88 + 192);
    }

    #[test]
    fn status_and_undo_ignore_action_entirely() {
        for current in CURRENTS {
            for has_backup in BACKUPS {
                let by_status = token_for(Command::Status, current, ACTIONS[0], has_backup);
                let by_undo = token_for(Command::Undo, current, ACTIONS[0], has_backup);
                for action in ACTIONS {
                    assert_eq!(
                        token_for(Command::Status, current, action, has_backup),
                        by_status
                    );
                    assert_eq!(
                        token_for(Command::Undo, current, action, has_backup),
                        by_undo
                    );
                }
            }
        }
    }

    #[test]
    fn undo_and_restore_tokens_are_driven_by_backup_presence_not_by_current() {
        for current in [Current::LinkedCorrect, Current::LinkedWrong] {
            assert_eq!(
                token_for(Command::Undo, current, Action::Noop, true),
                EntryOutcome::Restore
            );
            assert_eq!(
                token_for(Command::Undo, current, Action::Noop, false),
                EntryOutcome::Undo
            );
        }
    }

    #[test]
    fn unreadable_is_fail_under_every_command() {
        for command in COMMANDS {
            assert_eq!(
                token_for(
                    command,
                    Current::Unreadable,
                    Action::ReportUnreadable,
                    false
                ),
                EntryOutcome::Fail
            );
        }
    }

    #[test]
    fn absent_gets_three_different_tokens() {
        assert_eq!(
            token_for(Command::Preview, Current::Absent, Action::Create, false),
            EntryOutcome::Create
        );
        assert_eq!(
            token_for(Command::Apply, Current::Absent, Action::Create, false),
            EntryOutcome::Create
        );
        assert_eq!(
            token_for(Command::Status, Current::Absent, Action::Create, false),
            EntryOutcome::Absent
        );
        assert_eq!(
            token_for(Command::Undo, Current::Absent, Action::Create, false),
            EntryOutcome::Ok
        );
    }

    #[test]
    fn token_names_match_the_contract_literals() {
        for (token, name) in TOKEN_NAMES {
            assert_eq!(token_name(token), name);
        }
    }

    #[test]
    fn every_token_lands_in_exactly_one_summary_column() {
        for (index, column) in COLUMNS.iter().enumerate() {
            for token in *column {
                let hits = COLUMNS.iter().filter(|other| other.contains(token)).count();
                assert_eq!(hits, 1, "{token:?} 计入了 {hits} 列");
                let counts = tally(&[*token]);
                let observed = [counts.0, counts.1, counts.2, counts.3];
                assert_eq!(
                    observed[index], 1,
                    "{token:?} 未落在第 {index} 列: {observed:?}"
                );
                assert_eq!(observed.iter().sum::<usize>(), 1);
            }
        }
        // 12 个 token 恰好覆盖四列.
        assert_eq!(COLUMNS.iter().map(|c| c.len()).sum::<usize>(), 12);
    }

    /// 由契约的四列清单独立算出期望计数, 与 `tally` 对照.
    fn expected_counts(outcomes: &[EntryOutcome]) -> (usize, usize, usize, usize) {
        let mut counts = (0, 0, 0, 0);
        for outcome in outcomes {
            for (index, column) in COLUMNS.iter().enumerate() {
                if column.contains(outcome) {
                    match index {
                        0 => counts.0 += 1,
                        1 => counts.1 += 1,
                        2 => counts.2 += 1,
                        _ => counts.3 += 1,
                    }
                }
            }
        }
        counts
    }

    #[test]
    fn summary_counts_are_mutually_exclusive_and_sum_to_n() {
        let mixed: Vec<Vec<EntryOutcome>> = vec![
            vec![],
            vec![EntryOutcome::Ok],
            vec![
                EntryOutcome::Create,
                EntryOutcome::Ok,
                EntryOutcome::Conflict,
                EntryOutcome::Missing,
            ],
            vec![
                EntryOutcome::Replace,
                EntryOutcome::Restore,
                EntryOutcome::Undo,
                EntryOutcome::Skip,
                EntryOutcome::Foreign,
                EntryOutcome::Wrong,
                EntryOutcome::Absent,
                EntryOutcome::Fail,
            ],
            TOKEN_NAMES.iter().map(|(token, _)| *token).collect(),
        ];

        for outcomes in mixed {
            let counts = tally(&outcomes);
            assert_eq!(counts, expected_counts(&outcomes));
            let sum = counts.0 + counts.1 + counts.2 + counts.3;
            assert_eq!(sum, outcomes.len(), "四项计数之和必须等于 N");

            let line = summary_line(&outcomes);
            assert_eq!(
                line,
                format!(
                    "{} 个条目: {} 成功, {} 已是最新, {} 未处理, {} 失败\n",
                    outcomes.len(),
                    counts.0,
                    counts.1,
                    counts.2,
                    counts.3
                )
            );
        }
    }

    #[test]
    fn summary_line_matches_the_contract_example_verbatim() {
        let outcomes = [
            EntryOutcome::Create,
            EntryOutcome::Ok,
            EntryOutcome::Conflict,
            EntryOutcome::Missing,
        ];
        assert_eq!(
            summary_line(&outcomes),
            "4 个条目: 1 成功, 1 已是最新, 1 未处理, 1 失败\n"
        );
        assert!(summary_line(&[]).starts_with("0 个条目: 0 成功, 0 已是最新, 0 未处理, 0 失败"));
    }

    fn all_rows() -> Vec<ReportRow> {
        let mut rows = Vec::new();
        for command in COMMANDS {
            for current in CURRENTS {
                for action in ACTIONS {
                    for has_backup in BACKUPS {
                        for backup in [
                            None,
                            Some(Path::new("/home/u/.x.dotlink-backup.20250101T000000")),
                        ] {
                            rows.push(row_from_state(
                                command,
                                &entry("shell/bashrc", "~/.bashrc"),
                                current,
                                action,
                                "/repo/other",
                                has_backup,
                                backup,
                            ));
                        }
                    }
                }
            }
        }
        rows
    }

    #[test]
    fn required_notes_are_never_empty() {
        for row in all_rows() {
            if matches!(
                row.token,
                EntryOutcome::Fail
                    | EntryOutcome::Skip
                    | EntryOutcome::Conflict
                    | EntryOutcome::Missing
            ) {
                assert!(!row.note.trim().is_empty(), "{:?} 行的说明为空", row.token);
            }
            assert!(!row.note.trim().is_empty(), "{:?} 行没有说明", row.token);
        }
    }

    #[test]
    fn required_information_is_present_in_the_notes() {
        let backup = "/home/u/.bashrc.dotlink-backup.20250101T000000";
        let e = entry("shell/bashrc", "~/.bashrc");

        // 备份位置: replace / restore 必须给出完整路径.
        for command in [Command::Preview, Command::Apply] {
            let note = note_for(
                command,
                Current::ForeignFile,
                Action::ReplaceAndBackup,
                &e,
                "",
                Some(Path::new(backup)),
            );
            assert!(note.contains(backup), "replace 行缺少备份路径: {note}");
        }
        let restore = note_for(
            Command::Undo,
            Current::LinkedCorrect,
            Action::Noop,
            &e,
            "",
            Some(Path::new(backup)),
        );
        assert!(
            restore.contains(backup),
            "restore 行缺少备份路径: {restore}"
        );

        // 实际指向: wrong 与 foreign (占用物为符号链接时).
        let wrong = note_for(
            Command::Status,
            Current::LinkedWrong,
            Action::Noop,
            &e,
            "/repo/shell/zshrc",
            None,
        );
        assert!(
            wrong.contains("/repo/shell/zshrc"),
            "wrong 行缺少实际指向: {wrong}"
        );

        // 占用物类型: foreign 必须说明是符号链接 / 文件 / 目录.
        let link = note_for(
            Command::Status,
            Current::ForeignLink,
            Action::Noop,
            &e,
            "/etc/other",
            None,
        );
        assert!(link.contains("符号链接") && link.contains("/etc/other"));
        let file = note_for(
            Command::Status,
            Current::ForeignFile,
            Action::Noop,
            &e,
            "",
            None,
        );
        assert!(file.contains("普通文件"), "{file}");
        let dir = note_for(
            Command::Status,
            Current::ForeignDir,
            Action::Noop,
            &e,
            "",
            None,
        );
        assert!(dir.contains("目录"), "{dir}");

        // 失败原因: missing 给出缺失的源条目; fail 给出底层错误.
        let missing = note_for(
            Command::Apply,
            Current::SourceMissing,
            Action::ReportMissingSource,
            &e,
            "",
            None,
        );
        assert!(missing.contains("shell/bashrc"), "{missing}");
        let fail = note_for(
            Command::Apply,
            Current::Unreadable,
            Action::ReportUnreadable,
            &e,
            "Permission denied (os error 13)",
            None,
        );
        assert!(fail.contains("Permission denied"), "{fail}");

        // conflict 行必须说明原因 (该条目 on_conflict = "skip").
        let conflict = note_for(
            Command::Apply,
            Current::ForeignFile,
            Action::RefuseConflict,
            &e,
            "",
            None,
        );
        assert!(conflict.contains("skip"), "{conflict}");
    }

    #[test]
    fn preview_and_apply_wording_is_distinguishable_on_shared_tokens() {
        let e = entry("shell/bashrc", "~/.bashrc");
        for (current, action) in [
            (Current::Absent, Action::Create),
            (Current::ForeignFile, Action::ReplaceAndBackup),
        ] {
            let preview = note_for(Command::Preview, current, action, &e, "", None);
            let apply = note_for(Command::Apply, current, action, &e, "", None);
            assert_ne!(preview, apply, "{current:?} 下 preview 与 apply 措辞相同");
            assert!(preview.contains("将"), "preview 应使用将来时: {preview}");
            assert!(!apply.contains("将"), "apply 不应使用将来时: {apply}");
            assert_eq!(
                token_for(Command::Preview, current, action, false),
                token_for(Command::Apply, current, action, false)
            );
        }
    }

    #[test]
    fn render_emits_one_line_per_row_plus_the_summary() {
        let rows = all_rows();
        let out = render(&rows);
        assert!(out.ends_with('\n'));
        assert_eq!(out.lines().count(), rows.len() + 1);

        let lines: Vec<&str> = out.lines().collect();
        for (index, row) in rows.iter().enumerate() {
            let line = lines[index];
            let expected_prefix = format!("{}/{} ", index + 1, rows.len());
            assert!(line.starts_with(&expected_prefix), "{line}");
            let remainder = &line[expected_prefix.len()..];
            let token = remainder.split_whitespace().next().unwrap_or_default();
            assert_eq!(token, token_name(row.token), "{line}");
        }
        assert_eq!(lines[rows.len()], summary_line_of(&rows).trim_end());
    }

    #[test]
    fn render_shows_the_original_target_string_not_the_resolved_path() {
        let raw = "~/.config/nvim";
        let e = entry("nvim", raw);
        let rows = vec![row_from_state(
            Command::Apply,
            &e,
            Current::Absent,
            Action::Create,
            "",
            false,
            None,
        )];
        let out = render(&rows);
        assert!(out.contains(raw), "{out}");
        assert!(!out.contains("/home/u/.config/nvim"), "{out}");
    }

    #[test]
    fn render_columns_are_aligned_as_in_the_contract_example() {
        let rows: Vec<ReportRow> = [
            (EntryOutcome::Create, "~/.bashrc"),
            (EntryOutcome::Ok, "~/.zshrc"),
            (EntryOutcome::Conflict, "~/.gitconfig"),
            (EntryOutcome::Missing, "~/.config/nvim"),
        ]
        .into_iter()
        .map(|(token, target)| ReportRow {
            target_display: target.to_string(),
            token,
            note: "说明".to_string(),
        })
        .collect();

        let out = render(&rows);
        let lines: Vec<&str> = out.lines().collect();
        for (index, row) in rows.iter().enumerate() {
            let prefix = format!("{}/4 ", index + 1);
            let line = lines[index];
            let rest = &line[prefix.len()..];
            let (token_field, tail) = rest.split_at(TOKEN_WIDTH);
            assert_eq!(token_field.trim(), token_name(row.token), "{line}");
            assert_eq!(tail.chars().next(), Some(' '), "{line}");
            let (target_field, note) = tail[1..].split_at(TARGET_WIDTH - 1);
            assert_eq!(target_field.trim(), row.target_display, "{line}");
            assert_eq!(note.chars().next(), Some(' '), "{line}");
            assert_eq!(&note[1..], "说明", "{line}");
        }
    }

    /// `contracts/cli.md` 的四行示例必须被逐字复现: 这是"行格式"一节唯一的可断言落点.
    #[test]
    fn contract_example_block_is_reproduced_verbatim() {
        let bashrc = entry("bashrc", "~/.bashrc");
        let zshrc = entry("zshrc", "~/.zshrc");
        let gitconfig = entry("gitconfig", "~/.gitconfig");
        let nvim = entry("nvim", "~/.config/nvim");

        let rows = vec![
            row_from_state(
                Command::Apply,
                &bashrc,
                Current::Absent,
                Action::Create,
                "",
                false,
                None,
            ),
            row_from_state(
                Command::Apply,
                &zshrc,
                Current::LinkedCorrect,
                Action::Noop,
                "",
                false,
                None,
            ),
            row_from_state(
                Command::Apply,
                &gitconfig,
                Current::ForeignFile,
                Action::RefuseConflict,
                "",
                false,
                None,
            ),
            row_from_state(
                Command::Apply,
                &nvim,
                Current::SourceMissing,
                Action::ReportMissingSource,
                "",
                false,
                None,
            ),
        ];

        assert_eq!(
            render(&rows),
            concat!(
                "1/4 create    ~/.bashrc           未部署 -> 已创建链接\n",
                "2/4 ok        ~/.zshrc            已是最新\n",
                "3/4 conflict  ~/.gitconfig        冲突占用 (普通文件, 未覆盖; 该条目 on_conflict = \"skip\")\n",
                "4/4 missing   ~/.config/nvim      源缺失 (仓库中不存在: nvim)\n",
                "4 个条目: 1 成功, 1 已是最新, 1 未处理, 1 失败\n",
            )
        );
    }

    #[test]
    fn rendered_text_contains_no_fullwidth_punctuation() {
        let mut text = render(&all_rows());
        for (_, name) in TOKEN_NAMES {
            text.push_str(name);
        }
        for ch in text.chars() {
            let code = ch as u32;
            let wide = (0x3000..=0x303F).contains(&code) || (0xFF01..=0xFF5E).contains(&code);
            assert!(!wide, "输出含全角标点 {ch:?} (U+{code:04X})");
        }
    }
}

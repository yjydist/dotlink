//! dotlink: 声明式 dotfile 软链接管理.
//!
//! 本模块是全部模块的唯一串行接入点: 平台门禁 -> 参数解析 -> 清单载入 ->
//! **一次** `classify` -> 按子命令分发 -> 渲染报告. 四个子命令消费**同一个**
//! `classify` 返回值, 因此"同一状态下四条命令结论一致"由数据流保证 (FR-017),
//! 而不是靠四处逻辑各自对齐 (`research.md` R7).
//!
//! 三处"唯一来源"在这里只被接线, 不被重写:
//!
//! - token 与说明措辞: `render` 层 (`row_from_state` / `fail_row`);
//! - 逐条结果侧的退出码 (0 / 1): `exit_code_for`, 输入即报告行本身;
//! - 派发**之前**的结局 (2 / 3): `Error::exit_code`.

// 模块全部私有: 唯一的对外表面是下面的 `pub fn run` 与三条 `pub use`.
//
// 被门面重新导出的项 (`Command` / `EntryOutcome` / `exit_code_for`) 在各自模块内仍是
// `pub`, 私有模块边界的收窄使 `Entry` / `EntryState` / `Action` / `Manifest` / `Current`
// 等内部类型不再是外部契约的一部分 (`research.md` 的"深模块"门禁).
mod apply;
mod backup;
mod classify;
mod cli;
mod entry;
mod error;
mod manifest;
mod os;
mod render;
mod undo;

pub use cli::Command;
pub use error::{EntryOutcome, exit_code_for};

use std::path::PathBuf;

use crate::classify::{Action, Current, EntryState};
use crate::entry::Entry;
use crate::error::Error;
use crate::manifest::Manifest;
use crate::render::ReportRow;

/// 编排: 平台门禁 -> 解析参数 -> 载入清单 -> 分发命令 -> 渲染报告, 返回进程退出码.
///
/// 顺序是契约的一部分 (FR-015 / FR-019, `research.md` R6): 平台检查早于参数解析与
/// 清单读取, 使不受支持的平台上不产生任何标准输出, 也不触碰文件系统.
pub fn run() -> i32 {
    // 1. 平台门禁. 唯一来源是 `Error::exit_code` (退 3).
    if !platform_supported() {
        let error = Error::UnsupportedPlatform;
        eprintln!("dotlink: {error}");
        return error.exit_code();
    }

    // 2. 参数解析. `clap` 自行处理 `-h` / `-V` 与"子命令缺失或拼错 / 未知选项":
    //    后者在读取清单之前向标准错误打印用法说明并以 2 结束进程, 因此它既不属
    //    四类结局, 也不进 `Error` 枚举.
    let command = cli::parse();

    // 3. 清单取自**当前工作目录**, 不向上搜索父目录 (FR-015). 校验失败 (含当前
    //    工作目录或 `HOME` 不可用) 在这里结束, 退出码的唯一来源是 `Error::exit_code`.
    let manifest = match manifest::load() {
        Ok(manifest) => manifest,
        Err(error) => {
            eprintln!("dotlink: {error}");
            return error.exit_code();
        }
    };

    // 4. 判定**只做一次**: 四条命令消费的是同一个 `Vec<EntryState>`.
    let states = classify::classify(&manifest);

    // 5. 分发到四条命令之一; 每一步同时产出报告行.
    let rows = match command {
        Command::Preview => preview(&manifest, &states),
        Command::Apply => apply(&manifest, &states),
        Command::Status => status(&manifest, &states),
        Command::Undo => undo(&manifest, &states),
    };

    // 6. 渲染与退出码. 逐条结果侧的 0 / 1 全部由 `exit_code_for` 给出.
    print!("{}", render::render(&rows));
    exit_code_of(command, &rows)
}

/// FR-019 / `research.md` R6: 仅 macOS 与 Linux 受支持.
///
/// 用 `cfg!` 而非 `#[cfg]`: 门禁是**运行时**拒绝, 使二进制在不受支持的平台上仍能
/// 构建成功并给出可读提示, 而不是以编译错误收场.
fn platform_supported() -> bool {
    cfg!(any(target_os = "macos", target_os = "linux"))
}

/// 由报告行导出退出码.
///
/// 报告行是逐条结果的**唯一**载体, 因此"输出说成功而退出码说失败"在结构上不可能:
/// 两个条件 (`contracts/cli.md` 的"退出码"小节) 都只看这一份 token 序列.
fn exit_code_of(command: Command, rows: &[ReportRow]) -> i32 {
    let outcomes: Vec<EntryOutcome> = rows.iter().map(|row| row.token).collect();
    exit_code_for(command, &outcomes)
}

/// `preview`: 渲染 `Current` 与 `Action`, 不执行任何动作.
///
/// 本路径不产生任何文件系统写操作 (含创建父目录, SC-006): `render` 只读判定结果,
/// 这里额外的 `backup_path_for` 也只做占用检测.
fn preview(manifest: &Manifest, states: &[EntryState]) -> Vec<ReportRow> {
    manifest
        .entries
        .iter()
        .zip(states)
        .map(|(entry, state)| {
            // FR-016 要求 `replace` 行给出备份的**完整路径**. preview 不产生备份,
            // 因此给出按当前实况算出的那个名字 -- 与随后 apply 的写入结果同源.
            let predicted = match state.action {
                Action::ReplaceAndBackup => Some(backup::backup_path_for(&entry.target_path)),
                _ => None,
            };
            render::row_from_state(
                Command::Preview,
                entry,
                state.current,
                state.action,
                &state.detail,
                false,
                predicted.as_deref(),
            )
        })
        .collect()
}

/// `apply`: 逐条目执行 `EntryState.action`.
///
/// 单条目失败**不中断**其余条目 (FR-014): 失败只把该行改写为 `fail` 并附带原因, 由
/// 条件 2 使退出码为 1. "哪些动作有副作用"由 `apply::apply_entry` 一处决定, 本层不
/// 重复该知识.
fn apply(manifest: &Manifest, states: &[EntryState]) -> Vec<ReportRow> {
    manifest
        .entries
        .iter()
        .zip(states)
        .map(|(entry, state)| match apply::apply_entry(entry, state) {
            Ok(backup) => render::row_from_state(
                Command::Apply,
                entry,
                state.current,
                state.action,
                &state.detail,
                backup.is_some(),
                backup.as_deref(),
            ),
            Err(error) => render::fail_row(entry, &error.to_string()),
        })
        .collect()
}

/// `status`: 只渲染 `Current`.
///
/// `action` 原样传入, 但 `render` 的 `status` 分支不消费它 (token 与措辞都只由
/// `Current` 导出, `data-model.md` 3.3), 因此这里没有"报告动作"的余地.
fn status(manifest: &Manifest, states: &[EntryState]) -> Vec<ReportRow> {
    manifest
        .entries
        .iter()
        .zip(states)
        .map(|(entry, state)| {
            render::row_from_state(
                Command::Status,
                entry,
                state.current,
                state.action,
                &state.detail,
                false,
                None,
            )
        })
        .collect()
}

/// `undo` 的一条目: **动手之前**取得的备份快照, 与执行后的失败原因.
struct UndoItem<'a> {
    entry: &'a Entry,
    state: &'a EntryState,
    /// 同目录最新备份. token 是 `restore` 还是 `undo` 由它决定, 与 `Current` 是
    /// `LinkedCorrect` 还是 `LinkedWrong` 无关 (`contracts/cli.md` 的 `undo` 分支表).
    backup: Option<PathBuf>,
    /// 备份快照或移除/恢复失败的原因 (FR-014: 不中断其余条目).
    failure: Option<String>,
}

/// `undo`: 消费同一个 `classify` 返回值, 走撤销分支.
///
/// **目标谓词在动手之前求值** (`contracts/cli.md` 的"`undo` 的谓词在动手之前求值"):
/// 谓词问的是"本次要撤销的清单上是否还有本工具创建的链接". 撤销之后链接已不存在,
/// 若在动手后求值将恒为 0, 于是"刚部署好则退 1"与"撤销后该谓词为真"无法同时成立.
/// 因此这里先由判定结果取一份快照 (逐条 token + 备份), 再执行移除; 快照同时就是报告
/// 与退出码的输入 -- 二者同源, 不需要第二份逻辑.
fn undo(manifest: &Manifest, states: &[EntryState]) -> Vec<ReportRow> {
    let mut plan: Vec<UndoItem> = manifest
        .entries
        .iter()
        .zip(states)
        .map(|(entry, state)| {
            // 备份查找取不到结果不等于"无备份": 目录读取失败是环境故障, 与
            // `Unreadable` 同类处置 -- 该条目记为 `fail` 且不再触碰文件系统, 底层原因
            // 随行报告; 把它吞成"无备份"会让条目落进成功列的 `undo` 并移除链接.
            let snapshot = match state.current {
                Current::LinkedCorrect | Current::LinkedWrong => {
                    backup::find_latest_backup(&entry.target_path)
                }
                _ => Ok(None),
            };
            match snapshot {
                Ok(backup) => UndoItem {
                    entry,
                    state,
                    backup,
                    failure: None,
                },
                Err(error) => UndoItem {
                    entry,
                    state,
                    backup: None,
                    failure: Some(error.to_string()),
                },
            }
        })
        .collect();

    // 快照已定, 现在才动手. "哪些 `Current` 允许移除"由 `undo::undo_entry` 一处决定
    // (`Foreign*` / `Absent` / `SourceMissing` / `Unreadable` 下它不触碰任何路径);
    // 快照同时就是恢复动作的输入, 因此报告中写的备份路径与实际恢复的必然是同一个.
    for item in plan.iter_mut() {
        // 快照阶段已失败的条目与 `Unreadable` 同类: 环境故障下什么也不移除, 避免
        // "删掉了链接却不知道有无备份可恢复"造成的无谓破坏.
        if item.failure.is_some() {
            continue;
        }
        let (entry, state) = (item.entry, item.state);
        if let Err(error) = undo::undo_entry(entry, state, item.backup.as_deref()) {
            item.failure = Some(error.to_string());
        }
    }

    plan.into_iter()
        .map(|item| match item.failure {
            Some(reason) => render::fail_row(item.entry, &reason),
            None => render::row_from_state(
                Command::Undo,
                item.entry,
                item.state.current,
                item.state.action,
                &item.state.detail,
                item.backup.is_some(),
                item.backup.as_deref(),
            ),
        })
        .collect()
}

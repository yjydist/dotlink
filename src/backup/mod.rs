//! 备份路径的构造, 备份的写入, 与"最近一次备份"的查找
//! (openspec/specs/backup-lifecycle/spec.md).
//!
//! 备份不是需要维护的数据结构, 而是"文件系统上的命名约定": 位置固定在被覆盖路径的
//! **同一目录**, 名称携带 UTC 的秒级时间戳与消歧序号. "最新"因此可由名字直接
//! 判定: 先比时间戳, 同一秒内再比序号的**数值** (裸名视为 0) -- 而不是整名字典序,
//! 序号 `10` 在字典序上反而小于 `9`. 时间戳必须是 UTC: 定长的 15 字符形态只在同一
//! 时间轴上才能保证字符串序即时间序.
//!
//! 两处不变量由本模块独自承担:
//!
//! - **完整性**: 复制先落到**进程级唯一**的暂存名, 全部复制成功后才原子改名为
//!   最终名, 因此一个"名字看起来完整"的备份必然是完整内容 (中断残留只能是暂存名,
//!   失败清理会尽力移除它). 四类暂存名都以 `.partial` 结尾: 备份为
//!   `<原名>.<标记><时间戳>.<pid>.partial`, 恢复为
//!   `<原名>.<pid>.dotlink-restore.partial`, `apply` 的替换为
//!   `<原名>.<pid>.dotlink-replace.partial`, `undo` 的撤销为
//!   `<原名>.<pid>.dotlink-undo.partial`; 换名重试时消歧序号插在 `.partial` 之前.
//! - **唯一性**: 暂存对象排他创建, 最终名以原子不覆盖移动落位. 两者占用时均追加
//!   序号消歧, 并发备份和遗留暂存对象都不会被覆盖或混入新备份.
//!
//! 模块内部按职责分为两层: `name` 负责命名, 时间戳与查找, `copy` 负责复制, 发布,
//! 恢复与同步. 本文件只保留 crate 级 API 与共享类型; 测试与测试接缝分别位于
//! `tests.rs` 与 `test_support.rs`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::error::Error;

use name::{SELF_HEAL_MARKER, USER_MARKER};

mod copy;
mod name;
#[cfg(test)]
mod test_support;
#[cfg(all(test, unix))]
mod tests;

pub(crate) use copy::{RestoreError, restore_backup, write_backup};
#[cfg(test)]
pub(crate) use name::find_latest_backup;
pub(crate) use name::{BackupFinder, backup_path_for};
#[cfg(test)]
pub(crate) use test_support::fail_backup_path_once;

/// 备份命名空间: 用户内容与工具自愈产生的旧链接分开存放.
///
/// 命名空间只影响名字中的标记, 不改变暂存, 复制, 发布与同步路径.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BackupKind {
    /// 被替换的外部占用物: 普通文件, 目录或外部符号链接.
    User,
    /// 本工具创建但指向错误的旧链接 (重建前先备份链接本身).
    SelfHeal,
}

impl BackupKind {
    fn marker(self) -> &'static str {
        match self {
            Self::User => USER_MARKER,
            Self::SelfHeal => SELF_HEAL_MARKER,
        }
    }
}

/// 已发布的备份: 最终路径与复制起点处顶层对象的元数据.
///
/// 元数据来自 [`copy::copy_object`] 首次对源做的 `symlink_metadata`, 因此"备份的内容"与
/// "身份基准"同源: 调用者可以用它复核删除的对象是不是备份里保存的那一个, 而不必在
/// 备份之前另取一次快照 (设计 D1).
#[derive(Debug)]
pub(crate) struct PublishedBackup {
    pub(crate) path: PathBuf,
    pub(crate) source_metadata: fs::Metadata,
}

/// 统一的条目级错误措辞 (含底层 io 错误).
fn failed(context: &str, path: &Path, error: &io::Error) -> Error {
    Error::EntryFailed(format!("{context} {} 失败: {error}", path.display()))
}

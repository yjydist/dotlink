//! unix 平台的符号链接原语.

use std::io;
use std::path::{Path, PathBuf};

/// 建立指向 `target` 的符号链接 `link_path`.
pub fn create_symlink(target: &Path, link_path: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link_path)
}

/// 读取符号链接的原始文本 (不解析为绝对路径).
pub fn read_link(link_path: &Path) -> io::Result<PathBuf> {
    std::fs::read_link(link_path)
}

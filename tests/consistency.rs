//! 四命令一致性: 同一份清单与同一文件系统状态上, 四条命令对同一条目给出同一个结论
//! (openspec/specs/entry-classification/spec.md 的 FR-017).
//!
//! 契约来源: `src/lib.rs` 在分发到命令之前只调用**一次** `classify`, 四条命令消费的是
//! 同一个 `Vec<EntryState>`; token 由 `src/render.rs::token_for` 按
//! `(命令, Current, Action, 同目录是否有备份)` 唯一决定. 因此"同一状态下四者结论一致"
//! 由数据流保证 (`entry-classification` 的"一致性 MUST 由数据流保证, 而不是由四处逻辑
//! 各自对齐"), 本文件把同一份判定在四条命令上的四个投影并列断言: 任一命令偏离预期,
//! 或两条命令对同一条目的判定来源不再同源, 这里都会失败.
//!
//! 七个条目覆盖: 已是最新 / 未部署 / 指向错误 (仓库内链接) / 别人链接 /
//! 目标被普通文件占用 / 目标被目录占用 / 源缺失; `on_conflict` 全部取缺省值 (skip).
//!
//! 本文件经**真实二进制**断言, 因此依赖 `src/lib.rs` 的命令接入.

mod support;

use std::fs;
use std::path::{Path, PathBuf};

use support::{TempEnv, entry_line_count, line_for, manifest_of, physical_repo, run, token_of};

/// 四条命令, 顺序与 [`EXPECTED`] 的每一行一一对应.
const COMMANDS: [&str; 4] = ["status", "preview", "apply", "undo"];

/// 同一份清单: 7 个条目, 覆盖七种不同的判定结论.
///
/// 第三个字段 `"-"` 表示清单不写 `on_conflict`, 即使用缺省值 skip.
const ENTRIES: [(&str, &str, &str); 7] = [
    // 1. 已是最新: 目标是指向声明源的正确链接.
    ("shell/bashrc", "~/.bashrc", "-"),
    // 2. 未部署: 目标路径不存在.
    ("shell/zshrc", "~/.zshrc", "-"),
    // 3. 指向错误: 链接落在仓库内, 但不是该条目声明的源.
    ("shell/gitconfig", "~/.gitconfig", "-"),
    // 4. 别人链接: 链接落在仓库之外.
    ("shell/vimrc", "~/.vimrc", "-"),
    // 5. 目标被普通文件占用.
    ("nvim/init.lua", "~/.config/nvim/init.lua", "-"),
    // 6. 目标被目录占用.
    ("nvim/lua", "~/.config/nvim/lua", "-"),
    // 7. 源缺失: 仓库中不存在该源, 目标也不存在.
    ("shell/gone", "~/.gone", "-"),
];

/// 四个命令的逐条目行里的 target, 顺序与 [`ENTRIES`] 一致.
const TARGETS: [&str; 7] = [
    "~/.bashrc",
    "~/.zshrc",
    "~/.gitconfig",
    "~/.vimrc",
    "~/.config/nvim/init.lua",
    "~/.config/nvim/lua",
    "~/.gone",
];

/// 每条命令在 7 个条目上的期望 token, 与 [`COMMANDS`] / [`TARGETS`] 同序.
///
/// 每一行都是 `token_for` 对同一份 `Current` 在该命令下的投影:
/// - `status` 只渲染 `Current`, 不渲染 `Action`;
/// - `preview` / `apply` 由 `(Current, Action)` 导出, 二者共用同一张 token 表;
/// - `undo` 由 `(Current, 同目录是否有备份)` 导出, 本夹具没有任何备份.
///
/// 条目 3 是仓库内的错误链接 (本工具产物), 不受 `on_conflict` 约束, 在
/// `preview` / `apply` 下固定报 `replace`; 条目 4-6 是外部占用, 在缺省
/// `on_conflict = skip` 下得到 `RefuseConflict`, 故 `preview` / `apply` 报
/// `conflict`; `undo` 对 `Foreign*` 报 `skip`, 对仓库内的错误链接报 `undo`
/// (它也是本工具的链接, 无备份时只移除).
const EXPECTED: [[&str; 7]; 4] = [
    [
        "ok", "absent", "wrong", "foreign", "foreign", "foreign", "missing",
    ],
    [
        "ok", "create", "replace", "conflict", "conflict", "conflict", "missing",
    ],
    [
        "ok", "create", "replace", "conflict", "conflict", "conflict", "missing",
    ],
    ["undo", "ok", "undo", "skip", "skip", "skip", "missing"],
];

/// 构造与 [`ENTRIES`] 对应的七个条目状态, 返回全新的夹具.
///
/// `apply` / `undo` 会改动文件系统, 因此每条命令都必须从本函数新建夹具; 四次调用
/// 产生的清单与文件系统状态完全相同, 四条命令因而"看到"同一份输入.
fn fixture() -> TempEnv {
    let env = TempEnv::new();

    // 先写全部源文件 (第 7 条的源刻意不写).
    env.write_source("shell/bashrc", "managed-bashrc\n");
    env.write_source("shell/zshrc", "managed-zshrc\n");
    env.write_source("shell/gitconfig", "managed-gitconfig\n");
    env.write_source("shell/other", "other\n");
    env.write_source("shell/vimrc", "managed-vimrc\n");
    env.write_source("nvim/init.lua", "managed-nvim\n");
    env.write_source("nvim/lua", "managed-lua\n");

    env.write_manifest(&manifest_of(&ENTRIES));

    // 1. 已是最新: 指向声明源的正确链接.
    link_to(&env, &physical_repo(&env).join("shell/bashrc"), ".bashrc");

    // 2. 未部署: ~/.zshrc 保持不存在.

    // 3. 指向错误: 链接落在仓库内, 但指向另一个源.
    link_to(&env, &physical_repo(&env).join("shell/other"), ".gitconfig");

    // 4. 别人链接: 链接落在仓库之外.
    let outside = outside_file(&env, "foreign");
    link_to(&env, &outside, ".vimrc");

    // 5. 目标被普通文件占用.
    let plain = env.home().join(".config/nvim/init.lua");
    fs::create_dir_all(plain.parent().expect("目标父目录")).expect("创建目标父目录");
    fs::write(&plain, "pre-existing\n").expect("写入占用物");

    // 6. 目标被目录占用.
    fs::create_dir_all(env.home().join(".config/nvim/lua")).expect("创建占用目录");

    // 7. 源缺失: 既不写源, 也不创建目标.
    assert!(
        fs::symlink_metadata(physical_repo(&env).join("shell/gone")).is_err(),
        "前置条件: 第 7 条的源必须不存在"
    );

    env
}

/// 在 HOME 下建立指向 `destination` 的符号链接 (`target` 用清单里的书写形态定位).
fn link_to(env: &TempEnv, destination: &Path, target: &str) {
    let path = env.home().join(target);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("创建目标父目录");
    }
    if fs::symlink_metadata(&path).is_ok() {
        fs::remove_file(&path).expect("清除既有的目标占用物");
    }
    std::os::unix::fs::symlink(destination, &path).expect("建立符号链接");
}

/// 仓库之外夹具目录 (同一测试进程内所有 `fixture()` 共用同一个).
fn outside_dir() -> PathBuf {
    std::env::temp_dir().join(format!(
        "dotlink-consistency-outside-{}",
        std::process::id()
    ))
}

/// 仓库之外的一个已存在文件 (绝对路径), 用于构造 `foreign` 的别人链接.
///
/// 夹具的 HOME 位于仓库之内, 指向 HOME 的链接会被正确地判为仓库内; 构造 `foreign`
/// 必须落到仓库之外的区域.
fn outside_file(env: &TempEnv, name: &str) -> PathBuf {
    let dir = outside_dir();
    fs::create_dir_all(&dir).expect("创建仓库之外的目录");
    let path = dir.join(name);
    fs::write(&path, "user own content\n").expect("写入仓库之外的文件");
    assert!(
        path.is_absolute(),
        "别人链接必须指向绝对路径: {}",
        path.display()
    );
    assert!(
        !path.starts_with(env.repo()),
        "仓库之外的夹具路径不得落在仓库内: {}",
        path.display()
    );
    path
}

// ---------------------------------------------------------------------------
// 同一份输入上四条命令的逐条结论
// ---------------------------------------------------------------------------

#[test]
fn the_four_commands_reach_the_same_conclusion_on_the_same_manifest() {
    for (column, command) in COMMANDS.iter().enumerate() {
        // 每条命令都在全新的同构夹具上运行: apply / undo 的写操作因此不会影响
        // 下一条命令的输入, 四次运行看到的清单与文件系统状态逐字节相同.
        let env = fixture();
        let observed = run(&env, &[command]);

        // 七种状态并存: 无论按哪条命令的口径, 都存在偏离 / 待办 / 失败, 故四条命令
        // 均退 1 (missing 与 fail 只由"无遗留待办"一条拦下, 同样退 1).
        assert_eq!(
            observed.code, 1,
            "`{command}` 应退 1; stdout:\n{}\nstderr:\n{}",
            observed.stdout, observed.stderr
        );

        // 每个条目恰好一行, 总数与清单一致 (末尾汇总行不计入).
        assert_eq!(
            entry_line_count(&observed.stdout),
            TARGETS.len(),
            "`{command}` 必须为 7 个条目各给出一行:\n{}",
            observed.stdout
        );

        for (index, target) in TARGETS.iter().enumerate() {
            let line = line_for(&observed.stdout, target);
            assert_eq!(
                token_of(line),
                EXPECTED[column][index],
                "`{command}` 在 {target} 上的 token 与既定判定不符\
                 (四条命令消费同一次 classify, 不得各自得出不同结论): {line}"
            );
        }
    }

    // 回收本文件在仓库之外创建的夹具目录, 不给 TMPDIR 留下残留.
    let _ = fs::remove_dir_all(outside_dir());
}

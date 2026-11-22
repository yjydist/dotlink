//! Linux user namespace 下的挂载边界端到端用例.
//!
//! 用例在 `unshare -Ur -m sh -c` 中构造 tmpfs 与 bind mount, 再运行真实的 `dotlink`
//! 二进制. 需要非特权 user namespace 与 mount 能力; 环境不支持时打印原因并跳过,
//! 不以失败收场. macOS 没有等价机制, 整个文件在该平台不编译出任何用例.

#![cfg(target_os = "linux")]

mod support;

use std::fs;
use std::path::Path;
use std::process::Command;

use support::TempEnv;

/// 场景脚本在无法挂载时使用的退出码: 调用方据此打印原因并跳过.
const SKIP_CODE: i32 = 77;

/// 构造一份"目标为目录"的单条目清单.
fn install_manifest(env: &TempEnv, target: &Path) {
    let target = target.to_string_lossy().into_owned();
    env.write_manifest(&support::manifest_of(&[(
        "nvim.lua",
        &target,
        "replace_and_backup",
    )]));
}

/// 探测当前环境能否创建带 mount 的 user namespace 并挂载 tmpfs.
fn mount_namespace_available() -> bool {
    let probe = r#"
command -v mount >/dev/null 2>&1 || exit 77
workspace="$(mktemp -d)" || exit 77
mount -t tmpfs tmpfs "$workspace" || exit 77
umount "$workspace" 2>/dev/null
rmdir "$workspace" 2>/dev/null
"#;
    match Command::new("unshare")
        .args(["-Ur", "-m", "sh", "-c", probe])
        .output()
    {
        Ok(output) if output.status.success() => true,
        Ok(output) => {
            let reason = String::from_utf8_lossy(&output.stderr);
            eprintln!(
                "跳过挂载边界用例: unshare 或 mount 不可用: {}",
                reason.trim()
            );
            false
        }
        Err(error) => {
            eprintln!("跳过挂载边界用例: 无法执行 unshare: {error}");
            false
        }
    }
}

/// 在 user namespace 中运行场景脚本; 返回 `false` 表示环境不支持并已打印原因.
///
/// 脚本约定: `$1` 仓库根, `$2` 临时主目录, `$3` dotlink 二进制. 退出码 77 表示
/// 该场景的挂载能力不可用; 其余非零退出码是测试自身失败.
fn run_scenario(env: &TempEnv, script: &str) -> bool {
    let binary = assert_cmd::cargo::cargo_bin("dotlink");
    let output = Command::new("unshare")
        .args(["-Ur", "-m", "sh", "-c", script])
        .arg("dotlink-mount-boundaries")
        .arg(env.repo())
        .arg(env.home())
        .arg(binary)
        .env("HOME", env.home())
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("USERPROFILE")
        .output()
        .unwrap_or_else(|error| panic!("执行 unshare 失败: {error}"));

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.status.code() == Some(SKIP_CODE) {
        let reason = stdout
            .lines()
            .chain(stderr.lines())
            .find_map(|line| line.strip_prefix("DOTLINK_SKIP "))
            .unwrap_or("环境不支持该挂载场景");
        eprintln!("跳过挂载边界用例: {reason}");
        return false;
    }
    assert!(
        output.status.success(),
        "场景脚本失败: {}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        output.status
    );
    true
}

/// 读取脚本写入 `$home/result.txt` 的 `key=value` 行.
fn result_scalar(env: &TempEnv, key: &str) -> String {
    let path = env.home().join("result.txt");
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("读取 {} 失败: {error}", path.display()));
    text.lines()
        .find_map(|line| line.strip_prefix(&format!("{key}=")))
        .unwrap_or_else(|| panic!("结果缺少 {key}:\n{text}"))
        .to_string()
}

/// `dotlink` 的完整报告 (脚本重定向到 `$home/out.txt`).
fn report(env: &TempEnv) -> String {
    let path = env.home().join("out.txt");
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("读取 {} 失败: {error}", path.display()))
}

/// 断言该条目以 `fail` 结束且退出码为 1, 并返回报告行文本.
fn assert_failed_entry(env: &TempEnv, target: &Path) -> String {
    let report = report(env);
    let line = support::line_for(&report, &target.display().to_string());
    assert_eq!(
        support::token_of(line),
        "fail",
        "条目必须报告 fail, 实际报告: {report}"
    );
    assert_eq!(
        result_scalar(env, "status"),
        "1",
        "该条目失败时退出码必须是 1"
    );
    line.to_string()
}

/// 目录下不得出现最终名的备份 (`.partial` 残渣允许存在, 它不会被当作完整备份).
fn assert_no_final_backup(dir: &Path) {
    let finals: Vec<String> = support::backups_in(dir)
        .into_iter()
        .filter(|name| !name.ends_with(".partial"))
        .collect();
    assert!(finals.is_empty(), "不得发布最终备份名: {finals:?}");
}

fn assert_plain_directory(path: &Path) {
    let metadata = fs::symlink_metadata(path)
        .unwrap_or_else(|error| panic!("读取 {} 失败: {error}", path.display()));
    assert!(metadata.is_dir(), "{} 必须仍是目录", path.display());
    assert!(
        !metadata.file_type().is_symlink(),
        "{} 不得被替换为链接",
        path.display()
    );
}

/// 目标目录内含 tmpfs 挂载点: 备份整体失败, 挂载内容与目标保持原样.
#[test]
fn a_tmpfs_inside_a_directory_target_fails_without_touching_the_mount() {
    if !mount_namespace_available() {
        return;
    }
    let env = TempEnv::with_home_outside_repo();
    let target = env.home().join("nvim");
    fs::create_dir_all(target.join("mnt")).unwrap();
    fs::write(target.join("init.lua"), "old").unwrap();
    env.write_source("nvim.lua", "managed\n");
    install_manifest(&env, &target);

    let script = r##"
set -u
repo="$1"; home="$2"; bin="$3"
target="$home/nvim"
mount -t tmpfs tmpfs "$target/mnt" || { echo "DOTLINK_SKIP 无法挂载 tmpfs"; exit 77; }
printf 'mounted' > "$target/mnt/payload" || exit 90
cd "$repo" || exit 90
"$bin" apply > "$home/out.txt" 2> "$home/err.txt"
code=$?
{
  printf 'status=%s\n' "$code"
  printf 'payload=%s\n' "$(cat "$target/mnt/payload")"
  printf 'mounted=%s\n' "$(grep -q " $target/mnt " /proc/self/mountinfo && echo yes || echo no)"
} > "$home/result.txt"
exit 0
"##;
    if !run_scenario(&env, script) {
        return;
    }

    assert_failed_entry(&env, &target);
    assert_eq!(result_scalar(&env, "payload"), "mounted");
    assert_eq!(result_scalar(&env, "mounted"), "yes");
    assert_eq!(fs::read_to_string(target.join("init.lua")).unwrap(), "old");
    assert_plain_directory(&target);
    assert_no_final_backup(env.home());
}

/// 目标目录内含源仓库的 bind mount: 源仓库的清单与源文件保持原样.
#[test]
fn a_repository_bind_mount_inside_a_directory_target_is_rejected() {
    if !mount_namespace_available() {
        return;
    }
    let env = TempEnv::with_home_outside_repo();
    let target = env.home().join("nvim");
    fs::create_dir_all(target.join("repo")).unwrap();
    fs::write(target.join("init.lua"), "old").unwrap();
    env.write_source("nvim.lua", "managed\n");
    install_manifest(&env, &target);
    let manifest_before = fs::read_to_string(env.repo().join("dotlink.toml")).unwrap();

    let script = r##"
set -u
repo="$1"; home="$2"; bin="$3"
target="$home/nvim"
mount --bind "$repo" "$target/repo" || { echo "DOTLINK_SKIP 无法执行 bind mount"; exit 77; }
cd "$repo" || exit 90
"$bin" apply > "$home/out.txt" 2> "$home/err.txt"
code=$?
{
  printf 'status=%s\n' "$code"
  printf 'source=%s\n' "$(cat "$repo/nvim.lua")"
  printf 'manifest=%s\n' "$(test -f "$repo/dotlink.toml" && echo present || echo missing)"
  printf 'mounted=%s\n' "$(grep -q " $target/repo " /proc/self/mountinfo && echo yes || echo no)"
} > "$home/result.txt"
exit 0
"##;
    if !run_scenario(&env, script) {
        return;
    }

    assert_failed_entry(&env, &target);
    assert_eq!(result_scalar(&env, "source"), "managed");
    assert_eq!(result_scalar(&env, "manifest"), "present");
    assert_eq!(result_scalar(&env, "mounted"), "yes");
    assert_eq!(fs::read_to_string(target.join("init.lua")).unwrap(), "old");
    assert_eq!(
        fs::read_to_string(env.repo().join("dotlink.toml")).unwrap(),
        manifest_before,
        "源仓库的清单不得被修改或删除"
    );
    assert_eq!(
        fs::read_to_string(env.repo().join("nvim.lua")).unwrap(),
        "managed\n"
    );
    assert_plain_directory(&target);
    assert_no_final_backup(env.home());
}

/// 目标自身是挂载点: 不复制挂载内容, 挂载点与其内容保持原样.
#[test]
fn a_target_that_is_itself_a_mount_point_is_rejected() {
    if !mount_namespace_available() {
        return;
    }
    let env = TempEnv::with_home_outside_repo();
    let target = env.home().join("nvim");
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("init.lua"), "old").unwrap();
    env.write_source("nvim.lua", "managed\n");
    install_manifest(&env, &target);

    let script = r##"
set -u
repo="$1"; home="$2"; bin="$3"
target="$home/nvim"
mount -t tmpfs tmpfs "$target" || { echo "DOTLINK_SKIP 无法挂载 tmpfs"; exit 77; }
printf 'mounted' > "$target/payload" || exit 90
cd "$repo" || exit 90
"$bin" apply > "$home/out.txt" 2> "$home/err.txt"
code=$?
{
  printf 'status=%s\n' "$code"
  printf 'payload=%s\n' "$(cat "$target/payload")"
  printf 'mounted=%s\n' "$(grep -q " $target " /proc/self/mountinfo && echo yes || echo no)"
} > "$home/result.txt"
exit 0
"##;
    if !run_scenario(&env, script) {
        return;
    }

    assert_failed_entry(&env, &target);
    assert_eq!(result_scalar(&env, "payload"), "mounted");
    assert_eq!(result_scalar(&env, "mounted"), "yes");
    assert_eq!(fs::read_to_string(target.join("init.lua")).unwrap(), "old");
    assert_plain_directory(&target);
    assert_no_final_backup(env.home());
}

/// 目录内单个文件是 bind mount: 在读取内容之前失败, 挂载内容与目标保持原样.
#[test]
fn a_file_mount_inside_a_directory_target_is_rejected() {
    if !mount_namespace_available() {
        return;
    }
    let env = TempEnv::with_home_outside_repo();
    let target = env.home().join("nvim");
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("init.lua"), "old").unwrap();
    fs::write(env.home().join("pinned.txt"), "pinned").unwrap();
    env.write_source("nvim.lua", "managed\n");
    install_manifest(&env, &target);

    let script = r##"
set -u
repo="$1"; home="$2"; bin="$3"
target="$home/nvim"
mount --bind "$home/pinned.txt" "$target/init.lua" || { echo "DOTLINK_SKIP 无法执行文件 bind mount"; exit 77; }
cd "$repo" || exit 90
"$bin" apply > "$home/out.txt" 2> "$home/err.txt"
code=$?
{
  printf 'status=%s\n' "$code"
  printf 'pinned=%s\n' "$(cat "$home/pinned.txt")"
  printf 'mounted=%s\n' "$(grep -q " $target/init.lua " /proc/self/mountinfo && echo yes || echo no)"
} > "$home/result.txt"
exit 0
"##;
    if !run_scenario(&env, script) {
        return;
    }

    assert_failed_entry(&env, &target);
    assert_eq!(result_scalar(&env, "pinned"), "pinned");
    assert_eq!(result_scalar(&env, "mounted"), "yes");
    assert_eq!(
        fs::read_to_string(env.home().join("pinned.txt")).unwrap(),
        "pinned",
        "文件挂载点的内容不得被修改"
    );
    assert_eq!(fs::read_to_string(target.join("init.lua")).unwrap(), "old");
    assert_plain_directory(&target);
    assert_no_final_backup(env.home());
}

/// 备份目录内含挂载点: `undo` 报告失败, 链接回滚, 备份与挂载内容不变.
#[test]
fn undo_stops_at_a_mount_point_inside_the_backup() {
    if !mount_namespace_available() {
        return;
    }
    let env = TempEnv::with_home_outside_repo();
    let source = env.repo().join("nvim.lua");
    fs::write(&source, "managed\n").unwrap();
    let target = env.home().join("nvim");
    std::os::unix::fs::symlink(&source, &target).unwrap();

    let backup = env.home().join("nvim.dotlink-backup.20260101T000000");
    fs::create_dir_all(backup.join("lua")).unwrap();
    fs::create_dir_all(backup.join("mnt")).unwrap();
    fs::write(backup.join("lua/a.lua"), "old-a").unwrap();
    install_manifest(&env, &target);

    let script = r##"
set -u
repo="$1"; home="$2"; bin="$3"
target="$home/nvim"
backup="$home/nvim.dotlink-backup.20260101T000000"
mount -t tmpfs tmpfs "$backup/mnt" || { echo "DOTLINK_SKIP 无法挂载 tmpfs"; exit 77; }
printf 'mounted' > "$backup/mnt/payload" || exit 90
cd "$repo" || exit 90
"$bin" undo > "$home/out.txt" 2> "$home/err.txt"
code=$?
{
  printf 'status=%s\n' "$code"
  printf 'payload=%s\n' "$(cat "$backup/mnt/payload")"
  printf 'mounted=%s\n' "$(grep -q " $backup/mnt " /proc/self/mountinfo && echo yes || echo no)"
  printf 'link=%s\n' "$(readlink "$target")"
} > "$home/result.txt"
exit 0
"##;
    if !run_scenario(&env, script) {
        return;
    }

    assert_failed_entry(&env, &target);
    assert_eq!(result_scalar(&env, "payload"), "mounted");
    assert_eq!(result_scalar(&env, "mounted"), "yes");
    assert_eq!(
        result_scalar(&env, "link"),
        source.display().to_string(),
        "恢复失败时链接必须回滚到原路径"
    );
    assert_eq!(fs::read_link(&target).unwrap(), source);
    assert_eq!(
        fs::read_to_string(backup.join("lua/a.lua")).unwrap(),
        "old-a",
        "备份内容不得被修改"
    );
    let residue: Vec<String> = fs::read_dir(env.home())
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| {
            name.contains(".dotlink-undo.partial") || name.contains(".dotlink-restore.partial")
        })
        .collect();
    assert!(residue.is_empty(), "失败不得残留暂存对象: {residue:?}");
}

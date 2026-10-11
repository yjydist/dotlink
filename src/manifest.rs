//! 清单解析与全量校验 (V-1 到 V-8).

use std::path::Path;

use crate::entry::{self, Entry, OnConflict};
use crate::error::{Error, Result};

/// 清单文件的固定名字: 当前工作目录下的这一个, 不向上搜索父目录 (FR-015).
const MANIFEST_NAME: &str = "dotlink.toml";

/// 顶层唯一合法键.
const ENTRY_KEY: &str = "entry";
/// 条目内的三个合法键.
const SOURCE_KEY: &str = "source";
const TARGET_KEY: &str = "target";
const ON_CONFLICT_KEY: &str = "on_conflict";
/// V-8 的合法键清单 (顺序即说明中的展示顺序).
const ENTRY_KEYS: [&str; 3] = [SOURCE_KEY, TARGET_KEY, ON_CONFLICT_KEY];

/// `on_conflict` 的两个合法字面量.
const SKIP_VALUE: &str = "skip";
const REPLACE_VALUE: &str = "replace_and_backup";

/// V-8 说明中的两个层级名 (`contracts/manifest.md` 的两层合法键表).
const TOP_LEVEL: &str = "顶层";
const ENTRY_LEVEL: &str = "条目内";

/// 已解析的清单. `repo_root` 即清单文件所在目录.
#[derive(Debug, Clone)]
pub struct Manifest {
    pub repo_root: std::path::PathBuf,
    pub entries: Vec<Entry>,
}

/// 从当前工作目录读取并校验 `dotlink.toml`.
///
/// V-1 的前一半: 清单文件与**当前工作目录**都必须可用. 二者的说明必须可区分
/// (FR-015) -- 目录已被删除时报"找不到清单"会把用户引向错误的方向.
pub fn load() -> Result<Manifest> {
    let cwd = std::env::current_dir().map_err(|error| {
        Error::ManifestInvalid(format!("当前工作目录不可用, 无法定位清单: {error}"))
    })?;

    let path = cwd.join(MANIFEST_NAME);
    let text = std::fs::read_to_string(&path).map_err(|error| {
        Error::ManifestInvalid(format!("无法读取清单 ./{MANIFEST_NAME}: {error}"))
    })?;

    parse(&text, &cwd)
}

/// 解析并校验给定的清单文本.
///
/// 纯函数: 只读取 `text` 与 `repo_root` (以及展开 `~` 所需的 `HOME`), 不产生任何
/// 文件系统改动. 校验规则 V-1 到 V-8 见 `data-model.md` 第 1 节, 逐条对应如下:
/// V-2 语法 (本函数前半), V-3 条目非空, V-4 两字段非空, V-5 目标路径互不重复,
/// V-6 目标可解析为绝对路径, V-7 `on_conflict` 取值合法, V-8 只出现已定义的键.
///
/// 不校验的三件事: 源条目在仓库中是否存在 (那是运行时状态 `SourceMissing`),
/// `target` 是否与某个 `source` 指向同一路径 (自指清单合法), 以及条目数量上限.
pub fn parse(text: &str, repo_root: &Path) -> Result<Manifest> {
    // 手工从 `toml::Table` 读而不是反序列化进一个 `#[derive(Deserialize)]` 的中间
    // 结构: `#[serde(deny_unknown_fields)]` 的错误里没有条目下标, 而 V-4 到 V-8 都
    // 必须指出下标 / 键名 / 层级, 逐键遍历是唯一能同时给出这三者的形态.
    let root = toml::from_str::<toml::Table>(text).map_err(|error| syntax_error(text, &error))?;

    // V-8 (顶层): 只允许 `entry`.
    for name in root.keys() {
        if name != ENTRY_KEY {
            return Err(Error::ManifestInvalid(format!(
                "{TOP_LEVEL}出现未知键 `{name}`: 只允许 `{ENTRY_KEY}`"
            )));
        }
    }

    // V-3: `entry` 必须存在且非空.
    let Some(value) = root.get(ENTRY_KEY) else {
        return Err(Error::ManifestInvalid(format!(
            "清单缺少 `{ENTRY_KEY}`: 至少需要一个条目"
        )));
    };
    let Some(written_entries) = value.as_array() else {
        return Err(Error::ManifestInvalid(format!(
            "`{ENTRY_KEY}` 必须是条目数组 (`[[{ENTRY_KEY}]]`)"
        )));
    };
    if written_entries.is_empty() {
        return Err(Error::ManifestInvalid(format!(
            "`{ENTRY_KEY}` 不能为空: 至少需要一个条目"
        )));
    }

    let mut entries: Vec<Entry> = Vec::with_capacity(written_entries.len());

    for (index, written) in written_entries.iter().enumerate() {
        let Some(table) = written.as_table() else {
            return Err(invalid_entry(
                index,
                format!("必须是表 (`[[{ENTRY_KEY}]]`)"),
            ));
        };

        // V-8 (条目内): 只允许 source / target / on_conflict. 拼错的 `on_conflict`
        // 若被静默忽略会退化成本不存在的默认 `skip`, 比报错危险得多.
        for name in table.keys() {
            if !ENTRY_KEYS.contains(&name.as_str()) {
                return Err(invalid_entry(
                    index,
                    format!(
                        "{ENTRY_LEVEL}出现未知键 `{name}`: 只允许 {}",
                        allowed_keys()
                    ),
                ));
            }
        }

        // V-4: 两个必填字段都必须是非空字符串.
        let source = required_string(table, index, SOURCE_KEY)?;
        let target = required_string(table, index, TARGET_KEY)?;
        // V-7: 取值属于 {skip, replace_and_backup}, 缺省为 skip.
        let on_conflict = parse_on_conflict(table, index)?;

        // V-6: `entry::resolve` 是路径形态"能否解析"的唯一裁决处 (`~` 展开 / 相对
        // 仓库根的挂接 / 源侧只接受相对路径都在那里); 这里只补上条目下标与两个书写
        // 形态. `error` 自身已说明是哪一侧、哪种形态非法, 因此不再加统一前缀.
        let resolved =
            entry::resolve(&source, &target, on_conflict, repo_root).map_err(|error| {
                invalid_entry(
                    index,
                    format!("{error} ({SOURCE_KEY} = `{source}`, {TARGET_KEY} = `{target}`)"),
                )
            })?;

        // V-5: 比较 `os::normalize` 之后的 `target_path` (由 `resolve` 产出).
        // 不得用 `Path::canonicalize`: 待部署的目标通常尚不存在.
        if let Some(previous) = entries
            .iter()
            .position(|existing| existing.target_path == resolved.target_path)
        {
            return Err(Error::ManifestInvalid(format!(
                "条目 {previous} (`{}`) 与条目 {index} (`{}`) 声明了同一个目标路径: {}",
                entry_label(previous),
                entry_label(index),
                resolved.target_path.display()
            )));
        }

        entries.push(resolved);
    }

    Ok(Manifest {
        repo_root: repo_root.to_path_buf(),
        entries,
    })
}

/// V-2: 把 TOML 解析错误转成含**出错位置**与**原因**的单行说明.
fn syntax_error(text: &str, error: &toml::de::Error) -> Error {
    let position = match error.span() {
        Some(span) => {
            let end = span.start.min(text.len());
            let line = text.as_bytes()[..end]
                .iter()
                .filter(|byte| **byte == b'\n')
                .count()
                + 1;
            format!("第 {line} 行")
        }
        None => "位置未知".to_string(),
    };

    // 解析器给的原因可能自带换行 (例如 "invalid string" + "expected ..."), 说明必须
    // 是单行, 因此折成一句.
    let reason = error
        .message()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("; ");

    Error::ManifestInvalid(format!("清单语法错误 ({position}): {reason}"))
}

/// V-4: 取出必填的非空字符串键.
fn required_string(table: &toml::Table, index: usize, key: &str) -> Result<String> {
    let Some(value) = table.get(key) else {
        return Err(invalid_entry(index, format!("缺少必填键 `{key}`")));
    };
    let Some(written) = value.as_str() else {
        return Err(invalid_entry(
            index,
            format!("`{key}` 必须是字符串, 实际为 {}", value.type_str()),
        ));
    };
    if written.is_empty() {
        return Err(invalid_entry(index, format!("`{key}` 不能为空")));
    }

    Ok(written.to_string())
}

/// V-7: `on_conflict` 缺省为 `skip`; 出现时必须恰为两个合法字面量之一.
///
/// 这里逐个字面量比对而不是交给 serde 的枚举反序列化: 说明必须能指出**下标与非法
/// 值**, 而字段级的 serde 错误拿不到条目下标.
fn parse_on_conflict(table: &toml::Table, index: usize) -> Result<OnConflict> {
    let Some(value) = table.get(ON_CONFLICT_KEY) else {
        return Ok(OnConflict::Skip);
    };
    let Some(written) = value.as_str() else {
        return Err(invalid_entry(
            index,
            format!(
                "`{ON_CONFLICT_KEY}` 必须是字符串, 实际为 {}",
                value.type_str()
            ),
        ));
    };

    match written {
        SKIP_VALUE => Ok(OnConflict::Skip),
        REPLACE_VALUE => Ok(OnConflict::ReplaceAndBackup),
        illegal => Err(invalid_entry(
            index,
            format!(
                "`{ON_CONFLICT_KEY}` 的取值 `{illegal}` 非法: 只允许 `{SKIP_VALUE}` 与 `{REPLACE_VALUE}`"
            ),
        )),
    }
}

/// 条目级校验失败的统一说明: V-4 到 V-8 的每一条都必须能指出条目下标.
fn invalid_entry(index: usize, detail: impl std::fmt::Display) -> Error {
    Error::ManifestInvalid(format!("条目 {index} (`{}`): {detail}", entry_label(index)))
}

/// 条目下标的写法: `entry[3]`.
fn entry_label(index: usize) -> String {
    format!("{ENTRY_KEY}[{index}]")
}

/// V-8 说明中的合法键清单.
fn allowed_keys() -> String {
    ENTRY_KEYS
        .iter()
        .map(|key| format!("`{key}`"))
        .collect::<Vec<_>>()
        .join(" / ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::OnConflict;
    use std::path::PathBuf;

    /// 测试用的仓库根. `parse` 是纯函数, 不要求该路径存在.
    fn repo() -> PathBuf {
        PathBuf::from("/repo")
    }

    /// 断言 `parse` 整体拒绝, 且说明中含全部 `needles`.
    ///
    /// 说明文字是给人读的, 但"哪个下标 / 哪个键 / 哪一层级"必须落成文字, 因此以
    /// 子串形式锁定这三类信息.
    #[track_caller]
    fn assert_invalid(text: &str, needles: &[&str]) {
        let error = parse(text, &repo())
            .err()
            .unwrap_or_else(|| panic!("期望整体拒绝, 实际接受了这份清单:\n{text}"));
        let message = match error {
            Error::ManifestInvalid(message) => message,
            other => panic!("期望 Error::ManifestInvalid, 实际为 {other:?}"),
        };
        for needle in needles {
            assert!(message.contains(needle), "说明中缺少 {needle:?}: {message}");
        }
    }

    /// 在**没有** `HOME` 的子进程中跑 `parse`, 返回标准错误里的说明.
    ///
    /// edition 2024 起 `std::env::set_var` 是 `unsafe` (进程级环境变量与其它线程存在
    /// 数据竞争), 因此 `HOME` 缺失这一分支只能在隔离子进程里验证.
    fn parse_without_home(text: &str) -> String {
        let output = std::process::Command::new(std::env::current_exe().expect("测试二进制路径"))
            .args([
                "--ignored",
                "--exact",
                "--nocapture",
                "manifest::tests::home_absent_probe",
            ])
            .env("DOTLINK_TEST_MANIFEST", text)
            .env_remove("HOME")
            .output()
            .expect("重新执行测试二进制");

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success() && stdout.contains("1 passed"),
            "子进程未跑到探针: {stdout}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stderr).into_owned()
    }

    /// 子进程侧的探针: 把 `HOME` 缺失时的说明打到标准错误, 供父进程断言.
    ///
    /// 默认 `#[ignore]`, 只在 `parse_without_home` 显式拉起的子进程中运行.
    #[test]
    #[ignore = "由 parse_without_home 在隔离子进程中拉起"]
    fn home_absent_probe() {
        let text = std::env::var("DOTLINK_TEST_MANIFEST").expect("父进程应传入清单文本");
        assert!(
            std::env::var_os("HOME").is_none(),
            "该探针必须在没有 HOME 的进程中运行"
        );
        match parse(&text, &repo()) {
            Err(Error::ManifestInvalid(message)) => eprintln!("{message}"),
            other => panic!("HOME 缺失时期望整体拒绝, 实际为 {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // V-1: 清单文件存在且可读 (文件不存在这一半属 `load`, 由 tests/manifest_errors.rs
    // 端到端覆盖; 此处只锁定"解析入口不接受一份没有条目的清单")
    // -----------------------------------------------------------------------

    #[test]
    fn v1_a_manifest_without_the_entry_key_is_rejected() {
        assert_invalid("# 只有注释, 没有条目\n", &["entry"]);
    }

    // -----------------------------------------------------------------------
    // V-2: TOML 语法合法 (报告出错位置与原因)
    // -----------------------------------------------------------------------

    #[test]
    fn v2_syntax_error_reports_the_position_of_the_failure() {
        // 值不是合法的 TOML 值: 出错位置在第 2 行
        assert_invalid(
            "[[entry]]\nsource = a\ntarget = \"b\"\n",
            &["清单语法错误", "第 2 行"],
        );

        // 表头未闭合: 出错位置在第 1 行 (与上一条不同的行号, 以证明位置真的被算出)
        assert_invalid("[[entry]\nsource = \"a\"\n", &["清单语法错误", "第 1 行"]);
    }

    #[test]
    fn v2_syntax_error_reports_a_reason() {
        let error = parse("[[entry]]\n[", &repo()).expect_err("语法非法必须整体拒绝");
        let Error::ManifestInvalid(message) = error else {
            panic!("语法非法必须归入 ManifestInvalid");
        };
        let (position, reason) = message.rsplit_once("): ").expect("说明应含位置与原因");
        assert!(position.contains("第 "), "缺少出错位置: {message}");
        assert!(!reason.trim().is_empty(), "缺少出错原因: {message}");
    }

    // -----------------------------------------------------------------------
    // V-3: `entry` 非空
    // -----------------------------------------------------------------------

    #[test]
    fn v3_entry_must_be_present_and_non_empty() {
        assert_invalid("", &["entry", "至少需要一个条目"]);
        assert_invalid("entry = []\n", &["entry", "至少需要一个条目"]);
        // 顶层是 `[entry]` 表而非 `[[entry]]` 数组表
        assert_invalid("[entry]\nsource = \"a\"\ntarget = \"b\"\n", &["entry"]);
    }

    #[test]
    fn v3_a_non_table_element_in_the_entry_array_is_rejected() {
        assert_invalid("entry = [1]\n", &["entry[0]"]);
    }

    // -----------------------------------------------------------------------
    // V-4: 每个条目 `source` 与 `target` 均非空 (指出下标)
    // -----------------------------------------------------------------------

    #[test]
    fn v4_empty_source_points_at_the_entry_index() {
        assert_invalid(
            "[[entry]]\nsource = \"\"\ntarget = \"~/.bashrc\"\n",
            &["entry[0]", "source", "不能为空"],
        );
    }

    #[test]
    fn v4_empty_target_points_at_the_entry_index() {
        // 下标 1: 第一个条目是合法的, 只有第二个条目的 target 为空
        assert_invalid(
            "[[entry]]\nsource = \"a\"\ntarget = \"b\"\n\n[[entry]]\nsource = \"c\"\ntarget = \"\"\n",
            &["entry[1]", "target", "不能为空"],
        );
    }

    #[test]
    fn v4_a_missing_source_or_target_key_is_rejected() {
        assert_invalid("[[entry]]\ntarget = \"b\"\n", &["entry[0]", "source"]);
        assert_invalid("[[entry]]\nsource = \"a\"\n", &["entry[0]", "target"]);
    }

    #[test]
    fn v4_a_non_string_source_or_target_is_rejected() {
        assert_invalid(
            "[[entry]]\nsource = 1\ntarget = \"b\"\n",
            &["entry[0]", "source"],
        );
        assert_invalid(
            "[[entry]]\nsource = \"a\"\ntarget = 1\n",
            &["entry[0]", "target"],
        );
    }

    // -----------------------------------------------------------------------
    // V-5: 不存在两个条目声明同一个规范化后的目标路径 (指出冲突的两个下标)
    // -----------------------------------------------------------------------

    #[test]
    fn v5_two_entries_with_the_same_normalized_target_conflict() {
        // 书写形态不同, 规范化后同一路径
        assert_invalid(
            "[[entry]]\nsource = \"a\"\ntarget = \"~/.bashrc\"\n\n[[entry]]\nsource = \"b\"\ntarget = \"~/./.bashrc\"\n",
            &["entry[0]", "entry[1]", "目标路径"],
        );
        assert_invalid(
            "[[entry]]\nsource = \"a\"\ntarget = \"link/x\"\n\n[[entry]]\nsource = \"b\"\ntarget = \"link/../link/x\"\n",
            &["entry[0]", "entry[1]", "目标路径"],
        );
    }

    #[test]
    fn v5_the_reported_pair_is_the_first_collision() {
        // 第 3 个条目与第 1 个冲突: 报的是 (0, 2) 而不是别的一对
        assert_invalid(
            "[[entry]]\nsource = \"a\"\ntarget = \"dup\"\n\n[[entry]]\nsource = \"b\"\ntarget = \"other\"\n\n[[entry]]\nsource = \"c\"\ntarget = \"dup\"\n",
            &["entry[0]", "entry[2]", "目标路径"],
        );
    }

    #[test]
    fn v5_the_same_source_with_different_targets_is_allowed() {
        let text = "[[entry]]\nsource = \"a\"\ntarget = \"one\"\n\n[[entry]]\nsource = \"a\"\ntarget = \"two\"\n";
        let manifest = parse(text, &repo()).expect("同一源配不同目标是合法用法");
        assert_eq!(manifest.entries.len(), 2);
        assert_eq!(manifest.entries[0].source_path, repo().join("a"));
        assert_eq!(manifest.entries[1].source_path, repo().join("a"));
        assert_eq!(manifest.entries[0].target_path, repo().join("one"));
        assert_eq!(manifest.entries[1].target_path, repo().join("two"));
    }

    // -----------------------------------------------------------------------
    // V-6: 每条目 `target` 可解析为绝对路径 (指出下标与原因)
    // -----------------------------------------------------------------------

    #[test]
    fn v6_tilde_without_home_points_at_the_entry_index() {
        // 下标 1: 第一个条目的目标是绝对的, 只有第二个条目需要 HOME
        let message = parse_without_home(
            "[[entry]]\nsource = \"a\"\ntarget = \"/etc/x\"\n\n[[entry]]\nsource = \"b\"\ntarget = \"~/.bashrc\"\n",
        );
        for needle in ["entry[1]", "target", "绝对路径", "HOME"] {
            assert!(message.contains(needle), "说明中缺少 {needle:?}: {message}");
        }
    }

    #[test]
    fn v6_absolute_and_relative_targets_are_resolvable() {
        // 绝对 target 不依赖仓库根. 断言与实现同源的规范化结果, 而不是字面量:
        // macOS 上 `/etc` 是 `/private/etc` 的符号链接, 写死字面量会把本机细节钉
        // 进断言 (Linux 上 `/etc` 不是别名, 两侧本就相等).
        let written = "/etc/x";
        let text = format!(
            "[[entry]]\nsource = \"a\"\ntarget = \"{written}\"\n\n[[entry]]\nsource = \"b\"\ntarget = \"link/b\"\n"
        );
        let manifest = parse(&text, &repo()).expect("绝对路径与相对仓库根的路径都合法");
        assert_eq!(
            manifest.entries[0].target_path,
            crate::os::normalize(Path::new(written))
        );
        assert_eq!(
            manifest.entries[1].target_path,
            crate::os::normalize(&repo().join("link/b"))
        );
    }

    // -----------------------------------------------------------------------
    // V-7: `on_conflict` 取值属于 {skip, replace_and_backup} (指出下标与非法值)
    // -----------------------------------------------------------------------

    #[test]
    fn v7_illegal_on_conflict_reports_index_and_value() {
        assert_invalid(
            "[[entry]]\nsource = \"a\"\ntarget = \"b\"\non_conflict = \"yes\"\n",
            &["entry[0]", "on_conflict", "yes"],
        );
        // 大小写不同不是合法取值: 取值是两个精确字面量
        assert_invalid(
            "[[entry]]\nsource = \"a\"\ntarget = \"b\"\n\n[[entry]]\nsource = \"c\"\ntarget = \"d\"\non_conflict = \"ReplaceAndBackup\"\n",
            &["entry[1]", "on_conflict", "ReplaceAndBackup"],
        );
        // 非字符串
        assert_invalid(
            "[[entry]]\nsource = \"a\"\ntarget = \"b\"\non_conflict = 1\n",
            &["entry[0]", "on_conflict"],
        );
    }

    #[test]
    fn v7_both_legal_values_and_the_default_are_accepted() {
        let text = "[[entry]]\nsource = \"a\"\ntarget = \"one\"\non_conflict = \"skip\"\n\n[[entry]]\nsource = \"b\"\ntarget = \"two\"\non_conflict = \"replace_and_backup\"\n\n[[entry]]\nsource = \"c\"\ntarget = \"three\"\n";
        let manifest = parse(text, &repo()).expect("合法取值与缺省都应当被接受");
        assert_eq!(manifest.entries[0].on_conflict, OnConflict::Skip);
        assert_eq!(
            manifest.entries[1].on_conflict,
            OnConflict::ReplaceAndBackup
        );
        assert_eq!(manifest.entries[2].on_conflict, OnConflict::Skip);
    }

    // -----------------------------------------------------------------------
    // V-8: 只出现已定义的键 (两层各自校验, 指出键名与所在层级)
    // -----------------------------------------------------------------------

    #[test]
    fn v8_unknown_top_level_key_reports_key_name_and_level() {
        assert_invalid("[foo]\nbar = 1\n", &["foo", "顶层"]);
        assert_invalid(
            "bogus = 1\n[[entry]]\nsource = \"a\"\ntarget = \"b\"\n",
            &["bogus", "顶层"],
        );
    }

    #[test]
    fn v8_unknown_top_level_key_is_rejected_even_when_entries_are_valid() {
        assert_invalid(
            "[[entry]]\nsource = \"a\"\ntarget = \"b\"\n\n[foo]\nbar = 1\n",
            &["foo", "顶层"],
        );
    }

    #[test]
    fn v8_unknown_entry_key_reports_key_name_and_level() {
        // 拼错的 on_conflict 若被静默忽略, 用户会以为配置生效了却没有生效
        assert_invalid(
            "[[entry]]\nsource = \"a\"\ntarget = \"b\"\nonconflict = \"replace_and_backup\"\n",
            &["entry[0]", "onconflict", "条目内"],
        );
        assert_invalid(
            "[[entry]]\nsorce = \"a\"\ntarget = \"b\"\n",
            &["entry[0]", "sorce", "条目内"],
        );
        // 下标 1: 第一个条目合法, 只有第二个条目有拼错的键
        assert_invalid(
            "[[entry]]\nsource = \"a\"\ntarget = \"b\"\n\n[[entry]]\nsource = \"c\"\ntarget = \"d\"\nsorce = \"e\"\n",
            &["entry[1]", "sorce", "条目内"],
        );
    }

    // -----------------------------------------------------------------------
    // 校验范围之外的形态: 必须被接受
    // -----------------------------------------------------------------------

    #[test]
    fn a_complete_manifest_is_accepted_and_resolved() {
        let text = "[[entry]]\nsource = \"shell/bashrc\"\ntarget = \"~/.bashrc\"\n\n[[entry]]\nsource = \"git/gitconfig\"\ntarget = \"~/.gitconfig\"\non_conflict = \"replace_and_backup\"\n";
        let manifest = parse(text, &repo()).expect("合法清单");
        assert_eq!(manifest.repo_root, repo());
        assert_eq!(manifest.entries.len(), 2);
        assert_eq!(manifest.entries[0].source, "shell/bashrc");
        assert_eq!(manifest.entries[0].target, "~/.bashrc");
        assert_eq!(manifest.entries[0].source_path, repo().join("shell/bashrc"));
        assert_eq!(
            manifest.entries[1].on_conflict,
            OnConflict::ReplaceAndBackup
        );
    }

    #[test]
    fn a_self_referential_manifest_is_still_valid() {
        // 校验不比较 target 与 source: 二者的重合由判定阶段的 conflict 暴露
        let text = "[[entry]]\nsource = \"shell/bashrc\"\ntarget = \"shell/bashrc\"\n";
        let entry = parse(text, &repo())
            .expect("自指清单合法")
            .entries
            .remove(0);
        assert_eq!(entry.source_path, entry.target_path);
    }

    #[test]
    fn a_missing_source_entry_is_not_a_manifest_defect() {
        // 源条目在仓库中不存在属运行时状态 SourceMissing, 不属校验范围
        let text = "[[entry]]\nsource = \"definitely/gone\"\ntarget = \"link/gone\"\n";
        let manifest = parse(text, &repo()).expect("不校验源条目是否存在");
        assert_eq!(manifest.entries.len(), 1);
    }
}

//! 清单解析与全量校验 (V-1 到 V-11).

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use crate::entry::{self, Entry, OnConflict};
use crate::error::{Error, Result};
use crate::os;

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

/// V-8 说明中的两个层级名 (openspec/specs/manifest-schema/spec.md 的 V-8 两层合法键表).
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
/// 文件系统改动. 校验规则 V-1 到 V-8 见 `openspec/specs/manifest-schema/spec.md`, 逐条对应如下:
/// V-2 语法 (本函数前半), V-3 条目非空, V-4 两字段非空, V-5 目标路径互不重复,
/// V-6 目标可解析为绝对路径, V-7 `on_conflict` 取值合法, V-8 只出现已定义的键.
/// V-9 (目标落点不得退化) 与 V-10 (目标不得自指) 是其后新增的两条落点校验,
/// 定义见 `openspec/specs/manifest-schema/spec.md`.
///
/// V-11 在全部条目解析后检查目标之间, 目标与源之间的路径隔离.
/// 不校验源条目是否存在 (那是运行时状态 `SourceMissing`) 或条目数量上限.
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
    // 仓库根与主目录在整个解析中不变: 各规范化一次即可, 不随条目数重复访问文件系统.
    // V-9 的仓库形态比较基准与 `entry::resolve_with` 的越界判定同源.
    let repo = os::normalize(repo_root);
    let home = entry::home().map(|path| os::normalize(&path));
    // `HOME` 自身可能是符号链接 (例如 `/home/u -> /mnt/u`), 于是同一主目录有两个
    // 路径形态: `HOME` 的书写形态, 以及解析它的符号链接后的物理形态. V-9 必须两种
    // 都识别, 否则把主目录落点写成物理形态时会被静默放行, 一次 `replace_and_backup`
    // 就会备份并替换整个主目录.
    let home_physical = home.as_ref().ok().map(|home| physical_home(home));
    // V-5 的重复检测: 目标路径 -> 首次声明它的条目下标. 用哈希表把逐条目两两比较的
    // O(N^2) 降到 O(N), 并保持"报第一对冲突"的语义 (get 返回的是最早的下标).
    let mut first_declared_at: HashMap<std::path::PathBuf, usize> =
        HashMap::with_capacity(written_entries.len());

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

        // V-6: `entry::resolve_with` 是路径形态"能否解析"的唯一裁决处 (`~` 展开 /
        // 相对仓库根的挂接 / 源侧只接受相对路径都在那里); 这里只补上条目下标与两个
        // 书写形态. `error` 自身已说明是哪一侧, 哪种形态非法, 因此不再加统一前缀.
        let resolved = entry::resolve_with(
            &source,
            &target,
            on_conflict,
            repo_root,
            &repo,
            home.as_deref(),
        )
        .map_err(|error| {
            invalid_entry(
                index,
                format!("{error} ({SOURCE_KEY} = `{source}`, {TARGET_KEY} = `{target}`)"),
            )
        })?;

        // V-9: 目标落点不得退化 (文件系统根 / 主目录 / 仓库根本身或其祖先).
        reject_degenerate_target(
            index,
            &resolved,
            &repo,
            home.as_deref().ok(),
            home_physical.as_deref(),
        )?;
        // V-10: 目标不得指向本条目自己的 source -- 归一化后重合时建出的链接指向
        // 自己, 恒为断链. 跨条目和祖先关系由全量 V-11 校验处理.
        if resolved.target_path == resolved.source_path {
            return Err(invalid_entry(
                index,
                format!(
                    "`{target}` 归一化后与本条目的 `{SOURCE_KEY}` 相同 (均为 `{}`): 自指链接恒为断链",
                    resolved.target_path.display()
                ),
            ));
        }

        // V-5: 比较 `os::normalize` 之后的 `target_path` (由 `resolve_with` 产出).
        // 不得用 `Path::canonicalize`: 待部署的目标通常尚不存在.
        if let Some(&previous) = first_declared_at.get(resolved.target_path.as_path()) {
            return Err(Error::ManifestInvalid(format!(
                "条目 `{}` 与条目 `{}` 声明了同一个目标路径: {}",
                entry_label(previous),
                entry_label(index),
                resolved.target_path.display()
            )));
        }
        first_declared_at.insert(resolved.target_path.clone(), index);

        entries.push(resolved);
    }

    reject_overlapping_paths(&entries, &first_declared_at)?;

    Ok(Manifest {
        repo_root: repo_root.to_path_buf(),
        entries,
    })
}

fn reject_overlapping_paths(
    entries: &[Entry],
    targets: &HashMap<std::path::PathBuf, usize>,
) -> Result<()> {
    let mut sources = BTreeMap::new();
    for (index, entry) in entries.iter().enumerate() {
        sources.entry(entry.source_path.as_path()).or_insert(index);
    }

    for (index, entry) in entries.iter().enumerate() {
        for ancestor in entry.target_path.ancestors().skip(1) {
            if let Some(&other) = targets.get(ancestor) {
                return Err(path_overlap(index, entry, other, TARGET_KEY, ancestor));
            }
        }
        for ancestor in entry.target_path.ancestors() {
            if let Some(&other) = sources.get(ancestor) {
                return Err(path_overlap(index, entry, other, SOURCE_KEY, ancestor));
            }
        }
        // Path 按组件排序: 若存在以 target 为祖先的源, 下界的第一项即在该子树内.
        if let Some((&source, &other)) = sources.range(entry.target_path.as_path()..).next()
            && source.starts_with(&entry.target_path)
        {
            return Err(path_overlap(index, entry, other, SOURCE_KEY, source));
        }
    }
    Ok(())
}

fn path_overlap(index: usize, entry: &Entry, other: usize, kind: &str, path: &Path) -> Error {
    invalid_entry(
        index,
        format!(
            "目标 `{}` ({}) 与条目 `{}` 的 {kind} `{}` 存在路径重叠: 目标不得相互嵌套, 也不得与任一源路径重合或互相包含",
            entry.target,
            entry.target_path.display(),
            entry_label(other),
            path.display()
        ),
    )
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
        // 当前 toml 解析器的错误都带 span: 该回退只防御依赖版本行为变化, 不是可
        // 构造的输入路径, 因此没有对应测试 (属 design.md 验证层级里的代码审阅证据).
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

/// 条目级校验失败的统一说明: V-4 到 V-10 的每一条都必须能指出条目下标.
///
/// 下标只以 `entry[{index}]` 的形态出现一次.
fn invalid_entry(index: usize, detail: impl std::fmt::Display) -> Error {
    Error::ManifestInvalid(format!("条目 `{}`: {detail}", entry_label(index)))
}

/// 条目下标的写法: `entry[3]`.
fn entry_label(index: usize) -> String {
    format!("{ENTRY_KEY}[{index}]")
}

/// `physical_home` 用来"占位"的探针子项: 它不需要存在, 只是为了让 `HOME` 成为
/// `normalize` 眼中的祖先段 (末段才不解析).
const HOME_PHYSICAL_PROBE: &str = ".dotlink-home-probe";

/// `HOME` 的物理形态: 末段也解析为符号链接链的终点.
///
/// [`os::normalize`] 刻意不解析末段 (末段往往是待判定的部署链接). 主目录不同: V-9
/// 拒绝的是"主目录本身", 而 `HOME` 本身可能就是符号链接 (例如 `/home/u -> /mnt/u`),
/// 用户也可能把该主目录写成解析后的物理路径.
///
/// 给 `HOME` 补一个不会存在的探针子项, 使 `HOME` 成为祖先段而被解析; 探针自身只
/// 保持词法形态, 取父目录即主目录的物理形态. 不能用 `home.join(".")` 达到同一效果:
/// `Path::components` 会把中间的 `.` 段消掉, 探针必须是一个真实的普通段.
/// `HOME` 不存在时整条路径退化为词法形态, 与书写形态相同, 不引入新的拒绝面.
fn physical_home(home: &Path) -> std::path::PathBuf {
    let mut resolved = os::normalize(&home.join(HOME_PHYSICAL_PROBE));
    resolved.pop();
    resolved
}

/// V-9: 目标落点不得退化 -- 归一化后命中以下位置之一即整体拒绝.
///
/// 三个落点的共同点是命中它们的条目一旦落地 (`replace_and_backup` 尤甚) 就会毁掉
/// 远超单个链接范围的内容: 文件系统根或主目录本身被替换备份, 毁掉的是一整棵树;
/// 仓库根本身或其祖先被替换备份, 毁掉的是源仓库. `.` / `..` / 裸 `~` 都是极易写出
/// 的笔误形态, 与 V-6 的立场一致, 静默放行比报错危险得多.
///
/// 主目录有两个比较基准 (`HOME` 的书写形态与解析符号链接后的物理形态), 二者都要
/// 拒绝: `HOME` 自身可能是符号链接, 只比较一种形态会让另一种写法的主目录落点漏过.
fn reject_degenerate_target(
    index: usize,
    entry: &Entry,
    repo: &Path,
    home: Option<&Path>,
    home_physical: Option<&Path>,
) -> Result<()> {
    let written = &entry.target;
    let landing = &entry.target_path;

    // 落点已过 `os::normalize`, 文件系统根的形态恰是 `/` (任何上溯都被 `normalize`
    // 停在根).
    if landing.as_path() == Path::new("/") {
        return Err(invalid_entry(
            index,
            format!("`{written}` 归一化后是文件系统根: 不能以整个文件系统根为落点"),
        ));
    }
    // 主目录: 裸 `~`, 书成绝对路径的等价形态, 以及 `HOME` 是符号链接时同一主目录的
    // 物理形态. HOME 无法确定时两个基准都为 None, 不作比较 -- 需要 HOME 的 `~` 形态
    // 已由 V-6 报错, 绝对路径不得因缺 HOME 而被拒.
    if [home, home_physical]
        .into_iter()
        .flatten()
        .any(|basis| landing.as_path() == basis)
    {
        return Err(invalid_entry(
            index,
            format!("`{written}` 归一化后是当前用户主目录: 不能以主目录本身为落点"),
        ));
    }
    // 仓库根本身或其祖先 (`.` / `..` 之类): 落点覆盖源仓库.
    if repo.starts_with(landing) {
        return Err(invalid_entry(
            index,
            format!("`{written}` 归一化后是仓库根本身或其祖先: 不能以源仓库为落点"),
        ));
    }
    Ok(())
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
        assert_invalid_at(text, &repo(), needles);
    }

    /// 同上, 但可指定仓库根 (V-9 的"祖先"形态需要比 `/repo` 更深的仓库根才能
    /// 与文件系统根区分开).
    #[track_caller]
    fn assert_invalid_at(text: &str, repo_root: &Path, needles: &[&str]) {
        let error = parse(text, repo_root)
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
        // 非字符串: 说明必须带上 TOML 类型文本, 指出实际写成了什么类型
        assert_invalid(
            "[[entry]]\nsource = \"a\"\ntarget = \"b\"\non_conflict = 1\n",
            &["entry[0]", "on_conflict", "实际为 integer"],
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
    // V-9: 目标落点不得退化 (文件系统根 / 当前用户主目录 / 仓库根本身或其祖先)
    // -----------------------------------------------------------------------

    #[test]
    fn v9_the_file_system_root_target_is_rejected() {
        assert_invalid(
            "[[entry]]\nsource = \"a\"\ntarget = \"/\"\n",
            &["entry[0]", "文件系统根", "`/`"],
        );
    }

    #[test]
    fn v9_the_home_target_is_rejected() {
        // 裸 `~` 与书成绝对路径的等价形态都归一化到主目录本身.
        assert_invalid(
            "[[entry]]\nsource = \"a\"\ntarget = \"~\"\n",
            &["entry[0]", "主目录", "`~`"],
        );
        let Some(home) = std::env::var_os("HOME").filter(|value| !value.is_empty()) else {
            return;
        };
        let written = PathBuf::from(home);
        assert_invalid(
            &format!(
                "[[entry]]\nsource = \"a\"\ntarget = \"{}\"\n",
                written.display()
            ),
            &["entry[0]", "主目录"],
        );
    }

    #[test]
    fn v9_the_repo_root_and_its_ancestors_are_rejected() {
        // 仓库根取深一层的路径: 对 `/repo` 而言唯一的祖先是文件系统根, 会先命中
        // V-9 的第一条, 测不到"祖先"这一形态.
        let repo_root = Path::new("/a/b/repo");
        for written in [".", "..", "sub/../.."] {
            assert_invalid_at(
                &format!("[[entry]]\nsource = \"a\"\ntarget = \"{written}\"\n"),
                repo_root,
                &["entry[0]", "仓库根本身或其祖先", &format!("`{written}`")],
            );
        }
    }

    #[test]
    fn v9_legal_target_shapes_are_not_harmed() {
        // `~/sub` 不是主目录本身; `/etc/hosts` 这类绝对路径与相对仓库根的
        // `link/bashrc` 都各有合法形态.
        let text = "[[entry]]\nsource = \"a\"\ntarget = \"~/sub\"\n\n[[entry]]\nsource = \"b\"\ntarget = \"/etc/hosts\"\n\n[[entry]]\nsource = \"c\"\ntarget = \"link/bashrc\"\n";
        let manifest = parse(text, &repo()).expect("合法落点不得被误伤");
        assert_eq!(manifest.entries.len(), 3);
        assert_eq!(
            manifest.entries[1].target_path,
            crate::os::normalize(Path::new("/etc/hosts"))
        );
        assert_eq!(
            manifest.entries[2].target_path,
            crate::os::normalize(&repo().join("link/bashrc"))
        );
    }

    // -----------------------------------------------------------------------
    // V-10: 目标不得指向本条目自己的 source (自指链接恒为断链)
    // -----------------------------------------------------------------------

    #[test]
    fn v10_a_self_referential_target_is_rejected() {
        assert_invalid(
            "[[entry]]\nsource = \"shell/bashrc\"\ntarget = \"shell/bashrc\"\n",
            &["entry[0]", "自指", "shell/bashrc"],
        );
        // 书写形态不同, 归一化后仍是同一条路径.
        assert_invalid(
            "[[entry]]\nsource = \"shell/bashrc\"\ntarget = \"shell/../shell/bashrc\"\n",
            &["entry[0]", "自指"],
        );
    }

    #[test]
    fn v11_a_target_cannot_replace_another_entries_source() {
        let text = "[[entry]]\nsource = \"a\"\ntarget = \"b\"\n\n[[entry]]\nsource = \"b\"\ntarget = \"c\"\n";
        assert_invalid(text, &["entry[0]", "entry[1]", "source", "路径重叠"]);
    }

    #[test]
    fn v11_compares_path_components_and_allows_shared_sources() {
        let text = "[[entry]]\nsource = \"config\"\ntarget = \"config-extra\"\n\n[[entry]]\nsource = \"config\"\ntarget = \"config.link/rc\"\n\n[[entry]]\nsource = \"z\"\ntarget = \"config-extra-extra\"\n";
        assert_eq!(parse(text, &repo()).unwrap().entries.len(), 3);
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
    fn a_missing_source_entry_is_not_a_manifest_defect() {
        // 源条目在仓库中不存在属运行时状态 SourceMissing, 不属校验范围
        let text = "[[entry]]\nsource = \"definitely/gone\"\ntarget = \"link/gone\"\n";
        let manifest = parse(text, &repo()).expect("不校验源条目是否存在");
        assert_eq!(manifest.entries.len(), 1);
    }

    // -----------------------------------------------------------------------
    // V-9 的补充: HOME 自身是符号链接时, 物理形态同样要识别 (端到端用例见
    // tests/manifest_errors.rs 的 v9_a_symlinked_home_is_rejected_through_its_physical_form)
    // -----------------------------------------------------------------------

    /// `physical_home` 必须解析 `HOME` 的**末段**符号链接: `os::normalize` 对末段
    /// 刻意保留原样, 而 V-9 需要主目录的物理形态作为第二个比较基准.
    #[test]
    fn physical_home_resolves_a_symlinked_final_segment() {
        let dir = tempfile::tempdir().expect("创建临时目录");
        let root = std::fs::canonicalize(dir.path()).expect("临时目录的物理路径");
        let physical = root.join("real-home");
        std::fs::create_dir(&physical).expect("创建主目录");
        let alias = root.join("home-alias");
        std::os::unix::fs::symlink(&physical, &alias).expect("建立指向主目录的别名链接");

        // 写入形态的末段不被解析, 因此与物理形态不同 -- 这正是 V-9 原先的漏点.
        let written = os::normalize(&alias);
        assert_ne!(written, os::normalize(&physical));
        // 补一个 `.` 段后 HOME 成为祖先段, 其符号链接被解析.
        assert_eq!(physical_home(&written), os::normalize(&physical));
    }
}

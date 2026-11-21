#!/usr/bin/env python3
"""`check-punctuation.py` 的回归测试.

用 `--root` 指向临时目录, 覆盖: 各白名单位置 (`.github/`, `openspec/specs`,
`openspec/changes`, `src`, `tests`, `scripts`, `AGENTS.md`, `README.md`) 会被检查,
白名单之外的位置不会被检查, 符号链接被跳过, 非法 UTF-8 退 1, 参数错误退 2.
仅使用标准库.
"""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent / "check-punctuation.py"
FULLWIDTH_COMMA = "\uff0c"


def run_check_tree(
    files: dict[str, str | bytes] | None = None,
    symlinks: dict[str, str] | None = None,
) -> subprocess.CompletedProcess[str]:
    """在临时根目录中布置文件与符号链接, 再运行检查脚本.

    `files` 的值是 `str` 时按 UTF-8 写入并补一个换行, 是 `bytes` 时原样写入.
    `symlinks` 的值是相对链接目标 (相对链接所在目录).
    """
    with tempfile.TemporaryDirectory() as raw_root:
        root = Path(raw_root)
        for relative, content in (files or {}).items():
            path = root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            if isinstance(content, bytes):
                path.write_bytes(content)
            else:
                path.write_text(f"{content}\n", encoding="utf-8")
        for relative, target in (symlinks or {}).items():
            path = root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            os.symlink(target, path)
        return subprocess.run(
            [sys.executable, str(SCRIPT), "--root", str(root)],
            capture_output=True,
            text=True,
            check=False,
        )


def run_check(files: dict[str, str]) -> subprocess.CompletedProcess[str]:
    """只含给定文本文件的临时根目录里运行检查脚本."""
    return run_check_tree(files)


class ScanScopeTest(unittest.TestCase):
    """白名单的边界: 应检查的位置与应忽略的位置."""

    def test_github_yml_and_md_are_scanned(self) -> None:
        result = run_check(
            {
                ".github/workflows/probe.yml": f"# 说明{FULLWIDTH_COMMA}注释",
                ".github/PULL_REQUEST_TEMPLATE.md": f"## 说明{FULLWIDTH_COMMA}注释",
            }
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn(".github/workflows/probe.yml:1:", result.stdout)
        self.assertIn(".github/PULL_REQUEST_TEMPLATE.md:1:", result.stdout)
        self.assertIn("2 处禁止标点", result.stderr)

    def test_github_yaml_is_scanned(self) -> None:
        result = run_check(
            {".github/workflows/probe.yaml": f"# 说明{FULLWIDTH_COMMA}注释"}
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn(".github/workflows/probe.yaml:1:", result.stdout)

    def test_openspec_config_files_are_scanned(self) -> None:
        for name in ("config.yaml", "config.yml"):
            with self.subTest(name=name):
                result = run_check(
                    {f"openspec/{name}": f"context: 说明{FULLWIDTH_COMMA}注释"}
                )
                self.assertEqual(result.returncode, 1)
                self.assertIn(f"openspec/{name}:1:", result.stdout)

    def test_whitelisted_trees_and_documents_are_scanned(self) -> None:
        cases = {
            "AGENTS.md": f"# 说明{FULLWIDTH_COMMA}注释",
            "README.md": f"## 说明{FULLWIDTH_COMMA}注释",
            "src/lib.rs": f'// 说明{FULLWIDTH_COMMA}注释\nfn probe() {{}}',
            "tests/probe.rs": f"// 说明{FULLWIDTH_COMMA}注释\nfn probe() {{}}",
            "scripts/probe.py": f"# 说明{FULLWIDTH_COMMA}注释",
            "openspec/specs/capability/spec.md": f"## 说明{FULLWIDTH_COMMA}注释",
            "openspec/changes/change/proposal.md": f"## 说明{FULLWIDTH_COMMA}注释",
        }
        for relative, line in cases.items():
            with self.subTest(relative=relative):
                result = run_check({relative: line})
                self.assertEqual(result.returncode, 1, result.stderr)
                self.assertIn(f"{relative}:1:", result.stdout)

    def test_symlinks_are_skipped(self) -> None:
        # 链接目标放在白名单之外: 若检查跟随符号链接, 就会报出命中.
        result = run_check_tree(
            files={"outside.txt": f"说明{FULLWIDTH_COMMA}注释"},
            symlinks={
                "tests/probe.rs": "../outside.txt",
                "AGENTS.md": "outside.txt",
            },
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "")
        self.assertEqual(result.stderr, "")

    def test_files_outside_the_whitelist_are_ignored(self) -> None:
        result = run_check(
            {
                ".github/workflows/probe.txt": f"# 说明{FULLWIDTH_COMMA}注释",
                ".github/notes.rst": f"# 说明{FULLWIDTH_COMMA}注释",
                "openspec/CONTRIBUTING.md": f"## 说明{FULLWIDTH_COMMA}注释",
                "openspec/config.json": f'{{"context": "说明{FULLWIDTH_COMMA}注释"}}',
            }
        )
        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "")
        self.assertEqual(result.stderr, "")

    def test_whitelisted_files_without_hits_exit_zero(self) -> None:
        result = run_check(
            {
                ".github/workflows/probe.yml": "# 说明, 全为 ASCII 标点",
                "openspec/config.yaml": "context: |\n  - 说明, 全为 ASCII 标点",
            }
        )
        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "")
        self.assertEqual(result.stderr, "")


class ArgumentHandlingTest(unittest.TestCase):
    """退出码契约: 参数错误退 2."""

    def test_missing_root_exits_two(self) -> None:
        with tempfile.TemporaryDirectory() as raw_root:
            missing = Path(raw_root) / "missing"
            result = subprocess.run(
                [sys.executable, str(SCRIPT), "--root", str(missing)],
                capture_output=True,
                text=True,
                check=False,
            )
        self.assertEqual(result.returncode, 2)
        self.assertIn("不是目录", result.stderr)


class DecodingTest(unittest.TestCase):
    """非法 UTF-8 是失败而不是跳过: 退 1 并在标准错误说明."""

    def test_invalid_utf8_exits_one(self) -> None:
        result = run_check_tree(files={"src/bad.rs": b"\xff\xfe\n"})
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stdout, "")
        self.assertIn("不是合法的 UTF-8", result.stderr)
        self.assertIn("src/bad.rs", result.stderr)


if __name__ == "__main__":
    unittest.main()

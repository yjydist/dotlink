#!/usr/bin/env python3
"""`check-punctuation.py` 的回归测试.

用 `--root` 指向临时目录, 覆盖三点: `.github/` 下的 `.yml` / `.yaml` / `.md` 与
`openspec/config.yaml` / `openspec/config.yml` 会被检查, 白名单之外的位置不会被检查,
参数错误退 2. 仅使用标准库.
"""

from __future__ import annotations

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent / "check-punctuation.py"
FULLWIDTH_COMMA = "\uff0c"


def run_check(files: dict[str, str]) -> subprocess.CompletedProcess[str]:
    """在只含给定文件的临时根目录里运行检查脚本."""
    with tempfile.TemporaryDirectory() as raw_root:
        root = Path(raw_root)
        for relative, line in files.items():
            path = root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(f"{line}\n", encoding="utf-8")
        return subprocess.run(
            [sys.executable, str(SCRIPT), "--root", str(root)],
            capture_output=True,
            text=True,
            check=False,
        )


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


if __name__ == "__main__":
    unittest.main()

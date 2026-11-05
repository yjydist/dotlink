#!/usr/bin/env python3
"""检查纳入检查范围的文本中的非 ASCII 标点.

扫描范围与禁止码点见 AGENTS.md 与 openspec 变更的 design D6:

- AGENTS.md 与 README.md;
- openspec/specs/**/*.md 与 openspec/changes/**/*.md;
- src/**/*.rs 与 tests/**/*.rs;
- scripts/**/*.sh 与 scripts/**/*.py.

仅使用标准库. 有命中或文件无法按 UTF-8 解码时退 1, 无命中退 0, 参数错误退 2.
文件路径相对 `--root` 输出; 符号链接不跟随.
"""

from __future__ import annotations

import argparse
import os
import sys
from pathlib import Path

# CJK 标点, 全角形式与弯引号等, 加上单独的破折号, 省略号与间隔号.
FORBIDDEN_RANGES = (
    (0x3000, 0x303F),
    (0xFF00, 0xFFEF),
    (0x2018, 0x201F),
)
FORBIDDEN_CODEPOINTS = frozenset((0x2014, 0x2026, 0x00B7))

TEXT_FILES = ("AGENTS.md", "README.md")
TEXT_TREES = (
    ("openspec/specs", (".md",)),
    ("openspec/changes", (".md",)),
    ("src", (".rs",)),
    ("tests", (".rs",)),
    ("scripts", (".sh", ".py")),
)
SKIP_DIRS = frozenset((".agents", ".git", "target"))


def is_forbidden(codepoint: int) -> bool:
    """该码点是否属于禁止的标点集合."""
    if codepoint in FORBIDDEN_CODEPOINTS:
        return True
    return any(low <= codepoint <= high for low, high in FORBIDDEN_RANGES)


def iter_files(root: Path):
    """按固定顺序产出检查范围内的普通文件, 跳过符号链接与构建目录."""
    for relative in TEXT_FILES:
        path = root / relative
        if path.is_file() and not path.is_symlink():
            yield path

    for relative_dir, suffixes in TEXT_TREES:
        base = root / relative_dir
        if not base.is_dir() or base.is_symlink():
            continue
        for dirpath, dirnames, filenames in os.walk(base, followlinks=False):
            directory = Path(dirpath)
            dirnames[:] = sorted(
                name
                for name in dirnames
                if name not in SKIP_DIRS and not (directory / name).is_symlink()
            )
            for name in sorted(filenames):
                path = directory / name
                if path.suffix in suffixes and not path.is_symlink():
                    yield path


def scan(path: Path, root: Path) -> tuple[int, bool]:
    """扫描单个文件, 返回 (命中数, 是否解码失败)."""
    relative = path.relative_to(root).as_posix()
    try:
        text = path.read_text(encoding="utf-8")
    except UnicodeDecodeError as error:
        print(f"{relative}: 不是合法的 UTF-8: {error}", file=sys.stderr)
        return 0, True
    except OSError as error:
        print(f"{relative}: 无法读取: {error}", file=sys.stderr)
        return 0, True

    hits = 0
    for line_number, line in enumerate(text.splitlines(), 1):
        for column, character in enumerate(line, 1):
            codepoint = ord(character)
            if is_forbidden(codepoint):
                print(
                    f"{relative}:{line_number}:{column}: "
                    f"U+{codepoint:04X}: {line.rstrip()}"
                )
                hits += 1
    return hits, False


def parse_args(argv: list[str] | None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="检查 AGENTS.md, README, OpenSpec 制品, 源码与脚本中的非 ASCII 标点."
    )
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(__file__).resolve().parent.parent,
        help="仓库根目录 (默认: 本脚本所在目录的上一级).",
    )
    args = parser.parse_args(argv)
    root = args.root.resolve()
    if not root.is_dir():
        parser.error(f"--root 不是目录: {root}")
    args.root = root
    return args


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    hits = 0
    failures = 0
    for path in iter_files(args.root):
        file_hits, failed = scan(path, args.root)
        hits += file_hits
        failures += int(failed)

    if hits:
        print(f"{hits} 处禁止标点", file=sys.stderr)
    if failures:
        print(f"{failures} 个文件无法读取或解码", file=sys.stderr)
    return 0 if hits == 0 and failures == 0 else 1


if __name__ == "__main__":
    sys.exit(main())

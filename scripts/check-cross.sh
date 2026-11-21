#!/usr/bin/env bash
# 可选本地门禁: 在已安装的交叉目标上运行 cargo check, 并在 cargo-audit 可用时
# 运行依赖审计. 目标或工具缺失时跳过并说明原因; 不接入 scripts/check.sh, 也不
# 引入 CI.
set -euo pipefail

project_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd -- "$project_root"

run_target_check() {
  local target=$1
  if ! command -v rustup >/dev/null 2>&1; then
    echo "跳过 $target: 未安装 rustup, 无法枚举已安装的目标"
    return 0
  fi
  if ! rustup target list --installed | grep -qx "$target"; then
    echo "跳过 $target: 目标未安装 (可用 rustup target add $target 安装)"
    return 0
  fi
  echo "运行 cargo check --locked --target $target"
  cargo check --locked --target "$target"
}

run_target_check x86_64-unknown-linux-gnu
run_target_check x86_64-pc-windows-gnu

if command -v cargo-audit >/dev/null 2>&1; then
  echo "运行 cargo audit"
  cargo audit
else
  echo "跳过 cargo audit: 未安装 cargo-audit (可用 cargo install cargo-audit 安装)"
fi

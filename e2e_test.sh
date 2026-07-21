#!/usr/bin/env bash
set -euo pipefail

DOTLINK="${DOTLINK_BIN:-./bin/dotlink}"
if [[ "$DOTLINK" != /* ]]; then
  DOTLINK="$(pwd)/$DOTLINK"
fi
SANDBOX=""
PASS=0
FAIL=0

setup_sandbox() {
  SANDBOX=$(mktemp -d /tmp/dotlink-test-XXXXXX)
  mkdir -p "$SANDBOX/repo/zsh" "$SANDBOX/repo/nvim" "$SANDBOX/repo/git" "$SANDBOX/home/.config"

  echo "# zshrc" > "$SANDBOX/repo/zsh/.zshrc"
  echo "-- init" > "$SANDBOX/repo/nvim/init.lua"
  echo "[user]" > "$SANDBOX/repo/git/.gitconfig"

  cat > "$SANDBOX/repo/dotlink.toml" <<EOF
[[link]]
source = "zsh/.zshrc"
target = "$SANDBOX/home/.zshrc"

[[link]]
source = "nvim"
target = "$SANDBOX/home/.config/nvim"

[[link]]
source = "git/.gitconfig"
target = "$SANDBOX/home/.gitconfig"
EOF
}

cleanup() {
  if [[ "${DEBUG:-0}" == "1" && -n "$SANDBOX" ]]; then
    echo "DEBUG: sandbox preserved at $SANDBOX"
  elif [[ -n "$SANDBOX" ]]; then
    rm -rf "$SANDBOX"
  fi
}
trap cleanup EXIT

run_dotlink() {
  (cd "$SANDBOX/repo" && "$DOTLINK" "$@")
}

assert_exit_code() {
  local expected=$1
  shift
  local actual=0
  (cd "$SANDBOX/repo" && "$DOTLINK" "$@") >/dev/null 2>&1 || actual=$?
  if [[ "$actual" -eq "$expected" ]]; then
    PASS=$((PASS + 1))
  else
    echo "FAIL: expected exit code $expected, got $actual (cmd: dotlink $*)"
    FAIL=$((FAIL + 1))
  fi
}

assert_symlink() {
  local target=$1
  local source=$2
  if [[ -L "$target" ]]; then
    local dest
    dest=$(readlink "$target")
    if [[ "$dest" == "$source" ]]; then
      PASS=$((PASS + 1))
    else
      echo "FAIL: $target points to $dest, expected $source"
      FAIL=$((FAIL + 1))
    fi
  else
    echo "FAIL: $target is not a symlink"
    FAIL=$((FAIL + 1))
  fi
}

assert_not_exists() {
  local path=$1
  if [[ ! -e "$path" && ! -L "$path" ]]; then
    PASS=$((PASS + 1))
  else
    echo "FAIL: $path should not exist"
    FAIL=$((FAIL + 1))
  fi
}

assert_status_contains() {
  local expected=$1
  local output
  output=$(run_dotlink status)
  if echo "$output" | grep -q "$expected"; then
    PASS=$((PASS + 1))
  else
    echo "FAIL: status output missing '$expected'"
    echo "  got: $output"
    FAIL=$((FAIL + 1))
  fi
}

assert_dry_run_no_change() {
  local target=$1
  run_dotlink apply --dry-run >/dev/null 2>&1
  if [[ ! -e "$target" && ! -L "$target" ]]; then
    PASS=$((PASS + 1))
  else
    echo "FAIL: dry-run should not create $target"
    FAIL=$((FAIL + 1))
  fi
}

# --- Build if needed ---
if [[ ! -x "$DOTLINK" ]]; then
  echo "Building dotlink..."
  go build -o bin/dotlink ./cmd/dotlink
fi

# --- Tests ---
setup_sandbox

echo "=== Test: initial status is missing ==="
assert_status_contains "missing"

echo "=== Test: dry-run does not modify filesystem ==="
assert_dry_run_no_change "$SANDBOX/home/.zshrc"

echo "=== Test: apply creates symlinks ==="
run_dotlink apply >/dev/null
assert_symlink "$SANDBOX/home/.zshrc" "$SANDBOX/repo/zsh/.zshrc"
assert_symlink "$SANDBOX/home/.config/nvim" "$SANDBOX/repo/nvim"
assert_symlink "$SANDBOX/home/.gitconfig" "$SANDBOX/repo/git/.gitconfig"

echo "=== Test: status shows linked-correct ==="
assert_status_contains "linked-correct"

echo "=== Test: apply again shows already linked ==="
output=$(run_dotlink apply)
if echo "$output" | grep -q "already linked"; then
  PASS=$((PASS + 1))
else
  echo "FAIL: expected 'already linked' in output"
  FAIL=$((FAIL + 1))
fi

echo "=== Test: remove deletes symlinks ==="
run_dotlink remove >/dev/null
assert_not_exists "$SANDBOX/home/.zshrc"
assert_not_exists "$SANDBOX/home/.config/nvim"
assert_not_exists "$SANDBOX/home/.gitconfig"

echo "=== Test: source still exists after remove ==="
if [[ -f "$SANDBOX/repo/zsh/.zshrc" ]]; then
  PASS=$((PASS + 1))
else
  echo "FAIL: source file was deleted"
  FAIL=$((FAIL + 1))
fi

echo "=== Test: conflict exit code 3 ==="
echo "existing" > "$SANDBOX/home/.zshrc"
assert_exit_code 3 apply

echo "=== Test: force overwrites conflict ==="
run_dotlink apply --force >/dev/null
assert_symlink "$SANDBOX/home/.zshrc" "$SANDBOX/repo/zsh/.zshrc"

echo "=== Test: source missing exit code 4 ==="
cat > "$SANDBOX/repo/dotlink.toml" <<EOF
[[link]]
source = "nonexistent"
target = "$SANDBOX/home/.missing"
EOF
assert_exit_code 4 apply

echo "=== Test: config parse failure exit code 2 ==="
echo "[[broken" > "$SANDBOX/repo/dotlink.toml"
assert_exit_code 2 apply

# --- Summary ---
echo ""
echo "Results: $PASS passed, $FAIL failed"
if [[ "$FAIL" -gt 0 ]]; then
  exit 1
fi

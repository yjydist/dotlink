package command

import (
	"errors"
	"os"
	"path/filepath"
	"testing"

	"github.com/yjydist/dotlink/internal/config"
)

func setup(t *testing.T) (baseDir string, cfg *config.Config) {
	t.Helper()
	baseDir = t.TempDir()
	cfg = &config.Config{BaseDir: baseDir}
	return baseDir, cfg
}

func TestApplyCreateSymlink(t *testing.T) {
	baseDir, cfg := setup(t)
	target := filepath.Join(t.TempDir(), "target", ".zshrc")

	src := filepath.Join(baseDir, "zsh/.zshrc")
	if err := os.MkdirAll(filepath.Dir(src), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(src, []byte("content"), 0o644); err != nil {
		t.Fatal(err)
	}

	cfg.Link = []config.Link{{Source: "zsh/.zshrc", Target: target}}

	results, err := Apply(cfg, false, false)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}
	if results[0].Action != "created symlink" {
		t.Errorf("action = %q, want %q", results[0].Action, "created symlink")
	}

	dest, err := os.Readlink(target)
	if err != nil {
		t.Fatalf("target is not a symlink: %v", err)
	}
	if dest != src {
		t.Errorf("symlink points to %q, want %q", dest, src)
	}
}

func TestApplyAlreadyLinked(t *testing.T) {
	baseDir, cfg := setup(t)
	targetDir := t.TempDir()
	target := filepath.Join(targetDir, ".zshrc")

	src := filepath.Join(baseDir, ".zshrc")
	if err := os.WriteFile(src, []byte("content"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(src, target); err != nil {
		t.Fatal(err)
	}

	cfg.Link = []config.Link{{Source: ".zshrc", Target: target}}

	results, err := Apply(cfg, false, false)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if results[0].Action != "already linked" {
		t.Errorf("action = %q, want %q", results[0].Action, "already linked")
	}
}

func TestApplyConflictNotSymlink(t *testing.T) {
	baseDir, cfg := setup(t)
	targetDir := t.TempDir()
	target := filepath.Join(targetDir, ".zshrc")

	src := filepath.Join(baseDir, ".zshrc")
	if err := os.WriteFile(src, []byte("content"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(target, []byte("existing"), 0o644); err != nil {
		t.Fatal(err)
	}

	cfg.Link = []config.Link{{Source: ".zshrc", Target: target}}

	_, err := Apply(cfg, false, false)
	if !errors.Is(err, ErrTargetConflict) {
		t.Fatalf("expected ErrTargetConflict, got: %v", err)
	}
}

func TestApplyConflictSymlinkElsewhere(t *testing.T) {
	baseDir, cfg := setup(t)
	targetDir := t.TempDir()
	target := filepath.Join(targetDir, ".zshrc")

	src := filepath.Join(baseDir, ".zshrc")
	if err := os.WriteFile(src, []byte("content"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink("/some/other/path", target); err != nil {
		t.Fatal(err)
	}

	cfg.Link = []config.Link{{Source: ".zshrc", Target: target}}

	_, err := Apply(cfg, false, false)
	if !errors.Is(err, ErrTargetConflict) {
		t.Fatalf("expected ErrTargetConflict, got: %v", err)
	}
}

func TestApplyForceOverwrite(t *testing.T) {
	baseDir, cfg := setup(t)
	targetDir := t.TempDir()
	target := filepath.Join(targetDir, ".zshrc")

	src := filepath.Join(baseDir, ".zshrc")
	if err := os.WriteFile(src, []byte("content"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(target, []byte("existing"), 0o644); err != nil {
		t.Fatal(err)
	}

	cfg.Link = []config.Link{{Source: ".zshrc", Target: target}}

	results, err := Apply(cfg, true, false)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if results[0].Action != "recreated symlink" {
		t.Errorf("action = %q, want %q", results[0].Action, "recreated symlink")
	}

	dest, err := os.Readlink(target)
	if err != nil {
		t.Fatalf("target is not a symlink after force: %v", err)
	}
	if dest != src {
		t.Errorf("symlink points to %q, want %q", dest, src)
	}
}

func TestApplyDryRun(t *testing.T) {
	baseDir, cfg := setup(t)
	target := filepath.Join(t.TempDir(), ".zshrc")

	src := filepath.Join(baseDir, ".zshrc")
	if err := os.WriteFile(src, []byte("content"), 0o644); err != nil {
		t.Fatal(err)
	}

	cfg.Link = []config.Link{{Source: ".zshrc", Target: target}}

	results, err := Apply(cfg, false, true)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if results[0].Action != "would create symlink" {
		t.Errorf("action = %q, want %q", results[0].Action, "would create symlink")
	}

	if _, err := os.Lstat(target); !errors.Is(err, os.ErrNotExist) {
		t.Error("dry-run should not create the symlink")
	}
}

func TestApplySourceMissing(t *testing.T) {
	_, cfg := setup(t)
	target := filepath.Join(t.TempDir(), ".zshrc")

	cfg.Link = []config.Link{{Source: "nonexistent", Target: target}}

	_, err := Apply(cfg, false, false)
	if !errors.Is(err, ErrSourceMissing) {
		t.Fatalf("expected ErrSourceMissing, got: %v", err)
	}
}

func TestApplyDirectorySymlink(t *testing.T) {
	baseDir, cfg := setup(t)
	target := filepath.Join(t.TempDir(), "nvim")

	src := filepath.Join(baseDir, "nvim")
	if err := os.MkdirAll(filepath.Join(src, "plugin"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(src, "init.lua"), []byte("-- init"), 0o644); err != nil {
		t.Fatal(err)
	}

	cfg.Link = []config.Link{{Source: "nvim", Target: target}}

	results, err := Apply(cfg, false, false)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if results[0].Action != "created symlink" {
		t.Errorf("action = %q, want %q", results[0].Action, "created symlink")
	}

	info, err := os.Lstat(target)
	if err != nil {
		t.Fatal(err)
	}
	if info.Mode()&os.ModeSymlink == 0 {
		t.Error("target should be a symlink to directory")
	}
}

package command

import (
	"errors"
	"os"
	"path/filepath"
	"testing"

	"github.com/yjydist/dotlink/internal/config"
)

func TestRemoveSymlink(t *testing.T) {
	baseDir := t.TempDir()
	targetDir := t.TempDir()
	target := filepath.Join(targetDir, ".zshrc")

	src := filepath.Join(baseDir, ".zshrc")
	if err := os.WriteFile(src, []byte("content"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(src, target); err != nil {
		t.Fatal(err)
	}

	cfg := &config.Config{
		BaseDir: baseDir,
		Link:    []config.Link{{Source: ".zshrc", Target: target}},
	}

	results, err := Remove(cfg, false)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if results[0].Action != "removed" {
		t.Errorf("action = %q, want %q", results[0].Action, "removed")
	}

	if _, err := os.Lstat(target); !errors.Is(err, os.ErrNotExist) {
		t.Error("symlink should have been removed")
	}

	if _, err := os.Stat(src); err != nil {
		t.Error("source file should not be deleted")
	}
}

func TestRemoveNotSymlink(t *testing.T) {
	baseDir := t.TempDir()
	targetDir := t.TempDir()
	target := filepath.Join(targetDir, ".zshrc")

	src := filepath.Join(baseDir, ".zshrc")
	if err := os.WriteFile(src, []byte("content"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(target, []byte("regular file"), 0o644); err != nil {
		t.Fatal(err)
	}

	cfg := &config.Config{
		BaseDir: baseDir,
		Link:    []config.Link{{Source: ".zshrc", Target: target}},
	}

	results, err := Remove(cfg, false)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if results[0].Action != "not a symlink, skipped" {
		t.Errorf("action = %q, want %q", results[0].Action, "not a symlink, skipped")
	}

	if _, err := os.Stat(target); err != nil {
		t.Error("regular file should not be deleted")
	}
}

func TestRemoveSymlinkElsewhere(t *testing.T) {
	baseDir := t.TempDir()
	targetDir := t.TempDir()
	target := filepath.Join(targetDir, ".zshrc")

	src := filepath.Join(baseDir, ".zshrc")
	if err := os.WriteFile(src, []byte("content"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink("/some/other/path", target); err != nil {
		t.Fatal(err)
	}

	cfg := &config.Config{
		BaseDir: baseDir,
		Link:    []config.Link{{Source: ".zshrc", Target: target}},
	}

	results, err := Remove(cfg, false)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if results[0].Action != "symlink to elsewhere, skipped" {
		t.Errorf("action = %q, want %q", results[0].Action, "symlink to elsewhere, skipped")
	}

	if _, err := os.Lstat(target); err != nil {
		t.Error("symlink to elsewhere should not be deleted")
	}
}

func TestRemoveNotFound(t *testing.T) {
	baseDir := t.TempDir()
	target := filepath.Join(t.TempDir(), ".zshrc")

	cfg := &config.Config{
		BaseDir: baseDir,
		Link:    []config.Link{{Source: ".zshrc", Target: target}},
	}

	results, err := Remove(cfg, false)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if results[0].Action != "not found, skipped" {
		t.Errorf("action = %q, want %q", results[0].Action, "not found, skipped")
	}
}

func TestRemoveDryRun(t *testing.T) {
	baseDir := t.TempDir()
	targetDir := t.TempDir()
	target := filepath.Join(targetDir, ".zshrc")

	src := filepath.Join(baseDir, ".zshrc")
	if err := os.WriteFile(src, []byte("content"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(src, target); err != nil {
		t.Fatal(err)
	}

	cfg := &config.Config{
		BaseDir: baseDir,
		Link:    []config.Link{{Source: ".zshrc", Target: target}},
	}

	results, err := Remove(cfg, true)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if results[0].Action != "would remove" {
		t.Errorf("action = %q, want %q", results[0].Action, "would remove")
	}

	if _, err := os.Lstat(target); err != nil {
		t.Error("dry-run should not remove the symlink")
	}
}

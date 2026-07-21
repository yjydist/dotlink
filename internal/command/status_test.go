package command

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/yjydist/dotlink/internal/config"
)

func TestStatusLinkedCorrect(t *testing.T) {
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

	results, err := Status(cfg)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if results[0].Status != StatusLinkedCorrect {
		t.Errorf("status = %q, want %q", results[0].Status, StatusLinkedCorrect)
	}
}

func TestStatusLinkedElsewhere(t *testing.T) {
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

	results, err := Status(cfg)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if results[0].Status != StatusLinkedElsewhere {
		t.Errorf("status = %q, want %q", results[0].Status, StatusLinkedElsewhere)
	}
}

func TestStatusExistsNotLink(t *testing.T) {
	baseDir := t.TempDir()
	targetDir := t.TempDir()
	target := filepath.Join(targetDir, ".zshrc")

	src := filepath.Join(baseDir, ".zshrc")
	if err := os.WriteFile(src, []byte("content"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(target, []byte("existing"), 0o644); err != nil {
		t.Fatal(err)
	}

	cfg := &config.Config{
		BaseDir: baseDir,
		Link:    []config.Link{{Source: ".zshrc", Target: target}},
	}

	results, err := Status(cfg)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if results[0].Status != StatusExistsNotLink {
		t.Errorf("status = %q, want %q", results[0].Status, StatusExistsNotLink)
	}
}

func TestStatusMissing(t *testing.T) {
	baseDir := t.TempDir()
	target := filepath.Join(t.TempDir(), ".zshrc")

	src := filepath.Join(baseDir, ".zshrc")
	if err := os.WriteFile(src, []byte("content"), 0o644); err != nil {
		t.Fatal(err)
	}

	cfg := &config.Config{
		BaseDir: baseDir,
		Link:    []config.Link{{Source: ".zshrc", Target: target}},
	}

	results, err := Status(cfg)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if results[0].Status != StatusMissing {
		t.Errorf("status = %q, want %q", results[0].Status, StatusMissing)
	}
}

func TestStatusSourceMissing(t *testing.T) {
	baseDir := t.TempDir()
	target := filepath.Join(t.TempDir(), ".zshrc")

	cfg := &config.Config{
		BaseDir: baseDir,
		Link:    []config.Link{{Source: "nonexistent", Target: target}},
	}

	results, err := Status(cfg)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if results[0].Status != StatusSourceMissing {
		t.Errorf("status = %q, want %q", results[0].Status, StatusSourceMissing)
	}
}

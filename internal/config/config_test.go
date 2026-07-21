package config

import (
	"os"
	"path/filepath"
	"testing"
)

func TestLoad(t *testing.T) {
	tests := []struct {
		name    string
		content string
		wantN   int
		wantErr bool
	}{
		{
			name: "valid config with multiple links",
			content: `
[[link]]
source = "zsh/.zshrc"
target = "~/.zshrc"

[[link]]
source = "nvim"
target = "~/.config/nvim"
`,
			wantN: 2,
		},
		{
			name:    "empty config",
			content: "",
			wantN:   0,
		},
		{
			name: "single link",
			content: `
[[link]]
source = "git/.gitconfig"
target = "~/.gitconfig"
`,
			wantN: 1,
		},
		{
			name:    "invalid toml",
			content: "[[link\nsource = broken",
			wantErr: true,
		},
		{
			name: "empty source field",
			content: `
[[link]]
source = ""
target = "~/.zshrc"
`,
			wantErr: true,
		},
		{
			name: "empty target field",
			content: `
[[link]]
source = "zsh/.zshrc"
target = ""
`,
			wantErr: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			dir := t.TempDir()
			path := filepath.Join(dir, "dotlink.toml")
			if err := os.WriteFile(path, []byte(tt.content), 0o644); err != nil {
				t.Fatal(err)
			}

			cfg, err := Load(path)
			if tt.wantErr {
				if err == nil {
					t.Fatal("expected error, got nil")
				}
				return
			}
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if len(cfg.Link) != tt.wantN {
				t.Errorf("got %d links, want %d", len(cfg.Link), tt.wantN)
			}
			if cfg.BaseDir != dir {
				t.Errorf("BaseDir = %q, want %q", cfg.BaseDir, dir)
			}
		})
	}
}

func TestLoadFileNotFound(t *testing.T) {
	_, err := Load("/nonexistent/path/dotlink.toml")
	if err == nil {
		t.Fatal("expected error for missing file, got nil")
	}
}

func TestLoadRelativePath(t *testing.T) {
	dir := t.TempDir()
	content := `
[[link]]
source = "a"
target = "b"
`
	path := filepath.Join(dir, "dotlink.toml")
	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}

	old, _ := os.Getwd()
	if err := os.Chdir(dir); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = os.Chdir(old) })

	cfg, err := Load("dotlink.toml")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !filepath.IsAbs(cfg.BaseDir) {
		t.Errorf("BaseDir should be absolute, got %q", cfg.BaseDir)
	}
}

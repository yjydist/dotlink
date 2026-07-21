package util_test

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/yjydist/dotlink/internal/util"
)

func TestExpandPath(t *testing.T) {
	home, err := os.UserHomeDir()
	if err != nil {
		t.Fatalf("无法获取用户主目录: %v", err)
	}

	t.Setenv("DOTLINK_VAR", "hello")
	t.Setenv("DOTLINK_ANOTHER", "world")
	t.Setenv("DOTLINK_WITH_SPACE", "foo bar")

	tests := []struct {
		name     string
		input    string
		expected string
	}{
		{
			name:     "空路径",
			input:    "",
			expected: "",
		},
		{
			name:     "波浪号展开",
			input:    "~/config",
			expected: filepath.Join(home, "config"),
		},
		{
			name:     "单个 $VAR",
			input:    "$DOTLINK_VAR/config.toml",
			expected: "hello/config.toml",
		},
		{
			name:     "多个 $VAR",
			input:    "$DOTLINK_VAR/$DOTLINK_ANOTHER",
			expected: "hello/world",
		},
		{
			name:     "单个 ${VAR}",
			input:    "${DOTLINK_VAR}/config.toml",
			expected: "hello/config.toml",
		},
		{
			name:     "多个混合 $VAR 和 ${VAR}",
			input:    "$DOTLINK_VAR/${DOTLINK_ANOTHER}/$DOTLINK_VAR",
			expected: "hello/world/hello",
		},
		{
			name:     "波浪号、$VAR 和 ${VAR} 混合",
			input:    "~/configs/$DOTLINK_VAR/${DOTLINK_ANOTHER}",
			expected: filepath.Join(home, "configs", "hello", "world"),
		},
		{
			name:     "变量值包含空格",
			input:    "/tmp/${DOTLINK_WITH_SPACE}/baz",
			expected: "/tmp/foo bar/baz",
		},
		{
			name:     "未设置的环境变量替换为空字符串",
			input:    "/tmp/$DOTLINK_NOT_SET/baz",
			expected: "/tmp//baz",
		},
		{
			name:     "普通绝对路径保持不变",
			input:    "/usr/local/bin",
			expected: "/usr/local/bin",
		},
		{
			name:     "普通相对路径保持不变",
			input:    "relative/path",
			expected: "relative/path",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := util.ExpandPath(tt.input)
			if err != nil {
				t.Fatalf("ExpandPath(%q) 返回错误: %v", tt.input, err)
			}
			if got != tt.expected {
				t.Errorf("ExpandPath(%q) = %q, want %q", tt.input, got, tt.expected)
			}
		})
	}
}

func TestResolveLink(t *testing.T) {
	cwd, err := os.Getwd()
	if err != nil {
		t.Fatalf("无法获取当前目录: %v", err)
	}
	home, err := os.UserHomeDir()
	if err != nil {
		t.Fatalf("无法获取用户主目录: %v", err)
	}

	t.Setenv("DOTLINK_HOME", "myenv")

	tests := []struct {
		name       string
		baseDir    string
		srcRaw     string
		tgRaw      string
		wantSource string
		wantTarget string
	}{
		{
			name:       "相对 source 基于 cfg 目录，相对 target 基于 cwd",
			baseDir:    "/etc/dotlink",
			srcRaw:     "files/gitconfig",
			tgRaw:      ".gitconfig",
			wantSource: "/etc/dotlink/files/gitconfig",
			wantTarget: filepath.Join(cwd, ".gitconfig"),
		},
		{
			name:       "绝对路径保持不变",
			baseDir:    "/etc/dotlink",
			srcRaw:     "/absolute/source",
			tgRaw:      "/absolute/target",
			wantSource: "/absolute/source",
			wantTarget: "/absolute/target",
		},
		{
			name:       "展开波浪号和环境变量",
			baseDir:    "/etc/dotlink",
			srcRaw:     "~/dotfiles/$DOTLINK_HOME",
			tgRaw:      "${DOTLINK_HOME}",
			wantSource: filepath.Join(home, "dotfiles", "myenv"),
			wantTarget: filepath.Join(cwd, "myenv"),
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			gotSource, gotTarget, err := util.ResolveLink(tt.baseDir, tt.srcRaw, tt.tgRaw)
			if err != nil {
				t.Fatalf("ResolveLink(%q, %q, %q) 返回错误: %v", tt.baseDir, tt.srcRaw, tt.tgRaw, err)
			}
			if gotSource != tt.wantSource {
				t.Errorf("source = %q, want %q", gotSource, tt.wantSource)
			}
			if gotTarget != tt.wantTarget {
				t.Errorf("target = %q, want %q", gotTarget, tt.wantTarget)
			}
		})
	}
}
